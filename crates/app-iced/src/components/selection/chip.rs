//! Chip（`research/12b` §13）：高 **32** / 圆角 **8** / 水平内边距 **8** / 元素间距 **8** /
//! 图标 **18**（`IconSize::Button` = 官方 `AssistChipTokens.IconSize`）/ 描边 1px `OutlineVariant`。
//!
//! ## 三类（§13 的 Assist / Filter / Input）
//!
//! | 类 | 选中态 | 容器 | 文字 | 前置图标 |
//! |---|---|---|---|---|
//! | Assist | **无**（一次动作，不是开关） | 透明 + 1px 描边 | `OnSurface` | `Primary` |
//! | Filter | 有 | 未选透明+描边；选中 `SecondaryContainer` **无描边** | `OnSurfaceVariant` / `OnSecondaryContainer` | 选中态才给勾 |
//! | Input | 有 | 同 Filter | 同 Filter | 可给 + 后置关闭图标 |
//!
//! ## 为什么 Chip 不用 `selection::shell` 那条透明外壳
//!
//! M3 的 chip **没有圆形状态层**：hover / pressed 就是把状态层叠在**自己的容器**上，
//! 而容器尺寸 = 命中区尺寸。所以 chip 直接走 `button` 的样式闭包（与按钮家族同一条产线），
//! 不需要「48 外壳 + 40 圆」那套。
//!
//! ## 「选中」与「当前项」：两个档位，一套颜色（M206）
//!
//! 这两件事**不是**「选中 / 未选中」，而是「可点的选中」与「不可点的当前项」——它们必须长得
//! **一样**（同一个视觉语汇），差别只在**有没有反馈**：
//!
//! | 档位 | 构造器 | 容器 / 文字 | 对比度下限 | 五态反馈 | 什么时候用 |
//! |---|---|---|---|---|---|
//! | A 可点选中 | [`Chip::filter`] `(label, true, msg)` | `SecondaryContainer` / `OnSecondaryContainer` | **≥ 4.5:1**（WCAG AA 正文；实测 7.21:1 浅 / 7.21:1 深） | 有（hover/pressed 状态层） | 点它**会改变**选中项（下载组的分段单选、预置项开关） |
//! | B 不可点当前项 | [`Chip::current`] | **同 A，一个像素都不差** | **≥ 4.5:1**（同 7.21:1） | **无**（没有可点性就没有反馈） | 「这就是当前生效的那个」，再点一次没有动作（Java 组的「默认」） |
//! | （不是档位）暂不可用 | [`Chip::badge`] / [`.disabled()`](Chip::disabled) | `OnSurface`@12% / @38% | **不适用**：它是禁用语汇（实测 1.84:1 浅 / 2.28:1 深） | — | 真的点不了（下载进行中把「安装」灰掉、LTS/基线这类标签） |
//!
//! **判据只有一句**：这个 chip 点下去**会不会改变**它表示的值？
//! 会 → A；不会（它已经是当前值）→ B；**根本不能点**（忙/不可用）→ 禁用档。
//! 尤其：**不许拿 `badge` 表示「当前默认」**——那是把「不可用」的语汇贴在「答案」上，
//! M204 实测出来就是整页最不可读的一段（1.84:1）。这一条不靠调用方记住，`Chip::current`
//! 与 `paint::current_item` 的文档 + `pages::settings::java::tests::marker` 的三条断言
//! （对比度 / 同族 / 禁用语义）一起把它钉在组件层。

use iced::widget::{button, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow};

use unlucky_design::icons::IconKind;
use unlucky_design::typography::TypeRole;

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{Base, Interaction};
use super::geometry::ChipGeometry;
use super::paint::{self, ChipVariant};

/// 一个 chip（可带前后图标；`on_press` 为 `None` = 禁用）。
#[derive(Debug, Clone)]
pub struct Chip<'a, M> {
    label: &'a str,
    variant: ChipVariant,
    selected: bool,
    leading: Option<IconKind>,
    trailing: Option<IconKind>,
    on_press: Option<M>,
    state: Option<Interaction>,
    /// 「当前项」标记（[`Chip::current`]）：选中配色照画，但**不**降成禁用配色、也不吃五态。
    marker: bool,
    /// 脚下那一层的底（M234，禁用态合成用）。
    base: Base,
}

impl<'a, M> Chip<'a, M> {
    /// Assist chip：辅助动作，**没有**选中态。
    pub fn assist(label: &'a str, on_press: M) -> Self {
        Self::new(label, ChipVariant::Assist, false, on_press)
    }

    /// Filter chip：筛选开关（`selected` 决定容器是否 `SecondaryContainer`）。
    pub fn filter(label: &'a str, selected: bool, on_press: M) -> Self {
        Self::new(label, ChipVariant::Filter, selected, on_press)
    }

    /// Input chip：用户输入的实体（可选中 + 可带关闭图标）。
    pub fn input(label: &'a str, on_press: M) -> Self {
        Self::new(label, ChipVariant::Input, false, on_press)
    }

    /// **纯展示**的 filter chip（选中态、无消息 ⇒ 永远不可交互）。
    ///
    /// 与 `filter(label, true, msg).disabled()` 等价，但**不要求调用方编一条永远收不到的消息**。
    ///
    /// ⚠️ **它不是「当前项」**（M206 纠正）：`badge` 走的是**禁用**配色（`OnSurface`@38% 叠
    /// surface，实测 **1.84:1 浅 / 2.28:1 深**），那是「暂不可用」的语汇。要表达「这行就是当前
    /// 生效的那个」，用 [`Chip::current`]——它和下载组的选中 chip 同一套颜色，7.21:1。
    /// 仍然用 `badge` 的两处都是**真禁用语义**：下载进行中把「安装」灰掉、以及 LTS/基线/推荐这类标签。
    pub fn badge(label: &'a str) -> Self {
        Self {
            label,
            variant: ChipVariant::Filter,
            selected: true,
            leading: None,
            trailing: None,
            on_press: None,
            state: None,
            marker: false,
            base: Base::CANVAS,
        }
    }

    /// **当前项**标记：`SecondaryContainer` + `OnSecondaryContainer`（与可点的选中 chip
    /// **同一套配色**），没有消息 ⇒ 也不吃 hover / pressed / 禁用态。
    ///
    /// 语义：「这一行就是当前生效的那个」。它与 [`Chip::filter`]`(label, true, msg)` 的差别
    /// **只在有没有动作**——点当前值不会改变任何东西，所以给它一个永远不响的消息是撒谎
    /// （`Message::Noop` 已删，见 `Chip::badge` 的旧注）。判据表见本文档开头的「选中与当前项」。
    ///
    /// 为什么不能写成「`filter(.., true, ..)` + `.disabled()`」：`disabled()` 会走禁用配色
    /// （38% 文字、12% 容器），当前项就又变成不可读了——这正是 M206 要修的那件事。
    pub fn current(label: &'a str) -> Self {
        Self {
            label,
            variant: ChipVariant::Filter,
            selected: true,
            leading: None,
            trailing: None,
            on_press: None,
            state: None,
            marker: true,
            base: Base::CANVAS,
        }
    }

    /// 转成禁用：**只清消息**，图标 / 选中态 / 状态层全部保留（复审 P2-1）。
    ///
    /// 官方禁用态是「容器/描边 12%、文字与**图标** 38%」——只降**不删**。早先的
    /// `Chip::disabled(label, variant)` 把 leading / trailing / selected 一起丢掉，实测
    /// assist 78→54、filter·on 74→50（连填充和勾都没了），禁用列里 `filter·on` 与 `filter`
    /// 逐像素同形，看不出选中与否。
    pub fn disabled(mut self) -> Self {
        self.on_press = None;
        self
    }

    fn new(label: &'a str, variant: ChipVariant, selected: bool, on_press: M) -> Self {
        Self {
            label,
            variant,
            selected,
            leading: None,
            trailing: None,
            on_press: Some(on_press),
            state: None,
            marker: false,
            base: Base::CANVAS,
        }
    }

    /// 前置图标（Assist / Filter 用）。
    pub fn leading(mut self, kind: IconKind) -> Self {
        self.leading = Some(kind);
        self
    }

    /// 后置图标（Input chip 的关闭键）。
    pub fn trailing(mut self, kind: IconKind) -> Self {
        self.trailing = Some(kind);
        self
    }

    /// 强制视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }

    /// **告诉 chip 它踩在什么底上**（M234）：禁用态那 12% 容器/描边与 38% 内容叠在这个底上。
    pub fn base(mut self, base: impl Into<Base>) -> Self {
        self.base = base.into();
        self
    }

    /// 静息时的外观——**唯一**把「这颗 chip 属于哪个档位」翻成颜色的地方。
    ///
    /// `chip()` 用它求文字/图标色，测试也用它量对比度：档位的判据（当前项 / 选中 / 禁用）
    /// 只有这一处，所以「实现改回旧配色」这类变异会被
    /// `pages::settings::java::tests::marker` 的对比度断言直接抓住。
    pub fn paint(&self, ui: &Ui) -> paint::ChipPaint {
        let radius = ChipGeometry::of(ui).radius;
        if self.marker {
            return paint::current_item(ui, radius);
        }
        // 没有消息 = 禁用（`Chip::badge` 与 `.disabled()` 都走这里）。
        let state = if self.on_press.is_none() {
            Interaction::Disabled
        } else {
            self.state.unwrap_or(Interaction::Enabled)
        };
        paint::chip(ui, self.base, self.variant, self.selected, state, radius)
    }
}

/// 画一个 chip。
pub fn chip<'a, M: Clone + 'a>(ui: &'a Ui, spec: Chip<'a, M>) -> Element<'a, M> {
    let geometry = ChipGeometry::of(ui);
    let variant = spec.variant;
    let selected = spec.selected;
    let marker = spec.marker;
    let forced = spec.state;
    let base = spec.base;

    // 文字/图标色只看「选中 + 禁用」（A1：Filter/Input 的 label 色在 hover/focus/pressed 不变），
    // 所以这里静态求值；容器（含状态层）在样式闭包里跟态走。档位的判据在 `Chip::paint` 一处
    // （M206：当前项标记走 `paint::current_item`——**不是**禁用态。它的不可点性与「忙/不可用」
    // 是两件事：前者是「已经选中的那个」（选中配色），后者是「暂不可用」（38% 文字））。
    let at_rest = spec.paint(ui);

    let mut content = row![].align_y(Alignment::Center).spacing(geometry.gap);
    if let Some(kind) = spec.leading {
        // Q9：图标一律经固定尺寸容器；盒 18 ≠ RENDER_PX(24)，所以必须走 `view_at` 按 18 烘焙，
        // 否则线宽会被布局缩水（复审 r1 P2-2）。
        content = content.push(super::super::layout::centered_fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, at_rest.icon, geometry.icon),
        ));
    }

    let style = ui.style(TypeRole::LabelLarge);
    content = content.push(
        text(spec.label)
            .size(style.size_px)
            .font(ui.font_of(ui.emphasized(TypeRole::LabelLarge)))
            .color(at_rest.label),
    );

    if let Some(kind) = spec.trailing {
        content = content.push(super::super::layout::centered_fixed(
            geometry.icon,
            geometry.icon,
            icons::view_at::<M>(kind, at_rest.icon, geometry.icon),
        ));
    }

    // 垂直居中必须在内容侧做（`button` 把内容放左上角）
    let body = container(content)
        .height(Length::Fill)
        .align_y(Alignment::Center);

    let element = button(body)
        .height(Length::Fixed(geometry.height))
        .padding(Padding {
            top: 0.0,
            right: geometry.pad,
            bottom: 0.0,
            left: geometry.pad,
        })
        .style(move |_theme, status| {
            // 标记不吃五态：没有可点性就没有反馈（`--force-state` 也不该把它画成 hover）。
            let paint = if marker {
                paint::current_item(ui, geometry.radius)
            } else {
                let state = forced.unwrap_or_else(|| Interaction::from_status(status));
                paint::chip(ui, base, variant, selected, state, geometry.radius)
            };
            button::Style {
                background: paint.frame.fill.map(Background::Color),
                text_color: paint.label,
                border: Border {
                    color: paint.frame.outline.unwrap_or(iced::Color::TRANSPARENT),
                    width: if paint.frame.outline.is_some() {
                        paint.frame.stroke
                    } else {
                        0.0
                    },
                    radius: iced::border::Radius::new(paint.frame.radius),
                },
                shadow: Shadow::default(),
                snap: true,
            }
        });

    match spec.on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}
