//! D43：内置色卡的**固定中性色相**——切卡只换强调色，窗口底色的冷暖不跟着换。
//!
//! ## 这条测试要钉住的五件事
//!
//! 1. **天蓝卡（出厂默认）逐值不变**：固定的中性色相等于天蓝档的中性色相，所以天蓝卡必须与
//!    [`Scheme::baseline`] 的 49 个角色**逐个相等**（M70 定案的不动点，p1 级硬约束）。
//! 2. **八张卡的表面色完全相同**：`surface` 的 CAM16 色相一致（±1°）、实测彩度落在窄带内
//!    （±1.5），且改前那条「9 倍差」被同时量出来当反证（见 `the_old_per_card_tinting...`）。
//! 3. **动态路径不许被顺手改成固定中性**：这条必须走 **`resolve_source` 公开入口**测——
//!    `Card::scheme` 那一层根本没有「取色」这回事，测它等于测空气（复审 r1 p1-1 就是这个：
//!    把 `resolve::try_level` 的取色分支改成固定中性，8 条测试全绿）。见
//!    `resolved_image_scheme_keeps_tinting_from_the_seed`。
//! 4. **WCAG 不因换中性色相而掉档**：8 张卡 × 浅/深两套 × 26 对文字/图标对全部 ≥ 4.5:1。
//! 5. 想让「八卡同底色」的带宽松掉，`the_old_per_card_tinting...` 会红（它量的是改前路径）。
//!
//! ## 为什么容差是这个带宽（不是随手取整）
//!
//! - **色相 ±1°**：八张卡现在共用一个中性调色板，`surface` 是**同一个 hex**，跨卡差实测 0.00°。
//!   容差留 1° 是为了挡住「将来某张卡偷偷换回 TonalSpot」——那种回退的最小偏离是 **43°**
//!   （dusk 280.31° vs 236.86°），1° 的带宽足以把 0 与 43° 分开，同时不误报 8 bit 量化。
//! - **彩度 ±1.5（CAM16，单位与请求值 12 同刻度）**：tone 98 的 8 bit 量化会把「请求 12」
//!   读回成 6.41（浅）/11.70（深），这层压缩是**同一个 hex 的常数偏移**，跨卡比较时抵消；
//!   真正的危险是暖相回退——rose 1.29、amber 1.89，比基线低 4.5~5.1，远超 1.5 的带宽；
//!   紫相（lavender 5.87）只低 0.54，靠上面的色相断言抓（298° 偏 61°）。带宽取 1.5 = 请求
//!   彩度的 1/8，比「改前最小的一张卡偏离基线」还小，所以「9 倍差回来了」一定红。
//!
//! 改前/改后的 `surface` 实测表在子模块 [`dump`]（那里同时是验收图的数据源）。

// 子模块放在 `tests/card_neutral/` 目录里（cargo 只把 `tests/*.rs` 顶层文件当一个 target，
// 目录里的文件不会被当成独立 target；crate 根的子模块默认在 `tests/` 下找，所以显式 `#[path]`）。
#[path = "card_neutral/common.rs"]
mod common;
#[path = "card_neutral/dump.rs"]
mod dump;

use common::{before_d43, flavors, hue_gap};
use unlucky_design::color::{
    Hct, Role, Scheme, SchemeFlavor, DEFAULT_NEUTRAL_CHROMA, NEUTRAL_HUE_DEG,
};
use unlucky_design::palette::card::card_scheme_is_readable;
use unlucky_design::palette::{
    resolve_source, Card, CardKey, ImageInput, Level, CARDS, DEFAULT_CARD, TEXT_PAIRS,
};

/// 天蓝卡就是产品默认色：固定的中性色相**沿用**它，所以 49 个角色必须一位不差。
///
/// 这条是 M88 的 p1 级硬约束。它红了只说明一件事：把「沿用天蓝的中性」实现成了「另选一个中性」。
#[test]
fn sky_card_is_bit_identical_to_the_product_baseline() {
    for flavor in flavors() {
        let baseline = Scheme::baseline(flavor);
        let sky = DEFAULT_CARD.scheme(flavor);
        for (role, got) in sky.iter() {
            assert_eq!(
                got,
                baseline.get(role),
                "天蓝卡（出厂默认）的 {} / {} 变了：{} → {}",
                role.name(),
                flavor.name(),
                baseline.get(role).to_hex(),
                got.to_hex()
            );
        }
    }
}

/// 八张卡的 `surface` 共用同一个中性调色板：色相一致（±1°）、彩度落在窄带（±1.5）。
#[test]
fn preset_cards_share_one_neutral_hue_and_chroma() {
    for flavor in flavors() {
        let hcts: Vec<(&str, Hct)> = CARDS
            .iter()
            .map(|card| {
                (
                    card.key,
                    Hct::from_color(card.scheme(flavor).get(Role::Surface)),
                )
            })
            .collect();
        let reference = hcts[0].1;
        for (key, hct) in &hcts {
            assert!(
                hue_gap(hct.hue, reference.hue) <= 1.0,
                "{key} / {} 的 surface 色相 {:.2}° 偏离基线 {:.2}° 超过 ±1°",
                flavor.name(),
                hct.hue,
                reference.hue
            );
            assert!(
                (hct.chroma - reference.chroma).abs() <= 1.5,
                "{key} / {} 的 surface 实测彩度 {:.2} 偏离基线 {:.2} 超过 ±1.5",
                flavor.name(),
                hct.chroma,
                reference.chroma
            );
        }
        // 更强的口径：八张卡的 surface 是**同一个色值**（不只是「相近」）。
        let surfaces: Vec<String> = CARDS
            .iter()
            .map(|card| card.scheme(flavor).get(Role::Surface).to_hex())
            .collect();
        assert!(
            surfaces.iter().all(|hex| hex == &surfaces[0]),
            "{} 模式下八张卡的 surface 不再逐值相同：{surfaces:?}",
            flavor.name()
        );
    }
}

/// 反证：上面那条断言**量得出**问题。改前的路径浅色档彩度差 9 倍，改成固定中性后只剩 1 倍。
///
/// 没有这条的话，「±1.5 的带宽」只是一句自我声明：谁把断言写松（比如改成 ±20），
/// `preset_cards_share_one_neutral_hue_and_chroma` 照样绿，而窗口又开始逐卡染色。
///
/// ⚠️ **9 倍差只出现在浅色档**：tone 98（近白）的暖相色域装不下彩度 12，rose 只兑现到 1.29；
/// 深色档的 tone 6 反而每张卡都装得下（11.63~12.45，只差 1.07 倍）。深色档的问题不是彩度，
/// 而是**色相**（14.66°~300.57°，整整一个色环）——它在深底上表现为「有的卡偏紫、有的偏绿」。
/// 所以两个模式的反证分开断言，别用一条 `> 8` 覆盖两个模式（那样深色档会红，且说不清为什么）。
#[test]
fn the_old_per_card_tinting_really_was_a_ninefold_gap() {
    let surface_hct = |scheme: &dyn Fn(Card) -> Scheme| {
        CARDS
            .iter()
            .map(|card| Hct::from_color(scheme(*card).get(Role::Surface)))
            .collect::<Vec<_>>()
    };
    let chroma_ratio = |hcts: &[Hct]| {
        let max = hcts.iter().map(|h| h.chroma).fold(f64::MIN, f64::max);
        let min = hcts.iter().map(|h| h.chroma).fold(f64::MAX, f64::min);
        max / min
    };
    let hue_span = |hcts: &[Hct]| {
        // 色相是环：这里只关心「跨了多少」，用最大凸包跨度（简单排序后首尾差）就够粗判。
        let mut hues: Vec<f64> = hcts.iter().map(|h| h.hue).collect();
        hues.sort_by(|a, b| a.partial_cmp(b).expect("有限色相"));
        hues[hues.len() - 1] - hues[0]
    };

    // 浅色档：彩度 9 倍差（改前）→ 1 倍（改后）。
    let old_light = surface_hct(&|card| before_d43(card, SchemeFlavor::Light));
    assert!(
        chroma_ratio(&old_light) > 8.0,
        "改前（浅色）的 surface 彩度差应当能量出「9 倍级」跳变，实测 {:.2} 倍——\
         这条反证已经失效，上面的 ±1.5 带宽就不可信了",
        chroma_ratio(&old_light)
    );
    let new_light = surface_hct(&|card| card.scheme(SchemeFlavor::Light));
    assert!(
        chroma_ratio(&new_light) < 1.5,
        "固定中性后（浅色）八张卡的 surface 彩度差还有 {:.2} 倍",
        chroma_ratio(&new_light)
    );

    // 深色档：彩度本来就一致，问题是色相跨了整个环 → 改后色相跨度收敛到 0。
    let old_dark = surface_hct(&|card| before_d43(card, SchemeFlavor::Dark));
    assert!(
        hue_span(&old_dark) > 180.0,
        "改前（深色）的 surface 色相应当横跨大半个色环，实测 {:.2}°",
        hue_span(&old_dark)
    );
    assert!(
        chroma_ratio(&old_dark) < 1.5,
        "改前（深色）的彩度本来就一致（tone 6 装得下彩度 12），实测 {:.2} 倍",
        chroma_ratio(&old_dark)
    );
    let new_dark = surface_hct(&|card| card.scheme(SchemeFlavor::Dark));
    assert!(
        hue_span(&new_dark) < 1.0,
        "改后（深色）色相跨度 {:.2}°",
        hue_span(&new_dark)
    );
    assert!(chroma_ratio(&new_dark) < 1.5);
}

/// 换卡只换强调族：不随强调色变的角色必须逐值相同，强调族必须真的换。
///
/// 「哪些角色属于强调族」不靠手抄清单，而是**实测**：拿另一个强调色再生成一套方案，值会变的
/// 那些角色就是强调族（primary/secondary/tertiary 全族 + `surface-tint` + `inverse-primary` +
/// 12 个 `*-fixed*` = 26 个），不随强调色变的 23 个（中性表面、描边、error、shadow/scrim）
/// 则必须留在基线——这样将来往 tone 表里加角色也不会漏掉分类。
///
/// ⚠️ 浅色档只有 **23** 个会变：`on-primary` / `on-secondary` / `on-tertiary` 的 tone 是 100，
/// 任何色相都解析成纯白，所以强调色换了它们也不变（深色档的 tone 是 20，会变）。
#[test]
fn cards_only_change_the_accent_families() {
    for flavor in flavors() {
        let baseline = Scheme::baseline(flavor);
        let recolored = Scheme::from_accent(flavor, CardKey::Teal.card().seed);
        let (mut accent_roles, mut neutral_roles) = (Vec::new(), Vec::new());
        for role in Role::ALL {
            if recolored.get(role) == baseline.get(role) {
                neutral_roles.push(role);
            } else {
                accent_roles.push(role);
            }
        }
        assert_eq!(
            (accent_roles.len(), neutral_roles.len()),
            if flavor.is_dark() { (26, 23) } else { (23, 26) },
            "强调族 / 中性族的划分变了（{} 模式下换强调色会动 {} 个）",
            flavor.name(),
            accent_roles.len()
        );

        let sky = DEFAULT_CARD.scheme(flavor);
        for card in CARDS {
            let scheme = card.scheme(flavor);
            for role in &neutral_roles {
                assert_eq!(
                    scheme.get(*role),
                    sky.get(*role),
                    "{} / {} 的 {}（不随强调色变的角色）不该跟着卡变",
                    card.key,
                    flavor.name(),
                    role.name()
                );
            }
            if card.key != DEFAULT_CARD.key {
                assert_ne!(
                    scheme.get(Role::Primary),
                    sky.get(Role::Primary),
                    "{} / {} 的主色与天蓝卡相同：卡没生效",
                    card.key,
                    flavor.name()
                );
            }
        }
    }
}

/// 卡方案 = 用该卡种子做「自定义强调色」：中性表面留在基线，强调族与官方 `from_seed` 对得上。
///
/// 这条把 D43 接到已有的 `tests/official_accent.rs`（192 项官方对账）上：色卡的强调族不是
/// 另一套算法，就是那条已经对过账的路径。
#[test]
fn card_scheme_matches_from_accent_when_chroma_is_the_default() {
    for card in CARDS {
        if card.neutral_chroma != DEFAULT_NEUTRAL_CHROMA {
            continue; // 将来的卡可以带别的中性彩度；那时它的表面色与 baseline 不同是有意的
        }
        for flavor in flavors() {
            assert_eq!(
                card.scheme(flavor),
                Scheme::from_accent(flavor, card.seed),
                "{} / {} 的色卡方案与 from_accent 不一致",
                card.key,
                flavor.name()
            );
        }
    }
}

/// **生成器层**：同一张图，两张不同色卡下强调色相同、底色由图像 seed 决定。
///
/// ⚠️ 只测 `Seed::scheme` 这一层，**不覆盖接线**：取色分支在实际入口
/// `palette::resolve::try_level` 里，接线由 `resolved_image_scheme_keeps_tinting_from_the_seed`
/// 守（复审 r1 p1-1 的病根就是这条用例看着像守护、其实测不到接线）。
#[test]
fn image_scheme_ignores_the_preset_neutral_hue() {
    // 4×4 纯 `#B23A48`（rose 卡的种子色）——色相远离固定的 239.5°，好坏都能分辨。
    let pixels = [0xB2u8, 0x3A, 0x48, 0xFF].repeat(16);
    let seed = unlucky_design::palette::seed_from_rgba(&pixels, 4, 4).expect("纯色图有种子");

    let cards = [CardKey::Sky.card(), CardKey::Teal.card()];
    let schemes: Vec<Scheme> = cards
        .iter()
        .map(|card| seed.scheme(SchemeFlavor::Light, card.neutral_chroma))
        .collect();

    assert_eq!(
        schemes[0].get(Role::Primary),
        schemes[1].get(Role::Primary),
        "取色方案的主色只由图像种子决定，不该跟着当前色卡变"
    );
    assert_eq!(
        schemes[0].get(Role::Surface),
        schemes[1].get(Role::Surface),
        "取色方案的底色由图像种子决定（当前八张卡的中性彩度相同，所以两套方案的表面色相同）"
    );
    let surface_hue = Hct::from_color(schemes[0].get(Role::Surface)).hue;
    // 精确口径：中性**调色板**的色相必须来自图像种子（不是量化后的表面色——tone 98 的
    // 8 bit 量化会让实测色相漂 2.7°~7°，见模块文档；拿它做等式断言只会引入假红）。
    for card in cards {
        let neutral_hue = seed.palettes(card.neutral_chroma).neutral.hue_deg();
        assert!(
            (neutral_hue - seed.hue).abs() < 1.0e-9,
            "取色路径的中性色相 {neutral_hue:.2}° 没有跟着图像种子 {:.2}° 走",
            seed.hue
        );
        assert!(
            hue_gap(neutral_hue, NEUTRAL_HUE_DEG) > 100.0,
            "取色路径的中性色相被拉到了色卡的固定中性色相——动态路径的逐色染色被弄丢了（D43 保留它）"
        );
    }
    // 观感层：实测的表面色相明显靠近图像种子，而不是靠色卡的冷调中性色相。
    assert!(
        hue_gap(surface_hue, seed.hue) < hue_gap(surface_hue, NEUTRAL_HUE_DEG),
        "取色方案的 surface 色相 {surface_hue:.2}° 更接近色卡中性色相而不是图像种子 {:.2}°",
        seed.hue
    );
    for card in cards {
        assert_ne!(
            schemes[0].get(Role::Surface),
            card.scheme(SchemeFlavor::Light).get(Role::Surface),
            "取色方案的表面色与色卡路径相同，说明取色路径没在按图染色"
        );
    }
}

/// **接线的**守护测试（复审 r1 p1-1）：走产品公开入口 [`resolve_source`]，不是 `Card::scheme`。
///
/// 上一版只有生成器层那条用例：实测把 `palette::resolve::try_level` 的取色分支改成固定中性
/// 时**8 条测试全绿**，而同一张 `#B23A48` 图的 `surface` 已从 `#FFF8F7`（暖白，跟着图走）
/// 退化成 `#F6FAFF`（色卡冷白）。断言测 A 层、接线在 B 层——这条把整条链跑通。
#[test]
fn resolved_image_scheme_keeps_tinting_from_the_seed() {
    // 4×4 纯 `#B23A48`：种子色相 15.5°（底色 8 bit 量化后读回 22.09°），离 239.51° 很远。
    let pixels = [0xB2u8, 0x3A, 0x48, 0xFF].repeat(16);
    let flavor = SchemeFlavor::Light;
    let mut resolved: Vec<(CardKey, Scheme)> = Vec::new();
    for key in [CardKey::Sky, CardKey::Teal] {
        let (scheme, provenance) = resolve_source(
            Some(ImageInput::pixels(&pixels, 4, 4)),
            None,
            Some(key),
            flavor,
        );
        assert_eq!(
            provenance.level,
            Level::BackgroundImage,
            "{key:?}：背景图这一级没被用上（回落链本身变了）"
        );
        resolved.push((key, scheme));
    }

    let surface = resolved[0].1.get(Role::Surface);
    // ① 底色在按图染色：必须是暖白，而不是色卡那条冷调中性。这一格就是「变异检测器」。
    assert_eq!(
        surface.to_hex(),
        "#FFF8F7",
        "取色方案的 surface 被拉回了色卡中性色相"
    );
    let surface_hue = Hct::from_color(surface).hue;
    assert!(
        hue_gap(surface_hue, 22.0) < 5.0,
        "surface 色相 {surface_hue:.2}° 没跟着图走"
    );
    assert!(
        hue_gap(surface_hue, NEUTRAL_HUE_DEG) > 100.0,
        "surface 色相 {surface_hue:.2}° 贴到了色卡固定的 {NEUTRAL_HUE_DEG:.2}°"
    );
    // ② 两张卡下强调色与底色都相同：底色只由图像决定，色卡只贡献中性彩度。
    assert_eq!(
        (resolved[0].1.get(Role::Primary), surface),
        (
            resolved[1].1.get(Role::Primary),
            resolved[1].1.get(Role::Surface)
        ),
        "同一张图在两张卡下的主色与底色都该相同（八张卡的中性彩度都是 12）"
    );
    // ③ 与色卡路径明确不同：否则「取色」就等于「色卡 + 换强调色」。
    assert_ne!(
        surface,
        DEFAULT_CARD.scheme(flavor).get(Role::Surface),
        "取色方案的 surface 与天蓝卡的相同——取色路径退化成色卡路径了"
    );
}

/// 8 张卡 × 浅/深 × 26 对：换中性色相后仍全部过 WCAG AA（`wcag::check` 全表），
/// 并把最受中性色相影响的一对（on-surface / surface）钉在实测量级上。
#[test]
fn every_card_scheme_meets_wcag_aa_under_the_fixed_neutral() {
    for card in CARDS {
        for flavor in flavors() {
            let scheme = card.scheme(flavor);
            assert!(
                card_scheme_is_readable(card, flavor),
                "{} / {} 未过 WCAG AA 全表（{} 对逐对断言在 `palette::wcag::check`）",
                card.key,
                flavor.name(),
                TEXT_PAIRS.len()
            );
            // 中性色相变了，最受影响的一对是 on-surface / surface（改前逐卡 16.23~16.44 浅 /
            // 14.30~14.42 深）。改后八张卡同一个底色，这一对也逐卡相同，钉住量级免得漂移。
            // 区间按模式分开：写成 `(浅区间) || (深区间)` 会让浅/深两个值互换也照样绿。
            let body = scheme.contrast(Role::OnSurface, Role::Surface);
            let band = match flavor {
                SchemeFlavor::Light => 16.30..=16.40,
                SchemeFlavor::Dark => 14.28..=14.36,
            };
            assert!(
                band.contains(&body),
                "{} / {}：on-surface/surface = {body:.3}，超出 {} 模式的实测区间 {band:?}",
                card.key,
                flavor.name(),
                flavor.name()
            );
        }
    }
}
