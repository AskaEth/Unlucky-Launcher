//! 面板 / 浮层的**边界**：GPU 档一圈 rim（预合成），CPU 档 1px `outline_variant`（§1.6）。
//!
//! ## 为什么在 `components/` 根目录
//!
//! 这一列原本只服务菜单（M443 的 `inputs/menu/rim.rs`）。M444 把它接到**产品**的弹窗与提示条上，而
//! 「再写一份」是明令禁止的：取值、预合成、两档降级一旦有第二份实现，下一批就会漂掉。所以它升级成组件库
//! 的共享地基（与 `interaction.rs` / `layout.rs` / `divider.rs` 同级），两个消费方（`inputs/menu.rs`、
//! `app/dialog/view.rs`）共用**这一个** `panel_border`。
//!
//! M444 当时还有第三个消费方：提示条（`app/view/toast.rs`）。**M445 把它退掉了**——那块面的底是
//! `InverseSurface`，按主题取色的 rim 落在上面不是「看不见」就是「过重」（数字与判据见「什么时候**不该**
//! 用 rim」）。退法是**不调本函数**（改回只有圆角的 `Border`），**不是**留一个恒关的开关：留开关会让
//! 下一个人以为「配一下就能开」。
//!
//! ## 只消费 `rim` / `outline` 两列
//!
//! 唯一取值入口是 `GlassSpec::for_tier`，它给六列；本文件只读 `rim` 与 `outline`（边界）。其余四列
//! 接不接是**调用方**的决定（阴影走 `menu/shadow.rs`；tint / blur / noise 今天都还没落地），写在各
//! 调用方的模块文档里——本文件不替它们决定，也不在表外另存副本。
//!
//! ## 什么时候**不该**用 rim
//!
//! **反色表面**（`InverseSurface` 底的提示条，以及任何与「容器面」极性相反的深底 / 浅底）上，按主题取色的
//! rim 两头都可能输。这不是实现问题，是**色彩关系**问题：rim 的墨色方向按「容器面」定死（§1.2 顶部光源：
//! 亮档用黑、暗档用白），换到反色底上就与底**同向**——预合成剩下的差只有 α 乘出来的那几级。
//!
//! M444 把 S3 的 rim 接上提示条后实测（产品路径，`ULL_WINDOW_SIZE=1180x1700`，胶囊上沿 (590,1636)）：
//!
//! | 档 | 这块面的边是什么 | 底 → 边（实测） | 差 |
//! |---|---|---|---|
//! | GPU 浅 | S3 rim（黑 α0.07）预合成到 `InverseSurface` | 47,49,51 → 44,45,47 | **−3** |
//! | GPU 深 | S3 rim（白 α0.14）预合成到 `InverseSurface` | 228,234,224 → 231,237,228 | **+3** |
//! | CPU 浅 | §1.6 降级成 1px `OutlineVariant` | 47,49,51 → 190,200,205 | **+148** |
//! | CPU 深 | §1.6 降级成 1px `OutlineVariant` | 228,234,224 → 60,74,61 | **−168** |
//!
//! 同一份 `panel_border` 在**容器面**上是有用的：弹窗 S4 浅档 190,200,205 → **210,210,211**、深档
//! 60,74,61 → **61,66,61**（离卡片底约 −17~−18 级，看得见一点、不刺眼），菜单 M443 实测同一段位（−17 级）。
//! 换到反色底上，GPU 档只剩 −3/+3（画上了、量得出、肉眼看不见），CPU 档那条 outline 反而成了 150 级
//! 的一圈亮边（比弹窗重一个数量级）。**两种失败都不如没有边**，所以 M445 把提示条退回了「只有圆角」的
//! `Border`（`app/view/toast.rs`，直接不调本函数），弹窗与菜单保留。三态对照（M444 前 / M444 后 / 退回后，
//! × 浅深 × CPU/GPU）见 `DevDocs/artifacts/m445/`。
//!
//! ## 判据：先算「rim 色与底色的差」，再决定画不画
//!
//! 预合成之后 `Δ = α × (rim 墨色 − 底)`（逐分量），**这个差不出图就能算**——它同时解释了上面六个数字：
//! 提示条 0.07 × (0 − 47) ≈ **−3**、0.14 × (255 − 228) ≈ **+4**、弹窗 0.08 × (0 − 228) ≈ **−18**。
//!
//! - `|Δ| ≲ 5` ⇒ **画了等于没画**，别画（GPU 提示条落在这一格）；
//! - `|Δ| ≳ 100` ⇒ **太重**：说明这个角色在为「容器面」定的墨色关系下不适用于这块底，要么换角色色、
//!   要么不画（CPU 提示条的 +148/−168 落在这一格）；
//! - 中间段（弹窗 / 菜单的 17~18 级）是有效区间——「看得见一点」正是边界该有的强度。
//!
//! **这条判据抓得住什么**：反色 / 深底上「白费一道边」与「过重一道边」两端，都能在写代码前算出来，
//! 不必等出图才发现（M444 的 −3/+3 正是 α×墨 的直接结果）。**抓不住什么**：①底是**渐变 / 图片**时单一
//! `Δ` 不成立，要逐段算或取最坏处（今天没有这种面的调用方）；②中间段的宽窄是观感阈值不是物理量——它只把
//! M444 实测的 −3/+3、+148/−168 与有效的 17~18 分开，不宣称 6 或 99 一定不好；③CPU 档画的不是 rim 而是
//! `outline_variant`（另一个角色），本判据只告诉你它在这块底上会有多显眼，换不换色仍是令牌表的决定；
//! ④它不回答「这块面该不该有边」，只回答「有的话能不能被看见、是不是过头」。
//!
//! **第五档：底带 α**——上面这条 `Δ` 算得再准也没用，因为「底」本身不确定。见下一节。
//!
//! ## 半透明底：**不许**预合成
//!
//! 预合成要回答「这道边压在哪一层」，而这个答案只在底**不透明**时唯一。底带 α 时，边像素的真实
//! 颜色是「面板 ⊕ rim」再叠到**背后那层**上——背后是什么由调用点决定（遮罩？壁纸？照片？），
//! 本模块看不到，也不该猜。
//!
//! 硬拿「它的不透明角色色」当底塞进来的误差可以**算**出来（M446 给的式子，M449 就是这条）：
//!
//! ```text
//! Δ = (1 − rimα) × (1 − α) × (B − P)
//! ```
//!
//! - `rimα`：rim 自己的 α（S4 浅档 0.08、深档 0.14）；
//! - `α`：面板实际画出去的不透明度（`Ui::layer()` 出的 `Color.a`，壁纸模式下用户可拉到 0.55/0.4）；
//! - `B`：面板背后那层的实际颜色；`P`：面板的**不透明角色色**（`ui.color(role)`）。
//!
//! 预合成算的是「P ⊕ rim」，屏幕上却是「(α·P ⊕ (1−α)·B) ⊕ rim」，两式之差只剩这一项。
//! 弹窗实测（产品路径 GPU 档，`DevDocs/artifacts/m449/`）：浅档 `α = 0.55` 时面板自己掉到 204、
//! 预合成出来的边是 210——比面板**亮** 6 级（暗描边变成白描边），按本式算的误差 **−22 级**；
//! 深档面板 22、边 61（亮带 39 级），误差 **−6 级**。浅深两档的误差方向相反于设计意图，
//! 而且预交代价是「猜错底」，不是「偏一点点」。α = 1.0 时 Δ ≡ 0（那一档四条边界带逐像素比为 0），
//! 所以照旧预合成。
//!
//! 口径：**底 α < 1 ⇒ 不画 rim，由调用方显式降级**（今天唯一的降级 = 只留圆角，见
//! `app/dialog/view/border.rs`）。`rim_over_fill` 的 `debug_assert_eq!(fill.a, 1.0)` 只是护栏。
//!
//! **这条判据抓得住什么**：①「传了一个 α<1 的底」——assert 直接炸；②误差有多大——式子能算，
//! 不必出图（M449 的几十级是算出来的，出图只是复核）。**抓不住什么**：①**传了一个 α=1 但不是
//! 实际绘制底的颜色**——这正是 M449 修的那条缺陷，assert 永远抓不到，只有调用点自己能守；
//! ②背后是渐变 / 图片时 `B` 不是常数，`Δ` 只能取最坏处；③半透明底上「不画边」之后这块面的
//! 观感（面板与背后的色差够不够当边界）——那要出图看，本判据不回答。
//!
//! ## 为什么预合成（根因在依赖层，不在我们的代码）
//!
//! 同一份 `Border`，两个后端的画法**不是一回事**：
//!
//! | 后端 | 画法 | 出处 |
//! |---|---|---|
//! | tiny-skia（CPU 档） | 在填充**之上**描一道（stroke） | `iced_tiny_skia-0.14.1/src/engine.rs:204-246` |
//! | wgpu（GPU 档） | `mixed_color = mix(fill, border_color, mask)`：边框像素上的**填充被边框色替换** | `iced_wgpu-0.14.0/src/shader/quad/solid.wgsl` |
//!
//! 于是把「rim 颜色 × α」原样交给 `Border`，在 GPU 上拿到的**不是**「面板 ⊕ 7% 黑」，而是
//! 「页面 ⊕ 7%×AA 黑」：M443 实测浅色档面板边 238,238,239 → **241,241,242（+3 级，肉眼不可见）**；
//! 同一帧里把 α 临时改成 1.0 重出图，边像素精确变成 (0,0,0) ⇒ **环确实画上了，只是被画成「挖掉填充」**。
//!
//! 所以这里先把 rim 预合成到**面板底**上（`fill ⊕ rim`，α = 1 的实色）再交给 `Border`：两档拿到同一
//! 组数字，也不再依赖后端怎么解释一个半透明边框。混合在 sRGB 分量空间做（iced `Color` 的分量空间），
//! 与 tiny-skia 的 stroke 同口径。
//!
//! ⚠️ **预合成假定两件事**：①传给它的底不透明；②**那个底就是屏幕上那块底**。
//! `debug_assert_eq!(fill.a, 1.0)` 只钉住①，而且是**兜底护栏**、不是兼容手段——M449 修的那条
//! 生产缺陷里，调用方传的确实是不透明色，只是那个色不等于实际绘制底（面板画的是 `Ui::layer()`，
//! 带面板 α），于是预合成算的是另一个色。**②没有任何 assert 能替你检查**，只能由调用点负责。
//!
//! 所以口径是：**半透明底不许预合成，调用方必须显式降级**（今天唯一的降级 = 只留圆角、不画边，
//! 见 `app/dialog/view/border.rs`）。判据与误差公式见下面的「半透明底：**不许**预合成」。

use iced::border::Radius;
use iced::{Border, Color};

use unlucky_design::glass::{GlassSpec, GlassTier, RimEdge, RimSpec};

use crate::theme::{to_iced, Ui};

/// 边界（`container::Style` 的 `border`）：GPU 档 rim，CPU 档 1px `outline_variant`。
///
/// - `tier`：这个面是哪一档（`GlassTier::S3` 浮层 / `S4` 模态…）。形态（三态）、颜色、α、宽度
///   全由它决定，**调用方不许自己拼**；
/// - `fill`：**面板实际画的那个底**（`Ui::layer()` 出的那一份），且必须不透明——GPU 档的 rim 要
///   预合成到它上面（见模块文档）。**带 α 的底不许传给本函数**：预合成没有唯一答案，调用方必须
///   显式降级（今天唯一的降级 = 只留圆角，见 `app/dialog/view/border.rs`）；
/// - `radius`：圆角，由调用方按自己的 `ShapeScale` 求值后传进来（两档同值：§1.6 要求降级只换材质、
///   不换几何），本函数不碰几何。
pub(crate) fn panel_border(ui: &Ui, tier: GlassTier, fill: Color, radius: Radius) -> Border {
    match edge(ui, tier, fill) {
        Some((color, width)) => Border {
            color,
            width,
            radius,
        },
        None => Border {
            radius,
            ..Border::default()
        },
    }
}

/// 这一档要画的描边：`Some((色, 宽度))` 就画，`None` 只留圆角。
fn edge(ui: &Ui, tier: GlassTier, fill: Color) -> Option<(Color, f32)> {
    let glass = GlassSpec::for_tier(tier, ui.render_tier());
    match glass.rim.edge {
        // 四边全画：与 iced 的 `Border` 一一对应；预合成成实色（见模块文档）。
        RimEdge::All => Some((rim_over_fill(ui, glass.rim, fill), glass.rim.width_px)),
        // CPU 档（§1.6）：没有 rim，边界降到 1px `outline_variant`；角色与宽度都由令牌给全。
        RimEdge::None => glass
            .outline
            .map(|outline| (ui.color(outline.color), outline.width_px)),
        // 只画顶边（S2）：iced 的 `Border` 四边同色同宽，表达不了 ⇒ **不画**，宁可没有边也不把它
        // 画成四边。今天没有 S2 调用方；谁要用，得先有一层能画单边的描边（不是本函数的活）。
        RimEdge::Top => None,
    }
}

/// 预合成：`面板底 ⊕ rim`，结果**不透明**（留给渲染器的是一道实色描边）。
///
/// 为什么要预合成、以及它假定了什么，见模块文档；混合在 sRGB 分量空间做。
fn rim_over_fill(ui: &Ui, rim: RimSpec, fill: Color) -> Color {
    // 预合成假定底不透明**且就是屏幕上那块底**：底若半透明，这道边该压在哪一层取决于它背后的东西，
    // 那时必须由调用方显式降级（不画边），不许拿角色色来凑（M449）。
    debug_assert_eq!(
        fill.a, 1.0,
        "rim 预合成假定底不透明：半透明底不许预合成，调用方必须显式降级（见模块文档「半透明底：不许预合成」）"
    );
    let flavor = ui.flavor();
    let ink = to_iced(rim.color.get(flavor));
    let alpha = rim.alpha.get(flavor);
    let rest = 1.0 - alpha;
    Color {
        r: ink.r * alpha + fill.r * rest,
        g: ink.g * alpha + fill.g * rest,
        b: ink.b * alpha + fill.b * rest,
        a: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use unlucky_design::color::{Role, SchemeFlavor};
    use unlucky_design::glass::RenderTier;
    use unlucky_design::shape::ShapeScale;
    use unlucky_design::typography::Platform;

    use crate::theme::Families;

    const FAMILIES: Families = Families {
        text: "Segoe UI",
        display: "Segoe UI",
        mono: "Consolas",
    };

    fn ui(flavor: SchemeFlavor) -> Ui {
        Ui::with_platform(flavor, Platform::Windows, FAMILIES)
    }

    /// CPU 档（§1.6）的边界必须**逐位等于** `Ui::outlined(shape)`——产品弹窗改前用的就是那一句，所以
    /// 那处替换在 CPU 档是逐像素不变（观感差异只出现在 GPU 档的 rim）。
    #[test]
    fn the_cpu_border_is_exactly_ui_outlined() {
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            for shape in [ShapeScale::ExtraLarge, ShapeScale::ExtraSmall] {
                let cpu = ui(flavor).with_render_tier(RenderTier::Cpu);
                let fill = cpu.color(Role::SurfaceContainerHigh);
                let got = panel_border(&cpu, GlassTier::S4, fill, cpu.radius_dp(shape));
                assert_eq!(got, cpu.outlined(shape), "{flavor:?} / {shape:?}");
            }
        }
    }

    /// 只画顶边（S2）表达不了：`Border` 四边同色同宽 ⇒ 宁可不画，也不画成四边。
    #[test]
    fn a_top_only_rim_draws_no_edge_but_keeps_the_radius() {
        let gpu = ui(SchemeFlavor::Light).with_render_tier(RenderTier::Gpu);
        let radius = gpu.radius_dp(ShapeScale::Large);
        let fill = gpu.color(Role::SurfaceContainerHigh);
        let got = panel_border(&gpu, GlassTier::S2, fill, radius);
        assert_eq!(got.width, 0.0, "S2 是顶边 rim：画不出单边就不画");
        assert_eq!(got.radius.top_left, radius.top_left, "不画面 ≠ 不圆角");
    }
}
