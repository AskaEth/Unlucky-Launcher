//! `--screen=gallery` 的壳：**视觉验收 harness**（D30 / D39 §5），不是产品页面。
//!
//! ## 它是什么、不是什么
//!
//! - 是：**状态矩阵的取景框**。每个家族在自己的 `gallery/<family>.rs` 里摆出
//!   「五变体 × 五态」的矩阵，一张截图就能判完该家族（浅/深各一张 = 10 张证据）。
//! - 不是：页面。gallery 允许堆叠、不分页、不做空态，**难看不算问题**；
//!   但**每个组件本身**必须按官方口径做到位。
//!
//! ## 并行地基（后续四条线怎么加东西）
//!
//! 五个家族文件在下面 `mod` 里**全部预先声明**了，每个都有自己的 `section()`：
//!
//! ```text
//! gallery/buttons.rs     ← COMPBASE（本 mission）
//! gallery/selection.rs   ← 后续 mission（只改这个文件）
//! gallery/inputs.rs      ← 同上
//! gallery/feedback.rs    ← 同上
//! gallery/navigation.rs  ← 同上
//! ```
//!
//! 加组件 = 只改自己那个文件里的 `section()`，**不用碰本文件**（这是「五条线并行不冲突」的
//! 具体保证，也是 tower 会亲手验的一条）。
//!
//! ## DemoMessage 为什么独立
//!
//! 契约 §2：组件不 import `crate::app::Message`。gallery 用自己定义的消息枚举，
//! 正好当「组件能脱离业务验收」的示范——`app.rs` 只做一次 `.map(Message::Demo)` 就接上了。
//!
//! ## 截图命令（D30）
//!
//! ```text
//! unlucky-launcher-iced --software-renderer --screen=gallery \
//!     --screenshot out.png --screenshot-theme=light
//! ```
//!
//! `--screenshot` 不带主题时**默认 Light**（可复现；见 `cli::Options::capture_flavor`）。
//!
//! ## 段落过滤：环境变量 `ULL_GALLERY_SECTION`（M76 起）
//!
//! 五段拼成一列、外面套 `scrollable`，而截图只截**首屏** `1180×760`。段数一多，后面的家族
//! 就落到折叠线以下（实测：selection 段从 y≈735 起，只剩标题一行）——那条线的
//! 「渲染层像素断言」就没有一个像素可量。所以这里加一个**段落选择器**：
//!
//! ```text
//! ULL_GALLERY_SECTION=all        # 默认：五段全渲染（行为与加开关之前逐像素一致）
//! ULL_GALLERY_SECTION=buttons    # 只渲染 buttons 段
//! ULL_GALLERY_SECTION=selection  # 只渲染 selection 段（本 mission 的取证与 probe 用它）
//! ```
//!
//! 取值：`all` / `buttons` / `divider` / `selection` / `inputs` / `feedback` / `navigation`。
//! 语义是「**不构建**没被选中的段」（不是构建后隐藏）：`column` 里根本不 push 它，
//! 于是该家族的 `view` 一行都不跑。未识别的值**回落 `all`** 并在 stderr 打一行提示
//! （静默回落会让人拿错图还以为拍的是那一段）。
//!
//! 为什么用环境变量而不是 `--screen=gallery:<family>`：`cli.rs` 不归组件线（M82 在改它），
//! 而环境变量既能被测试（`cargo test` 里 `std::env::set_var`）又不用跟别人的 CLI 解析抢文件。
//! 各家族的 `demo()` 签名**不因此改变**——本文件只决定「push 哪几段」。
//!
//! 实际渲染了哪几段会打一行 `[gallery] section=... → 渲染 ...`（首帧一次），
//! 拿它当出图日志的证据。

use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::{column, container, scrollable, text};
use iced::{Alignment, Background, Element, Length};

use unlucky_design::color::Role;
use unlucky_design::elevation::MaterialLayer;
use unlucky_design::shape::ShapeScale;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::shot::flavor_slug;
use crate::theme::Ui;

use super::divider;
use super::interaction::Interaction;
use super::layout;

mod buttons;
mod feedback;
mod inputs;
mod navigation;
#[cfg(test)]
mod probe;
#[cfg(test)]
mod scan;
mod selection;

/// 段落过滤的环境变量名（语义见模块文档「段落过滤」）。
pub const SECTION_ENV: &str = "ULL_GALLERY_SECTION";

/// 段落开关：**构建**哪几段（`false` = 这一段连 `view` 都不跑）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sections {
    buttons: bool,
    divider: bool,
    selection: bool,
    inputs: bool,
    feedback: bool,
    navigation: bool,
}

impl Sections {
    /// 五段全渲染（默认）。
    const ALL: Self = Self {
        buttons: true,
        divider: true,
        selection: true,
        inputs: true,
        feedback: true,
        navigation: true,
    };

    /// 只有一段。
    fn only(name: &str) -> Option<Self> {
        let mut sections = Self {
            buttons: false,
            divider: false,
            selection: false,
            inputs: false,
            feedback: false,
            navigation: false,
        };
        match name {
            "buttons" => sections.buttons = true,
            "divider" => sections.divider = true,
            "selection" => sections.selection = true,
            "inputs" => sections.inputs = true,
            "feedback" => sections.feedback = true,
            "navigation" => sections.navigation = true,
            _ => return None,
        }
        Some(sections)
    }

    /// 读 `ULL_GALLERY_SECTION`。
    ///
    /// 未设置 / 空 / `all`（大小写不敏感）→ 全渲染；**未识别的值 → 回落全渲染 + stderr 一行**。
    fn from_env() -> Self {
        let Ok(raw) = std::env::var(SECTION_ENV) else {
            return Self::ALL;
        };
        let name = raw.trim().to_ascii_lowercase();
        if name.is_empty() || name == "all" {
            return Self::ALL;
        }
        if let Some(sections) = Self::only(&name) {
            return sections;
        }
        eprintln!(
            "[gallery] {SECTION_ENV}={raw:?} 不是已知段（all|buttons|divider|selection|inputs|feedback|navigation），已回落 all"
        );
        Self::ALL
    }

    /// 本次渲染的段名（stderr 日志 / 测试断言用）。
    fn names(self) -> Vec<&'static str> {
        let mut names = Vec::new();
        for (on, name) in [
            (self.buttons, "buttons"),
            (self.divider, "divider"),
            (self.selection, "selection"),
            (self.inputs, "inputs"),
            (self.feedback, "feedback"),
            (self.navigation, "navigation"),
        ] {
            if on {
                names.push(name);
            }
        }
        names
    }
}

/// `[gallery] ... 渲染 ...` 只打一次（`view` 每帧都会跑）。
static SECTION_LOGGED: AtomicBool = AtomicBool::new(false);

fn log_sections(sections: Sections) {
    if SECTION_LOGGED.swap(true, Ordering::Relaxed) {
        return;
    }
    let raw = std::env::var(SECTION_ENV).unwrap_or_else(|_| "all".to_owned());
    eprintln!(
        "[gallery] section={raw} → 渲染 {} 段：{}",
        sections.names().len(),
        sections.names().join(", ")
    );
}

/// gallery 自己的消息（与业务 `Message` 解耦，契约 §2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DemoMessage {
    /// 某个演示按钮被按下（`&'static str` = 演示 id，只用于日志）。
    Pressed(&'static str),
}

impl DemoMessage {
    /// 演示 id（日志用；将来加变体时这里跟着改，`app.rs` 不用动）。
    pub fn id(&self) -> &'static str {
        match self {
            DemoMessage::Pressed(id) => id,
        }
    }
}

/// gallery 根视图（整窗：不带标题栏与导航轨，验收图上只有组件）。
pub fn view<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let page = ui.style(TypeRole::BodySmall);

    let header = column![
        text("Component gallery")
            .size(ui.emphasized(TypeRole::TitleLarge).size_px)
            .font(ui.emphasized_font(TypeRole::TitleLarge))
            .color(ui.color(Role::OnSurface)),
        text(format!(
            "M3 Expressive component base · flavor={} · 1180×760 @100% DPI · \
             focus 与 pressed 同为 10%（令牌口径，图上一样是对的）",
            flavor_slug(ui)
        ))
        .size(page.size_px)
        .font(ui.font_of(page))
        .color(ui.color(Role::OnSurfaceVariant)),
    ]
    .spacing(ui.space(Space::X1));

    // 小节之间**不画分隔线**：rubric（15 §22.4）要求同屏同类线 ≤3，
    // 而验收图里要给分隔线自己留证据（见 `divider_proof`）。分组靠 32px 留白 + 标题。
    //
    // 段落过滤（`ULL_GALLERY_SECTION`）：**没被选中的段根本不 push**，于是该家族的
    // `view` 一行都不跑——不是「构建后隐藏」。默认 `all` 与加开关之前逐像素一致。
    let picked = Sections::from_env();
    log_sections(picked);
    let mut sections = column![].spacing(ui.space(Space::X8)).width(Length::Fill);
    if picked.buttons {
        sections = sections.push(buttons::section(ui));
    }
    if picked.divider {
        sections = sections.push(divider_proof(ui));
    }
    if picked.selection {
        sections = sections.push(selection::section(ui));
    }
    if picked.inputs {
        sections = sections.push(inputs::section(ui));
    }
    if picked.feedback {
        sections = sections.push(feedback::section(ui));
    }
    if picked.navigation {
        sections = sections.push(navigation::section(ui));
    }

    // 滚动条：默认是主题给的 10px 深灰 thumb，压过组件本体（视觉 reviewer P2-4）。
    // 这里把它改成 **透明轨 + OutlineVariant thumb**；宽度 10px 是 `scrollable` 的内部常量
    // （该控件不暴露 Scrollbar 配置），所以只做了「低对比」这一半，已登记为偏离 D7。
    let body = scrollable(sections)
        .height(Length::Fill)
        .width(Length::Fill)
        .style(move |theme, status| {
            // `scrollable::Style` 没有 `Default`：底色/间隙从主题默认值出发，只覆盖竖轨
            let base = scrollable::default(theme, status);
            scrollable::Style {
                vertical_rail: scrollable::Rail {
                    background: None,
                    border: iced::Border::default(),
                    scroller: scrollable::Scroller {
                        background: Background::Color(ui.color(Role::OutlineVariant)),
                        border: iced::Border {
                            radius: ui.radius(ShapeScale::Full, ui.space(Space::X2)),
                            ..iced::Border::default()
                        },
                    },
                },
                ..base
            }
        });

    // 页面留白走 `layout::inset`（左右/上下各 24）：这是 Q8/Q9 那两个 helper 的第三个兄弟，
    // 存在意义是「只改内边距、不改尺寸」，也顺手避免把 Padding 的上右下左写反。
    let page_padding = ui.space(Space::X6);
    container(layout::inset(
        page_padding,
        page_padding,
        column![header, body].spacing(ui.space(Space::X4)),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .style(|_theme| container::Style {
        background: Some(Background::Color(ui.layer(MaterialLayer::Canvas))),
        ..container::Style::default()
    })
    .into()
}

/// 分隔线的视觉证据（§22）：一条通栏 + 一条列表内缩进，各自带说明。
///
/// 单独一节而不是散落各处：`15 §22.4` 说「同屏同类线 ≤3」，这两条是**唯一**计入的两条。
fn divider_proof<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let caption = ui.style(TypeRole::LabelSmall);
    let label = |text_value: &'a str| {
        text(text_value)
            .size(caption.size_px)
            .font(ui.font_of(caption))
            .color(ui.color(Role::OnSurfaceVariant))
    };

    column![
        heading(
            ui,
            "divider · full / list-inset",
            "15 §22：厚度 1px（stroke::HAIRLINE）· 颜色 OutlineVariant · 列表内左缩进 16 · 同屏 ≤3 条"
        ),
        label("full (通栏，区块分隔)"),
        divider::full::<DemoMessage>(ui),
        label("list (左缩进 16)"),
        divider::list::<DemoMessage>(ui),
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 一个家族小节的标题（`index.title`）+ 一行判据说明。
///
/// 五个家族文件共用（`pub(super)`），保证五段的节奏一致——这也是 rubric D4
/// 「一屏字号档 ≤4」的落点：小节只有 `TitleMedium` + `BodySmall` 两档。
pub(crate) fn heading<'a>(ui: &'a Ui, title: &'a str, note: &'a str) -> Element<'a, DemoMessage> {
    let note_style = ui.style(TypeRole::BodySmall);
    column![
        text(title)
            .size(ui.emphasized(TypeRole::TitleMedium).size_px)
            .font(ui.emphasized_font(TypeRole::TitleMedium))
            .color(ui.color(Role::OnSurface)),
        text(note)
            .size(note_style.size_px)
            .font(ui.font_of(note_style))
            .color(ui.color(Role::OnSurfaceVariant)),
    ]
    .spacing(ui.space(Space::X1))
    .into()
}

/// 状态矩阵的列头：五态的固定次序（`Interaction::ALL` 的口径）。
///
/// 列宽 = [`cell_width`]（**160** = `Space::X8 × 5`，是矩阵栅格，不是组件尺寸）。
pub(crate) fn state_cells(ui: &Ui) -> Vec<(&'static str, f32)> {
    Interaction::ALL
        .iter()
        .map(|state| (state.name(), cell_width(ui)))
        .collect()
}

/// 矩阵单元格宽度（只影响 harness 的取景，不是组件尺寸）。
///
/// 160 = `Space::X8 × 5`：5 列 + 1 个标签列 + 4 个列间距 + 页左右各 24
/// = 6×160 + 4×8 + 48 = **1040 < 1180**，所以第 5 列（disabled）不再被窗口右缘裁掉。
/// 早先是 192（×6），总宽 1232 撑破窗口，disabled 列被裁掉一半（视觉 reviewer P2-3）。
pub(crate) fn cell_width(ui: &Ui) -> f32 {
    ui.space(Space::X8) * 5.0
}

/// 一个矩阵行的左侧标签（变体名）。
pub(crate) fn row_label<'a>(ui: &'a Ui, name: &'a str) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::BodySmall);
    container(
        text(name)
            .size(style.size_px)
            .font(ui.font_of(style))
            .color(ui.color(Role::OnSurfaceVariant)),
    )
    .width(Length::Fixed(cell_width(ui)))
    .align_y(Alignment::Center)
    .into()
}
