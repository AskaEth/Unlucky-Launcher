//! 第二栏（主版本桶）与「一项画成什么样」的判定（从 `rails.rs` 拆出来守 400 行红线）。
//!
//! 两条渲染路径吃同一份数据：[`major_entries`] 产出 `[全部版本, 1.21, 1.20, …]`，
//! 竖栏（[`major_rail`]）与 chips 行（`rails::stacked_chips`）各画一遍。
//! 图标条档（64 宽）里「全部版本」换图标 + tooltip 的取舍见 [`major_shape`]。

use iced::widget::{column, container};
use iced::{Alignment, Element, Length};

use unlucky_design::spacing::Space;

use unlucky_i18n::{Catalog, Key};

use unlucky_design::icons::IconKind;

use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::pages::download::group;
use crate::pages::download::model::Download;
use crate::pages::frame;
use crate::pages::layout::RAIL_ICON_WIDTH;
use crate::pages::Message;
use crate::theme::Ui;

use super::{labeled, rail_title};

/// 第二栏：主版本桶（第一项永远是「全部版本」；没有主版本概念的版本不单独占一项）。
pub(in crate::pages::download) fn major_rail<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    width: f32,
) -> Element<'a, Message> {
    let icon_only = width <= RAIL_ICON_WIDTH;

    let mut body = column![].spacing(ui.space(Space::X1)).width(Length::Fill);
    if !icon_only {
        body = body.push(rail_title(
            ui,
            catalog.text(Key::VersionMajorTitle),
            frame::gap(ui.metrics().nav_item_gap),
        ));
    }

    for entry in major_entries(catalog, model) {
        body = body.push(major_item(ui, entry, icon_only));
    }

    body.into()
}

/// 第二级侧栏的一项（「全部版本」与每个主版本桶共用一套结构，好让**两条渲染路径**
/// ——竖栏与 chips 行 —— 都用同一份数据，测试也只钉这一份）。
///
/// 字段全部**借**模型里的桶标签与目录里的静态串：这两条渲染路径都要把标签塞进 iced 控件树，
/// 借本地的 `String` 会让整棵树的生命周期塌到函数作用域里（E0515）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::pages::download) struct MajorEntry<'a> {
    /// 选中它要发的键（`None` = 「全部版本」）
    pub key: Option<&'a str>,
    /// 画出来的标签
    pub label: &'a str,
    /// 此刻是否选中（**按生效值算**：搜索期间恒为「全部」，见 `group::bucket_selected`）
    pub selected: bool,
}

/// 一项在 iced 里**画成什么样**（纯函数产出，单测直接钉它 —— 两种形状的取舍见 [`major_shape`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::pages::download) enum MajorShape<'a> {
    /// `ListItem` 单行文字（展开档的每一项、图标条档的桶标签）
    Text(&'a str),
    /// 图标 + tooltip 全称（图标条档的「全部版本」，见 [`major_shape`] 的宽度账）
    Icon(&'a str),
}

/// 图标条档画成什么样：**只有「全部版本」这一项不是文字**。
///
/// ## 64 宽的可见文本宽是 **32**，不是 40（M123 取证实测）
///
/// `ListItem` 两侧内边距各 16（`pad_start` / `pad_end`），所以 64 宽的栏里文本格
/// = 64 − 32 = **32**，再长就被 `clip(true)` 切掉。`BodyLarge` 16px 下：
///
/// | 标签 | 墨迹（736×760 实测） | 32 的文本格里放得下？ |
/// |---|---|---|
/// | `1.21` | 26px（x=217..242） | ✓ 余量 **6** |
/// | `1.20` | 28px（x=217..244） | ✓ 余量 **4** |
/// | `24w` | 29px（x=216..244） | ✓ 余量 **3**（最紧的一条） |
/// | `All versions` | 墨迹画到 x=247，裁切边在 248 | ✗ 被切：实际只画出「All v」（缺陷 2 的原图） |
/// | `全部版本` | 4 全角字 × 16 = **64**（精确值，不是估计） | ✗ 只画得出「全部」，且顶到裁切边 |
///
/// 桶标签的余量是 **3~6px**（视字形：`24w` 最紧 3，`1.21` 最松 6）。短标签「全部」= 2 × 16 = **32**，
/// 与文本格**一样宽、零余量**——比桶标签那 3~6px 更薄，裁不裁取决于字体渲染的小数位，
/// 是一个查不出也守不住的边界，所以「换短标签」这条退路不采用。于是图标条档的
/// 「全部版本」**不画文字**：换成图标（字形见 [`major_shape`] 的下文），
/// 全称进 tooltip（宽度不设限，中英文都完整）。
///
/// 桶标签本来就是 3~5 字符的短串，两档都照旧画原文。
///
/// ## 用哪个字形（r1 复审 P3-1）
///
/// 这里用 **`IconKind::Logs`（文档 + 正文行）**，不用第一级「全部类型」那个网格
/// （`category_icon(Category::All)`）：Compact 档两条 64 宽的栏在屏上相邻，若两级都用网格 +
/// 同一个 40×40 `Tonal` 药丸，**第一级「全部」与第二级「全部版本」像素级同形**，
/// 只能靠 hover 出 tooltip 才分得清是哪个轴。文档/清单字形读作「整份版本清单」，
/// 与网格（「全部类型」）分得开。
///
/// 更贴切的「版本 / 历史 / 层叠」字形本仓没有（图标语汇只有 28 个描边字形，登记在
/// `unlucky_design::icons`），新增字形要动 `crates/design`（超出本页范围），此处按既有字形借用。
pub(in crate::pages::download) const fn major_shape(
    entry: MajorEntry<'_>,
    icon_only: bool,
) -> MajorShape<'_> {
    if icon_only && entry.key.is_none() {
        MajorShape::Icon(entry.label)
    } else {
        MajorShape::Text(entry.label)
    }
}

/// 第二级侧栏要画的全部项：`[全部版本, 1.21, 1.20, …]`，顺序 = 桶的版本序。
///
/// 抽成纯函数是为了让「最后一个桶可点」这件事**可断言**（复审 P2-2 的要求）：
/// 竖栏与 chips 行都从这一份数据画，chips 行外面还套了横向滚动，所以「画得出来」= 「够得着」。
/// 返回 `Vec` 而不是迭代器：调用方两处都要 len（chips 的宽度预算）与随机访问。
pub(in crate::pages::download) fn major_entries<'a>(
    catalog: Catalog,
    model: &'a Download,
) -> Vec<MajorEntry<'a>> {
    let mut entries = vec![MajorEntry {
        key: None,
        label: catalog.text(Key::VersionMajorAll),
        selected: group::bucket_selected(model, None),
    }];

    for bucket in &model.buckets {
        // `None` 桶 = 远古版那类没有主版本的版本：第二级只有一个「全部」，不画空桶
        let Some(key) = bucket.key.as_deref() else {
            continue;
        };
        entries.push(MajorEntry {
            key: Some(key),
            label: key,
            selected: group::bucket_selected(model, Some(key)),
        });
    }

    entries
}

/// 主版本的一项：形状由 [`major_shape`] 决定（图标条档的「全部版本」是图标 + tooltip，
/// 其余都是 `ListItem` 单行文字）。
fn major_item<'a>(ui: &'a Ui, entry: MajorEntry<'a>, icon_only: bool) -> Element<'a, Message> {
    let press = Message::SelectMajor(entry.key.map(str::to_string));

    match major_shape(entry, icon_only) {
        MajorShape::Text(label) => {
            let mut item = ListItem::new(label, press);
            if entry.selected {
                item = item.selected();
            }
            list_item(ui, item)
        }
        // 图标条档的「全部版本」：图标（与本页第一级的「全部」同一个网格）+ 全称 tooltip。
        // 高度对齐 `ListItem` 的行高，两档的行节奏才一样。
        MajorShape::Icon(full) => {
            container(labeled(ui, full, IconKind::Logs, entry.selected, press))
                .width(Length::Fill)
                .height(Length::Fixed(ui.metrics().row_height))
                .align_x(Alignment::Center)
                .align_y(Alignment::Center)
                .into()
        }
    }
}
