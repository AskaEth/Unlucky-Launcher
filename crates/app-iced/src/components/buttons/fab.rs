//! FAB 四态（规格卡 `FAB / SMALL - DEFAULT - MEDIUM - EXTENDED`）。
//!
//! ## 规格卡原值（`[spec]` = 图上的坐标/字号，几何全部来自 [`FabSize`] 的令牌访问器）
//!
//! | 档 | 尺寸 | 圆角 | 图标 | 标签 | 规格卡依据 |
//! |---|---|---|---|---|---|
//! | Small | 40×40 | 12（`Medium`） | 20 | — | `+` 徽章式小 FAB |
//! | Default | 56×56 | 16（`Large`） | 26 | — | M3 默认 FAB |
//! | Medium | 80×80 | 20（`LargeIncreased`） | 34 | — | 中大档 |
//! | Extended | 160×56（最小宽） | 16（`Large`） | 24 | 15 | `+ New`：起始内距 28、图标↔标签 8 |
//!
//! ## 配色：`PrimaryContainer` / `OnPrimaryContainer`
//!
//! 规格卡上四颗 FAB 都是**淡绿底 + 深绿字形**，M3 的对应角色正是 `primary-container` /
//! `on-primary-container`（`boards/ink.rs` 的 `[spec]` 行注释也这么标：`FAB 底` / `FAB 图标`）。
//! 所以这里不引入新角色，也不画阴影——规格卡上是纯色块，M3 的 FAB 高程由容器色表达。
//!
//! 色值不写在这份文档里：`tests/no_hardcoded_tokens.rs` 会扫 `src/**` 的十六进制色值，
//! 要核对数字请看 `boards/ink.rs` 的 `ANCHORS` 表（那是唯一真源）。
//!
//! ## 为什么单独一个文件
//!
//! 按钮家族（五式 × 五态 × 五档）已经在 `buttons/mod.rs` + `assemble.rs` 里贴着 400 行；FAB 的
//! 几何与按钮**不共享**（圆角不是半高、图标随档放大、扩展态有自己的内距），硬塞进去只会让两边
//! 的读取路径都变长。共享的只有 `interaction` 的状态层与图标烘焙工具。

use iced::widget::{button, container, row, text};
use iced::{Alignment, Background, Border, Element, Length, Padding, Shadow};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::inkcat::size::FabSize;
use unlucky_design::typography::{TypeRole, TypeStyle, Weight};

use crate::icons;
use crate::theme::Ui;

use super::super::interaction::{self, Base, Interaction};
use super::super::layout;

/// 扩展态右侧内距（px）。规格卡只量到起始 28（`FabSize::pad_start_px`），右侧没有量；
/// 取同值 **28** 让「图标 + 标签」在 160 宽里左右对称（`[估]`，与起始同源的唯一对称解）。
pub const EXTENDED_PAD_END: f32 = 28.0;

/// 一颗 FAB：图标必有，标签只有扩展态有；`on_press = None` 即禁用。
#[derive(Debug, Clone)]
pub struct Fab<'a, M> {
    icon: IconKind,
    label: Option<&'a str>,
    size: FabSize,
    on_press: Option<M>,
    state: Option<Interaction>,
}

impl<'a, M> Fab<'a, M> {
    /// 标准 FAB（默认档 = 56）。
    pub fn new(icon: IconKind, on_press: M) -> Self {
        Self {
            icon,
            label: None,
            size: FabSize::Default,
            on_press: Some(on_press),
            state: None,
        }
    }

    /// 扩展态（药丸 + 图标 + 标签）。`label` 文案由调用方给（组件不碰 i18n）。
    pub fn extended(icon: IconKind, label: &'a str, on_press: M) -> Self {
        Self {
            label: Some(label),
            size: FabSize::Extended,
            ..Self::new(icon, on_press)
        }
    }

    /// 换档（Small / Default / Medium / Extended）。
    pub fn size(mut self, size: FabSize) -> Self {
        self.size = size;
        self
    }

    /// 强制视觉态（gallery 取证用；运行期不要调）。
    pub fn state(mut self, state: Interaction) -> Self {
        self.state = Some(state);
        self
    }
}

/// 禁用态的内容色：`OnPrimaryContainer` @38% 叠在 **FAB 自己的容器**（`PrimaryContainer`）上。
///
/// ## 为什么底是 FAB 自己的容器，不是画布（M234 复审 P2-2）
///
/// 五式按钮的禁用容器是「`OnSurface`@10% 叠在脚下那一层」——透明的那种甚至没有容器；所以它们的
/// 内容要叠 `0.10 容器 → 父底`。FAB 不一样：`Interaction::Disabled.layer()` 是 `None`，
/// 样式闭包走 `None => Some(ui.color(Role::PrimaryContainer))`，**禁用时背景仍是满档
/// `PrimaryContainer`**（官方 `FloatingActionButtonTokens` 也没有 DisabledContainerOpacity）。
/// 所以内容只能叠 `PrimaryContainer` 本身，`on_container = false`——那一层 10% 在 FAB 身上
/// 根本不存在，给了反而多算一层灰。
///
/// 修前传 `Base::CANVAS`：38% 的 `OnPrimaryContainer` 合在画布色上，出来是一块与 FAB 底无关的灰
/// （Silver 浅色实测是一块灰 `rgb(0.549,0.627,0.667)`），而不是「深色字压在淡蓝容器上」
/// 的应有结果（同板上正确值是 `rgb(0.329,0.561,0.737)`；数字见 artifacts/m234 的 README）。
///
/// `button()` 与样式闭包**共用这一个函数**：早先两处各写一遍，两次都传了同一个错值。
fn content_color(ui: &Ui, forced: Option<Interaction>) -> iced::Color {
    match forced {
        Some(Interaction::Disabled) => interaction::disabled_button_content(
            ui,
            Base::of(Role::PrimaryContainer),
            Role::OnPrimaryContainer,
            false,
        ),
        _ => ui.color(Role::OnPrimaryContainer),
    }
}

/// 画一颗 FAB。
pub fn fab<'a, M: Clone + 'a>(ui: &'a Ui, spec: Fab<'a, M>) -> Element<'a, M> {
    let size = spec.size;
    let forced = spec.state.or(if spec.on_press.is_none() {
        Some(Interaction::Disabled)
    } else {
        None
    });
    // 内容色（含禁用档）只走 `content_color` 一处
    let ink = content_color(ui, forced);

    let glyph = layout::fixed(
        size.icon_px(),
        size.icon_px(),
        icons::view_at::<M>(spec.icon, ink, size.icon_px()),
    );

    let content: Element<'a, M> = match spec.label {
        // 扩展态：图标 + 标签（起始内距 28、间距 8，规格卡实测值）
        Some(label) => {
            let label_style = TypeStyle {
                weight: Weight::Medium,
                ..TypeRole::BodyMedium.style()
            };
            let label_px = size.label_px();
            row![
                glyph,
                text(label)
                    .size(label_px)
                    .font(ui.font_of(TypeStyle {
                        size_px: label_px,
                        ..label_style
                    }))
                    .color(ink),
            ]
            .spacing(size.gap_px())
            .align_y(Alignment::Center)
            .padding(Padding {
                top: 0.0,
                right: EXTENDED_PAD_END,
                bottom: 0.0,
                left: size.pad_start_px(),
            })
            .into()
        }
        None => container(glyph)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Alignment::Center)
            .align_y(Alignment::Center)
            .into(),
    };

    let shape = size.shape();
    let radius = size.radius_px();
    let style = move |_theme: &iced::Theme, status: button::Status| {
        let state = forced.unwrap_or_else(|| Interaction::from_status(status));
        let background = match state.layer() {
            // 状态层叠在容器色上（与按钮族同一条 `overlay_color` 口径）
            Some(_) => interaction::overlay_color(
                ui,
                state,
                Role::OnPrimaryContainer,
                Role::PrimaryContainer,
            )
            .or(Some(ui.color(Role::PrimaryContainer))),
            None => Some(ui.color(Role::PrimaryContainer)),
        };
        let text_color = content_color(ui, Some(state));

        button::Style {
            background: background.map(Background::Color),
            text_color,
            border: Border {
                radius: iced::border::Radius::new(radius),
                ..Border::default()
            },
            // 规格卡不画阴影；M3 的 FAB 高程靠容器色（rubric §7 的「L0/L1 不用阴影」同源）
            shadow: Shadow::default(),
            snap: true,
        }
    };

    // 四档的宽都由令牌给（方态是边长、扩展态是**最小**宽 160）。
    //
    // ⚠️ iced 0.14 的 `Length` 只有 `Fill/FillPortion/Shrink/Fixed`，**没有 Min**（本仓 G15 / G17-d
    // 已登记同一堵墙），所以「最小宽随标签撑开」在这里退化成**固定宽**：标签比 160 内距容得下的
    // 更长时会被裁，短时也不会缩。登记为偏离（见 `FabSize::width_px` 的文档）；真要 Min 语义得等
    // iced 给 `Length::Min`，或在组件里先量文本再算宽——那要引字体度量，不在本轮。
    let width = Length::Fixed(size.width_px());

    let element = button(content)
        .width(width)
        .height(Length::Fixed(size.height_px()))
        .padding(Padding::default())
        .style(style);

    debug_assert!(radius > 0.0, "FAB 圆角必须来自 ShapeScale（{shape:?}）");
    debug_assert!(
        matches!(size, FabSize::Extended) || size.pad_start_px() == 0.0,
        "只有扩展态有起始内距"
    );

    match spec.on_press {
        Some(message) => element.on_press(message).into(),
        None => element.into(),
    }
}

#[cfg(test)]
mod tests;
