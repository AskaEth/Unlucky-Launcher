//! 三个设置键的真读写（M197）：`java.installRoot` / `java.vendorPriority` / `java.auto_install`。
//!
//! 与 `store.rs`（M175 的内存 / JVM / 默认 JDK）同一条通路：`core::setting::store::SettingStore`
//! —— 读一份 → 改几个键 → 存 → **再从磁盘读回来**。写盘只认 `Saved` 一档，其余一律当失败
//! （`WriteError` 的三档与 `store.rs` 同一套类型，界面文案也同一套）。
//!
//! ## 「生效值 + 从哪一层来」为什么在这里算
//!
//! 两个键都可能**留空**，而留空的语义分别指向另外两层：
//!
//! | 键 | 留空 = | 生效值的第二层 |
//! |---|---|---|
//! | `java.installRoot` | 按 `core.data_mode` 派生落点 | `JavaInstallRoot::resolve`（portable / standard） |
//! | `java.vendorPriority` | 按地区默认序 | `vendor::order(&[], domestic)` |
//!
//! 所以「现在到底装到哪 / 按什么顺序找」只有把**落盘值**代进 core 的纯函数才说得清。
//! 这两个纯函数本身就是真正下载时用的那一对（`install` 也走 `resolve` 与 `order`），
//! 于是界面上的「生效」与真跑起来的行为不会各说一套。
//!
//! ## 地区判定只做一次
//!
//! `domestic` 从 `cache/region.json` 读（引擎建过一次就有）；读不到时按环境信号判一次并写回
//! ——与 `shared::build` 的做法一致。它**不联网**（`region_system` 只跑 `tzutil` / `reg`），
//! 且本函数只在首屏与配置文件变化时被调用，不进心跳。

use std::path::{Path, PathBuf};

use unlucky_core::download::cache::now_ms;
use unlucky_core::download::region::{decide_from, load_cached, store_cached};
use unlucky_core::download::region_system::detect_signals;
use unlucky_core::game::java::vendor::{self, JdkVendor};
use unlucky_core::game::java::{JavaInstallRoot, LocationMode};
use unlucky_core::platform::data_dir::DataMode;
use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::platform::paths::{Platform, ProcessEnv};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::{SaveOutcome, SettingStore};

use super::prefs::{Effective, OrderLayer, Prefs, RootLayer, Snapshot};
use super::store::{unwritable, WriteError};

/// 读三键真值。
#[must_use]
pub fn load(config: &Path) -> Snapshot {
    let store = SettingStore::load(config, launcher_schema());
    snapshot(store.doc())
}

/// 写三键，返回**落盘后读回来**的真值。
///
/// # Errors
/// 本地校验不过（[`WriteError::Refused`]）、文件只读 / 不可信（[`WriteError::Unwritable`]）
/// 或 schema 拒绝（[`WriteError::Schema`]）时返回原因。
pub fn save(config: &Path, prefs: &Prefs) -> Result<Snapshot, WriteError> {
    prefs.validate().map_err(WriteError::Refused)?;
    let mut store = SettingStore::load(config, launcher_schema());
    let root = prefs.install_root.trim().to_string();
    let ids = prefs.priority_ids();
    // `java.vendorPriority` 是数组键，而 `SettingStore::set` 只收 `serde_json::Value`——
    // app-iced 的依赖表里没有 serde_json（本 mission 也改不了 manifest），所以不现造一个
    // `Value`：从文档里取**当前那个数组**（schema 默认就是空数组）清空再填，形状与 schema 一致。
    let mut array = store
        .doc()
        .effective("java.vendorPriority")
        .cloned()
        .unwrap_or_default();
    {
        let Some(items) = array.as_array_mut() else {
            return Err(WriteError::Schema(
                "java.vendorPriority 当下不是数组（schema 的默认值是空数组）".to_string(),
            ));
        };
        items.clear();
        for id in &ids {
            items.push(id.as_str().into());
        }
    }
    store
        .set("java.installRoot", root.into())
        .and_then(|()| store.set("java.auto_install", prefs.auto_install.into()))
        .and_then(|()| store.set("java.vendorPriority", array))
        .map_err(|error| WriteError::Schema(format!("{error}")))?;
    match store.save() {
        Ok(SaveOutcome::Saved(_)) => Ok(load(config)),
        Ok(SaveOutcome::Skipped(reason)) => Err(WriteError::Unwritable(unwritable(reason))),
        Err(error) => Err(WriteError::Schema(format!("{error}"))),
    }
}

/// 生效值 + 来源层（渲染器只翻文案）。
#[must_use]
pub fn effective(config: &Path, prefs: &Prefs) -> Effective {
    let store = SettingStore::load(config, launcher_schema());
    let doc = store.doc();
    let data_mode = doc
        .effective_str("core.data_mode")
        .and_then(DataMode::parse)
        .unwrap_or_else(|| DataMode::default_for(Platform::current()));
    let override_root = {
        let text = prefs.saved_install_root.trim();
        (!text.is_empty()).then(|| PathBuf::from(text))
    };
    let layout = home_of(config).map(LauncherLayout::new);
    let (root, root_mode, root_layer, root_unresolved) = match &layout {
        Some(layout) => match JavaInstallRoot::resolve(
            data_mode,
            layout,
            override_root.as_deref(),
            Platform::current(),
            &ProcessEnv,
        ) {
            Ok(resolved) => (
                resolved.root.display().to_string(),
                resolved.mode.as_str(),
                layer_of(resolved.mode),
                false,
            ),
            Err(error) => {
                eprintln!("[java] 解析安装根失败：{error}");
                (
                    String::new(),
                    data_mode.as_str(),
                    if override_root.is_some() {
                        RootLayer::Setting
                    } else {
                        RootLayer::DataMode
                    },
                    true,
                )
            }
        },
        None => (String::new(), data_mode.as_str(), RootLayer::DataMode, true),
    };

    let domestic = domestic_of(config);
    let priority = prefs
        .saved_priority
        .iter()
        .filter_map(|id| JdkVendor::parse(id))
        .collect::<Vec<_>>();
    let order = vendor::order(&priority, domestic);
    Effective {
        root,
        root_mode,
        root_layer,
        root_unresolved,
        order: order.iter().map(|vendor| vendor.display()).collect(),
        order_layer: if priority.is_empty() {
            OrderLayer::Region
        } else {
            OrderLayer::Setting
        },
        domestic,
    }
}

/// 现在的地区判定（`cache/region.json` 优先，没有就判一次并写回）。
///
/// 与 `core::download::shared::build` 同源：同一个 `region.json`、同一套信号。
/// 读不到缓存目录（配置路径不像 `<home>/config/launcher.json`）时按**国内**算——
/// 那是 D23 出厂口径，也是唯一一个「不知道还更保守」的取值（镜像先试不会更差）。
#[must_use]
pub fn domestic_of(config: &Path) -> bool {
    let Some(home) = home_of(config) else {
        return true;
    };
    let cache = LauncherLayout::new(home).cache();
    let now = now_ms();
    let path = cache.join("region.json");
    if let Some(profile) = load_cached(&path, now) {
        return profile.is_domestic();
    }
    let profile = decide_from(&detect_signals(), now);
    let _ = std::fs::create_dir_all(&cache);
    let _ = store_cached(&path, &profile);
    profile.is_domestic()
}

/// 托管 JDK 该装到哪（下载作业用；与界面上的「生效」同一个函数）。
#[must_use]
pub fn install_root(config: &Path, saved_install_root: &str) -> Option<JavaInstallRoot> {
    let store = SettingStore::load(config, launcher_schema());
    let data_mode = store
        .doc()
        .effective_str("core.data_mode")
        .and_then(DataMode::parse)
        .unwrap_or_else(|| DataMode::default_for(Platform::current()));
    let home = home_of(config)?;
    let layout = LauncherLayout::new(home);
    let override_root = {
        let text = saved_install_root.trim();
        (!text.is_empty()).then(|| PathBuf::from(text))
    };
    JavaInstallRoot::resolve(
        data_mode,
        &layout,
        override_root.as_deref(),
        Platform::current(),
        &ProcessEnv,
    )
    .ok()
}

/// 下载引擎的缓存根（`<home>/cache`）：让 JDK 归档与其它下载共用同一份缓存 / 健康度。
#[must_use]
pub fn cache_root(config: &Path) -> Option<PathBuf> {
    home_of(config).map(|home| LauncherLayout::new(home).cache())
}

/// launcher home（`<home>/config/launcher.json` 的上一级再上一级）。
fn home_of(config: &Path) -> Option<&Path> {
    config.parent().and_then(Path::parent)
}

/// 落点模式 → 「从哪一层来」。
fn layer_of(mode: LocationMode) -> RootLayer {
    match mode {
        LocationMode::Custom => RootLayer::Setting,
        LocationMode::Standard | LocationMode::Portable => RootLayer::DataMode,
    }
}

/// 文档 → 快照。
fn snapshot(doc: &unlucky_core::setting::document::SettingDocument) -> Snapshot {
    Snapshot {
        install_root: doc
            .effective_str("java.installRoot")
            .unwrap_or_default()
            .trim()
            .to_string(),
        auto_install: doc
            .effective_bool("java.auto_install")
            .unwrap_or(super::prefs::DEFAULT_AUTO_INSTALL),
        priority: doc
            .effective("java.vendorPriority")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    }
}
