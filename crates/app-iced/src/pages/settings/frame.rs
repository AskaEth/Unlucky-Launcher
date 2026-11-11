//! 设置页骨架：页头、左列分组导航、表单列的滚动区、演示卡片。
//!
//! ## 为什么这里还再导出一批 `pages::frame` 的名字
//!
//! M99 拆目录时立的硬要求是**纯搬移**：调用点一字不改。设置页原来的代码从 `super::frame` 取词
//! （`frame::text_role` / `frame::card` / `frame::ICON_PX` …）；拆成目录后 `frame` 这个名字落到了
//! 本文件上，于是把用到的那几件**再导出**一次，`mod.rs` / `tests.rs` 的调用点就能原样保留。
//! 这不是新 API，只是一条转发（新增控件仍归 [`crate::pages::frame`]）。

pub(super) use super::super::frame::{
    card, divider, glyph_box, left_aligned, page, panes, scroll_style, single_column, text_action,
    text_role, Header, ICON_PX, SCROLLBAR_MARGIN, SCROLLBAR_WIDTH,
};

use iced::widget::{column, container, mouse_area, row};
use iced::{Alignment, Background, Border, Element, Length, Padding, Theme};

use unlucky_design::color::Role;
use unlucky_design::component_map::Component;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;
use unlucky_i18n::{Key, Lang};

use crate::pages::appearance::Change;
use crate::pages::{Action, Message};
use crate::theme::Ui;

use super::Group;

/// 页头：标题 + 副标题 + 两个占位动作。
///
/// M106：三条文案走 catalog（`lang` 由 `settings::view` 传下来）。页标题复用 `NavSettings`——
/// 左列与页头说的是同一件事，不该有第二条译文。
pub(super) fn header<'a>(ui: &'a Ui, lang: Lang) -> Header<'a> {
    let catalog = lang.catalog();
    Header::new(catalog.text(Key::NavSettings))
        .subtitle(catalog.text(Key::SettingsSubtitle))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsResetSection),
            Action::ResetSection,
        ))
        .action(text_action(
            ui,
            catalog.text(Key::SettingsSave),
            Action::SaveSettings,
        ))
}

// --------------------------------------------------------------- 侧列

/// 左列分组导航（supporting-pane 的辅助列，宽 = 令牌 `nav_drawer_width` = 240）。
pub(super) fn group_nav<'a>(ui: &'a Ui, selected: usize, lang: Lang) -> Element<'a, Message> {
    let catalog = lang.catalog();
    let items = column(
        super::GROUPS
            .iter()
            .enumerate()
            .map(|(index, group)| {
                let label = super::GROUP_LABELS
                    .get(index)
                    .map_or(group.name, |key| catalog.text(*key));
                group_item(ui, group, label, index, index == selected)
            })
            .collect::<Vec<_>>(),
    )
    .spacing(ui.space(Space::X1))
    .padding(Padding::from([ui.space(Space::X2), ui.space(Space::X2)]))
    .width(Length::Fill);

    container(items)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(ui.layer(MaterialLayer::Card))),
            border: Border {
                radius: ui.radius_dp(ShapeScale::Large),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn group_item<'a>(
    ui: &'a Ui,
    group: &'a Group,
    label: &'a str,
    index: usize,
    selected: bool,
) -> Element<'a, Message> {
    let spec = Component::NavigationItem.spec();
    let height = ui.metrics().row_height;

    let content_color = if selected {
        ui.color(Role::OnSecondaryContainer)
    } else {
        ui.color(Role::OnSurfaceVariant)
    };

    let inner = row![
        glyph_box(group.icon, content_color, ICON_PX),
        text_role(ui, spec.text, false, label, content_color),
    ]
    .spacing(ui.space(Space::X3))
    .width(Length::Fill)
    .align_y(Alignment::Center);

    let pill = container(inner)
        .width(Length::Fill)
        .height(height)
        .padding(Padding::from([0.0, ui.space(Space::X3)]))
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: selected.then(|| Background::Color(ui.color(Role::SecondaryContainer))),
            border: Border {
                radius: ui.radius(ShapeScale::Full, height),
                ..Border::default()
            },
            ..container::Style::default()
        });

    // 点选切分组：走页面自己的 `Message::Appearance(SelectGroup)`（只改视图状态，不落盘）。
    mouse_area(pill)
        .on_press(Message::Appearance(Change::SelectGroup(index)))
        .into()
}

// --------------------------------------------------------------- 表单列

/// 表单列的滚动区：只有正文滚，页头不动（与实例页列表列同一套口径）。
///
/// 「外观」分组有 7 个控件 + 最多 3 行诊断，1180×760 下会超出可视区，所以这一层是必需的。
///
/// ## M173：滚动条**并排**在内容右侧
///
/// 原来这里是 `scrollable(content)` 裸调 + 轨宽 6 / 内缩 2。iced 的竖直滚动条是浮层：轨道按
/// 视口右边缘定位（`iced_widget-0.14.2/src/scrollable.rs:1970`），而内容拿到的横向约束是**整个
/// 视口宽**（同文件 `:437`，`Scrollbar::spacing` 默认 `None` ⇒ 右内边距 0）。于是滚动条占的那条
/// 10dp 竖带（轨 6 + 两侧内缩 2×2）整条压在表单列最右边——用户看到的就是「滚动条骑在卡片上」。
///
/// 改法在 [`crate::app::components::scroll::vertical`]：内容右侧恒留一条 10dp 的槽，轨道画在槽里。
/// 槽宽来自同一个式子 `轨宽 + 2 × margin`，所以：
///
/// - 内容可用宽度 = 视口宽（424）− 槽宽（10）= **414**；
/// - 长页面（出滑块）与短页面（不出滑块）**内容宽度一致**——槽是内边距，不依赖 iced 的
///   `is_scrollbar_visible`（那正是 `Scrollbar::spacing` 不能用在这里的理由：它只在可见时预留，
///   滑块一出现内容就横跳 10dp）。
///
/// 数值由 `components/scroll.rs` 与 `tests/scroll_fit.rs` 的**布局级**断言钉住（`Renderer = ()`
/// 真跑 iced 的 `layout()`），不是看图。
///
/// 只泛型到 `Renderer` 是为了让上面那条断言能在 `()` 渲染器上跑；产品调用点不写泛型。
pub(super) fn scroll<'a, Renderer>(
    ui: &'a Ui,
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
) -> Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::text::Renderer + 'a,
{
    crate::app::components::scroll::vertical(
        SCROLLBAR_WIDTH,
        SCROLLBAR_MARGIN,
        scroll_style(ui),
        content,
    )
}

/// 演示行构成的一组设置 = 一张卡，行间 1px 分隔线（M3 设置列表的常规做法）。
///
/// 已经接了真控件的分组（`GROUP.body` 是 `Some`）不走这里。
pub(super) fn demo_card<'a>(
    ui: &'a Ui,
    rows: &'a [(&'static str, &'static str)],
) -> Element<'a, Message> {
    let mut stacked = column![].spacing(0.0).width(Length::Fill);

    for (index, (label, value)) in rows.iter().enumerate() {
        if index > 0 {
            stacked = stacked.push(divider(ui));
        }
        stacked = stacked.push(setting_row(ui, label, value));
    }

    card(ui, stacked)
}

fn setting_row<'a>(ui: &'a Ui, label: &'a str, value: &'a str) -> Element<'a, Message> {
    let height = ui.metrics().control_height + Space::X2.dp();

    row![
        container(text_role(
            ui,
            TypeRole::BodyMedium,
            false,
            label,
            ui.color(Role::OnSurface),
        ))
        .width(Length::Fill)
        .align_y(Alignment::Center),
        text_role(
            ui,
            TypeRole::BodySmall,
            false,
            value,
            ui.color(Role::OnSurfaceVariant),
        ),
    ]
    .height(height)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .into()
}

// --------------------------------------------------------------- M196 守门

/// M196：设置页正文**不许**在最后一组之后挂一张空态卡。
///
/// ## 守的是哪一件事
///
/// 用户 2026-11-08 在 M174 的助手页与 M175 的 Java 页都看到同一张不属于本页的空态卡（四格图标 +
/// 「没有逐实例覆盖」+ 说明被窗口下沿截断），两张图 1180×1100、卡片位置完全一致。根因是
/// `settings/mod.rs` 的 `form()` 在分组循环之后**无条件**叠了一件固定 320dp 高的空态位
/// （M84 骨架期的占位，M99 纯搬移时原样留下）——于是任何一组下面都长出同一张卡。
///
/// ## 两条判据怎么合起来咬死它
///
/// 正文由两半拼成：`mod.rs` 的 [`section_columns`](super::section_columns) **生产**「每组一件」的列，
/// `form` 只把它们堆起来再滚出去。于是：
///
/// - [`the_form_stacks_nothing_but_its_sections`] 守**收尾**：正文的唯一来源是 `section_columns`
///   （`push(` 恰好一次、实参必须是循环变量、尾表达式是 `frame::scroll`）—— 多一件就红；
/// - [`the_form_holds_exactly_one_column_per_group`] 守**生产**：真调 `section_columns` 数它交出的
///   列数（窄窗 == `GROUPS.len()`，单选各组 == 1）—— 多塞一件就红，**不管那件叫什么名字**。
///
/// 两半合起来 = 「正文恰好这些列，没有尾巴」。
///
/// ## 为什么住在 `frame.rs` 而不是 `pages/settings/tests/`
///
/// `tests/` 归四条 section 线所有（各改各的），而这里守的是**本文件与 `mod.rs` 之间的分工**：
/// 壳层不提供空态位、分发点不在尾部多堆一件。判据要是哪天挪进 `tests/`，也请原样保留这两条。
///
/// ## 抓得住 / 抓不住（据实写）
///
/// 抓得住（四种变异都实测过）：①把卡折叠回循环
/// （`stack = stack.push(section).push(frame::demo_card(…))`）⇒ 收尾那条数到 2 处 `push(`，红
/// （与 `cargo fmt` 是否拆行无关）；②换成别的来源（`stack = stack.push(frame::text_role(…))`）⇒
/// 实参不是 `section`，红；③塞进列清单（`section_columns` 里多 push 一件）⇒ 列数那条红；
/// ④整段改写成 `extend` / `chain` / 尾表达式再包一层 ⇒ 收尾那条红。
/// 抓不住：**某一组自己的列里**换内容（比如某个分组真的需要自己的空态）。那不是本缺陷——本页允许
/// 组内有空态，前提是它长在该组的列里、用本页语义的文案。`DevDocs/artifacts/m196/` 那组图是**当时**
/// 的证据，不是将来的防线（静态图只能证明「现在没有」）。
#[cfg(test)]
mod no_stray_empty_state {
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::typography::Platform;

    use crate::pages::update::Update;
    use crate::theme::{Families, Ui};

    use super::super::{demo_groups, section_columns, Language, States, GROUPS};

    /// 设置页的分发点（本文件是它的被调用方）。
    const FORM: &str = include_str!("mod.rs");

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    /// `form()` 的函数体：从它的签名取到下一个函数（`section_columns`）的签名之前。
    ///
    /// 只取 `form` 自己：`section_columns` 里面有 `columns.push(`，那是另一条判据管的事。
    fn form_body() -> &'static str {
        let start = FORM
            .find("fn form<'a>(")
            .expect("settings/mod.rs 里找不到 form()：分发点被改名了，本守门要跟着改");
        let rest = &FORM[start..];
        let end = rest
            .find("\nfn section_columns")
            .expect("settings/mod.rs 里找不到 section_columns()：列生产被改名了，本守门要跟着改");
        &rest[..end]
    }

    fn ui() -> Ui {
        Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
    }

    /// `form()` 的函数体，**剥掉每行 `//` 之后的部分**：判据数的是代码，注释里的 `push(` 不算。
    fn form_code() -> String {
        form_body()
            .lines()
            .map(|line| line.split("//").next().unwrap_or_default())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 演示 / harness 路径那份只读状态（与 `pages::page` 同源，不伪造第二份）。
    fn states() -> States<'static> {
        States {
            appearance: None,
            groups: demo_groups(),
            settings: &[],
        }
    }

    /// 判据 ①（**收尾**）：正文的柱子只有一个来源——`section_columns` 交出的那一列。
    ///
    /// M195 r2 复审实测的盲区：原来数的是 `stack = stack.push(` 的出现**次数**，于是
    /// `stack = stack.push(section).push(frame::demo_card(ui, &[]));` 一句折叠里塞第二件时计数仍是 1
    /// （第二个 `.push(` 前面没有 `stack = `）⇒ 判据 ① 与 ② 同时绿。那是在数**写法**。现在按**角色**
    /// 断言：循环头直接迭代 `section_columns`、`push(` 恰好一次且实参是那个循环变量、尾表达式恰好是
    /// `frame::scroll(ui, stack)`、没有 `extend`/`chain`/`once`/`concat`/`append`。`push(` 按**次数**
    /// 数（拆行不影响），注释里的不算（先剥掉每行 `//`）。变异证明见模块文档的「抓得住」。
    #[test]
    fn the_form_stacks_nothing_but_its_sections() {
        let body = form_code();

        assert!(
            body.contains("for section in section_columns("),
            "`form()` 的循环头必须直接迭代 `section_columns(...)`：正文节点的唯一来源就是它"
        );

        let pushes = body.matches("push(").count();
        assert_eq!(
            pushes, 1,
            "`form()` 里出现了 {pushes} 处 `push(`：分组循环里只该有一处。多出来的那一处就是\
             「每组下面多一件」（M196 那张卡）——同一句里折叠成 `.push(a).push(b)` 也算两处"
        );

        let at = body.find("push(").expect("上面刚数过一次") + "push(".len();
        let argument = body[at..].split(')').next().unwrap_or_default().trim();
        assert_eq!(
            argument, "section",
            "被压进正文的必须是 `section_columns` 交出的那一列（循环变量 `section`），实际是 \
             `{argument}`：别的来源（演示卡 / 空态 / 任何控件）都不许进正文"
        );

        for append in ["extend(", "chain(", "once(", "concat(", "append("] {
            assert!(
                !body.contains(append),
                "`form()` 用了 `{append}`：正文只许从 `section_columns` 逐列 push 进来"
            );
        }

        let tail: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        assert_eq!(
            tail.iter().rev().take(2).copied().collect::<Vec<_>>(),
            vec!["}", "frame::scroll(ui, stack)"],
            "`form()` 必须以 `frame::scroll(ui, stack)` 收尾（后面紧跟函数右花括号）：\
             正文到最后一组就结束，别再包一层或往后叠东西"
        );
    }

    /// 判据 ②（**生产**）：真跑 `section_columns`，量它到底交出几列。
    ///
    /// 这是行为断言：调的是产品路径上那个真函数，**不认任何名字**。谁往正文里多塞一件（M196 那张
    /// 「没有逐实例覆盖」的卡当初就是这么塞的），列数就不等于组数。
    ///
    /// 为什么不走 `tests/scroll_fit.rs` 那条「`()` 渲染器真跑布局」的路：`form` / `section_columns`
    /// 交出的是**产品渲染器**的 `Element`，要按那条路量子节点数，得把 `general` / `update` / `ai`
    /// 三条线的 `body` 一并泛型化（它们不在本 mission 的 scope 内）。量列数拿到的是同一个不变量，
    /// 且不需要泛型。
    #[test]
    fn the_form_holds_exactly_one_column_per_group() {
        let ui = ui();
        let update = Update::default();

        let narrow = section_columns(&ui, None, states(), Language::default(), &update);
        assert_eq!(
            narrow.len(),
            GROUPS.len(),
            "窄窗（按组堆叠）的正文应当是「每组一件」：多出来的那一件就是 M196 那张卡的位置"
        );

        for index in 0..GROUPS.len() {
            let single = section_columns(&ui, Some(index), states(), Language::default(), &update);
            assert_eq!(
                single.len(),
                1,
                "supporting-pane 只该画被选中的那一组（下标 {index}），实际拿到 {} 列",
                single.len()
            );
        }
    }
}
