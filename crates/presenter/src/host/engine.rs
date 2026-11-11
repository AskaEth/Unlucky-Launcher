//! 下载引擎与 Java 落点的装配（versions / install / java 三个任务共用）。
//!
//! **引擎是进程级的、按数据根共享的**（M165）：本模块不再自己 `DownloadEngine::new`，而是向
//! [`unlucky_core::download::shared`] 要那个数据根上已经存在的引擎。于是连接池、信号量限流、
//! 镜像健康度、内容寻址缓存都跨请求累积，而不是每次请求清零（逐项代价与边界见那个模块的文档）。
//!
//! 与 CLI `install::build_engine` 同源：内容寻址缓存落 `.ULL/cache/`，地区判定走
//! 24h 缓存 + 环境信号（**不做网络探测**，与 D7 的「不探测时」约定一致）；区别只在生命周期
//! ——CLI 一条命令一个进程，建一次就够；GUI 进程要在成百上千次请求之间复用同一份。

use std::path::PathBuf;
use std::sync::Arc;

use unlucky_core::download::shared;
use unlucky_core::download::DownloadEngine;
use unlucky_core::game::java::vendor::JdkVendor;
use unlucky_core::game::java::{self, JavaInstallRoot};
use unlucky_core::platform::paths::ProcessEnv;

use super::env::HostEnv;

/// 取该数据根上**共享**的下载引擎（缓存/健康度/地区判定都在数据根里）。
///
/// 同一个数据根无论被请求多少次，都是同一个引擎；不同数据根各一份（缓存/健康度不串味）。
///
/// **取引擎前先把下载设置灌进 core 的下载调优**（M193，按数据根存）：`download.concurrency` /
/// `download.retry_limit` / `download.gh_proxy` 变了就重建引擎（值没变时 [`settings::download::apply_tuning`]
/// 一行不动，所以每次调用都灌是安全的）。这是「用户改了设置 ⇒ 下一次下载真的按新值跑」的落点：
/// 不在这里灌，进程级引擎会带着启动时那份参数活到进程结束——那正是「设置页看着有、实际没用」。
#[must_use]
pub fn engine(env: &HostEnv) -> Arc<DownloadEngine> {
    super::settings::download::apply_tuning(env);
    shared::engine_for(env.layout.cache())
}

/// 让这个数据根的共享引擎失效：下一次 [`engine`] 重新构造（重读地区判定/健康度/缓存索引）。
///
/// 在飞请求手里的 `Arc` 不受影响。设置页「重新检测地区」清掉 `region.json` 之后必须调它，
/// 否则那句话（「下次下载重新判定」）在进程级引擎下就不成立了。
pub fn forget(env: &HostEnv) {
    shared::forget(env.layout.cache());
}

/// 是否国内网络（决定默认发行版顺序与镜像优先级）。
pub fn domestic(env: &HostEnv) -> bool {
    engine(env).region().is_domestic()
}

/// 「重新检测地区」这个动作键（设置页的行 id、`settings::set` 的分派、`dispatch` 里「要不要重建
/// 自更新 Factory」的判断共用一个真相源）。
///
/// 动作不是普通设置：它要清 `region.json`、让 [`forget`] 失效共享引擎，并让自更新的 `Factory`
/// 换引擎（M165 复审 P2-1）——所以三处都得认出它。常量放这里（引擎/地区这一侧），不放已经顶到
/// 400 行红线的 `settings.rs`。
pub(super) const DETECT_REGION_ACTION: &str = "download.action.detect_region";

/// 设置里的 `java.vendorPriority`（空 = 按地区默认序）。
pub fn java_priority(env: &HostEnv) -> Vec<JdkVendor> {
    env.store()
        .effective("java.vendorPriority")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            let texts: Vec<String> = items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect();
            unlucky_core::game::java::vendor::parse_priority(&texts)
        })
        .unwrap_or_default()
}

/// 设置里的 `java.installRoot`（空 = 按数据目录模式派生）。
pub fn java_install_override(env: &HostEnv) -> Option<PathBuf> {
    env.store()
        .doc()
        .effective_str("java.installRoot")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from)
}

/// 托管 JDK 落点（解析失败 → 用户可读文案）。
pub fn java_install_root(env: &HostEnv) -> Result<JavaInstallRoot, String> {
    let override_root = java_install_override(env);
    JavaInstallRoot::resolve(
        env.mode(),
        &env.layout,
        override_root.as_deref(),
        env.platform,
        &ProcessEnv,
    )
    .map_err(|error| error.to_string())
}

/// 托管 JDK 可能存在的全部安装根（扫描运行时用）。
pub fn java_scan_roots(env: &HostEnv) -> Vec<PathBuf> {
    java::location::scan_roots(
        env.mode(),
        &env.layout,
        java_install_override(env).as_deref(),
        env.platform,
        &ProcessEnv,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 每个用例一个独立数据根（共享登记处是进程级的，键撞了会互相干扰）。
    fn root(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "ull-host-engine-{tag}-{}-{seq}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ULL").join("cache")).expect("cache");
        dir
    }

    #[test]
    fn engine_uses_the_data_root_cache() {
        let dir = root("cache");
        let env = HostEnv::at_root(&dir);
        let engine = engine(&env);
        assert_eq!(
            engine.config().max_concurrency,
            unlucky_core::download::config::DEFAULT_MAX_CONCURRENCY
        );
        assert_eq!(engine.concurrency_probe(), engine.config().max_concurrency);
        // 地区判定不联网：只读缓存/信号
        let _ = domestic(&env);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **M165 的守卫**：同一 host 的多次请求只建一个引擎。
    ///
    /// 断言的是**构造次数**（`shared::constructions`），不是「能不能下到文件」——每请求现建一个
    /// 引擎照样能下到文件，只有次数能把「复用」与「没复用」分开。
    ///
    /// 抓得住的变异：把 `core::download::shared::engine_for` 里 `engines.get` 的命中分支删掉
    /// （每次调用都现建），本用例红在 `constructions(...) == 1`。
    #[test]
    fn host_requests_share_one_engine() {
        let dir = root("share");
        let env = HostEnv::at_root(&dir);
        let cache_root = env.layout.cache();

        // 三条不同「请求」路径都从 engine() 取引擎（检索 / 清单 / 安装共用这一个漏斗）。
        let first = engine(&env);
        let second = engine(&env);
        let third = engine(&HostEnv::at_root(&dir));

        assert!(
            Arc::ptr_eq(&first, &second) && Arc::ptr_eq(&second, &third),
            "同一数据根的多次请求必须是同一个引擎"
        );
        assert_eq!(shared::constructions(&cache_root), 1, "只该构造一次引擎");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// 不同数据根**不共享**引擎（缓存/健康度串味比多建一个引擎严重得多）。
    #[test]
    fn different_data_roots_get_their_own_engine() {
        let dir_a = root("root-a");
        let dir_b = root("root-b");
        let env_a = HostEnv::at_root(&dir_a);
        let env_b = HostEnv::at_root(&dir_b);
        let engine_a = engine(&env_a);
        let engine_b = engine(&env_b);
        assert!(!Arc::ptr_eq(&engine_a, &engine_b));
        assert_eq!(shared::constructions(env_a.layout.cache()), 1);
        assert_eq!(shared::constructions(env_b.layout.cache()), 1);
        std::fs::remove_dir_all(&dir_a).ok();
        std::fs::remove_dir_all(&dir_b).ok();
    }

    /// `forget`（「重新检测地区」）之后下一次请求拿到新引擎。
    #[test]
    fn forget_makes_the_next_request_rebuild() {
        let dir = root("forget");
        let env = HostEnv::at_root(&dir);
        let before = engine(&env);
        forget(&env);
        let after = engine(&env);
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(shared::constructions(env.layout.cache()), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn java_merge_priority_reads_settings() {
        let dir = root("priority");
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.config()).expect("config");
        std::fs::write(
            env.launcher_config(),
            r#"{"java":{"vendorPriority":["zulu"]}}"#,
        )
        .expect("写配置");
        assert_eq!(java_priority(&env), vec![JdkVendor::Zulu]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
