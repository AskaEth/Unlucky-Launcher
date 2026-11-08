//! 列表列：搜索条 + 计数头 + **单行**版本行 + 底部安装面板。
//!
//! ## 这一列的职责边界
//!
//! | 位置 | 画什么 | 消息 |
//! |---|---|---|
//! | 搜索条 | 真实控件的 `search_bar`（56 高、药丸）+ 清空按钮 + 命中数 | [`Message::SetVersionQuery`] / [`Message::ClearVersionQuery`] |
//! | 计数头 | `{count} versions`（`LabelMedium` + `OnSurfaceVariant`，与实例页列表头同一个语言） | 无 |
//! | 列表 | `ListItem` + 三列短文本 + 尾随图标按钮 | [`Message::SelectVersion`] / [`Message::InstallVersion`] |
//! | 面板 | 见 [`super::install`]（M105 的安装选项区**不在这里**） | — |
//!
//! ## 版本行为什么必须是**单行**
//!
//! 任务硬约束：列表列最窄 424（行宽下限），此时「版本号 + 类型 + 日期 + 安装按钮」四样仍要在
//! 一行里。所以：版本号那一格是 `ListItem`（它自己的 `Wrapping::None` 保证不折行，超长由外层
//! clip 切掉），类型 / 日期 / 安装按钮三格是**定宽**（[`super::super::layout::rails`] 的音阶）。
//! 折行的唯一可能来源是定宽格子挤爆容器——那条由列表列下限 424 挡住（`layout/rails/tests.rs`
//! 的扫描断言）。
//!
//! ## 尾巴不能把列表压成缝（M125 缺陷 1）
//!
//! 这一列是 `column![搜索, 分隔线, 计数头, 列表(Fill), 尾巴]`：`body` 吃剩下的高度、`tail`
//! 按内容高钉死。iced 的 `Column` 不会把溢出交给滚动，所以**尾巴一高，列表就被压成一条缝**——
//! M123 在 700×760 拍的 `02-options-fabric-en-light-chips700.png` 里，8 行版本只剩 **18px**
//! 可视高（行高 44），版本号一个字都读不出来，用户想换版本点不到。
//!
//! 病根是 chips 档比三栏档多花两行 chip（tabs 48 + chips 48 + 两处间距），而选项区那一块
//! 固定 ≈316px（版本行 40 + 标题 16 + 五个 loader chip 34 + 标题 16 + 菜单 112 + 实例名 56）。
//! 同样的 760 高窗口里：三栏档列表还剩 158px（3 整行），chips 档只剩 18px。
//!
//! 修法是 [`TailShare`] 两档：三栏档照旧钉死（面板整块在屏内），chips 档**且面板真的占高度**
//! （选项区展开 / 安装中）时改成两半各 `Fill`——列表拿到 ≈182px（**4 整行**），面板拿到同样一半、
//! **自己可滚**（顶端那个实心「安装」按钮与五个 loader chip 仍在屏内，实例名那一格滚一下就到）。
//! 「刚进页面还没选版本」（`Hint`，面板只有一行提示）**不共享**：那一态 main 本来就有
//! **7 整行**在屏（不是 8：第 8 条墨迹其实是 `Pick a version, then install` 那句 hint，
//! 横向范围对不上版本号；700×760 上放 8 行要 352px，实测只剩 309~322px），五五分会砍成
//! **4 整行**、下面留 ≈160px 空白（r1 复审 P2-1 的回归，r2 修回 7 整行）。
//!
//! 这条判据按**档位**而不是按高度：页面的模型里只有 `window_width`，窗口高度没接进页面，
//! 所以「760 高到底够不够」在运行时是未知量（边界写在 [`TailShare`] 的文档里）。
//!
//! ## 搜索的三条口径
//!
//! | 情形 | 画什么 | 为什么 |
//! |---|---|---|
//! | 有命中 | 列表照常，侧栏**位置不动**，搜索条右侧显示「N 个匹配」 | 命中数是「我搜的词有货」的读数，不是分页读数 |
//! | 一个都不命中（跨类型也没有） | [`super::states::no_match`]：一句「没有匹配」+ 一句「换短一点的片段」 | 白屏会让人以为列表坏了 |
//! | 有命中但都不在当前类型 | [`super::states::elsewhere`]：说清「有 N 个在别的类型里」+ 一个「看全部类型」按钮 | 否则用户会以为自己搜错了 |
//!
//! ## 行距归零（M102，与本页的单行版本行同时成立）
//!
//! M102 把列表行距归零：原先页面在 `ListItem` 的可视高度之外又加了 `Space::X1`（4），行节奏
//! 与可视高度除不尽，实例页复现成「末行被卡片底边切半行」（视觉复审 ②）。规则：行距只由
//! 组件自己的几何决定，`Space` 里也没有 0 这一档 ⇒ `list()` 里**不写** `.spacing(…)`。
//! 本页的行是 44 的单行档，同样不加行距；[`ROW_GAP`] 只用在**行内**那三个格子的间距上。

use iced::widget::{column, container, row, scrollable};
use iced::{Alignment, Element, Length, Padding};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use unlucky_i18n::{Catalog, Key};

use crate::app::components::buttons::{self, IconVariant};
use crate::app::components::divider;
use crate::app::components::inputs::{search_bar, SearchBar};
use crate::app::components::navigation::list_item::{list_item, ListItem};
use crate::theme::Ui;

use super::super::frame;
use super::super::layout::rails::{LIST_DATE_WIDTH, LIST_KIND_WIDTH, ROW_GAP, ROW_ICON_BUTTON};
use super::super::layout::VersionRailTier;
use super::super::Message;
use super::install::{self, PanelState};
use super::model::{Download, VersionRow};
use super::states;

/// 列表列（三栏档与 chips 档共用一份）。
///
/// `share` 决定「列表」与「尾部面板」怎么分这块高度：三栏档钉死面板、chips 档各吃一半
/// （见 [`TailShare`] 与模块文档「尾巴不能把列表压成缝」）。
pub(super) fn content<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    share: TailShare,
) -> Element<'a, Message> {
    let visible = visible(model);

    let body: Element<'a, Message> = if model.no_match() {
        states::no_match(ui, catalog)
    } else if model.only_elsewhere() {
        states::elsewhere(ui, catalog, model.hits)
    } else if visible.is_empty() {
        states::empty(ui, catalog)
    } else {
        list(ui, catalog, model, &visible)
    };

    // 列表与尾部面板之间**总是**有一条分隔线（三态一致）：M123 的取证图里「选好了版本」
    // 那一态原先没有它，版本行与选项区第一行（版本号 + 安装按钮）读起来像同一块内容。
    let tail: Element<'a, Message> = column![divider::full(ui), install::panel(ui, catalog, model)]
        .spacing(ui.space(Space::X3))
        .width(Length::Fill)
        .into();

    // chips 档：面板收回自己的一半、内部可滚（它原先按内容高钉死，把列表压成 18px 的缝）。
    let tail: Element<'a, Message> = match share {
        TailShare::Pinned => tail,
        TailShare::Shared => scrollable(tail)
            .height(Length::Fill)
            .style(frame::scroll_style(ui))
            .into(),
    };

    column![
        search_row(ui, catalog, model),
        divider::full(ui),
        head(ui, catalog, visible.len()),
        body,
        tail,
    ]
    .spacing(ui.space(Space::X2))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// 列表与尾部面板怎么分这块高度（M125 缺陷 1 的修法）。
///
/// | 档 | 分法 | 为什么 |
/// |---|---|---|
/// | `Wide` / `Narrow` / `Compact` | [`TailShare::Pinned`]：面板按内容高钉在底部，列表吃剩下的 | 竖栏档在 760 高的窗口里列表还剩 158px（3 整行），面板整块在屏内——两样都对 |
/// | `Chips` + 面板**占高度**（选项区展开 / 安装中） | [`TailShare::Shared`]：两半各 `Fill`，面板自己可滚 | chips 档比三栏档多花两行 chip（tabs 48 + chips 48 + 间距），列表只剩 **18px**（M123 实测 0.4 行），面板 316px 全在屏内也没用——版本号读不出来就换不了版本 |
/// | 任何档 + 只画 `Hint`（没选版本，面板只有一行） | [`TailShare::Pinned`] | 一行提示不值得对半分：700×760 下 main 是 **7 整行**在屏（不是 8——第 8 条墨迹其实是 `Pick a version, then install` 那句 hint，横向范围对不上版本号；要放 8 行得 352px，实测只剩 309~322px），对半分只剩 **4 整行**、下面留 160px 空白（r1 复审 P2-1，r2 修回 7） |
///
/// 判据是**档位**不是高度：页面的模型里只有 `window_width`（`pages/model.rs`），窗口高度
/// 没有接进页面（要接得改 `pages/mod.rs` 的入口签名，超出本页的边界）。所以这里按「哪一档
/// 的竖排 chrome 会把列表压没」来分，并在报告里登记这条判据的边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TailShare {
    /// 面板钉在底部（三栏档）。
    Pinned,
    /// 面板与列表各吃一半（chips 档）。
    Shared,
}

impl TailShare {
    /// 档位 + **面板形态** → 分高方式（纯函数，单测直接钉它）。
    ///
    /// 两个条件必须同时成立才共享：
    /// 1. 档位是 `Chips`（多花两行 chip，竖排预算最紧的那一档）；
    /// 2. 面板真的**占高度**——`Options`（选中了版本、选项区展开）或 `Install`（进度 / 失败）。
    ///
    /// `Hint`（刚进页面还没选版本）只是**一行**提示（≈30px），对半分会让列表从 **7 整行**
    /// 掉到 **4 整行**、下半块留 ≈160px 空画布（r1 复审 P2-1 实测：main 在 700×760 的
    /// `Hint` 态是 7 整行——不是 8，第 8 条墨迹其实是 `Pick a version, then install` 那句
    /// hint，横向范围对不上版本号；r2 修回 7 整行）。所以 `Hint` **在任何档位都钉死**。
    pub(super) const fn of(tier: VersionRailTier, state: &PanelState<'_>) -> Self {
        match (tier, state) {
            (VersionRailTier::Chips, PanelState::Options(_) | PanelState::Install(_)) => {
                Self::Shared
            }
            _ => Self::Pinned,
        }
    }
}

/// 搜索条 + 清空按钮 + 命中数（三者一行；命中数只在有查询词时出现）。
pub(super) fn search_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
) -> Element<'a, Message> {
    let bar = search_bar(
        ui,
        SearchBar::new(model.query.as_str())
            .placeholder(catalog.text(Key::VersionSearchPlaceholder)),
        Message::SetVersionQuery,
    );

    let mut line = row![container(bar).width(Length::Fill)]
        .spacing(ui.space(Space::X2))
        .align_y(Alignment::Center)
        .width(Length::Fill);

    if model.has_query() {
        // 命中数是**跨类型**的读数（`Download::hits`），所以它在「当前分类没命中」时也照画
        line = line.push(frame::text_role_owned(
            ui,
            TypeRole::LabelMedium,
            false,
            hits_text(catalog, model.hits),
            ui.color(Role::OnSurfaceVariant),
        ));
        line = line.push(buttons::icon(
            ui,
            IconVariant::Standard,
            IconKind::Close,
            Some(Message::ClearVersionQuery),
            None,
        ));
    }

    line.into()
}

/// 命中数的文案：`1` 与 `>1` 是两条 key（英文的 `1 matches` 是错的，复审 P2-4）。
///
/// 中文两侧都是「个匹配」，但**不能**因此只留一条 key：英文的复数规则要在目录里可读，
/// 而且将来加第三种语言时这条分支已经在那儿了。
pub(super) fn hits_text(catalog: Catalog, count: usize) -> String {
    let key = if count == 1 {
        Key::VersionSearchHitsOne
    } else {
        Key::VersionSearchHits
    };
    catalog.render(key, &[("count", &count.to_string())])
}

/// 「命中在别的类型里」的正文（同样单复数两条 key）。
pub(super) fn elsewhere_text(catalog: Catalog, count: usize) -> String {
    let key = if count == 1 {
        Key::VersionSearchElsewhereOne
    } else {
        Key::VersionSearchElsewhere
    };
    catalog.render(key, &[("count", &count.to_string())])
}

/// 计数头（`LabelMedium`）：一行 = 一个 key，数字走占位符，不拼片段。
pub(super) fn head<'a>(ui: &'a Ui, catalog: Catalog, count: usize) -> Element<'a, Message> {
    frame::text_role_owned(
        ui,
        TypeRole::LabelMedium,
        false,
        catalog.render(Key::DownloadCount, &[("count", &count.to_string())]),
        ui.color(Role::OnSurfaceVariant),
    )
}

/// 当前可见的行：先按分类（模型已过滤），再按第二级侧栏**此刻生效**的主版本桶。
///
/// 桶的生效值来自 [`super::group::active_bucket`]：搜索期间它恒为「全部」，于是
/// 「命中数说 N 个、列表却 0 行」这种自相矛盾在结构上不可能出现（复审 P2-1）。
pub(super) fn visible(model: &Download) -> Vec<&VersionRow> {
    match super::group::active_bucket(model) {
        None => model.rows.iter().collect(),
        Some(key) => model
            .rows
            .iter()
            .filter(|entry| entry.major().as_deref() == Some(key))
            .collect(),
    }
}

/// 版本行列表（**独立滚动**：只有列表滚，安装面板钉在底部不跟着滚）。
fn list<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    visible: &[&'a VersionRow],
) -> Element<'a, Message> {
    let entries = column(
        visible
            .iter()
            .map(|entry| version_row(ui, catalog, model, entry))
            .collect::<Vec<_>>(),
    )
    // M102：行距归零（见模块文档），只留横向留白
    .padding(Padding::from([0.0, ui.space(Space::X2)]))
    .width(Length::Fill);

    scrollable(entries)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(frame::SCROLLBAR_WIDTH)
                .scroller_width(frame::SCROLLBAR_WIDTH)
                .margin(frame::SCROLLBAR_MARGIN),
        ))
        .height(Length::Fill)
        .style(frame::scroll_style(ui))
        .into()
}

/// 一行：`ListItem`（版本号，可点选）+ 类型 + 日期 + 安装图标按钮（**四样同一行**）。
///
/// 点版本号 = [`Message::SelectVersion`]（页面内选中，不 dispatch）；点尾随按钮 =
/// [`Message::InstallVersion`]。已装的版本给勾 + 禁用按钮（不重复安装）。
fn version_row<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &Download,
    entry: &'a VersionRow,
) -> Element<'a, Message> {
    let mut item = ListItem::new(entry.id.as_str(), Message::SelectVersion(entry.id.clone()))
        .leading(IconKind::Download);

    if entry.installed {
        item = item.trailing(IconKind::Check);
    }
    if model.selected.as_deref() == Some(entry.id.as_str()) {
        item = item.selected();
    }

    // 版本号那一格吃掉剩下的宽（`ListItem` 自己是 `Wrapping::None`：再长也**不折行**，
    // 由外层 clip 切掉）。定宽的三格因此永远落在右边，行宽下限 424 保证它们不会被挤出容器。
    row![
        container(list_item(ui, item)).width(Length::Fill),
        cell(
            ui,
            catalog.text(super::model::kind_label_key(entry.kind)),
            LIST_KIND_WIDTH
        ),
        cell(ui, entry.released.as_str(), LIST_DATE_WIDTH),
        install_action(ui, entry),
    ]
    .spacing(ui.space(ROW_GAP))
    .align_y(Alignment::Center)
    .width(Length::Fill)
    .into()
}

/// 一格定宽文本（`LabelMedium` + `OnSurfaceVariant`；单行，超长由外层 clip）。
fn cell<'a>(ui: &'a Ui, text: &'a str, width: f32) -> Element<'a, Message> {
    container(frame::text_role(
        ui,
        TypeRole::LabelMedium,
        false,
        text,
        ui.color(Role::OnSurfaceVariant),
    ))
    .width(Length::Fixed(width))
    .into()
}

/// 行尾的安装按钮：已装 → `Check`（`Primary`）；未装 → `Download` 图标按钮（命中区 40/48）。
///
/// 「本页唯一实心 accent」留在底部面板那个带文字的「安装」按钮上（rubric K2），行内用标准图标按钮：
/// 一行一个实心药丸会让列表变成一排强调色。
fn install_action<'a>(ui: &'a Ui, entry: &'a VersionRow) -> Element<'a, Message> {
    if entry.installed {
        return container(frame::glyph(
            IconKind::Check,
            ui.color(Role::Primary),
            frame::ICON_PX,
        ))
        .width(Length::Fixed(ROW_ICON_BUTTON))
        .align_x(Alignment::Center)
        .into();
    }

    container(buttons::icon(
        ui,
        IconVariant::Standard,
        IconKind::Download,
        Some(Message::InstallVersion(entry.id.clone())),
        None,
    ))
    .width(Length::Fixed(ROW_ICON_BUTTON))
    .align_x(Alignment::Center)
    .into()
}
