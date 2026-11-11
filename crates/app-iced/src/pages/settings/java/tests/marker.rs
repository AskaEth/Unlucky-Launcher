//! 「当前默认」那颗标记的**可读性**断言（M206）。
//!
//! 起因是 M204 的视觉复测量出来的两个数：Java 页用 `Chip::badge` 表示「哪个 JDK 是默认」，
//! 而 `badge` 走的是**禁用**配色（`OnSurface`@38% 叠 surface）⇒ 字/底只有 **1.84:1（浅）/
//! 2.28:1（深）**，成了整页最不可读的一段；而同一张设置页里相邻的「下载」组表达「选中」用的是
//! `SecondaryContainer` + `OnSecondaryContainer`（**7.21:1**）。同一个「选中」在两页是两种语汇。
//!
//! 这里钉三件事（每条都写过「抓得住 / 抓不住」）：
//!
//! | 断言 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`the_current_default_marker_clears_wcag_aa`] | 当前项标记的字/底对比度掉到 4.5:1 以下（例如 `view::action_chip` 改回 `Chip::badge`、或 `Chip::current` 的 `marker` 位丢了） | 字体粗细 / 字号是否够大（`LabelLarge` 是正文口径，本来就得 4.5）；真截图上的抗锯齿边缘 |
//! | [`the_current_default_marker_speaks_the_download_groups_language`] | 两页的「选中」被拆成两套配色（谁再另起一套立刻红） | 「两页在同一张图上看起来协调」这种整体观感（那要视觉 reviewer 判） |
//! | [`the_disabled_chip_still_looks_disabled`] | 有人为了让当前项好看，顺手把 `badge` / `.disabled()` 也接到选中配色上（「忙/不可用」于是看起来可点） | 禁用态够不够灰的美学判断——只钉到令牌：字 38%、容器 12% |
//!
//! **它们都不碰像素**：颜色是「角色 → 令牌 → 对比度」这条链上的值断言，真图另存
//! `DevDocs/artifacts/m206/`（D30 §2.1 三件套）。

use iced::Color;

use unlucky_design::color::{Color as TokenColor, Role, SchemeFlavor};
use unlucky_design::palette::wcag::AA_TEXT;
use unlucky_design::typography::Platform;
use unlucky_i18n::Lang;

use crate::app::components::interaction;
use crate::app::components::selection::paint::ChipPaint;
use crate::app::components::selection::Chip;
use crate::theme::{Families, Ui};

use super::super::seed;
use super::super::state::State;
use super::super::view::action_chip;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

const FLAVORS: [SchemeFlavor; 2] = [SchemeFlavor::Light, SchemeFlavor::Dark];

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// iced 的 sRGB 分量 → 令牌色（[`crate::theme::to_iced`] 的逆；`to_iced` 是从 8 bit 来的，
/// 所以来回一趟无误差）。
fn token(color: Color) -> TokenColor {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    TokenColor::rgba(q(color.r), q(color.g), q(color.b), q(color.a))
}

/// 一颗 chip 的「字色 vs 底色」。容器为空（透明）时按 surface 算——chip 的透明态本来就压在
/// 卡片底上，这条口径与 `wcag::check` 的 `(OnSurfaceVariant, Surface)` 一致。
fn ink_ratio(paint: &ChipPaint, ui: &Ui) -> f32 {
    let background = paint.frame.fill.unwrap_or_else(|| ui.color(Role::Surface));
    token(paint.label).contrast_ratio(token(background))
}

/// 截图种子里那两行 JDK（`ULL_JAVA_STATE=two` 用的同一份），其中**第二行是当前默认**。
fn seeded() -> State {
    seed::build("two").expect("种子 two 必须存在")
}

// ------------------------------------------------- ① 对比度：当前项标记必须过 AA 正文

/// 「当前默认」那颗标记的字色与底色 ≥ **4.5:1**（WCAG 2.x AA 正文口径），浅深两档都要过。
///
/// 改前实测（M204，`Chip::badge` 的禁用配色）：**1.84:1 浅 / 2.28:1 深**；改后（本断言钉的）：
/// **7.21:1 浅 / 7.21:1 深**——与下载组的选中 chip 同一个数，因为就是同一套角色。
///
/// 变异证明（只改实现侧）：把 `view::action_chip` 的 `Chip::current` 换回 `Chip::badge`，
/// 这条立刻红（比值掉回 1.84 / 2.28）；把 `Chip::current` 里的 `marker: true` 改成 `false`
/// 也一样红（它会退回「无消息 ⇒ 禁用」那条路）。
#[test]
fn the_current_default_marker_clears_wcag_aa() {
    let catalog = Lang::ZhCn.catalog();
    let state = seeded();

    for flavor in FLAVORS {
        let ui = ui(flavor);
        // 种子里选中的是第二行（`C:\Program Files\Microsoft\jdk-21.0.4`）。
        let marker_row = state
            .runtimes
            .iter()
            .find(|runtime| super::super::state::same_path(&runtime.binary, &state.selected))
            .expect("种子里必须有一行是当前默认");
        let paint = action_chip(catalog, &state, marker_row).paint(&ui);

        // 先量数（这条是任务书点名要的那条）：谁把配色改回禁用灰，红在这里。
        let ratio = ink_ratio(&paint, &ui);
        assert!(
            ratio >= AA_TEXT,
            "{flavor:?}：「当前默认」的字/底只有 {ratio:.2}:1，低于 AA 正文 {AA_TEXT}:1 —— \
             这正是 M206 要修的那段（改前 1.84 浅 / 2.28 深）"
        );

        // 再钉角色名：比值恰好过线也可能是碰巧换了个色，这里把「用的是哪两个角色」写死。
        assert_eq!(
            paint.frame.fill,
            Some(ui.color(Role::SecondaryContainer)),
            "{flavor:?}：当前项标记的容器必须是 SecondaryContainer（与下载组选中 chip 同族），\
             不是禁用态的 12% 灰"
        );
        assert_eq!(
            paint.label,
            ui.color(Role::OnSecondaryContainer),
            "{flavor:?}：当前项标记的字色必须是 OnSecondaryContainer，不是 OnSurface@38%"
        );
    }
}

// ------------------------------------------------- ② 同族：两页表达「选中」只用一套颜色

/// Java 的「当前默认」与下载组的选中 chip **必须逐字段同色**。
///
/// 下载组那一侧（M201 已验收）是 `Chip::filter(option, selected=true, msg)`；这里照原样构造一颗
/// ——**不是**照着角色名手抄一遍，所以谁把其中一页换套配色，这条就红。
///
/// 变异证明：把 `Chip::current` 的容器改成 `PrimaryContainer`（或任何另一个角色），红。
#[test]
fn the_current_default_marker_speaks_the_download_groups_language() {
    let catalog = Lang::ZhCn.catalog();
    let state = seeded();

    for flavor in FLAVORS {
        let ui = ui(flavor);
        let marker_row = state
            .runtimes
            .iter()
            .find(|runtime| super::super::state::same_path(&runtime.binary, &state.selected))
            .expect("种子里必须有一行是当前默认");
        let marker = action_chip(catalog, &state, marker_row).paint(&ui);

        // 下载组的分段单选：选中的那颗就是 `filter(.., true, ..)`。
        let download_selected = Chip::<()>::filter("8", true, ());
        assert_eq!(
            marker,
            download_selected.paint(&ui),
            "{flavor:?}：两页的「选中」必须是同一套视觉语汇（M206）"
        );

        // 反面：另一行（非默认）是可点的「设为默认」——未选中 filter，且**不是**禁用灰。
        let other = state
            .runtimes
            .iter()
            .find(|runtime| !super::super::state::same_path(&runtime.binary, &state.selected))
            .expect("种子里必须有一行不是默认");
        let actionable = action_chip(catalog, &state, other).paint(&ui);
        assert_eq!(
            actionable.label,
            ui.color(Role::OnSurfaceVariant),
            "{flavor:?}：「设为默认」是可点的未选中 filter（OnSurfaceVariant），不是禁用"
        );
        assert_ne!(
            actionable.frame.fill, marker.frame.fill,
            "{flavor:?}：可点与当前项必须分得开（一个透明描边 / 一个填充）"
        );
    }
}

// ------------------------------------------------- ③ 反向：禁用态仍然要看起来禁用

/// 修当前项**不许**顺手把禁用语义改亮：`Chip::badge` / `.disabled()` 仍走
/// 「字 `OnSurface`@38% + 容器 `OnSurface`@12%」，且与当前项标记**肉眼可分的两档**。
///
/// 变异证明：把 `Chip::paint` 里「没消息 = 禁用」那条改成走选中配色（或把 `badge` 改成
/// `marker: true`），这条红——那会让「下载进行中，安装按钮灰掉」看起来像可点。
#[test]
fn the_disabled_chip_still_looks_disabled() {
    let ui_light = ui(SchemeFlavor::Light);
    let ui_dark = ui(SchemeFlavor::Dark);

    for ui in [&ui_light, &ui_dark] {
        let flavor = ui.flavor();
        // 下载组里「忙 ⇒ 灰掉」的那颗与 Java 的「默认」标记在语义上相反，必须画得不一样。
        let disabled = Chip::<()>::filter("安装", true, ()).disabled().paint(ui);
        let marker = Chip::<()>::current("默认").paint(ui);

        let ink38 =
            interaction::disabled_content_over_role(ui, Role::Surface, Role::OnSurface, false);
        let container12 = interaction::disabled_colors(ui, Role::Surface).0;

        assert_eq!(
            disabled.label, ink38,
            "{flavor:?}：禁用字仍是 OnSurface@38%"
        );
        assert_eq!(
            disabled.frame.fill,
            Some(container12),
            "{flavor:?}：禁用容器仍是 OnSurface@12%（选中禁用的填充走 38%，见 selection::tests）"
        );
        assert_ne!(
            disabled.label, marker.label,
            "{flavor:?}：禁用与当前项的文字色不许相同"
        );
        assert_ne!(
            disabled.frame.fill, marker.frame.fill,
            "{flavor:?}：禁用与当前项的容器色不许相同"
        );

        // 禁用档**不**承诺 4.5:1（它是「用不了」的语汇，官方 α 就是 38%）：这条只记数值，
        // 不当门槛——否则将来有人真去调亮禁用态时会被一条假门禁拦住。
        let ratio = ink_ratio(&disabled, ui);
        assert!(
            ratio < AA_TEXT,
            "{flavor:?}：禁用档意外地过 AA 了（{ratio:.2}:1）——若是有意调亮，请同步改这条注释与 \
             `interaction::disabled` 的令牌口径，别让「禁用」和「当前项」两档混成一个"
        );
    }

    // `Chip::badge` 仍是禁用档（它就是「没消息 ⇒ 禁用」那条路的产物）。
    let badge = Chip::<()>::badge("LTS").paint(&ui_light);
    let disabled = Chip::<()>::filter("LTS", true, ())
        .disabled()
        .paint(&ui_light);
    assert_eq!(badge, disabled, "`Chip::badge` 与 `.disabled()` 必须同色");
}
