//! 界面语言这一个键的读写（M106）：`ui.language` 的**显式值**。
//!
//! ## 为什么单独一个文件
//!
//! `settings_bridge.rs` 是外观 / 主题 / 数据根那一摊的落点，已经贴着 400 行硬线；语言的读写只有
//! 两个方法 + 一个「删键」辅助，独立出来之后两边都还能长。`impl SettingsBridge` 写在别的模块是
//! Rust 允许的（同一 crate 的固有实现），只需要主文件把三个字段放成 `pub(super)`。
//!
//! ## 两条判据（都跟 schema 的默认值有关）
//!
//! 1. **读的是显式值**，不是 `effective_str`：`ui.language` 的 schema 默认值是 `zh_cn`，用它会把
//!    「没配过」读成「用户手选中文」——英文机器一启动就被强行切成中文（D28 违约）。
//! 2. **「跟随系统」= 删掉这个键**，不是写哨兵值：允许值只有 `zh_cn` / `en_us`（枚举校验会拒别的），
//!    而「跟随系统」的语义本来就是「没有显式选择」。
//!
//! 这两条都在 `.tower/comms/findings/` 里登了记（建议把 schema 默认值改成 `null` 或加
//! `follow_system` 取值，那时本文件的删键路径可以简化成一次 `store.set`）。

use unlucky_core::setting::document::SettingDocument;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::settings_bridge::SettingsBridge;

/// 界面语言键（M106）：`zh_cn` / `en_us`（允许值见 `core::setting::keys::LANGUAGES`）。
///
/// **不写这个键 = 跟随系统**，所以它没有「system」这种取值，见 [`SettingsBridge::persist_ui_language`]。
const UI_LANGUAGE: &str = "ui.language";

impl SettingsBridge {
    /// 配置里 `ui.language` 的**显式值**（M106）：`None` = 没写过 = **跟随系统**。
    ///
    /// 不用 `effective_str`：那会把 schema 默认值（`zh_cn`）当成「用户手选中文」，于是**没配过**
    /// 的英文机器会一启动就被强行切成中文（D28 的「纯英文环境不用管」直接违约）。
    #[must_use]
    pub fn load_ui_language(&self) -> Option<String> {
        let store = self.store.as_ref()?;
        store
            .doc()
            .get(UI_LANGUAGE)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    }

    /// 写 `ui.language`：`Some("zh_cn")` / `Some("en_us")` 手选，`None` = **跟随系统 = 删键**。
    ///
    /// 返回是否真的落盘（只读 / 内存态时是 `false`，配置写不进去不该把 UI 卡住）。
    pub fn persist_ui_language(&mut self, value: Option<&str>) -> bool {
        let Some(store) = self.store.as_mut() else {
            return false;
        };
        let changed = match value {
            Some(language) => store
                .set(UI_LANGUAGE, serde_json::Value::String(language.to_owned()))
                .is_ok(),
            None => store.doc().get(UI_LANGUAGE).is_some() && remove_ui_language(store),
        };
        if !changed {
            return false;
        }
        let saved = store.save().is_ok();
        if let Some(path) = self.path() {
            self.last_mtime = std::fs::metadata(path)
                .ok()
                .and_then(|meta| meta.modified().ok());
        }
        saved
    }
}

/// 把 `ui.language` 从文档里摘掉：`SettingDocument` 没有 remove API，而落盘写的是**全量视图**，
/// 所以把全量视图里的那个键去掉再 load 回一份文档就等价于删键（用户手写的未知键也原样带回）。
fn remove_ui_language(store: &mut SettingStore) -> bool {
    let mut value = store.doc().to_value();
    let removed = value
        .get_mut("ui")
        .and_then(serde_json::Value::as_object_mut)
        .and_then(|ui| ui.remove("language"))
        .is_some();
    if !removed {
        return false;
    }

    let (mut doc, _problems) = SettingDocument::load(&launcher_schema(), &value);
    // 落盘的闸门是 `is_dirty()`（`write_document` 写全量视图，不是脏字段补丁）。
    doc.mark_all_dirty();
    *store.doc_mut() = doc;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_config(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-bridge-lang-{label}-{}-{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("config")).expect("临时目录");
        let path = dir.join("config/launcher.json");
        let _ = std::fs::remove_file(&path);
        path
    }

    /// 抓得住：①把 `effective_str` 当读法（英文机器没配过 → 被默认值 `zh_cn` 强行切成中文）；
    /// ②「跟随系统」写成哨兵值（枚举校验会拒、下次启动会撒谎）；③删键时连别的键一起删。
    /// 抓不住：UI 是否真的按这个值选目录（那是 `app-iced/src/ui/lang.rs` 的往返测试）。
    #[test]
    fn ui_language_round_trips_and_follow_system_removes_only_that_key() {
        let path = temp_config("roundtrip");
        std::fs::write(
            &path,
            r#"{"appearance":{"theme_mode":"dark"},"custom":{"keep":true}}"#,
        )
        .expect("写配置");

        // 没写过 = 跟随系统（不是默认值 zh_cn）
        let bridge = SettingsBridge::with_path(&path);
        assert_eq!(bridge.load_ui_language(), None);

        // 手选落盘 → 新开的 bridge（= 重启）读得回
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(bridge.persist_ui_language(Some("zh_cn")));
        assert_eq!(
            SettingsBridge::with_path(&path)
                .load_ui_language()
                .as_deref(),
            Some("zh_cn")
        );

        // 切回跟随系统 = 删键，且别的键一个不动
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(bridge.persist_ui_language(None));
        assert_eq!(SettingsBridge::with_path(&path).load_ui_language(), None);
        let text = std::fs::read_to_string(&path).expect("读配置");
        assert!(text.contains("theme_mode"), "外观键被删了：{text}");
        assert!(text.contains("custom"), "用户手写的未知键被删了：{text}");

        // 幂等：没有键可删 = 没有落盘
        let mut bridge = SettingsBridge::with_path(&path);
        assert!(!bridge.persist_ui_language(None));

        // 内存态桥不许 panic
        assert_eq!(SettingsBridge::memory_only().load_ui_language(), None);
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }
}
