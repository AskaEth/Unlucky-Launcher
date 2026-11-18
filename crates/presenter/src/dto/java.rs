//! Java 页 DTO：四家发行版、回退链可视化与本地运行时清单（D23/D13）。
//!
//! 回退链是「可观测性」的载体：安装失败时用户要能看见「Temurin 跳过 → Oracle 失败 →
//! Microsoft 命中」这条链，而不是只拿到一句「下载失败」。链的每一步由 host 从
//! `core::game::java::source::ChainLog` 翻译过来。

use serde::{Deserialize, Serialize};

/// 一家发行版的展示行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaVendorView {
    /// 稳定 id（`temurin` / `oracle` / `microsoft` / `zulu`）
    pub id: String,
    /// 展示名
    pub display: String,
    /// 优先级序号（1 起）
    pub rank: u32,
    /// 支持的 Java 大版本摘要（`21/17/11`）
    pub majors: String,
    /// 可用源（镜像 + 吞吐备注）
    pub sources: Vec<String>,
    /// 是否已装过这家（托管目录里有现成的）
    pub installed: bool,
}

/// 回退链一步的结局。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JavaChainOutcome {
    /// 还没尝试（安装前的计划链）
    Pending,
    /// 跳过（不支持目标大版本）
    Skip,
    /// 尝试过但失败
    Failed,
    /// 命中（下载/安装成功）
    Hit,
}

impl JavaChainOutcome {
    /// 徽标文字。
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "待尝试",
            Self::Skip => "跳过",
            Self::Failed => "失败",
            Self::Hit => "命中",
        }
    }

    /// 从 core 的 `VendorAttempt` 结局字符串解析。
    pub fn parse(raw: &str) -> Self {
        match raw {
            "hit" => Self::Hit,
            "failed" => Self::Failed,
            "skip" | "skipped" => Self::Skip,
            _ => Self::Pending,
        }
    }
}

/// 回退链一步。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaChainStep {
    /// 发行版 id
    pub vendor: String,
    /// 结局
    pub outcome: JavaChainOutcome,
    /// 细节（跳过原因 / 失败摘要 / 命中的下载源）
    pub detail: String,
}

/// 本地已发现的 Java 运行时。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaRuntimeView {
    /// 可执行文件路径
    pub binary: String,
    /// 版本串（`17.0.9`）
    pub version: String,
    /// 大版本
    pub major: u32,
    /// 发行方
    pub vendor: String,
    /// 来源分类（system / managed / instance）
    pub origin: String,
}

/// Java 页快照（一次事件整体替换）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JavaPageView {
    /// 地区判定（`domestic` / `global`）
    pub region: String,
    /// 优先级的来源（`region_default` / `java.vendorPriority`）
    pub priority_source: String,
    /// 生效顺序（`temurin → oracle → microsoft → zulu`）
    pub order_text: String,
    /// 四家展示行（按生效顺序）
    pub vendors: Vec<JavaVendorView>,
    /// 回退链（计划链或上一次安装的真实链）
    pub chain: Vec<JavaChainStep>,
    /// 本地运行时
    pub runtimes: Vec<JavaRuntimeView>,
    /// 托管目录
    pub managed_roots: Vec<String>,
    /// 是否有 Java 安装在跑
    pub busy: bool,
    /// 状态/错误文案
    pub message: String,
}

impl Default for JavaPageView {
    fn default() -> Self {
        Self::new()
    }
}

impl JavaPageView {
    /// 空态（托管目录还没扫到东西）。
    pub fn new() -> Self {
        Self {
            region: "global".to_string(),
            priority_source: "region_default".to_string(),
            order_text: String::new(),
            vendors: Vec::new(),
            chain: Vec::new(),
            runtimes: Vec::new(),
            managed_roots: Vec::new(),
            busy: false,
            message: String::new(),
        }
    }

    /// 计划链：按生效顺序铺一排「待尝试」（安装前的可视化）。
    pub fn planned_chain(vendors: &[JavaVendorView]) -> Vec<JavaChainStep> {
        vendors
            .iter()
            .map(|vendor| JavaChainStep {
                vendor: vendor.id.clone(),
                outcome: JavaChainOutcome::Pending,
                detail: String::new(),
            })
            .collect()
    }

    /// 命中的那一家的展示名（`None` = 链上还没命中）。
    pub fn hit_vendor(&self) -> Option<&str> {
        self.chain
            .iter()
            .find(|step| step.outcome == JavaChainOutcome::Hit)
            .map(|step| step.vendor.as_str())
    }

    /// 已安装的大版本列表（去重升序）。
    pub fn majors(&self) -> Vec<u32> {
        let mut majors: Vec<u32> = self.runtimes.iter().map(|runtime| runtime.major).collect();
        majors.sort_unstable();
        majors.dedup();
        majors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vendor(id: &str, rank: u32) -> JavaVendorView {
        JavaVendorView {
            id: id.to_string(),
            display: id.to_uppercase(),
            rank,
            majors: "21/17".to_string(),
            sources: Vec::new(),
            installed: false,
        }
    }

    #[test]
    fn planned_chain_is_all_pending_in_rank_order() {
        let vendors = vec![vendor("temurin", 1), vendor("oracle", 2)];
        let chain = JavaPageView::planned_chain(&vendors);
        assert_eq!(chain.len(), 2);
        assert!(chain
            .iter()
            .all(|step| step.outcome == JavaChainOutcome::Pending));
        assert_eq!(chain[0].vendor, "temurin");
    }

    #[test]
    fn hit_vendor_finds_the_hit_step() {
        let mut page = JavaPageView::new();
        page.chain = vec![
            JavaChainStep {
                vendor: "temurin".to_string(),
                outcome: JavaChainOutcome::Skip,
                detail: "不支持 8".to_string(),
            },
            JavaChainStep {
                vendor: "oracle".to_string(),
                outcome: JavaChainOutcome::Hit,
                detail: "BMCLAPI".to_string(),
            },
        ];
        assert_eq!(page.hit_vendor(), Some("oracle"));
    }

    #[test]
    fn outcome_parses_core_words() {
        assert_eq!(JavaChainOutcome::parse("hit"), JavaChainOutcome::Hit);
        assert_eq!(JavaChainOutcome::parse("failed"), JavaChainOutcome::Failed);
        assert_eq!(JavaChainOutcome::parse("skip"), JavaChainOutcome::Skip);
        assert_eq!(JavaChainOutcome::parse("?"), JavaChainOutcome::Pending);
    }

    #[test]
    fn majors_are_deduped_and_sorted() {
        let mut page = JavaPageView::new();
        page.runtimes = vec![
            JavaRuntimeView {
                binary: "a".to_string(),
                version: "17".to_string(),
                major: 17,
                vendor: "Temurin".to_string(),
                origin: "managed".to_string(),
            },
            JavaRuntimeView {
                binary: "b".to_string(),
                version: "17".to_string(),
                major: 17,
                vendor: "Oracle".to_string(),
                origin: "system".to_string(),
            },
        ];
        assert_eq!(page.majors(), vec![17]);
    }
}
