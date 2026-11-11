//! 加载器版本清单（下载页「安装选项」的版本下拉，M105）。
//!
//! ## 为什么要有这一支
//!
//! core 早就有现成能力：[`unlucky_core::addon::loader::LoaderSource::loader_versions`] 按
//! `(加载器, 原版版本)` 给出**官方那份**可用版本表（Fabric/Quilt 走 fabric meta 的 loader 列表，
//! Forge/NeoForge 走 maven 的 `maven-metadata.xml` 再按 `matches_mc` 过滤）。
//! 缺的从来不是能力，是 presenter 到 UI 的那一跳——于是用户在下载页只能装原版，
//! 「自己有的能力不接出来」是 M105 要收的口子。
//!
//! ## 一次查询只回答一个问题
//!
//! 事件里带 `(kind, mc_version)` 原样回传，`state.rs` 拿它当缓存键：**切原版版本 = 换键**，
//! 于是 A 版本的 Fabric 列表不可能挂到 B 版本下面（这条是 M105 复审明确要求的）。
//! 取不到（网络/该组合官方没有版本）走 `Err(人读原因)`，页面据此**禁掉确认**并给具名错误，
//! 绝不静默显示成空列表。

use unlucky_core::addon::loader::{LoaderKind as CoreLoaderKind, LoaderSource};
use unlucky_core::download::blocking::block_on;

use crate::dto::LoaderKind;

use super::env::HostEnv;
use super::job::{spawn, HostTask, JobSender};

/// 拉一份加载器版本列表（网络在后台线程里，UI 不卡）。
pub fn spawn_list(env: HostEnv, tx: JobSender, kind: LoaderKind, mc_version: String) {
    spawn("loader-versions", move || {
        let result = fetch(&env, kind, &mc_version);
        let _ = tx.send(HostTask::LoaderVersions {
            kind,
            mc_version,
            result,
        });
    });
}

/// 同步拉取（后台线程里跑）：DTO 的加载器词表 → core 的加载器词表 → 官方清单。
///
/// 原版（`Vanilla`）没有「加载器版本」这回事，直接给空列表而不是报错：调用方（页面）
/// 在选了原版时根本不画这个下拉，这里是防守。
pub fn fetch(env: &HostEnv, kind: LoaderKind, mc_version: &str) -> Result<Vec<String>, String> {
    if kind == LoaderKind::Vanilla {
        return Ok(Vec::new());
    }
    let Some(kind) = CoreLoaderKind::parse(kind.as_str()) else {
        return Err(format!("未知加载器 `{}`", kind.as_str()));
    };
    if mc_version.trim().is_empty() {
        return Err("没有选择原版版本".to_string());
    }
    let engine = super::engine::engine(env);
    let meta_cache = env.layout.meta_cache();
    block_on(async {
        LoaderSource::new(&engine, &meta_cache)
            .loader_versions(kind, mc_version.trim())
            .await
    })
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::dto::{AppEvent, CoreEvent};
    use crate::state::AppState;

    /// **缓存键必须是 `(加载器, 原版版本)`**（M105 的硬要求，复审点名的那条）。
    ///
    /// 现场：在 1.20.1 上查过 Fabric 的列表，接着点 1.21.9；键里若没有原版版本，1.21.9 会直接
    /// 显示 1.20.1 那份列表 —— 用户选了个「1.21.9 上不存在」的加载器版本，直到点安装才失败。
    #[test]
    fn cache_is_keyed_per_loader_and_minecraft_version() {
        let mut state = AppState::new();
        state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
            kind: LoaderKind::Fabric,
            mc_version: "1.20.1".to_string(),
            result: Ok(vec!["0.16.5".to_string()]),
        }));
        assert!(state
            .loader_versions_for(LoaderKind::Fabric, "1.20.1")
            .is_some());
        assert!(state
            .loader_versions_for(LoaderKind::Fabric, "1.21.9")
            .is_none());
        assert!(state
            .loader_versions_for(LoaderKind::Quilt, "1.20.1")
            .is_none());

        // 失败也是「查过了」：页面据此显示具名原因并禁掉确认，不再无限重试
        state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
            kind: LoaderKind::Quilt,
            mc_version: "1.20.1".to_string(),
            result: Err("network down".to_string()),
        }));
        assert!(state
            .loader_versions_for(LoaderKind::Quilt, "1.20.1")
            .is_some());
    }

    /// 原版没有加载器版本：给空表而不是报错（页面不画这个下拉，防守而已）。
    #[test]
    fn vanilla_has_no_loader_versions() {
        let dir = std::env::temp_dir().join(format!("ull-loader-versions-{}", std::process::id()));
        let env = HostEnv::at_root(&dir);
        assert_eq!(fetch(&env, LoaderKind::Vanilla, "1.20.1"), Ok(Vec::new()));
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 空版本号在联网前就被拒（不做无意义的请求，也给出人读原因）。
    #[test]
    fn empty_mc_version_is_rejected_before_network() {
        let dir = std::env::temp_dir().join(format!("ull-loader-empty-{}", std::process::id()));
        let env = HostEnv::at_root(&dir);
        let err = fetch(&env, LoaderKind::Fabric, " ").unwrap_err();
        assert!(err.contains("没有选择原版版本"), "{err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
