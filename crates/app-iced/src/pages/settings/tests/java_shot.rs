//! 「Java」卡片里的**禁用控件叠的是卡底**——页面级**像素**判据（M237）。
//!
//! ## 为什么非要从页面量
//!
//! `interaction::disabled::tests` 能证明「`Base` 传对了，合成就是对的」，`buttons::tests::base`
//! 能证明「`Button::base` 接到了样式闭包」——两条都**不**证明页面那天真的把 `frame::BASE` 传下去了
//! （M229 的教训：参数在、语义在，8 个调用点全传死 `Role::Surface`）。只有页面自己的像素能证明
//! 「这一屏里的禁用按钮是叠在卡底上画的」。
//!
//! ## 量什么
//!
//! `ULL_JAVA_STATE=scanning` 那一格里有一颗**禁用**的 Filled 按钮（「立即扫描」；「保存」同样是
//! 禁用态，但在这张卡的下半段、1180×760 的一帧里落在折叠线以下，量不到），
//! 它住在 `frame::card` 里（卡底 = `MaterialLayer::Card` = `SurfaceContainerLow`）。
//! 禁用容器 = `OnSurface` @10% 叠在底上，于是「叠卡底」与「叠画布」是**两个不同的逐位值**
//! （实测的 8 位 RGB，十进制写在这里——色值十六进制字面量在源码里是禁的，见
//! `tests/no_hardcoded_tokens.rs`）：
//!
//! | 板 | 叠画布（`Base::CANVAS`） | 叠卡底（`frame::BASE`） | 逐位差 |
//! |---|---|---|---|
//! | Silver（浅） | (226, 226, 227) | (221, 221, 222) | 5/255 |
//! | Ink（深） | (37, 43, 36) | (40, 45, 40) | 3/255 |
//!
//! 差得这么小正是**必须逐位匹配**的理由：肉眼（以及 `±10` 容差的扫描器）分不出这两档，
//! 而「叠错层」这件事本身是确定的。两个值都在测试里**现算**
//! （`interaction::disabled_button_colors`），不写死——换板 / 改令牌时判据跟着走；
//! 上表只是给人读的量测记录。
//!
//! ## 跑法（1180×760 @1.0，`--software-renderer`）
//!
//! ```text
//! EXE=${CARGO_TARGET_DIR:-target}/debug/unlucky-launcher-iced
//! OUT=${CARGO_TARGET_DIR:-target}/m237 && mkdir -p "$OUT"
//!
//! ULL_START_ROUTE=settings ULL_SETTINGS_GROUP=2 ULL_JAVA_STATE=scanning \
//!   $EXE --software-renderer --scale-factor=1 \
//!   --screenshot "$OUT/java-light.png" --screenshot-theme=light
//! ULL_M237_JAVA_SHOT="$OUT/java-light.png" cargo test -p unlucky-app-iced java_shot
//! ```
//!
//! `ULL_SETTINGS_GROUP=2` 是「Java」组（`settings::java::JAVA_GROUP`）；不给就画出 General 组，
//! 那张图上没有禁用填充按钮，判据会拿不到样本并**大声报出来**（不是静默通过）。
//!
//! ## CI 上这条判据是**跳过**的（复审 P1-1）
//!
//! `.github/workflows/ci.yml` 的 test 作业在 ubuntu 与 windows 上都跑
//! `cargo test --workspace --all-targets`，而**出图步骤只给 `options_shot` 备了四张图**
//! （`ULL_DL_*_SHOT`，Windows 一条）。本判据要的这张 Java 图**不在出图清单里**
//! （`grep -r ULL_M237 .github/` 零命中）⇒ 在 CI 上缺图是**预期**，实现里因此**跳过而不是失败**
//! （见 [`not_fed`]：初版写成「CI 就 panic」，`CI=true` 实测 exit 101，一合就红）。
//!
//! 要让它真的进 CI 守门（跨 scope，塔另排）：① ci.yml 加一条 `if: runner.os == 'Windows'` 的
//! 出图步骤（命令见上面的「跑法」）；② 把 `ULL_M237_JAVA_SHOT` 写进 `$GITHUB_ENV`；
//! ③ 把 `not_fed` 里那条 `eprintln!` 换成 `assert!(!(cfg!(windows) && in_ci()), …)`
//! ——与 `pages/download/tests/options_shot/feed.rs::not_fed`（`:104-124`）同一口径。
//!
//! ## 抓得住 / 抓不住
//!
//! - 抓得住：① 页面调用点漏了 `.base(..)`（回落到 `Base::CANVAS`）——卡区里会出现「叠画布」档、
//!   「叠卡底」档消失；② `frame::BASE` 被改成别的角色（判据跟着 `BASE` 走，于是与像素对不上）；
//!   ③ 出图通道换了组 / 换了主题（判据先用卡底色**认出**板，认不出就红）。
//! - 抓不住：① 面板 α < 1 的 GPU 档（本判据读的是 tiny-skia 的图，α 恒 1）；② 卡外的控件
//!   （判据只看卡区）；③ 具体是哪一颗按钮变成了卡底（只数颜色总量，不定位控件）。

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;

use crate::app::components::interaction::{self, Base};
use crate::pages::frame;
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 出图路径（判据的输入）。
const SHOT_ENV: &str = "ULL_M237_JAVA_SHOT";
/// 卡区里「叠卡底」的禁用容器像素下限（两颗 40 高的药丸，扣掉字形与圆角后实测 ≈3100）。
const MIN_HITS: usize = 1000;
/// 「正确档 : 错误档」的最小倍数（抗锯齿会在边缘撞上另一档的精确值，实测个位到十几像素）。
const STRAY_RATIO: usize = 10;
/// 卡区的下限（一张 400+ 宽的卡，实测 ≈32 万像素）——低于它说明这一屏根本没有卡。
const MIN_CARD_PX: usize = 50_000;

fn ui(flavor: SchemeFlavor) -> Ui {
    Ui::with_platform(flavor, Platform::Windows, FAMILIES)
}

/// iced 颜色 → 8 位三元组（与 `to_iced` 同一条口径：`f32` 0..1 四舍五入回 0..255）。
fn rgb(color: iced::Color) -> [u8; 3] {
    let byte = |channel: f32| (channel * 255.0).round().clamp(0.0, 255.0) as u8;
    [byte(color.r), byte(color.g), byte(color.b)]
}

/// 一张 RGB 图（只留本判据要的两件事：尺寸 + 取点）。
struct Shot {
    width: usize,
    height: usize,
    pixels: Vec<[u8; 3]>,
}

impl Shot {
    fn load(path: &std::path::Path) -> Self {
        let bytes = std::fs::read(path).expect("读截图");
        let image = image::load_from_memory(&bytes).expect("解 PNG").to_rgb8();
        let (width, height) = (image.width() as usize, image.height() as usize);
        Self {
            width,
            height,
            pixels: image.pixels().map(|pixel| pixel.0).collect(),
        }
    }

    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }

    /// 逐位相等的像素数（`±10` 容差在这里没用：两档底只差 5 个 RGB 单位）。
    fn count(&self, color: [u8; 3]) -> usize {
        self.pixels.iter().filter(|pixel| **pixel == color).count()
    }

    /// 某颜色像素的外接矩形 `(x0, y0, x1, y1)`（闭区间），一个都没有时 `None`。
    fn bbox(&self, color: [u8; 3]) -> Option<(usize, usize, usize, usize)> {
        let mut found = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.at(x, y) == color {
                    let (x0, y0, x1, y1) = found.unwrap_or((x, y, x, y));
                    found = Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y)));
                }
            }
        }
        found
    }

    /// 矩形内的逐位相等像素数。
    fn count_in(&self, color: [u8; 3], rect: (usize, usize, usize, usize)) -> usize {
        let (x0, y0, x1, y1) = rect;
        (y0..=y1.min(self.height - 1))
            .map(|y| {
                (x0..=x1.min(self.width - 1))
                    .filter(|x| self.at(*x, y) == color)
                    .count()
            })
            .sum()
    }
}

/// 按**卡底色**认出这张图是哪块板（认不出 = 这一屏没有卡 / 主题对不上，直接红）。
fn flavor_of(shot: &Shot) -> SchemeFlavor {
    let mut candidates = Vec::new();
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let card = rgb(ui(flavor).color(frame::BASE.role()));
        let hits = shot.count(card);
        if hits >= MIN_CARD_PX {
            candidates.push((flavor, card, hits));
        }
    }
    assert_eq!(
        candidates.len(),
        1,
        "这一屏里「卡底」色的像素必须恰好落在一种板上，实测 {candidates:?}：\
         检查出图命令是否带了 `--screenshot-theme` 与 `ULL_SETTINGS_GROUP=2`"
    );
    candidates[0].0
}

/// 没喂图时的统一裁决：**一律响亮地跳过**，绝不 panic。
///
/// ## 为什么这里不能照 `options_shot/feed.rs` 那样「CI 就硬失败」（复审 P1-1）
///
/// `options_shot` 那条硬失败（`feed.rs:104-124` 的 `!(cfg!(windows) && in_ci())`）成立的前提是
/// `.github/workflows/ci.yml` 的 test 作业**真的**在 Windows 上出那四张图、并把 `ULL_DL_*_SHOT`
/// 写进 `$GITHUB_ENV`（`ci.yml:92-123`）。本判据的图**不在任何出图清单里**
/// （`grep -r ULL_M237 .github/` 零命中），而 `cargo test --workspace --all-targets` 在 ubuntu 与
/// windows 两个 runner 上都跑 ⇒ 照抄那条口径会让**每个 PR 一亮红**：实测
/// `CI=true cargo test -p unlucky-app-iced --lib java_shot` 会 panic（初版就是这么错的）。
///
/// ## 要在 CI 上真的守门（跨 scope，由塔另排）
///
/// 1. `ci.yml` 加一条 `if: runner.os == 'Windows'` 的出图步骤（命令见本文件模块文档的「跑法」）；
/// 2. 把 `ULL_M237_JAVA_SHOT` 写进 `$GITHUB_ENV`（与 `ULL_DL_*_SHOT` 同一条路）；
/// 3. 那时把本函数末尾那条 `eprintln!` 换成
///    `assert!(!(cfg!(windows) && in_ci()), "守门未喂图：{SHOT_ENV} 没设 …")`
///    —— 与 `options_shot/feed.rs::not_fed` 同一口径：**只有「会出图的平台 + CI」才硬失败**，
///    别的平台缺图是预期。
fn not_fed() {
    let ci = std::env::var_os("CI").is_some_and(|value| !value.is_empty());
    eprintln!("[pages] 页面级像素判据未喂图：{SHOT_ENV} 没设，跳过（跑法见本文件模块文档）。");
    if ci {
        eprintln!(
            "[pages] 当前是 CI：本仓 CI 不出这张图（ci.yml 只给 options_shot 注入 ULL_DL_*_SHOT），\
             所以这里跳过是**预期**，不是漏跑。让它守门的步骤见 not_fed 的文档。"
        );
    }
}

#[test]
fn the_java_card_composites_disabled_controls_over_its_own_base() {
    let Some(path) = std::env::var_os(SHOT_ENV).filter(|value| !value.is_empty()) else {
        not_fed();
        return;
    };

    let shot = Shot::load(std::path::Path::new(&path));
    let flavor = flavor_of(&shot);
    let ui = ui(flavor);

    let over_card = rgb(interaction::disabled_button_colors(&ui, frame::BASE).0);
    let over_canvas = rgb(interaction::disabled_button_colors(&ui, Base::CANVAS).0);
    assert_ne!(
        over_card, over_canvas,
        "{flavor:?}: 叠卡底与叠画布的禁用容器算出来一样 —— 判据自己立不住"
    );

    // 卡区 = 卡底色像素的外接矩形（卡是这一屏最大的同色块）
    let card_color = rgb(ui.color(frame::BASE.role()));
    let rect = shot
        .bbox(card_color)
        .unwrap_or_else(|| panic!("{flavor:?}: 图里找不到卡底色 {card_color:?}"));

    let hits = shot.count_in(over_card, rect);
    let stray = shot.count_in(over_canvas, rect);

    eprintln!(
        "[pages] OK flavor={flavor:?} 卡区 {rect:?}：叠卡底 {over_card:?} {hits} px · \
         叠画布 {over_canvas:?} {stray} px"
    );

    assert!(
        hits >= MIN_HITS,
        "{flavor:?}: 卡区里「叠 {:?}」的禁用容器只有 {hits} px（应 ≥{MIN_HITS}）—— \
         卡片里的禁用控件没把 `frame::BASE` 传下去（还在按画布合成）",
        frame::BASE.role()
    );
    assert!(
        hits > stray * STRAY_RATIO,
        "{flavor:?}: 卡区里「叠卡底」{hits} px，而「叠画布」{stray} px（要求前者 ≥ 后者 ×{STRAY_RATIO}）—— \
         卡区里仍有一部分禁用控件按 `Role::Surface` 合成"
    );
}
