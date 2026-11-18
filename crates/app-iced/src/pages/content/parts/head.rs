//! 主列列头的**两档画法**与它们的几何（M358）。
//!
//! ## 为什么要两档
//!
//! 常规列头在 `ScrollWhole` 档（矮窗）里把首屏唯一那张结果卡挤出正文：880×560 带提示条时
//! 正文只有 **304**，而「列头 + 一张整卡」= **281 + 80 = 361 > 304** ⇒ 卡只露 23px。
//! 这个不可达在 M357 之前就存在（那时按 380 规划，361 > 380 同样不成立），M357 只是把它
//! 从「4px」退回「23px」。本条真修：**把列头里唯一可以随档位收敛的那一项收下来**。
//!
//! ## 五项逐条量过（2026-11 实测 `after/content-880-dark-zh.png`，261 行墨迹逐行数）
//!
//! | 项 | 常规档 | 收敛档 | 令牌来源 | 能不能收敛 |
//! |---|---|---|---|---|
//! | 搜索条 | 56 | 56 | `LayoutMetrics::search_bar_height` | **不能**：控件骨架（官方 `ContainerHeight`） |
//! | 筛选 | 4 × 32 + 3 × 8 = **152** | 一排横向滚动 = **48** | `ChipGeometry::height` + `Space::X2`；收敛档 = `control_height + X2` | **能**：chip 本身 32 不动，收的是**排数** |
//! | 提示行 | 16 | 16 | `TypeRole::BodySmall` 行高 | 不能：它是内容（CurseForge 为什么是灰的） |
//! | 分隔线 | 1 | 1 | `LayoutMetrics::divider_thickness` | 能省但只 1px，不值得动 |
//! | 计数行 | 16 | 16 | `TypeRole::LabelMedium` 行高 | 不能：内容 |
//! | 五条 `Space::X2` | 40 | 40 | `Space::X2` | 能省但省下来的是节奏，不是版面 |
//! | **合计** | **281** | **177** | | 收敛 104，全部来自「四排 → 一排」 |
//!
//! 实测核对：`after/content-880-dark-zh.png` 正文顶 156、首卡顶 437 ⇒ 列头 281；
//! 其中各段实测 y 分别落在 156（搜索）/ 220（第一排 chip）/ 372（提示行）/ 404（分隔线）/
//! 413（计数行）/ 437（首卡），与本表的加法逐项对上。
//!
//! ## 判据（算式，不是「看起来好一点」）
//!
//! `ScrollWhole` 档要成立的是「正文 − 列头 ≥ 一张整卡」：
//!
//! ```text
//! 无提示条 880×560：380 − 281 = 99 ≥ 80  ✔（常规档就够，与 M357 的图逐像素相同）
//! 带提示条 880×560：304 − 281 = 23 < 80  ✘ ⇒ 换收敛档
//!                   304 − 177 = 127 ≥ 80 ✔
//! ```
//!
//! 门槛 [`content_layout::compact_below`] = `常规列头 + 一张整卡`（两个入参都来自令牌/组件几何），
//! 所以「哪些尺寸下生效」也是算出来的：正文 < 361 才收。这条门槛永远落在 `ScrollWhole` 里
//! （361 < [`content_layout::MIN_BODY_HEIGHT`] 460），`Split` 档与 1180×900 的基线碰不到它。
//!
//! ## 记录更正（r2）：档位挂错了地方
//!
//! 复审 P2-1：**v1 把「列头档位」挂在了数据状态上**（`plan()` 的早退把 `kind` 一起否决回
//! `Full`），于是同一窗口尺寸下列头随数据变高变矮——880×560 带提示条时 `Ready` 是 177、
//! `Loading`/`Empty` 回 257，**每点一次筛选/搜索就跳 80px**（`set_content_query` 在
//! `offset == 0` 时清 `results` ⇒ 必走 `Loading`）；而且那两个态**根本没被修**
//! （列头仍吃 257，`Loading` 面板只剩 304 − 257 = 47）。
//!
//! 根因是把两条**只该约束 `viewport`** 的前提也拿来否决了 `kind`：列头是**画的**
//! （`view.rs` 无条件 `push(head_view)`），不画的只是列表——「列表没画就别收列头」这句只对
//! 取整成立。
//!
//! 现在的分工：
//!
//! | 量 | 由什么决定 | 理由 |
//! |---|---|---|
//! | `kind`（档位） | **尺寸 + 令牌 + `pack_before`** | 换条件时列头不该变高矮；判据读的是列头**最多**能有多高（[`full_head_max`]：提示行与计数行都算上），所以连「CurseForge 有没有 key」「当前是不是 Ready」都不影响它 |
//! | `chrome` / `viewport` | 上面三条 + `status == Ready` | 取整要求列表真的在画（M357 r2 的 P2-1）；列表没画时给 `Fill` |
//!
//! ## 记录更正（r3）：「代价最多早 24px」说小了，最坏 **48px**
//!
//! r2 写的是「收敛最多早 24px（两行 + 两条间距）」——两处都错：**每条 `Space::X2` 是 8 不是 4**，
//! 而且缺一行**不会**只丢它自己的一条间距（间距的条数 = 非零行数，缺一行就少一条）。
//! 逐项算（`full_head_max − 实际列头`，标准令牌）：
//!
//! | 情形 | 缺的行高 | 少的间距 | 合计 |
//! |---|---|---|---|
//! | 两行都在（`Ready` + CurseForge 没 key） | 0 | 0 | **0** |
//! | 缺提示行（CF 有 key，`Ready`） | 16（`BodySmall`） | 8（`Space::X2`） | **24** |
//! | 缺计数行（非 `Ready`，CF 没 key） | 16（`LabelMedium`） | 8 | **24** |
//! | **两行都缺（非 `Ready` 且 CF 有 key）** | 16 + 16 = 32 | 8 + 8 = 16 | **48**（最坏） |
//!
//! 也就是 `281 − 233 = 48`（233 = 搜索 56 + 四排 152 + 分隔线 1 + 3 条 `Space::X2` 24）。
//! 含义：正文高落在「实际列头 + 卡」与「门槛 361」之间的那 **≤ 48px** 里时，这一帧会用收敛档
//! 而其实常规档也装得下——版面因此**最多**比必要早收 48px。这是刻意的取舍：宁可早收，
//! 也不要让用户每换一次条件就看列头蹦一下。这个数由
//! `pages/content/tests/r2.rs::the_tier_threshold_never_over_estimates_by_more_than_two_lanes`
//! 逐项钉住，改 `full_head_max` 或 `lanes()` 就会红。

use iced::widget::column;
use iced::{Element, Length};

use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::Catalog;
use unlucky_presenter::dto::ContentKind;

use crate::app::components::divider;
use crate::app::components::navigation::geometry::ListItemGeometry;
use crate::app::components::selection::geometry::ChipGeometry;
use crate::pages::frame;
use crate::theme::Ui;

use super::super::layout::{self as content_layout, BodyMode};
use super::super::model::{Content, Status};
use super::{count_head, filter_strip, filters, row_pitches, search, source_notice, strip_height};
use crate::pages::Message as PageMessage;

/// 列头占掉的高度（正文顶 → 第一张卡的顶）。
///
/// 五项 + 间距，**每一项都是一个令牌或组件几何**；改搜索条高 / chip 高 / 行高 / 分隔线厚度
/// / 间距，这里跟着动（M357 r3 的口径，本条沿用并把「筛选」这一项换成了两档）。
///
/// 间距为什么是「行数 × `Space::X2`」：列头里相邻两块之间一条，最后一块到第一张卡之间还一条
/// （`main_column` 的外层 `column` 也是 `Space::X2`）。没有提示行时少一行、少一条。
#[must_use]
pub(crate) fn chrome_of(ui: &Ui, model: &Content, kind: Kind) -> f32 {
    lanes(
        ui,
        [
            ui.metrics().search_bar_height,
            filters_lane(ui, kind),
            notice_lane(model),
            ui.metrics().divider_thickness,
            count_lane(model),
        ],
    )
}

/// 常规列头**最多**能有多高（提示行与计数行都算上）。
///
/// 档位判据只读这个，不读 `model`：列头那两行随数据出现/消失（`curseforge` 有 key 就没有
/// 提示行、非 `Ready` 就没有计数行），拿「当前这一帧的实际列头」去判档 =
/// **同一尺寸的档位随数据变**（v1 的 P2-1：`Ready` 177 ↔ `Loading`/`Empty` 257，跳 80px）。
/// 按最多算 ⇒ 档位只是「尺寸 + 令牌」的函数。
///
/// **代价（r3 订正）**：这里比实际列头最多高出 **48**（两行都缺时：
/// `2 × (行高 16 + Space::X2 8)`；只缺一行是 24），所以正文高落在那 48px 窗口里时版面会
/// 比必要早收一步。算式与逐项表在模块文档的「记录更正（r3）」里，`tests/r2.rs` 钉住这个数。
#[must_use]
pub(crate) fn full_head_max(ui: &Ui) -> f32 {
    lanes(
        ui,
        [
            ui.metrics().search_bar_height,
            filters_lane(ui, Kind::Full),
            TypeRole::BodySmall.style().line_height_px,
            ui.metrics().divider_thickness,
            TypeRole::LabelMedium.style().line_height_px,
        ],
    )
}

/// 一张整卡的节距（三行档卡高 + 行距）。
///
/// 与 `parts::row_pitches` **同源**（`ListItemGeometry` + `ROW_GAP`），但**不读 `hits`**：
/// 结果为空时 `row_pitches` 给 0，拿它判档会让档位又回到「随数据变」的老毛病（v1 的 P2-1）。
///
/// `pub(crate)`：M361 的判据（`tests/m361.rs`）要拿同一个数当「至少一行卡」的下限，不许另抄一份。
pub(crate) fn one_card(ui: &Ui) -> f32 {
    ListItemGeometry::of(ui).height(RESULT_CARD_LINES) + ui.space(super::ROW_GAP)
}

/// 结果卡的行数档：`overline` + 标题 + 副标题 = 三行（`ListItem::lines()` 在有 overline 与
/// supporting 时就是这个数）。
///
/// 写成常量不是为了省事：`no_hardcoded_tokens` 的门禁扫 `.height(<数字>)`，而这里的 3 是**行数**
/// 不是像素（`ListItemGeometry::height` 的入参），一个裸字面量会被判成「骨架尺寸没走令牌」。
const RESULT_CARD_LINES: usize = 3;

/// 筛选那一块的高度（两档）。
fn filters_lane(ui: &Ui, kind: Kind) -> f32 {
    match kind {
        Kind::Full => 4.0 * ChipGeometry::of(ui).height + 3.0 * ui.space(Space::X2),
        Kind::Compact => strip_height(ui),
    }
}

/// 行 + 间距：`n` 行之间 `n − 1` 条，最后一行到下一块（第一张卡）之间再一条。
/// 高度为 0 的行（提示行 / 计数行缺失）不算，它那一条间距也跟着不算。
fn lanes(ui: &Ui, rows: [f32; 5]) -> f32 {
    let gap = ui.space(Space::X2);
    let mut total = 0.0;
    let mut count = 0.0;
    for row in rows.into_iter().filter(|row| *row > 0.0) {
        total += row;
        count += 1.0;
    }
    total + count * gap
}

/// 「来源不可用」那一行的高度（没有这一行时是 0，spacing 也跟着少一条）。
fn notice_lane(model: &Content) -> f32 {
    if model.curseforge {
        0.0
    } else {
        TypeRole::BodySmall.style().line_height_px
    }
}

/// 计数行的高度（只在 `Ready` 时有这一行）。
fn count_lane(model: &Content) -> f32 {
    if model.status == Status::Ready {
        TypeRole::LabelMedium.style().line_height_px
    } else {
        0.0
    }
}

/// 导入/导出区是不是排在列头**之前**（决定「列头是不是从正文顶开始的」）。
///
/// 只有整合包有这块区；判据在 [`content_layout::pack_before_results`]。**M374**：那个位置现在只在
/// `ScrollWhole` 档成立（`Split` 档这块区进了列表的滚动区，不再挡在列头之上）。
fn pack_before(model: &Content) -> bool {
    content_layout::pack_before_results(
        model.kind == ContentKind::Modpack,
        model.pack.is_entry_only(),
    )
}

/// 一帧的列头决议（三样一起算，免得调用点各自推一遍、推歪一处就切在卡中间）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Plan {
    /// 用哪一档列头
    pub(crate) kind: Kind,
    /// 列头占掉的高度；`None` = 这一帧**不能**按列头取整（列表没画 / 列头不在正文顶）
    pub(crate) chrome: Option<f32>,
    /// `ScrollWhole` 档的定高视口；`None` = 退回 `Fill`（与修前逐像素一致）
    pub(crate) viewport: Option<f32>,
}

/// 列头的两种画法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// 四排筛选（`Split` 档的常态，M109 起的版面）
    Full,
    /// 一排横向滚动的筛选（矮窗装不下「列头 + 一张卡」时）
    Compact,
}

/// **档位**（用哪一档列头）：只看尺寸与令牌，**不看 `status` / `hits`**。
///
/// ```text
/// kind = Compact  ⟺  pack 不在列头之前  AND  正文 < compact_below(full_head_max, one_card)
/// ```
///
/// * [`full_head_max`] 与 [`one_card`] 都不读 `model`，所以**同一尺寸下档位恒定**：换筛选条件、
///   搜索中、查询无结果，列头不会一会儿 177 一会儿 257（v1 的 P2-1：每次筛选跳 80px）。
/// * `pack_before` 是唯一的例外，而它变的原因是**用户自己动了那块区**（点导入 ⇒ 有进行中面板
///   ⇒ 导入区回到顶部）。那时列头本来就不在正文顶上，收它没有意义。
///   （**M374**：这一条只在 `ScrollWhole` 档还能改到档位——`Split` 档门槛 361 < 460，恒给 `Full`。）
///
/// 门槛仍然只有一个：正文 < 361。它 < [`content_layout::MIN_BODY_HEIGHT`](460) ⇒
/// `Split` 档永远给 `Full`。
#[must_use]
pub(crate) fn kind_of(ui: &Ui, model: &Content, body_height: f32) -> Kind {
    if pack_before(model) {
        return Kind::Full;
    }
    if content_layout::compact_below(full_head_max(ui), one_card(ui)) > body_height {
        Kind::Compact
    } else {
        Kind::Full
    }
}

/// 这一帧怎么画、怎么取整。
///
/// `kind` 由 [`kind_of`] 决定（与数据状态无关）。下面三条前提**只约束 `chrome` / `viewport`**：
///
/// 1. **列表真的在画**（`status == Ready`）。M357 r2 的 P2-1：`body` 在 `Ready` 之外全走
///    `super::status_body`，列表根本没上屏，而 `hits` 可能还留着上一批结果（presenter 在
///    「加载更多」时只置 `pending`）。这时按列头钉高点，加载面板会被钉矮。
///    **这一条不许再去否决 `kind`**：列头是画的（`view.rs` 无条件 `push` 它），不画的只是列表；
///    v1 恰在这里把 `kind` 一起否决了，才有「Loading/Empty 没被修 + 每次筛选跳 80px」。
/// 2. **列头就在正文顶上**（导入/导出区没有排在它前面）。`ScrollWhole` 档里纯入口态已经排到结果
///    之后（M358）、进行中态仍在那块区之后；**M374** 之后 `Split` 档那块区进了列表的滚动区，
///    不再挡在列头之上 ⇒ 那一档列头恒在正文顶，账目对得上（`Some` 里不含任何新常量）。
/// 3. **装得下一整张卡**：`chrome + cards ≤ body_height`。装不下时取整只会把滚动区撑到正文
///    之外（被父容器裁掉），首卡从「露 23px」退化成「4px」（M357 r2 的 P1）。
#[must_use]
pub(crate) fn plan(ui: &Ui, model: &Content, mode: BodyMode, body_height: f32) -> Plan {
    let kind = kind_of(ui, model, body_height);
    // 列头之上还挡着导入区只可能发生在 `ScrollWhole` 档（M374：`Split` 档那块区进滚动区了）
    if model.status != Status::Ready || (!mode.list_scrolls() && pack_before(model)) {
        return Plan {
            kind,
            chrome: None,
            viewport: None,
        };
    }

    let chrome = chrome_of(ui, model, kind);
    // `Split` 档让列表自己滚（`Length::Fill`），这里不给视口——1180×900 的基线就此不变。
    if mode.list_scrolls() {
        return Plan {
            kind,
            chrome: Some(chrome),
            viewport: None,
        };
    }

    let cards = frame::whole_rows_by(body_height - chrome, row_pitches(ui, model));
    Plan {
        kind,
        chrome: Some(chrome),
        viewport: (cards > 0.0 && chrome + cards <= body_height).then_some(chrome + cards),
    }
}

/// 列头的视图（搜索条 → 筛选 → 提示行 → 分隔线 → 计数行）。
///
/// 两档共用同一个骨架，只换中间那一块：常规档的 [`filters`] 自带提示行，收敛档的
/// 筛选条没有（那一排被 chip 占满），所以提示行由这里补在筛选条之下——
/// [`source_notice`] 是唯一来源，两档不会画成两种说法。
pub(crate) fn element<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Content,
    kind: Kind,
) -> Element<'a, PageMessage> {
    let mut head = column![search(ui, catalog, model)]
        .spacing(ui.space(Space::X2))
        .width(Length::Fill);

    // 筛选条**永远在**：它是控制面，不是数据。加载中把筛选条藏起来，用户就没法在慢网络下
    // 换条件或换类型（只能干等），这是「加载态顺手把入口收走」的经典坑。
    head = head.push(match kind {
        Kind::Full => filters(ui, catalog, model),
        Kind::Compact => filter_strip(ui, catalog, model),
    });
    if kind == Kind::Compact {
        if let Some(note) = source_notice(ui, catalog, model) {
            head = head.push(note);
        }
    }
    head = head.push(divider::full(ui));
    if model.status == Status::Ready {
        // 计数头只在有数据时出现：0 条的「0 results」和空态说的是同一件事，画两遍是噪音
        head = head.push(count_head(ui, catalog, model));
    }
    head.into()
}
