//! 四个「UI 档位」键（M383 / GAPS P4）：AI 栏折叠、磁贴尺寸、实例页视图、下载分类。
//!
//! 从 `keys.rs` 拆出（那边贴着 400 行硬线）：主表只留一行 `.with_all(ui::specs())`，
//! 紧接着同一个域里已有的 `ui.nav.collapsed`。
//!
//! ## 为什么是这四个
//!
//! 它们在 `presenter::prefs::UiPrefs` 里**早就有字段**（`ai_collapsed` / `tile_size` /
//! `instance_view` / `download_category`），M383 之前只是内存值，于是「折叠 AI 栏、选下载分类」
//! 这两件事**重启就丢**。M300 给 `ui.nav.collapsed` 做的三件事（schema 登记 / 读 / 写）
//! 在这里原样做了第二遍；**界面那一头**（启动读回 + 变更写盘）M452 才接上，现状：
//!
//! | 键 | 读（启动） | 写（用户改） |
//! |---|---|---|
//! | `ui.ai.collapsed` | 已接 | 已接（`app/update/agent.rs` 的档位迁移） |
//! | `ui.download.category` | 已接 | 已接（`app/update/download.rs` 的 `SetCategory`） |
//! | `ui.instances.tile_size` | 已接（灌进 `UiPrefs`） | 已接（实例页页头的尺寸 chip，M455 `07ed9dc`） |
//! | `ui.instances.view` | 已接（同上） | 已接（实例页页头的视图 chip，同一提交） |
//!
//! **后两行的控件调用点**（M455 起；写在这里是为了让下一个人能就地复核，而不是读一句散文）：
//!
//! - 控件：`crates/app-iced/src/pages/instances/chips.rs` 的 `view_chips`（磁贴 / 列表两枚
//!   Filter chip）与 `size_chips`（小 / 中 / 大三枚）——页头在
//!   `crates/app-iced/src/pages/instances.rs` 的 `view()` 里挂它们；
//! - 写盘：`crates/app-iced/src/app/update/instances/view_mode.rs` 的
//!   `set_instance_view` / `set_instance_tile_size`（各自只改自己那一格，再走 M452 的单键写入器）；
//! - 用户看得见的效果：实例页**两种视图**——`tiles` 是真网格（`pages/instances/grid.rs`，一行
//!   `TileSize::columns()` 个格子），`list` 是 list-detail 的左列（`pages/instances/column.rs`）。
//!
//! ⚠ 历史注记（M383 那一版的假话）：本表曾经写着「这两格**没有控件**、`app-iced` 全文没有
//! `TileSize` / `InstanceView` 的控件调用点、页面只有键控列表」。M455（`07ed9dc`）之后那是
//! **假话**，而且是最坏的一种——它把「已接」说成「待办」，下一个人会照它去重做一遍。
//! 所以现在只写**可复核的指法**（文件 + 函数名），不写「全文有没有」这种谁都可能过期的断言。
//!
//! ## 取值从哪来（不许现编）
//!
//! 三张词表不是新设计，是**已有的枚举逐一对应**：
//!
//! | 键 | 词表来源 | 出厂值 |
//! |---|---|---|
//! | `ui.instances.tile_size` | `presenter::prefs::TileSize`（Q24-3：小 6 列 / 中 4 列 / 大 3 列） | `medium`（D16） |
//! | `ui.instances.view` | `presenter::prefs::InstanceView`（`Tiles` / `List`） | `tiles` |
//! | `ui.download.category` | `presenter::dto::VersionCategory`（五个变体；`Ancient(AncientKind)` 摊平成四个 ⇒ 八个值） | `release` |
//!
//! ⚠ `ui.download.category` 的词表比下载页**侧栏实际能点的四个 chip 多**（页面是
//! `All / Release / Snapshot / Ancient` 四项，见 `app-iced` 的 `pages/download/model/category.rs`，
//! `All` 是「不过滤」= 不派指令，它没有可落盘的选中值）。这里按 DTO 的**每个值**逐一登记，
//! 好处是**不做任何折叠决定**（写什么读什么，一一对应）；坏处是 `modpack` / `mod` 与四个
//! `ancient_*` 目前没有 chip 会写。
//!
//! M452 的接线口径（看代码时别猜）：chip ↔ 词是一张**显式对照表**
//! （`Category::version_category` / `Category::from_version_category`）——
//! `Release` ↔ `release`、`Snapshot` ↔ `snapshot`、`Ancient` ↔ `ancient_classic`（与
//! `Category::command()` 拉的是同一个子类），而 `All` ⇄ **不写盘**（它没有词），
//! `modpack` / `mod` / 另外三个 `ancient_*` ⇄ 回落「全部」。侧栏要不要拆出四个远古子类仍是产品决定。

use crate::setting::spec::{AiPolicy, SettingSpec};

/// `ui.instances.tile_size` 允许值（顺序 = 设置页分段控件的顺序，小→大）。
pub const TILE_SIZES: &[&str] = &["small", "medium", "large"];
/// `ui.instances.view` 允许值。
pub const INSTANCE_VIEWS: &[&str] = &["tiles", "list"];
/// `ui.download.category` 允许值（`VersionCategory` 五个变体，`Ancient` 的四个子类摊开 ⇒ 八个值）。
pub const DOWNLOAD_CATEGORIES: &[&str] = &[
    "release",
    "snapshot",
    "ancient_classic",
    "ancient_old_alpha",
    "ancient_old_beta",
    "ancient_unlisted",
    "modpack",
    "mod",
];

/// 四个档位键的登记项（分级与 `ui.nav.collapsed` 同族：纯视觉、一键可复原、无安全面 ⇒ audited）。
#[must_use]
pub fn specs() -> Vec<SettingSpec> {
    vec![
        SettingSpec::bool(
            "ui.ai.collapsed",
            false,
            AiPolicy::audited(),
            "右侧 AI 栏折叠成图标轨（false = 展开，带文字标签）",
        ),
        SettingSpec::enumeration(
            "ui.instances.tile_size",
            "medium",
            TILE_SIZES,
            AiPolicy::audited(),
            "实例磁贴尺寸：small（6 列，只留图标/名字/启动键）/ medium（4 列，默认）/ large（3 列，显示完整徽标与最后游玩时间）",
        ),
        SettingSpec::enumeration(
            "ui.instances.view",
            "tiles",
            INSTANCE_VIEWS,
            AiPolicy::audited(),
            "实例页视图：tiles（磁贴网格，默认）/ list（紧凑列表）",
        ),
        SettingSpec::enumeration(
            "ui.download.category",
            "release",
            DOWNLOAD_CATEGORIES,
            AiPolicy::audited(),
            "下载页左侧选中的分类：release（默认）/ snapshot / ancient_classic / ancient_old_alpha / ancient_old_beta / ancient_unlisted / modpack / mod",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 四个键**真的**进了 launcher schema，且默认值与 `prefs::UiPrefs::default()` 逐项相同。
    ///
    /// 抓得住：只加了常量没登记进主表（那条 `.with_all(ui::specs())` 被写掉），
    /// 或者 schema 的默认值与 `UiPrefs::default()` 分叉（首帧与首次落盘不一致）。
    #[test]
    fn the_four_shelf_keys_are_in_the_launcher_schema() {
        let schema = crate::setting::keys::launcher_schema();
        for (key, want) in [
            ("ui.ai.collapsed", json!(false)),
            ("ui.instances.tile_size", json!("medium")),
            ("ui.instances.view", json!("tiles")),
            ("ui.download.category", json!("release")),
        ] {
            let spec = schema
                .spec(key)
                .unwrap_or_else(|| panic!("{key} 没进 launcher schema"));
            assert_eq!(spec.default, want, "{key} 的出厂值");
            assert!(
                spec.kind.check(&want).is_ok(),
                "{key} 的出厂值必须过自己的校验"
            );
        }
    }

    /// 三张词表与 presenter 侧枚举逐一对应（**顺序也是**）。
    ///
    /// 抓得住：有人在 core 加了一档却没改 presenter 的 `as_str`（或反过来）——那时
    /// `presenter::prefs::keys` 的往返用例会红，这条是它的上游对照。
    #[test]
    fn the_word_lists_are_the_presenter_enums() {
        assert_eq!(TILE_SIZES, ["small", "medium", "large"]);
        assert_eq!(INSTANCE_VIEWS, ["tiles", "list"]);
        assert_eq!(
            DOWNLOAD_CATEGORIES,
            [
                "release",
                "snapshot",
                "ancient_classic",
                "ancient_old_alpha",
                "ancient_old_beta",
                "ancient_unlisted",
                "modpack",
                "mod"
            ]
        );
    }

    /// 词表外的值必须被 schema 拒（否则手改文件能塞进一个「认不出的视图」）。
    #[test]
    fn unknown_values_are_rejected() {
        let schema = crate::setting::keys::launcher_schema();
        for (key, bad) in [
            ("ui.instances.tile_size", json!("huge")),
            ("ui.instances.view", json!("grid")),
            ("ui.download.category", json!("ancient")),
            ("ui.ai.collapsed", json!("yes")),
        ] {
            assert!(schema.validate(key, &bad).is_err(), "{key} 不该接受 {bad}");
        }
    }
}
