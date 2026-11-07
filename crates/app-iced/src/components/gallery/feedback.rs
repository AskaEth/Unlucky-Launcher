//! gallery 里的反馈类小节：把状态矩阵摆进 harness（**断言不在这里**，在
//! [`crate::app::components::feedback::probe`]）。
//!
//! ## 段落切分：`ULL_FEEDBACK_PART`（本文件自己的开关）
//!
//! `ULL_GALLERY_SECTION=feedback` 只能把**整个家族**拎出来，而四件的高度超过 1180×760 首屏，
//! 所以再加一层**家族内**切分（契约 §9.2 的口径：环境变量 + 默认向后兼容 + 未识别的值回落
//! 默认并打 stderr；区别只是它住在**本家族自己的文件**里，不动 `gallery/mod.rs`）：
//!
//! ```text
//! ULL_FEEDBACK_PART=all       # 默认：四件全渲染（progress → snackbar → empty → dialog）
//! ULL_FEEDBACK_PART=progress  # Linear 0/50/100 + 三相位 · Circular 0/25/50/75/100 + 不定态
//! ULL_FEEDBACK_PART=snackbar  # 四格：贴顶·上限宽 / 双行 / 宿主 300 / 宿主 344
//! ULL_FEEDBACK_PART=dialog    # 两形态并排 + 双动作 + 一屏 scrim
//! ULL_FEEDBACK_PART=empty     # 空态（带 CTA）
//! ```
//!
//! ## 宿主尺寸为什么写死
//!
//! 量宽度必须有确定的宿主宽：`Snackbar` 的 600 是「宿主给 800 时的夹取值」、344 与 300 是
//! 「宿主给多少就是多少」的反证（组件没有 min-width，G17-d）。`Linear` 轨道统一 320（一行 3 格 = 960 < 1128）。
//!
//! 复现（worktree 根目录）：
//!
//! ```text
//! ULL_GALLERY_SECTION=feedback ULL_FEEDBACK_PART=progress ./target/debug/unlucky-launcher-iced \
//!   --software-renderer --screen=gallery \
//!   --screenshot target/m78/progress-light.png --screenshot-theme=light
//! ULL_GALLERY_SHOT=target/m78/progress-light.png cargo test -p app-iced feedback::probe
//! ```

use iced::widget::{column, container, row, text};
use iced::{Element, Length};

use unlucky_design::color::Role;
use unlucky_design::icons::IconKind;
use unlucky_design::spacing::Space;
use unlucky_design::typography::TypeRole;

use crate::theme::Ui;

use super::super::feedback::gallery_parts::{Part, DIALOG, EMPTY, PROGRESS, SNACKBAR};
use super::super::feedback::{self, Dialog, EmptyState, Placement, Progress, Snackbar};
use super::super::interaction::Interaction;
use super::{heading, DemoMessage};

/// 线性进度的宿主宽（格宽，不是组件尺寸）。
const TRACK_HOST: f32 = 320.0;
/// `Snackbar` 的宿主宽：**条宽 + 2×12 外边距**（12 是官方外边距，组件的定位层会扣掉）。
///
/// 800（条夹到上限 600）/ 368（条 344）/ 324（条 300，用来证明**没有**下限，G17-d）。
const HOST_WIDE: f32 = 800.0;
const HOST_MIN: f32 = 368.0;
const HOST_NARROW: f32 = 324.0;
/// `Snackbar` 宿主高（比 68 + 2×12 略高，留出贴顶/贴底的判定余量）。
const HOST_TALL: f32 = 96.0;
/// 窄宿主：给 `Dialog::width` 递 360，量「窄宿主不再被裁」的那一格（复审 E）。
const NARROW_HOST: f32 = 360.0;

/// 单行提示条的宿主高。
///
/// **96 而不是 72**（复审 D）：72 = 48 + 2×12 时上下外边距相等，像素上分不出「贴底」还是
/// 「居中」；96 让贴底那格的上留白 = 36、下留白 = 12，探针能量到「条底距宿主底 = 12」。
const HOST_SHORT: f32 = 96.0;
/// 空态宿主高：内容本征高 ≈96+16+32+8+16+24+40 ≈ 232，留出上下各 40 的余量。
const HOST_EMPTY: f32 = 320.0;

/// 反馈类家族的小节。
pub(super) fn section<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let part = Part::from_env();
    part.log_once();

    let mut body = column![heading(
        ui,
        "feedback · progress / snackbar / dialog / empty state",
        "12b §7/§12/§14/§15：Linear 厚 4 圆角 full · Circular 40/4 · Snackbar 48/68 上限宽 600 · \
         Dialog R28 pad24 宽 480 · EmptyState 项目自定[估]（官方无此组件）"
    )]
    .spacing(ui.space(Space::X3));

    if part.has(PROGRESS) {
        body = body.push(progress_block(ui));
    }
    if part.has(SNACKBAR) {
        body = body.push(snackbar_block(ui));
    }
    if part.has(EMPTY) {
        body = body.push(empty_block(ui));
    }
    if part.has(DIALOG) {
        // 只有这一屏不画进度/提示条时，才把三形态与 scrim 都摆开（高度够）。
        let roomy = !part.has(PROGRESS) && !part.has(SNACKBAR);
        body = body.push(dialog_block(ui, roomy));
    }

    body.into()
}

/// 一行判据说明（`LabelSmall`，与矩阵的正文档分开）。
fn caption<'a>(ui: &'a Ui, value: &'a str) -> Element<'a, DemoMessage> {
    let style = ui.style(TypeRole::LabelSmall);
    text(value)
        .size(style.size_px)
        .font(ui.font_of(style))
        .color(ui.color(Role::OnSurfaceVariant))
        .into()
}

/// 带 1px 取景框的宿主：`Snackbar` 的贴底/贴顶需要一条**可见的宿主边界**才量得出来
/// （描边画在盒内但**不参与布局**：内容区仍是整盒宽，实测条宽不受影响）。
/// （透明宿主的底边在图上不存在，`Placement::Bottom` 就不可证）。
fn framed_host<'a>(
    ui: &'a Ui,
    width: f32,
    height: f32,
    content: impl Into<Element<'a, DemoMessage>>,
) -> Element<'a, DemoMessage> {
    let line = ui.color(Role::OutlineVariant);
    container(content)
        .width(Length::Fixed(width))
        .height(Length::Fixed(height))
        .style(move |_theme| container::Style {
            border: iced::Border {
                color: line,
                width: crate::theme::STROKE_PX,
                radius: iced::border::Radius::default(),
            },
            ..container::Style::default()
        })
        .into()
}

/// 固定宽宿主（量宽度用：宿主宽写死，组件内部的占比/夹取才有确定值）。
fn host<'a>(
    width: f32,
    height: Option<f32>,
    content: impl Into<Element<'a, DemoMessage>>,
) -> Element<'a, DemoMessage> {
    let mut box_ = container(content).width(Length::Fixed(width));
    if let Some(height) = height {
        box_ = box_.height(Length::Fixed(height));
    }
    box_.into()
}

/// 一列：[进度条, 说明]。
fn track<'a>(ui: &'a Ui, label: &'a str, progress: Progress) -> Element<'a, DemoMessage> {
    column![
        host(TRACK_HOST, None, feedback::linear(ui, progress)),
        caption(ui, label)
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 一列：[环, 说明]。
fn ring<'a>(ui: &'a Ui, label: &'a str, progress: Progress) -> Element<'a, DemoMessage> {
    column![feedback::circular(ui, progress), caption(ui, label)]
        .spacing(ui.space(Space::X2))
        .into()
}

/// §12：Linear 0/50/100 + 四个相位；Circular 0/25/50/75/100 + 三个相位。
fn progress_block<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let determinate = row![
        track(ui, "0%", Progress::determinate(0.0)),
        track(ui, "50%", Progress::determinate(0.5)),
        track(ui, "100%", Progress::determinate(1.0)),
    ]
    .spacing(ui.space(Space::X3));

    let indeterminate = row![
        track(ui, "phase 0.15", Progress::indeterminate(0.15)),
        track(ui, "phase 0.35", Progress::indeterminate(0.35)),
        track(ui, "phase 0.55", Progress::indeterminate(0.55)),
    ]
    .spacing(ui.space(Space::X3));

    let rings = row![
        ring(ui, "0%", Progress::determinate(0.0)),
        ring(ui, "25%", Progress::determinate(0.25)),
        ring(ui, "50%", Progress::determinate(0.5)),
        ring(ui, "75%", Progress::determinate(0.75)),
        ring(ui, "100%", Progress::determinate(1.0)),
        ring(ui, "ind 0.30", Progress::indeterminate(0.30)),
    ]
    .spacing(ui.space(Space::X4))
    .align_y(iced::Alignment::Start);

    column![
        caption(
            ui,
            "Linear · determinate 0% / 50% / 100%（轨道 SecondaryContainer · 指示条 Primary）"
        ),
        determinate,
        caption(
            ui,
            "Linear · indeterminate（官方 1750ms 双段追逐；下面是同一周期的三个相位）"
        ),
        indeterminate,
        caption(
            ui,
            "Circular · 直径 40 · 线宽 4 · 弧长 = 数值 × 360°（第六格 = 不定态 phase 0.30）"
        ),
        rings,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// 提示条的宿主：固定宽 + 提示条宿主高（比 68 + 2×12 略高，贴顶/贴底都量得到）。
fn bar_host<'a>(
    width: f32,
    content: impl Into<Element<'a, DemoMessage>>,
) -> Element<'a, DemoMessage> {
    host(width, Some(HOST_TALL), content)
}

/// §14：四格 —— 贴顶·上限宽 / 双行 + action + 关闭 / 宿主 300 / 宿主 344。
fn snackbar_block<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let press = |id| DemoMessage::Pressed(id);

    let wide = bar_host(
        HOST_WIDE,
        feedback::snackbar(
            ui,
            Snackbar::new("版本清单已刷新")
                .placement(Placement::Top)
                .dismiss(press("snackbar/wide")),
        ),
    );
    let two_lines = host(
        HOST_WIDE,
        Some(HOST_TALL * 2.0),
        feedback::snackbar(
            ui,
            Snackbar::new(
                "启动失败：找不到 Java 21。请在「设置 · Java」里指定运行时路径，或让启动器自动下载一个 21 的运行时。",
            )
                .two_lines()
                .placement(Placement::Top)
                .action("去设置", press("snackbar/settings"))
                .dismiss(press("snackbar/close"))
                .state(Interaction::Hovered),
        ),
    );
    let mid = framed_host(
        ui,
        HOST_MIN,
        HOST_SHORT,
        feedback::snackbar(
            ui,
            Snackbar::new("已删除实例")
                .placement(Placement::Bottom)
                .action("撤销", press("snackbar/undo")),
        ),
    );
    let narrow = framed_host(
        ui,
        HOST_NARROW,
        HOST_SHORT,
        feedback::snackbar(ui, Snackbar::new("宿主 324").placement(Placement::Bottom)),
    );

    column![
        caption(
            ui,
            "Snackbar · 贴顶 + 上限宽（宿主 800 → 夹到 600）· 关闭键 InverseOnSurface"
        ),
        wide,
        caption(
            ui,
            "Snackbar · 双行 68 + action InversePrimary + 关闭键（动作强制 hover 态）"
        ),
        two_lines,
        caption(
            ui,
            "宿主 324（取景框）⇒ 条 300：组件没有 min-width，宿主给多少就是多少（G17-d）；贴底 = 条底距框底 12"
        ),
        narrow,
        caption(ui, "宿主 368 ⇒ 条 344：最小宽的取值口径（[估]）"),
        mid,
    ]
    .spacing(ui.space(Space::X2))
    .into()
}

/// §15：空态（项目自定；浅/深两种主题各出一张图，主题来自 `--screenshot-theme`）。
fn empty_block<'a>(ui: &'a Ui) -> Element<'a, DemoMessage> {
    let press = |id| DemoMessage::Pressed(id);
    host(
        HOST_WIDE,
        Some(HOST_EMPTY),
        feedback::empty_state(
            ui,
            EmptyState::new(
                IconKind::Instances,
                "还没有任何实例",
                "建一个实例，或者从整合包导入。[项目自定 · 估]",
            )
            .action("新建实例", press("empty/create")),
        ),
    )
}

/// §7：三形态（无动作 / 单动作 / 双动作）+ 一屏 scrim。
///
/// `full = true` 表示**这一屏只渲染 dialog 段**（`ULL_FEEDBACK_PART=dialog` 或 `=empty` 之类
/// 单独出图）：此时多摆一屏 scrim，并把三形态都摆开；全家族同屏时留高度给别的段。
fn dialog_block<'a>(ui: &'a Ui, full: bool) -> Element<'a, DemoMessage> {
    let press = |id| DemoMessage::Pressed(id);

    // 页面把自己的**可用宽**递给对话框（组件取 `min(可用宽, 480)`）——窄窗口时靠这一手，
    // 组件自己无法从宿主推（iced 的 Length 没有 min-width，见 `Dialog::width` 的说明）。
    let plain = feedback::dialog(
        ui,
        Dialog::new("已是最新版本", "当前没有可用更新。").width(HOST_WIDE),
    );
    let single = feedback::dialog(
        ui,
        Dialog::new("已导入整合包", "共 42 个模组，缺失 0 个。")
            .confirm("知道了", press("dialog/ok")),
    );
    // 窄宿主那格：可用宽 360 ⇒ `min(480, 360) = 360`，且不得低于官方下限 280。
    let narrow = feedback::dialog(
        ui,
        Dialog::new("窄宿主 360", "可用宽 360 时取 min(480, 360)。").width(NARROW_HOST),
    );
    // 强制 hover：动作件的状态层必须来自 `interaction`（不是组件里自己乘 8%）。
    let double = feedback::dialog(
        ui,
        Dialog::new("删除实例「1.20.1 生存」？", "存档与模组会一起删除。")
            .double(
                ("取消", press("dialog/cancel")),
                ("删除", press("dialog/delete")),
            )
            .state(Interaction::Hovered),
    );

    let mut body = column![caption(
        ui,
        "Dialog · R28 · SurfaceContainerHigh + L3（**本项目不画阴影，见 D-1**）· pad 24（标题下 16 / 正文下 24）· 动作 Primary 间距 8",
    )]
    .spacing(ui.space(Space::X2));

    body = body.push(row![plain, single].spacing(ui.space(Space::X6)));
    // 双动作 + 窄宿主并排：探针要同时量到 480 与 360 两种宽
    body = body.push(row![double, narrow].spacing(ui.space(Space::X6)));
    if full {
        body = body.push(caption(
            ui,
            "Dialog · scrim = 黑@0.32 叠 canvas 的实色（D27 预合成；详见 dialog.rs 模块文档）",
        ));
        body = body.push(host(
            HOST_WIDE,
            Some(HOST_EMPTY),
            feedback::scrimmed(
                ui,
                feedback::dialog(ui, Dialog::new("已是最新版本", "当前没有可用更新。")),
            ),
        ));
    }

    body.into()
}
