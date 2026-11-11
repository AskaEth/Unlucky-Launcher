//! 进度指示器（`research/12b` §12）：`linear` 与 `circular`，各有确定态与不定态。
//!
//! ## 两种形态、四种态
//!
//! | 形态 | 确定态 | 不定态 |
//! |---|---|---|
//! | [`linear`] | 轨道 `SurfaceContainerHighest` + 指示条 `Primary`，厚 **4**、圆角 full | 官方 1750ms **双段**追逐（[`phase_segments`]） |
//! | [`circular`] | 40 环 + 4 线宽，弧长 = 数值 × 360° | 官方 6000ms（弧长 0.1↔0.87 + 旋转 **0→1440°**：全局 1080° 线性 + 关键帧 360°，[`CircularGeometry::indeterminate`]） |
//!
//! **轨道角色为什么是容器面（M248，与 A1 的登记差异）**：A1 的
//! `LinearProgressIndicatorTokens.TrackColor` 是 `SecondaryContainer`，本项目取
//! `SurfaceContainerHighest`。口径是「轨道 = 进度还剩多少的**背景尺**，属于表面层级；
//! 强调色只属于指示条」。取角色而不是色值，所以换品牌板 / 换 D38 取色方案它跟着变
//! （判据见 `tests/progress.rs` 的 `track_is_the_highest_container_step_and_follows_the_scheme`）。
//! 12b §12.2 的旧值仍留在 `geometry.rs` 的参考值表里当对照。
//!
//! ## 为什么不定态要调用方给 `phase`
//!
//! 契约 §2：组件不持有时钟。`phase`（0..1 的一个周期）由页面按 `Subscription` 推进；
//! harness 因此能拍**确定**的中间帧（否则每张图都是抓阄）。周期时长在几何里
//! （[`LinearGeometry::cycle_ms`] / [`CircularGeometry::cycle_ms`]），调用方按它换算。
//!
//! ## CPU 降级路径（D27）
//!
//! 本组件**零半透明、零模糊**：轨道/指示条都是不透明实色，环形是 `stroke` 实色。
//! 波浪形态（线性振幅 3 / 波长 40，环形振幅 1.6 / 波长 15，`[源]`）**未实现**，
//! 用直线段/等弧长替代——这是 `12b §12.4` 明确允许的登记项（见报告「偏离登记」）。
//!
//! ## 环形为什么手搓 SVG
//!
//! iced 的 `canvas` feature 没开（`Cargo.toml` 不归本 mission），而圆弧用容器边框画不出来。
//! 令牌图标的做法（24 网格 SVG path + `currentColor` + 内容哈希缓存）同样适用于几何图元：
//! 一份 handle 渲染两层（轨道一层、弧一层），相位不变时 iced 侧命中缓存。

use iced::border::Radius;
use iced::widget::{container, row, svg, Space};
use iced::{Background, Border, Element, Length};

use unlucky_design::color::Role;

use crate::theme::Ui;

use super::super::layout;
use super::geometry::{phase_segments, CircularGeometry, LinearGeometry};

/// `FillPortion` 的比例分辨率：0..1 的占比 × 1000 取整（1/1000 像素级，够用且不溢出 u16）。
const PORTIONS: f32 = 1000.0;

/// 弧长小于这个角度就不画激活层（圆头在 0 弧长上会画出一个点，看着像「已完成」）。
const MIN_SWEEP_DEG: f32 = 1.0;

/// 100% 时弧首尾重合，`A` 命令画不出整圆 ⇒ 超过这个角度改画整环。
const FULL_SWEEP_DEG: f32 = 359.9;

/// 进度的取值：确定（0..1）或不定（相位 0..1）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    /// 确定态：0..1。
    Determinate(f32),
    /// 不定态：一个周期内的相位（0..1，可越界，采样时按周期取模）。
    Indeterminate(f32),
}

/// 一个进度指示器的取值（**不持有任何状态**）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    amount: Amount,
}

impl Progress {
    /// 确定态：`value` 越界会被夹到 0..1（0% 只画轨道）。
    pub fn determinate(value: f32) -> Self {
        Self {
            amount: Amount::Determinate(value.clamp(0.0, 1.0)),
        }
    }

    /// 不定态：`phase` 是周期内的相位（调用方按 `cycle_ms` 推进）。
    pub fn indeterminate(phase: f32) -> Self {
        Self {
            amount: Amount::Indeterminate(phase),
        }
    }

    /// 取值（断言用；生产路径不读它，内部直接 match 字段）。
    #[cfg(test)]
    pub fn amount(self) -> Amount {
        self.amount
    }

    /// 本帧要画的占轨道比例区间（0..1）。
    ///
    /// 确定态 1 段、不定态 2 段（官方双线）；区间可能为空（相位在「两段都在屏幕外」的时刻），
    /// 调用方按 `end > start` 过滤。
    pub fn spans(self) -> Vec<(f32, f32)> {
        match self.amount {
            Amount::Determinate(value) => vec![(0.0, value.clamp(0.0, 1.0))],
            Amount::Indeterminate(phase) => phase_segments(phase).to_vec(),
        }
    }
}

/// 把 0..1 的区间**排序 + 合并重叠**，摊成「间隙 / 指示条」交替的列表（总长恒为 1.0）。
///
/// ## 为什么必须排序（复审 P1-A，视觉 r1 的 p1）
///
/// [`phase_segments`] 返回的两段**不保证有序**：第一线的前缘抢跑 250ms，所以它的区间在右，
/// 第二线从 0 起跑的区间在左。照顺序铺 `FillPortion` 会把左段接到右段**之后**，并且
/// `FillPortion` 之和一旦 ≠ 1000，iced 就按总和归一 —— 位置和宽度一起被压扁。
/// 实测 phase 0.55：本应两段 `[0.003,0.079]` + `[0.355,0.860]`（x 689..713 / 802..963），
/// 实际被画成一段 100px 的台阶（x 749..848），且 **34/100 个相位**上总和 >1（最大 1.99）。
///
/// 返回的每项是 `(是不是指示条, 占比)`；间隙项也显式给出来，才能断言「总和 = 1.0」。
pub fn layout_spans(spans: &[(f32, f32)]) -> Vec<(bool, f32)> {
    let mut clipped: Vec<(f32, f32)> = spans
        .iter()
        .map(|(start, end)| (start.clamp(0.0, 1.0), end.clamp(0.0, 1.0)))
        .filter(|(start, end)| end > start)
        .collect();
    clipped.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut merged: Vec<(f32, f32)> = Vec::new();
    for (start, end) in clipped {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }

    let mut items = Vec::new();
    let mut cursor = 0.0_f32;
    for (start, end) in merged {
        if start > cursor {
            items.push((false, start - cursor));
        }
        items.push((true, end - start));
        cursor = end;
    }
    if cursor < 1.0 {
        items.push((false, 1.0 - cursor));
    }
    items
}

/// 占比 → `FillPortion` 整数份额，**总和恒为 `PORTIONS`**（累计舍入，最后一项吃掉残差）。
///
/// 为什么不用「每项各自 round」：各项独立取整后总和可能是 999 或 1001，iced 按总和归一，
/// 就又回到 P1-A 那个「整条被压扁」的坑。这里让每个**累计边界**取整，最后一项补足，
/// 于是任意相位下份额之和都精确等于 1000。
pub fn portion_shares(items: &[(bool, f32)]) -> Vec<(bool, u16)> {
    let mut shares = Vec::with_capacity(items.len());
    let mut accumulated = 0.0_f32;
    let mut previous = 0_u16;
    for (index, (is_bar, fraction)) in items.iter().enumerate() {
        accumulated += fraction;
        let target = if index + 1 == items.len() {
            PORTIONS as u16
        } else {
            (accumulated * PORTIONS).round() as u16
        };
        let share = target.saturating_sub(previous);
        previous = target;
        shares.push((*is_bar, share));
    }
    shares
}

/// 进度家族的取色角色 `(轨道, 指示条)`。
///
/// **为什么要这个函数**：`Element` 是黑盒、`container::Style` 与 `svg::Style` 都是闭包，
/// 两者都不可内省（同 `buttons::icon_style_spec` 的教训：把 `stroke` 写死成 0 时纯函数测试照样绿）。
/// 取色点收成这一对角色，单测才能钉住「取的是哪个角色」而不只是「渲染没 panic」。
///
/// 轨道 = `SurfaceContainerHighest`（比画布高一档以上的容器面，与输入框底同层）；指示条 = `Primary`。
/// 环形与线性共用同一对：一根进度条两套配色口径就没人说得清哪个才是「进度」。
pub(crate) const fn progress_roles() -> (Role, Role) {
    (Role::SurfaceContainerHighest, Role::Primary)
}

/// `linear()` 真正塞进样式闭包的两个颜色（单测断言的就是它，口径同 `StyleSpec`）。
pub(crate) fn linear_colors(ui: &Ui) -> (iced::Color, iced::Color) {
    let (track, active) = progress_roles();
    (ui.color(track), ui.color(active))
}

/// 环形进度的取色（与 [`linear_colors`] 同源）。
pub(crate) fn circular_colors(ui: &Ui) -> (iced::Color, iced::Color) {
    linear_colors(ui)
}

/// 线性进度条（轨道 + 指示条，厚 4、圆角 full）。
///
/// 轨道宽 `Fill`（由宿主决定），指示条按占比分到 `FillPortion` 上——**不需要知道轨道像素宽**，
/// 所以 50% 在任何宿主宽度下都精确是半条。
pub fn linear<'a, M: 'a>(ui: &'a Ui, progress: Progress) -> Element<'a, M> {
    let geometry = LinearGeometry::of(ui);
    let (track_color, active_color) = linear_colors(ui);

    let mut bar = row![]
        .width(Length::Fill)
        .height(Length::Fixed(geometry.thickness));
    for (is_bar, share) in portion_shares(&layout_spans(&progress.spans())) {
        if share == 0 {
            continue;
        }
        bar = bar.push(if is_bar {
            container(Space::new().width(Length::Fill).height(Length::Fill))
                .width(Length::FillPortion(share))
                .height(Length::Fill)
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(active_color)),
                    border: Border {
                        radius: Radius::new(geometry.radius),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .into()
        } else {
            spacer(share)
        });
    }

    container(bar)
        .width(Length::Fill)
        .height(Length::Fixed(geometry.thickness))
        .style(move |_theme| container::Style {
            background: Some(Background::Color(track_color)),
            border: Border {
                radius: Radius::new(geometry.radius),
                ..Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

/// 环形进度（40 直径 / 4 线宽；确定态画弧，不定态按官方节奏扫）。
pub fn circular<'a, M: 'a>(ui: &'a Ui, progress: Progress) -> Element<'a, M> {
    let geometry = CircularGeometry::of(ui);
    let (sweep, rotation) = match progress.amount {
        Amount::Determinate(value) => (value.clamp(0.0, 1.0) * 360.0, 0.0),
        Amount::Indeterminate(phase) => {
            // 圈数直接乘 360（不做 rem_euclid）：每周期 4 圈是官方口径，截断就丢转速。
            let (sweep_fraction, turns) = geometry.indeterminate(phase);
            (sweep_fraction * 360.0, turns * 360.0)
        }
    };

    let (track_color, active_color) = circular_colors(ui);
    let track = glyph(&geometry, ring_source(&geometry), track_color);

    // 0% 只画轨道（不画弧，也不画圆头小点）；100% 画整环（`A` 命令的首尾重合画不出整圆）
    let active = if sweep < MIN_SWEEP_DEG {
        None
    } else if sweep >= FULL_SWEEP_DEG {
        Some(glyph(&geometry, ring_source(&geometry), active_color))
    } else {
        Some(glyph(
            &geometry,
            arc_source(&geometry, sweep, rotation),
            active_color,
        ))
    };

    let mut layers = vec![track];
    layers.extend(active);

    // Q9 的同族：几何图元也必须落在**固定尺寸**的盒子里，否则 usvg 会按 Contain 撑满父级。
    layout::fixed(
        geometry.diameter,
        geometry.diameter,
        iced::widget::stack(layers),
    )
}

/// 两段之间/两端的透明占位。
fn spacer<'a, M: 'a>(share: u16) -> Element<'a, M> {
    Space::new()
        .width(Length::FillPortion(share))
        .height(Length::Fill)
        .into()
}

/// 一层几何图元（固定尺寸 + `currentColor` 上色）。
fn glyph<'a, M: 'a>(
    geometry: &CircularGeometry,
    source: String,
    color: iced::Color,
) -> Element<'a, M> {
    svg(svg::Handle::from_memory(source.into_bytes()))
        .width(Length::Fixed(geometry.diameter))
        .height(Length::Fixed(geometry.diameter))
        .style(move |_theme, _status| svg::Style { color: Some(color) })
        .into()
}

/// 整环（轨道用，也用作 100% 的激活层）。
fn ring_source(geometry: &CircularGeometry) -> String {
    let (size, thickness) = (geometry.diameter, geometry.thickness);
    let center = size / 2.0;
    let radius = geometry.arc_radius();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" \
         viewBox=\"0 0 {size} {size}\"><circle cx=\"{center}\" cy=\"{center}\" r=\"{radius}\" \
         fill=\"none\" stroke=\"currentColor\" stroke-width=\"{thickness}\"/></svg>"
    )
}

/// 一段圆弧（`rotation` 起、扫 `sweep_deg`；0° = 12 点、顺时针 = M3 口径）。
fn arc_source(geometry: &CircularGeometry, sweep_deg: f32, rotation_deg: f32) -> String {
    let (size, thickness) = (geometry.diameter, geometry.thickness);
    let center = size / 2.0;
    let radius = geometry.arc_radius();
    let (x0, y0) = point(center, radius, rotation_deg);
    let (x1, y1) = point(center, radius, rotation_deg + sweep_deg);
    let large = u8::from(sweep_deg > 180.0);
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{size}\" height=\"{size}\" \
         viewBox=\"0 0 {size} {size}\"><path d=\"M {x0:.3} {y0:.3} A {radius} {radius} 0 \
         {large} 1 {x1:.3} {y1:.3}\" fill=\"none\" stroke=\"currentColor\" \
         stroke-width=\"{thickness}\" stroke-linecap=\"round\"/></svg>"
    )
}

/// 极角（0° = 12 点，顺时针）→ 画布坐标（y 向下，所以向上取负）。
fn point(center: f32, radius: f32, deg: f32) -> (f32, f32) {
    let rad = deg.to_radians();
    (center + radius * rad.sin(), center - radius * rad.cos())
}
