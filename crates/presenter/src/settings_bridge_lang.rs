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

use crate::settings_bridge::{Edit, SettingsBridge};

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
    /// 返回是否**真的落了盘**（内存态 / 只读 / 文件被更新版本写过 ⇒ `false`，配置写不进去不该把
    /// UI 卡住）。写法走 [`SettingsBridge::write_with`]（M174 r2 复审 P2-1）：与外观写回同一个
    /// 唯一出口，**入口自己不再碰 `store.save()`**——r1 修了外观那条、这一条漏着，于是「保存 AI
    /// 配置后 1 秒内切界面语言」照样把 `ai.*` 连密钥一起抹掉，而且本函数还返回 `true`。
    ///
    /// 为什么不自己 `store.reload()` 了事：这是**同一类缺陷的第二个入口**（第一个是
    /// [`SettingsBridge::persist`]）。收敛成 `write_with` 之后，下一个写入口在类型上就拿不到
    /// 「不重读的 save」——而不是靠每个人记得。
    pub fn persist_ui_language(&mut self, value: Option<&str>) -> bool {
        let outcome = self.write_with(|store| match value {
            Some(language) => {
                if store
                    .set(UI_LANGUAGE, serde_json::Value::String(language.to_owned()))
                    .is_err()
                {
                    return Edit::Rejected;
                }
                Edit::Changed
            }
            // 「跟随系统」= 删键：`SettingDocument` 没有 remove API，而落盘写的是全量视图，所以
            // 把全量视图里的那个键去掉再 load 回一份文档就等价于删键（用户手写的未知键原样带回）。
            None => {
                if store.doc().get(UI_LANGUAGE).is_none() {
                    return Edit::Unchanged;
                }
                if remove_ui_language(store) {
                    Edit::Changed
                } else {
                    Edit::Unchanged
                }
            }
        });
        outcome.is_saved()
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

    /// **P2-1（r2）的回归**：语言写回不许盖掉别人刚写的键。
    ///
    /// 现场（复审探针 `rev2_ai_keys_survive_a_language_write`）：保存 AI 配置后 1 秒内在同一页把
    /// 界面语言切成 English ⇒ `persist_ui_language` 拿旧快照整份覆盖 ⇒ 文件里只剩 `appearance` +
    /// `ui`，`ai` 整段消失，而本函数**返回 true**。
    ///
    /// 抓得住：`write_with` 少了 `store.reload()`（或某个入口又自己 `store.save()` 绕过它）。
    #[test]
    fn persisting_the_language_keeps_keys_another_writer_just_saved() {
        let path = temp_config("lang-keeps-ai");
        let mut bridge = SettingsBridge::with_path(&path);
        // 先让本桥真的加载过一次文件（长生命 store 的旧快照从这里来）。
        bridge.persist(
            &crate::prefs::AppearancePrefs::factory(),
            crate::prefs::ThemeMode::Light,
        );

        // 另一个写者（M174 的助手设置页）写进三个 AI 键。
        {
            use unlucky_core::setting::keys::launcher_schema;
            use unlucky_core::setting::store::SettingStore;
            let mut store = SettingStore::load(&path, launcher_schema());
            for (key, value) in [
                ("ai.base_url", "https://api.deepseek.com/v1"),
                ("ai.model", "deepseek-chat"),
                ("ai.api_key", "sk-must-survive-lang"),
            ] {
                store.set(key, value.to_string().into()).expect("写 AI 键");
            }
            store.save().expect("落盘");
        }

        // 同一秒内切界面语言（语言入口写回它那份整文档）。
        assert!(
            bridge.persist_ui_language(Some("en_us")),
            "语言这一项该写进去"
        );

        let text = std::fs::read_to_string(&path).expect("读文件");
        assert!(
            text.contains("sk-must-survive-lang"),
            "语言写回把刚保存的密钥盖掉了：{text}"
        );
        assert!(text.contains("deepseek-chat"), "模型名也被盖掉了：{text}");
        assert!(text.contains("\"en_us\""), "语言这一项该写进去：{text}");
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
    }

    /// 「真的落盘了吗」不许撒谎：只读文件（`$schema` 认不出 ⇒ `FileAccess::ReadOnly`）上
    /// `persist_ui_language` 必须是 `false`。
    #[test]
    fn persisting_the_language_reports_false_when_the_file_is_read_only() {
        let path = temp_config("lang-read-only");
        // `Invalid` 兼容档 ⇒ 只读：值照读，但不许被我们改写。
        std::fs::write(
            &path,
            r#"{"$schema":"https://example.invalid/ull/launcher/9.9.9","ui":{"language":"zh_cn"}}"#,
        )
        .expect("写配置");
        let before = std::fs::read_to_string(&path).expect("读配置");

        let mut bridge = SettingsBridge::with_path(&path);
        assert_eq!(
            bridge.load_ui_language().as_deref(),
            Some("zh_cn"),
            "值照读"
        );
        assert!(
            !bridge.persist_ui_language(Some("en_us")),
            "只读文件上写成功是谎话"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("读配置"),
            before,
            "被只读闸门拦下的写不许改动文件"
        );
        std::fs::remove_dir_all(path.parent().unwrap().parent().unwrap()).ok();
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
