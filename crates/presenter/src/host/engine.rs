//! 下载引擎与 Java 落点的装配（versions / install / java 三个任务共用）。
//!
//! 与 CLI `install::build_engine` 同源：内容寻址缓存落 `.ULL/cache/`，地区判定走
//! 24h 缓存 + 环境信号（**不做网络探测**，与 D7 的「不探测时」约定一致）。

use std::path::PathBuf;
use std::sync::Arc;

use unlucky_core::download::cache::now_ms;
use unlucky_core::download::region::{decide_from, load_cached, store_cached};
use unlucky_core::download::region_system::detect_signals;
use unlucky_core::download::{
    ContentCache, DownloadEngine, EngineConfig, HealthRegistry, MirrorTable,
};
use unlucky_core::game::java::vendor::JdkVendor;
use unlucky_core::game::java::{self, JavaInstallRoot};
use unlucky_core::platform::paths::ProcessEnv;

use super::env::HostEnv;

/// 建下载引擎（缓存/健康度/地区判定都在数据根里）。
pub fn engine(env: &HostEnv) -> DownloadEngine {
    let cache_root = env.layout.cache();
    let now = now_ms();
    let region_path = cache_root.join("region.json");
    let region = load_cached(&region_path, now).unwrap_or_else(|| {
        let profile = decide_from(&detect_signals(), now);
        let _ = store_cached(&region_path, &profile);
        profile
    });
    DownloadEngine::new(
        EngineConfig::default(),
        Arc::new(MirrorTable::builtin()),
        Arc::new(region),
        Arc::new(HealthRegistry::open_in(&cache_root, now)),
        Arc::new(ContentCache::open(&cache_root)),
    )
}

/// 是否国内网络（决定默认发行版顺序与镜像优先级）。
pub fn domestic(env: &HostEnv) -> bool {
    engine(env).region().is_domestic()
}

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

    #[test]
    fn engine_uses_the_data_root_cache() {
        let dir = std::env::temp_dir().join(format!("ull-host-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let env = HostEnv::at_root(&dir);
        std::fs::create_dir_all(env.layout.cache()).expect("cache");
        let engine = engine(&env);
        assert_eq!(engine.config().max_concurrency, 64);
        // 地区判定不联网：只读缓存/信号
        let _ = domestic(&env);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn java_merge_priority_reads_settings() {
        let dir = std::env::temp_dir().join(format!("ull-host-engine-p-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
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
