//! 四个「UI 档位」键的读写（M383 / GAPS P4）：AI 栏折叠、磁贴尺寸、实例页视图、下载分类。
//!
//! ## 为什么单独一卷
//!
//! 三条理由，按分量排：
//!
//! 1. `prefs.rs` 贴着 400 行硬线（搬过来时 399 行，余量 1）。`TileSize` / `InstanceView` 两个枚举
//!    连同它们的 `impl` 本来就是「取值 + 词表」，搬到本文件是**纯搬移**（`pub use` 保路径，
//!    `crate::prefs::TileSize` 一个字没变）。
//! 2. 四个键的取值来源在 `core::setting::keys::ui`，与它们的落盘词表是同一件事的两半
//!    （那边登记 schema，这边翻译值），放一起能一眼对照。
//! 3. 读写走 [`crate::settings_bridge::SettingsBridge`]，而那个类型的**外观**那一套已经占了 `settings_bridge.rs` 的
//!    244 行；把「界面偏好」这一族收在本文件，两族互不挤压。`load_ui_keys` / `persist_ui_keys`
//!    两个入口写在 `settings_bridge.rs` 里（与 `load_nav_collapsed` 并排），本文件只放**值**
//!    与**词表**。
//!
//! ## 语义边界（读者最常问的两件事）
//!
//! - **读**：缺键 / 值被手改成词表外的串 → **出厂值**（老配置不许把界面锁在一个认不出的档位上）。
//! - **写**：走 [`crate::settings_bridge::SettingsBridge::write_with`]（现读 → 改 → 落盘 → 刷 mtime），于是「刚存完 AI
//!   配置就切磁贴尺寸」不会把 `ai.*` 连密钥一起抹掉（M174 两轮复审的教训）。
//!
//! ## 界面接线（M452 起，别再看 M383/M384 的老话）
//!
//! | 键 | 读（启动） | 写（用户改） | 落点 |
//! |---|---|---|---|
//! | `ui.ai.collapsed` | 已接 | 已接 | 读 [`super::super::settings_bridge::SettingsBridge::load_ui_keys`]，写 [`SettingsBridge::persist_ai_collapsed`] |
//! | `ui.download.category` | 已接 | 已接 | 读同上，写 [`SettingsBridge::persist_download_category`] |
//! | `ui.instances.tile_size` | 已接 | 已接（M455） | 读同上，写 [`SettingsBridge::persist_tile_size`]；控件在实例页页头（`app-iced/pages/instances/grid.rs` 的画法 + `pages/instances.rs` 的 chip 组） |
//! | `ui.instances.view` | 已接 | 已接（M455） | 读同上，写 [`SettingsBridge::persist_instance_view`]；同一个控件组 |
//!
//! 「已接」的判据是**两头都在代码里**：`app-iced/src/app/state/construct.rs` 的首启读回
//! （`App::adopt_ui_prefs`）+ 变更点的写盘（`app/update/agent.rs`、`app/update/download.rs`），
//! 不是「bridge 上有这两个函数」。

use serde_json::Value;
use unlucky_core::setting::document::SettingDocument;
use unlucky_core::setting::store::SettingStore;

use crate::dto::VersionCategory;

use super::keys;

/// 磁贴尺寸（Q24-3：小 6 列 / 中 4 列 / 大 3 列）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TileSize {
    /// 小：6 列，只留图标/名字/启动键
    Small,
    /// 中：4 列（D16 默认）
    #[default]
    Medium,
    /// 大：3 列，显示完整徽标与最后游玩时间
    Large,
}

impl TileSize {
    /// 该尺寸对应的网格列数。
    pub fn columns(self) -> usize {
        match self {
            Self::Small => 6,
            Self::Medium => 4,
            Self::Large => 3,
        }
    }

    /// 设置页/实例页分段控件的文案。
    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "小",
            Self::Medium => "中",
            Self::Large => "大",
        }
    }

    /// 落盘取值（`ui.instances.tile_size`）：`small` / `medium` / `large`。
    ///
    /// 词表与 `unlucky_core::setting::keys::ui::TILE_SIZES` 逐值同序，由本文件的往返用例钉住。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// 从落盘取值解析；未知值按 `Medium` 兜底（手改坏配置不许把磁贴锁成认不出的一档）。
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "small" => Self::Small,
            "large" => Self::Large,
            _ => Self::Medium,
        }
    }
}

/// 实例页视图模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InstanceView {
    /// 磁贴网格（默认）
    #[default]
    Tiles,
    /// 紧凑列表
    List,
}

impl InstanceView {
    /// 落盘取值（`ui.instances.view`）：`tiles` / `list`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tiles => "tiles",
            Self::List => "list",
        }
    }

    /// 从落盘取值解析；未知值按 `Tiles` 兜底。
    pub fn from_key_value(value: &str) -> Self {
        match value {
            "list" => Self::List,
            _ => Self::Tiles,
        }
    }
}

/// `VersionCategory` → 落盘取值（`ui.download.category`）。
///
/// `Ancient(AncientKind)` 摊平成四个值，**不做任何折叠**（`ancient_old_beta` 读回还是
/// `Ancient(OldBeta)`）：这样「写什么读什么」是一一对应，将来侧栏拆出四个远古子类时不用迁移。
pub fn category_word(category: VersionCategory) -> &'static str {
    match category {
        VersionCategory::Release => "release",
        VersionCategory::Snapshot => "snapshot",
        VersionCategory::Ancient(kind) => match kind {
            crate::dto::AncientKind::Classic => "ancient_classic",
            crate::dto::AncientKind::OldAlpha => "ancient_old_alpha",
            crate::dto::AncientKind::OldBeta => "ancient_old_beta",
            crate::dto::AncientKind::Unlisted => "ancient_unlisted",
        },
        VersionCategory::Modpack => "modpack",
        VersionCategory::Mod => "mod",
    }
}

/// 落盘取值 → `VersionCategory`；未知值回出厂 `Release`。
pub fn category_from_key_value(value: &str) -> VersionCategory {
    match value {
        "snapshot" => VersionCategory::Snapshot,
        "ancient_classic" => VersionCategory::Ancient(crate::dto::AncientKind::Classic),
        "ancient_old_alpha" => VersionCategory::Ancient(crate::dto::AncientKind::OldAlpha),
        "ancient_old_beta" => VersionCategory::Ancient(crate::dto::AncientKind::OldBeta),
        "ancient_unlisted" => VersionCategory::Ancient(crate::dto::AncientKind::Unlisted),
        "modpack" => VersionCategory::Modpack,
        "mod" => VersionCategory::Mod,
        _ => VersionCategory::Release,
    }
}

/// 四个档位键的**可落盘快照**。
///
/// 为什么不直接用 `UiPrefs`：那个结构体还有 `nav_collapsed` / `theme` / `appearance` 三族字段，
/// 它们的读写在别处（`persist` / `persist_nav_collapsed`）。跟 `AppearanceBase` 同一个取舍——
/// 一个类型只装「一起读、一起写的那些键」，读者才不会以为改 `UiKeys` 会顺手改主题。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiKeys {
    /// 右侧 AI 栏折叠成 icon 轨
    pub ai_collapsed: bool,
    /// 磁贴尺寸
    pub tile_size: TileSize,
    /// 实例页视图
    pub instance_view: InstanceView,
    /// 下载页选中的分类
    pub download_category: VersionCategory,
}

impl Default for UiKeys {
    /// 出厂态：AI 栏展开、中磁贴（D16）、磁贴网格、正式版。
    fn default() -> Self {
        Self {
            ai_collapsed: false,
            tile_size: TileSize::Medium,
            instance_view: InstanceView::Tiles,
            download_category: VersionCategory::Release,
        }
    }
}

impl UiKeys {
    /// 从配置文档读一份快照：缺键 → 出厂值，词表外的值 → 出厂值（不报错、不 panic）。
    #[must_use]
    pub fn from_document(doc: &SettingDocument) -> Self {
        let factory = Self::default();
        let word = |key: &str| doc.effective_str(key);
        Self {
            ai_collapsed: doc.effective_bool(keys::AI_COLLAPSED).unwrap_or(false),
            tile_size: word(keys::TILE_SIZE).map_or(factory.tile_size, TileSize::from_key_value),
            instance_view: word(keys::INSTANCE_VIEW)
                .map_or(factory.instance_view, InstanceView::from_key_value),
            download_category: word(keys::DOWNLOAD_CATEGORY)
                .map_or(factory.download_category, category_from_key_value),
        }
    }

    /// 把四个键写进 store（**只**碰这四键，别的一律不动）。
    ///
    /// # Errors
    ///
    /// 任一个键被 schema 拒绝时返回那个人读原因（此时前面的键已经写进内存文档，但不会落盘：
    /// 调用方拿 `Err` 就不该 `save()`）。走 [`SettingsBridge::persist_ui_keys`] 时这一步由
    /// `write_with` 统一处理（拒绝 ⇒ 整次不落盘）。
    pub fn write_to(&self, store: &mut SettingStore) -> Result<(), String> {
        let pairs: [(&str, Value); 4] = [
            (keys::AI_COLLAPSED, Value::from(self.ai_collapsed)),
            (keys::TILE_SIZE, Value::from(self.tile_size.as_str())),
            (
                keys::INSTANCE_VIEW,
                Value::from(self.instance_view.as_str()),
            ),
            (
                keys::DOWNLOAD_CATEGORY,
                Value::from(category_word(self.download_category)),
            ),
        ];
        for (key, value) in pairs {
            store
                .set(key, value)
                .map_err(|error| format!("写 {key} 被拒：{error}"))?;
        }
        Ok(())
    }
}

impl crate::settings_bridge::SettingsBridge {
    /// 某个 UI 档位键在配置文件里**真的出现过**吗（缺键 ≠ 用了出厂值）。
    ///
    /// 为什么需要这一位：schema 的出厂值与界面首启的出厂态**并不总是同一件事**——
    /// `ui.ai.collapsed` 的 schema 出厂是 `false`（展开），而右栏首启是**折叠**
    /// （`AgentUi::new`：1180 下展开会把实例页挤窄）；`ui.download.category` 的 schema 出厂是
    /// `release`，下载页首启却是「全部」（`Category::All`，它没有可落盘的词）。
    ///
    /// 少了这一位，[`SettingsBridge::load_ui_keys`] 的返回值只能当「生效值」用，
    /// 一盖到界面上就等于把「用户从没选过」改写成「用户选了 schema 出厂值」，首启版面会变。
    #[must_use]
    pub fn ui_key_present(&self, key: &str) -> bool {
        self.store
            .as_ref()
            .is_some_and(|store| store.doc().get(key).is_some())
    }

    /// 只改**一个** UI 档位键（`settings_bridge::write_with` 是唯一写入口：现读 → 改 → 落盘）。
    ///
    /// 为什么不用 [`SettingsBridge::persist_ui_keys`]：那个入口写**四键**，比的也是四键的合并值
    /// ——拿 `UiKeys::default()` 改一格再写回，会把别人写下的 `ui.instances.tile_size` /
    /// `ui.instances.view` 抹回出厂（M174 两轮复审记的「整份文档覆盖」，缩小到四个键）。
    ///
    /// 判「有没有改动」用的是**文件里有没有这个键 + 值等不等**，不是「生效值等不等」：
    /// `ui.ai.collapsed` 的出厂值是 `false`，用户**显式展开**时生效值恰好等于出厂值，
    /// 但盘上还没有这一键——那时必须真的写一条，否则「展开着退出，重启回到折叠」照样发生。
    /// 返回 `true` = 磁盘上真的有新内容。
    fn persist_ui_key(&mut self, key: &str, value: Value) -> bool {
        use crate::settings_bridge::Edit;

        self.write_with(|store| {
            if store.doc().get(key) == Some(&value) {
                return Edit::Unchanged;
            }
            match store.set(key, value) {
                Ok(()) => Edit::Changed,
                Err(_) => Edit::Rejected,
            }
        })
        .is_saved()
    }

    /// 只改 `ui.ai.collapsed` 一档（其余三键一律不碰）。
    pub fn persist_ai_collapsed(&mut self, collapsed: bool) -> bool {
        self.persist_ui_key(keys::AI_COLLAPSED, Value::from(collapsed))
    }

    /// 只改 `ui.download.category` 一档（其余三键同上）。
    ///
    /// 词表是 [`VersionCategory`] 的八个值（`presenter::dto`）；页面上的「全部」没有对应词，
    /// 调用方不该为一个「不是档位的档位」编一个词出来（见 `core::setting::keys::ui` 的 ⚠）。
    pub fn persist_download_category(&mut self, category: VersionCategory) -> bool {
        self.persist_ui_key(
            keys::DOWNLOAD_CATEGORY,
            Value::from(category_word(category)),
        )
    }

    /// 只改 `ui.instances.tile_size` 一档（其余三键一律不碰；M455 的实例页尺寸选择器）。
    ///
    /// 与 [`Self::persist_ai_collapsed`] 走**同一个** [`Self::persist_ui_key`]：
    /// 那个入口才是「现读 → 只改这一键 → 落盘」的唯一实现，这里只负责把枚举翻成词。
    pub fn persist_tile_size(&mut self, size: TileSize) -> bool {
        self.persist_ui_key(keys::TILE_SIZE, Value::from(size.as_str()))
    }

    /// 只改 `ui.instances.view` 一档（其余三键同上；M455 实例页的视图切换）。
    pub fn persist_instance_view(&mut self, view: InstanceView) -> bool {
        self.persist_ui_key(keys::INSTANCE_VIEW, Value::from(view.as_str()))
    }
}

impl super::UiPrefs {
    /// 四个档位键的快照（与 [`super::UiPrefs`] 的同名字段一一对应）。
    ///
    /// 存在的理由只有一条：`UiPrefs` 的四个字段与 [`UiKeys`] 是同一件事的两份表示
    /// （前者给界面读，后者给落盘读），没有这个转换它们迟早分叉。反分叉由 `prefs::tests`
    /// 的 `the_two_representations_of_the_shelf_keys_agree` 钉住。
    #[must_use]
    pub fn ui_keys(&self) -> UiKeys {
        UiKeys {
            ai_collapsed: self.ai_collapsed,
            tile_size: self.tile_size,
            instance_view: self.instance_view,
            download_category: self.download_category,
        }
    }

    /// 用快照盖掉那四个字段（其余字段——导航折叠 / 主题 / 外观——一律不动）。
    pub fn adopt_ui_keys(&mut self, keys: UiKeys) {
        self.ai_collapsed = keys.ai_collapsed;
        self.tile_size = keys.tile_size;
        self.instance_view = keys.instance_view;
        self.download_category = keys.download_category;
    }
}
