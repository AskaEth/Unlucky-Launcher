//! 版本查询的发行版分派 + 回退链记录（D23）。
//!
//! 两件事：
//!
//! 1. [`fetch`]：把一次「某发行版某大版本的最新 GA」查询分派给对应实现——
//!    Temurin 走 `api.adoptium.net`（v1 既有），Oracle / MS / Zulu 各走自己的源表；
//! 2. [`ChainLog`]：按用户优先级依次尝试时，**每一跳的结果与降级原因**都记下来。
//!    这份记录直接进 `--json` 与事件流，是「为什么最后装的是 MS 而不是 Temurin」的唯一解释。
//!
//! 回退的粒度是**发行版级**（D23 原话）：版本不存在、镜像全 4xx、校验失败都算这一家失败，
//! 换下一家；同一家内部的镜像换源（TUNA→官方、华为云→南大）仍然归 `core::download` 引擎管。

use super::api;
use super::catalog;
use super::error::{JavaError, JavaErrorKind};
use super::microsoft;
use super::oracle;
use super::release::JdkRelease;
use super::target::AdoptiumTarget;
use super::vendor::JdkVendor;
use super::zulu;
use crate::download::{DownloadEngine, MirrorTable};

/// 查一次「某发行版某大版本」的归档。
///
/// `version = None` 是要该家**最新 GA**（D13 口径，行为与 M280 之前逐字节一致）；
/// `Some(v)` 要求精确版本：能按版本取的只有 Temurin / Oracle，Microsoft 只认内置快照里那一个，
/// Zulu（只有 `latest=true`）与 Termux（系统包定版本）直接报错——错误文案由
/// [`catalog::version_pin_error`] 统一出，**这里不换家、不回落 latest**。
///
/// # Errors
///
/// 平台无产物、清单不可达、该版本不存在、校验缺失、这家不支持指定版本时返回错误——调用方据此降级。
pub async fn fetch(
    engine: &DownloadEngine,
    vendor: JdkVendor,
    major: u32,
    target: AdoptiumTarget,
    version: Option<&str>,
) -> Result<JdkRelease, JavaError> {
    let client = engine.client();
    let table = engine.mirrors();
    match version {
        None => latest(client, table, vendor, major, target).await,
        Some(wanted) => pinned(client, table, vendor, major, target, wanted).await,
    }
}

/// 该家的最新 GA（`fetch` 的 `None` 那一路，M280 之前只有这一条）。
async fn latest(
    client: &reqwest::Client,
    table: &MirrorTable,
    vendor: JdkVendor,
    major: u32,
    target: AdoptiumTarget,
) -> Result<JdkRelease, JavaError> {
    match vendor {
        JdkVendor::Temurin => api::fetch_latest(client, major, target).await,
        JdkVendor::Oracle => oracle::fetch_latest(client, table, major, target).await,
        JdkVendor::Microsoft => {
            microsoft::fetch_latest(client, microsoft_table(table)?, major, target).await
        }
        JdkVendor::Zulu => zulu::fetch_latest(client, zulu_table(table)?, major, target).await,
        JdkVendor::Termux => Err(termux_archive_error()),
    }
}

/// 该家的指定版本（`fetch` 的 `Some` 那一路）。
async fn pinned(
    client: &reqwest::Client,
    table: &MirrorTable,
    vendor: JdkVendor,
    major: u32,
    target: AdoptiumTarget,
    wanted: &str,
) -> Result<JdkRelease, JavaError> {
    match vendor {
        JdkVendor::Temurin => api::fetch_version(client, major, target, wanted).await,
        JdkVendor::Oracle => oracle::fetch_version(client, table, major, target, wanted).await,
        JdkVendor::Microsoft => {
            microsoft::fetch_version(client, microsoft_table(table)?, major, target, wanted).await
        }
        // 上游只给最新一份（Zulu）/ 由系统包管理器定版本（Termux）：明确报错，
        // 绝不换成别的版本假装装成了用户点名的那一版。
        JdkVendor::Zulu | JdkVendor::Termux => Err(catalog::version_pin_error(
            vendor,
            major,
            wanted,
            &[],
            catalog::VersionListScope::Complete,
        )),
    }
}

fn microsoft_table(table: &MirrorTable) -> Result<&crate::download::MsOpenJdkTable, JavaError> {
    table.microsoft_openjdk().ok_or_else(|| {
        JavaError::new(
            JavaErrorKind::Metadata,
            "镜像清单里没有 microsoft_openjdk 表",
        )
        .with_hint("升级 mirrors.json 到 v3，或改用 --vendor temurin")
    })
}

fn zulu_table(table: &MirrorTable) -> Result<&crate::download::ZuluTable, JavaError> {
    table.zulu().ok_or_else(|| {
        JavaError::new(JavaErrorKind::Metadata, "镜像清单里没有 zulu 表")
            .with_hint("升级 mirrors.json 到 v3，或改用 --vendor temurin")
    })
}

fn termux_archive_error() -> JavaError {
    JavaError::new(
        JavaErrorKind::Unsupported,
        "Termux 的 JDK 由系统包管理器安装，不走归档下载链",
    )
    .with_hint("在 Termux 里用 `ull java install <major>`（默认走 termux 源）")
}

/// 一跳的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VendorAttempt {
    /// 尝试的发行版。
    pub vendor: JdkVendor,
    /// 是否命中（成功拿到可下载的归档）。
    pub ok: bool,
    /// 命中/降级的一句话原因（`"命中"` / `"没有 8 的 GA（只发 11/17/21/25）"`）。
    pub detail: String,
}

impl VendorAttempt {
    /// `--json` 形状。
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "vendor": self.vendor.id(),
            "ok": self.ok,
            "detail": self.detail,
        })
    }
}

/// 一条回退链的执行记录。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChainLog {
    attempts: Vec<VendorAttempt>,
}

impl ChainLog {
    /// 空记录。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 记一次「跳过」（该家不支持这个大版本，根本没查网络）。
    pub fn skip(&mut self, vendor: JdkVendor, detail: impl Into<String>) {
        self.attempts.push(VendorAttempt {
            vendor,
            ok: false,
            detail: detail.into(),
        });
    }

    /// 记一次失败（查到了但拿不到、或下载/校验失败）。
    pub fn failed(&mut self, vendor: JdkVendor, detail: impl Into<String>) {
        self.attempts.push(VendorAttempt {
            vendor,
            ok: false,
            detail: detail.into(),
        });
    }

    /// 记一次命中。
    pub fn hit(&mut self, vendor: JdkVendor, detail: impl Into<String>) {
        self.attempts.push(VendorAttempt {
            vendor,
            ok: true,
            detail: detail.into(),
        });
    }

    /// 是否已经命中（命中后不再往下试）。
    #[must_use]
    pub fn hit_vendor(&self) -> Option<JdkVendor> {
        self.attempts
            .iter()
            .find(|attempt| attempt.ok)
            .map(|attempt| attempt.vendor)
    }

    /// 全部记录。
    #[must_use]
    pub fn attempts(&self) -> &[VendorAttempt] {
        &self.attempts
    }

    /// 降级说明（「已跳过」的那些；给事件流与人读告警用）。
    #[must_use]
    pub fn degradations(&self) -> Vec<String> {
        self.attempts
            .iter()
            .filter(|attempt| !attempt.ok)
            .map(|attempt| format!("{}：{}", attempt.vendor.id(), attempt.detail))
            .collect()
    }

    /// 实际尝试了几家（不含跳过）。
    #[must_use]
    pub fn tried(&self) -> usize {
        self.attempts.len()
    }

    /// `--json` 形状。
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hit": self.hit_vendor().map(JdkVendor::id),
            "attempts": self.attempts.iter().map(VendorAttempt::to_json).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ChainLog;
    use crate::game::java::vendor::JdkVendor;

    #[test]
    fn chain_skips_unsupported_majors_and_records_the_reason() {
        let mut log = ChainLog::new();
        log.skip(JdkVendor::Oracle, "不支持 8（Oracle archive 从 9 起）");
        log.skip(JdkVendor::Microsoft, "没有 8 的 GA（只发 11/17/21/25）");
        log.hit(JdkVendor::Temurin, "命中 8.0.504+1");
        assert_eq!(log.hit_vendor(), Some(JdkVendor::Temurin));
        let degradations = log.degradations();
        assert_eq!(degradations.len(), 2);
        assert!(degradations[1].contains("microsoft"));
        assert_eq!(log.attempts().len(), 3);
        let json = log.to_json();
        assert_eq!(json["hit"], "temurin");
        assert_eq!(json["attempts"][0]["vendor"], "oracle");
    }

    #[test]
    fn failure_reasons_are_kept_for_the_event_stream() {
        let mut log = ChainLog::new();
        log.failed(JdkVendor::Temurin, "所有镜像都 4xx");
        log.hit(JdkVendor::Oracle, "命中 21.0.2");
        assert_eq!(
            log.degradations(),
            vec!["temurin：所有镜像都 4xx".to_string()],
            "降级原因必须原样可读（用户要能看懂为什么换了发行版）"
        );
        assert_eq!(log.tried(), 2);
    }
}
