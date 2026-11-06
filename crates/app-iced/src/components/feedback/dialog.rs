//! Dialog（`research/12b` §7）：scrim + 容器 + 标题/正文/动作三区。
//!
//! ## 官方值（`[源]` = A1，`DialogTokens` + `AlertDialogKt`，`javap` 核对）
//!
//! | # | 项 | 值 | 落点 |
//! |---|---|---|---|
//! | 7.1 | 圆角 | **28**（`CornerExtraLarge`） | `Component::Dialog.spec().shape` |
//! | 7.2 | 容器 | **`SurfaceContainerHigh`**（`DialogTokens.ContainerColor`） | `Ui::layer(MaterialLayer::Overlay)` |
//! | 7.3 | 内距 | 四边 **24**；标题下 **16**；正文下 **24** | [`DialogGeometry`]（`Space` 档位） |
//! | 7.4 | 宽 | 官方 **280..560**；本项目 **480** | [`DialogGeometry::width`] |
//! | 7.5 | 字级 | 标题 `HeadlineSmall 24/32`、正文 `BodyMedium 14/20`、动作 `LabelLarge 14/20 w500` | 组件层取角色 |
//! | 7.6 | 动作区 | 文本 **`Primary`**；主轴 **8** / 交叉 **12**；右对齐 | [`DialogGeometry`] |
//! | 7.7 | 遮罩 | `scrim` = 纯黑 @ **0.32**（`SCRIM_ALPHA`） | [`scrim`] |
//! | 7.8 | 数量 | 一屏 **1 个** modal | 页面责任（组件不计数） |
//!
//! ## 三种动作形态
//!
//! [`DialogActions::None`]（纯告知）/ [`DialogActions::Single`]（「知道了」）/
//! [`DialogActions::Double`]（「取消」+「删除」，右边是主操作）。双动作的 `dismiss` 在左、
//! `confirm` 在右（M3 的阅读序；本项目**不**做「危险操作反色」——官方 Dialog 动作只有 `Primary`）。
//!
//! ## scrim 为什么是**不透明预合成**
//!
//! 官方是「纯黑 @0.32 盖在内容上」。D27 要求 CPU 档零半透明（否则 CPU/GPU 两图不一致），
//! 所以 [`scrim`] 返回的是 `黑@0.32` **叠在 `MaterialLayer::Canvas` 上**之后的实色：
//! 逐像素与「真半透明铺在 surface 上」等价（遮罩下通常就是底色），且两档渲染一致。
//! 代价：遮罩下若是图片/卡片，合成目标不是 surface —— 这条登记为偏离 D-2。
//!
//! **深色档的观感（观察项 D-5）**：画布本身很暗，0.32 叠上去只从 (9,21,28) 压到 (6,14,19)
//! （≈4%），几乎看不出「压暗」。数值是官方 token 推出来的，**不调**：要更强的层级感应该动
//! Dialog 的容器档或阴影，而不是把 scrim 抬到 0.4（那就偏离 token 了）。
//!
//! ## 不画阴影（偏离 D-1）
//!
//! 官方 L3 有阴影，但 D27（CPU 禁模糊）+ 令牌层 `ShadowSpec` 仍是 M63 占位值 ⇒ 先不画，
//! 层级由 `SurfaceContainerHigh` + scrim 表达。

use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Background, Border, Element, Length, Padding};

use unlucky_design::color::{Role, Scheme};
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::typography::TypeRole;

use crate::theme::{to_iced, Ui};

use super::super::interaction::Interaction;
use super::action;
use super::panel::DialogGeometry;

/// 动作区的三种形态（「无动作」「单动作」「双动作」）。
#[derive(Debug, Clone)]
pub enum DialogActions<'a, M> {
    /// 无动作：只有标题与正文（纯告知型，例如「已是最新版本」）。
    None,
    /// 单动作：确认/知道了。
    Single {
        /// 动作文案。
        label: &'a str,
        /// 按下后的消息。
        on_press: M,
    },
    /// 双动作：左侧 dismiss、右侧 confirm。
    Double {
        /// 左（次要）动作。
        dismiss: (&'a str, M),
        /// 右（主）动作。
        confirm: (&'a str, M),
    },
}

impl<'a, M> DialogActions<'a, M> {
    /// 是否为空（图例/测试用）。
    pub fn is_empty(&self) -> bool {
        matches!(self, DialogActions::None)
    }
}

/// 一个对话框的装配描述（**不持有任何状态所有权**）。
#[derive(Debug, Clone)]
pub struct Dialog<'a, M> {
    title: &'a str,
    body: &'a str,
    actions: DialogActions<'a, M>,
    max_width: f32,
    state: Option<Interaction>,
}

impl<'a, M> Dialog<'a, M> {
    /// 标题 + 正文，默认无动作。
    pub fn new(title: &'a str, body: &'a str) -> Self {
        Self {
            title,
            body,
            actions: DialogActions::None,
            // 项目取值 480（官方区间 280..560 内）；窄宿主由页面传更小的可用宽。
            max_width: 480.0,
            state: None,
        }
    }

    /// 可用宽（默认 480）。实际宽 = `min(available, 480)`，且**不低于官方下限 280**
    /// （`DialogMinWidth`；可用宽传 200 时仍画 280 并溢出宿主 —— iced 无 min-width，见 G17-d）。
    ///
    /// 为什么不让组件自己从宿主推：iced 的 `Length` 没有 min-width，而 `Fill + max_width`
    /// 会被父级的精确分配顶掉（实测渲染成 548 而不是 480，见 `dialog()` 里的说明）。
    pub fn width(mut self, available: f32) -> Self {
        self.max_width = available;
        self
    }

    /// 单动作。
    pub fn confirm(mut self, label: &'a str, on_press: M) -> Self {
        self.actions = DialogActions::Single { label, on_press };
        self
    }

    /// 双动作（左 dismiss / 右 confirm）。
    pub fn double(mut self, dismiss: (&'a str, M), confirm: (&'a str, M)) -> Self {
        self.actions = DialogActions::Double { dismiss, confirm };
        self
    }

    /// 强制动作件的视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// 遮罩色：`黑 @ SCRIM_ALPHA` 叠在 `MaterialLayer::Canvas` 上之后的**实色**（D27）。
pub fn scrim(ui: &Ui) -> iced::Color {
    let scheme = Scheme::baseline(ui.flavor());
    let canvas = scheme.get(MaterialLayer::Canvas.surface_role());
    to_iced(scheme.scrim().over(canvas))
}

/// 把内容放进一块遮罩里（模态的「背后压暗」）。页面把它压在自己的内容上面即可。
pub fn scrimmed<'a, M: 'a>(ui: &'a Ui, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    let backdrop = scrim(ui);
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |_theme| container::Style {
            background: Some(Background::Color(backdrop)),
            ..container::Style::default()
        })
        .into()
}

/// 画一个对话框（不含遮罩；遮罩用 [`scrimmed`]）。
pub fn dialog<'a, M: Clone + 'a>(ui: &'a Ui, spec: Dialog<'a, M>) -> Element<'a, M> {
    let geometry = DialogGeometry::of(ui);
    let surface = ui.layer(MaterialLayer::Overlay);
    let radius = iced::border::Radius::new(geometry.radius);

    let title_style = ui.style(TypeRole::HeadlineSmall);
    let body_style = ui.style(TypeRole::BodyMedium);

    let mut content = column![].width(Length::Fill).spacing(0.0).push(
        text(spec.title)
            .size(title_style.size_px)
            .font(ui.font_of(title_style))
            .color(ui.color(Role::OnSurface)),
    );
    content = content.push(gap(geometry.title_gap)).push(
        text(spec.body)
            .size(body_style.size_px)
            .font(ui.font_of(body_style))
            .color(ui.color(Role::OnSurfaceVariant)),
    );

    if !spec.actions.is_empty() {
        content = content.push(gap(geometry.body_gap)).push(actions_row(
            ui,
            &geometry,
            spec.actions,
            spec.state,
        ));
    }

    // 宽度 = `min(spec.max_width, 480)`，**可用宽由调用方递**（`Dialog::width`）。
    //
    // 复审 P2-1 建议「Fill + max_width」：实测**不行**。`Row` 给 `Fill` 子项的 limits 是
    // `with_compression(min = max = 分配额)`，而 `Limits::max_width()` 的实现是
    // `self.max.width.min(max_width).max(self.min.width)` —— 父级那个 **min 会把 480 顶回去**，
    // 两列并排时每张卡渲染成 **548**（探针实测，见报告）。iced 没有 min-width，也没有在
    // 「父级精确分配」下可用的 max-width ⇒ 与 Snackbar 的最小宽同一堵墙（G17-d 之二）。
    // 所以：默认 480，窄宿主由页面传 `Dialog::width(可用宽)`。
    // 可用宽 → 实际宽：`min(480, 可用宽)` 再 `.max(280)`（官方 `DialogMinWidth`，复审 E）。
    // 可用宽 < 280 时仍画 280（溢出宿主）——iced 无 min-width，这条写进 G17-d 之二。
    let effective = geometry.effective_width(spec.max_width);
    container(content)
        .width(Length::Fixed(effective))
        .padding(Padding::from(geometry.padding))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius,
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 两区之间的固定留白（A1 的「标题下 16 / 正文下 24」）。
fn gap<'a, M: 'a>(height: f32) -> Element<'a, M> {
    Space::new()
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .into()
}

/// 动作区：右对齐，主轴间距 8（A1 `ButtonsMainAxisSpacing`）。
fn actions_row<'a, M: Clone + 'a>(
    ui: &'a Ui,
    geometry: &DialogGeometry,
    actions: DialogActions<'a, M>,
    state: Option<Interaction>,
) -> Element<'a, M> {
    let controls = match actions {
        DialogActions::None => row![],
        DialogActions::Single { label, on_press } => row![action::text_action(
            ui,
            label,
            Role::Primary,
            MaterialLayer::Overlay.surface_role(),
            Some(on_press),
            state,
        )],
        DialogActions::Double { dismiss, confirm } => row![
            action::text_action(
                ui,
                dismiss.0,
                Role::Primary,
                MaterialLayer::Overlay.surface_role(),
                Some(dismiss.1),
                state,
            ),
            action::text_action(
                ui,
                confirm.0,
                Role::Primary,
                MaterialLayer::Overlay.surface_role(),
                Some(confirm.1),
                state,
            ),
        ],
    };

    container(
        controls
            .spacing(geometry.action_gap)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .align_x(Alignment::End)
    .into()
}
