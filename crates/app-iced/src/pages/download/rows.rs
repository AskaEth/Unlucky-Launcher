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
//! 固定 ≈316px（版本行 40 + **六枚** loader chip 34 + 菜单 + 实例名 56；M222 之后菜单与实例名
//! 都在面板钉死区，见 `options/view.rs::section`）。
//! 同样的 760 高窗口里：三栏档列表还剩 158px（3 整行），chips 档只剩 18px。
//!
//! 修法是 [`TailShare`] 三档：**不带加载器版本列表**的选项区在三栏档照旧钉死（面板整块在屏内），
//! chips 档**且面板真的占高度**
//! （选项区展开 / 安装中）时改成「**面板先保预算、剩下按整行给列表**」
//! （`tail::narrow_rows` + `CHIPS_PANEL_BUDGET = 192`）：700/720×760 下列表 **132px = 3 整行**、
//! 面板拿走其余（208px）。M227 之前这一档是「Fill 与 Fill 五五分」——面板只分到 164px，
//! 钉死区（说明 + 菜单 + 实例名 ≈180px）把它吃穿，滚动区剩 0、head / 六枚 chip 全不在屏。
//! 三栏档 + 带版本列表的选项区走另一支（`tail::whole_rows` + `tail::panel_budget`）：
//! 1180×760 列表 **132px = 3 整行**、面板 330px（滚动视口 98px），那里菜单与说明都钉在面板底部。
//! M238 之前那支用的是**死的** `PANEL_BUDGET = 300`：面板实得 286px，滚动视口只有 54px
//! ⇒ 六枚 chip 被裁成 10px 的上缘（736 与 1180 同病，纵向预算与窗宽无关）。
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
use super::super::Message;
use super::install::{self, panel_state, PanelState};
use super::model::{Download, VersionRow};
use super::states;

mod tail;

pub(crate) use tail::{menu_items, TailShare};

/// 列表列（三栏档与 chips 档共用一份）。
///
/// `share` 决定「列表」与「尾部面板」怎么分这块高度（真实分支见 `tail::TailShare::of`）：
/// `Pinned` = 面板按内容高钉在底部、列表吃剩下的；`Shared` / `Narrow` = 面板先吃预算、
/// 列表按**整行**吃剩下的（`tail::whole_rows` / `tail::narrow_rows`，两者差别在面板内部
/// 钉什么，见 `tail::CHIPS_PANEL_BUDGET`）。哪些组合走哪一支由那一处决定，这里不重复判。
pub(super) fn content<'a>(
    ui: &'a Ui,
    catalog: Catalog,
    model: &'a Download,
    share: TailShare,
    list_height: Option<f32>,
) -> Element<'a, Message> {
    let visible = visible(model);

    let body: Element<'a, Message> = if model.no_match() {
        states::no_match(ui, catalog)
    } else if model.only_elsewhere() {
        states::elsewhere(ui, catalog, model.hits)
    } else if visible.is_empty() {
        states::empty(ui, catalog)
    } else {
        // 只有「面板先保预算」那两支（`Shared` / `Narrow`）才按整行取高：`Pinned` 档（面板矮）
        // 实测已经是整行（`04` 图），取整反而会把它从 6 行缩到 5 行。
        let height = if matches!(share, TailShare::Pinned) {
            None
        } else {
            list_height
        };
        list(ui, catalog, model, &visible, height)
    };

    // 列表与尾部面板之间**总是**有一条分隔线（三态一致）：M123 的取证图里「选好了版本」
    // 那一态原先没有它，版本行与选项区第一行（版本号 + 安装按钮）读起来像同一块内容。
    let tail: Element<'a, Message> = column![
        divider::full(ui),
        install::panel_with(ui, catalog, model, share)
    ]
    .spacing(ui.space(Space::X3))
    .width(Length::Fill)
    .into();

    // chips 档 / 带版本列表的选项区：面板吃满自己那份。
    //
    // `PanelState::Options` 走**面板内部**的滚动（上半块可滚 + 实例名钉页脚，见 `install::panel`），
    // 所以这里不再整块套一层 scrollable——那一层曾让窄窗出现两条并列滚动条，也因为把实例名
    // 卷进滚动区而让它掉到首屏外（D30 r2 S7）。其它面板（安装中/失败）内容短，保留外层滚动。
    let tail: Element<'a, Message> = match (share, panel_state(model)) {
        (TailShare::Pinned, _) | (_, PanelState::Options(_)) => tail,
        (TailShare::Shared | TailShare::Narrow, _) => scrollable(tail)
            .id(iced::widget::Id::new(PANEL_SCROLL_ID))
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

/// 面板那一层 `scrollable` 的 Id（M222 D30 r1 S2 的出图取证用，见 `pages::harness`）。
///
/// 用途只有一个：窄窗（chips 档 / 带版本列表的选项区）里面板自己可滚，出图要证明
/// 「实例名那格滚得到」——harness 拿到这个 Id 就能派一次 `scroll_to` 再拍。
pub(crate) const PANEL_SCROLL_ID: &str = "download-panel";

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
    height: Option<f32>,
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
        // `Some` = 面板先保预算那一档按**整行**取高（三栏档 `tail::whole_rows` /
        // chips 档 `tail::narrow_rows`）；`None` = `Pinned`（面板矮）用 Fill。
        .height(height.map_or(Length::Fill, Length::Fixed))
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
