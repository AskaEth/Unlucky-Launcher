//! M70：默认色候选（保守蓝 / 冷调蓝 / 浅冷蓝 / 天蓝）——四档的**唯一真源**与 pin。
//!
//! ## 这档候选到底改了什么
//!
//! 四档走的是**同一条** HCT 管道（`Palettes::from_seed_with_neutral_chroma` → `ColorSpec2021`
//! tone 表 → `HctSolver` gamut 映射），差别只有两个入参：
//!
//! | 档 | 种子 | 种子 HCT 色相 | 中性调色板彩度 | 表面 | 主色观感 |
//! |---|---|---|---|---|---|
//! | ① 保守蓝 | `#2196F3` | 253.7° | 6（官方） | 近白/近黑，几乎无色 | 钢蓝 |
//! | ② 冷调蓝 | `#2196F3` | 253.7° | 12 | 冷白/冷黑，带淡蓝调 | 钢蓝 |
//! | ③ 浅冷蓝（备选） | `#87CEEB`（CSS `skyblue`） | 225.7° | 12 | 浅天蓝 | 偏青（teal） |
//! | ④ 天蓝（**出厂默认，用户 2026-11-01 拍板**） | `#3C8DBC` | 239.5° | 12 | 浅天蓝 | 偏蓝的天蓝 |
//!
//! ③ 与 ④ 是同一件事的两种「蓝」：色相越低越偏青（teal），越高越偏钢蓝；
//! 239.5° 正好夹在中间，主色从 ③ 的 `#0C6780` 变成 `#236488`。
//!
//! 两条必须知道的物理事实（写在测试里，免得下次又有人想「把主色调浅一点」）：
//!
//! 1. **种子彩度是废参数**：Tonal Spot 把 primary 彩度钉在 36，种子只贡献色相。
//!    所以「低彩度天蓝种子」得到的色板与「高彩度天蓝种子」**逐像素相同**
//!    （`seed_chroma_never_leaks_into_the_primary_ramp` 在 `palettes.rs` 里也钉了这条）。
//!    ③ 与 ② 的真正差别是**色相**（226° vs 254°），不是彩度。
//! 2. **primary 的 tone 被钉在 40**，所以四档主色都是「中蓝」，不会更浅。
//!    「浅」只能落在中性色上——这就是②/③/④提高中性彩度的原因，也是本项目唯一一处
//!    对官方 Tonal Spot 的**有意偏离**（登记在 `color` 模块文档的「出厂默认色」小节）。
//!
//! ## 换默认色要动哪里
//!
//! `palettes::DEFAULT_SEED` / `palettes::DEFAULT_NEUTRAL_CHROMA` 两行常量，
//! 然后重跑本文件：pin 表会逐角色告诉你哪几个 hex 变了，照抄即可（pin 是**抄本**，不是推导）。

use unlucky_design::color::{
    Color, Hct, Palettes, Role, Scheme, SchemeFlavor, DEFAULT_NEUTRAL_CHROMA, DEFAULT_SEED,
};

/// 一档候选：只有两个变量（种子 + 中性彩度），其余全是官方常数。
struct Candidate {
    /// 短标签（色卡与 CSV 用）。
    tag: &'static str,
    /// 中文档名。
    title: &'static str,
    /// 种子 hex。
    seed_hex: &'static str,
    /// 中性调色板彩度（官方 Tonal Spot 是 6）。
    neutral_chroma: f64,
    /// 9 个展示角色的 **(角色, light, dark)** 抄本。
    pins: [(Role, &'static str, &'static str); 9],
}

/// 四档候选（色卡 `DevDocs/ops/skyseed-candidates/` 用的就是这份数据）。
const CANDIDATES: [Candidate; 4] = [
    Candidate {
        tag: "C1",
        title: "① 保守蓝（中性彩度 6 = 官方）",
        seed_hex: "#2196F3",
        neutral_chroma: 6.0,
        pins: [
            (Role::Primary, "#36618E", "#A0CAFD"),
            (Role::OnPrimary, "#FFFFFF", "#003258"),
            (Role::PrimaryContainer, "#D1E4FF", "#194975"),
            (Role::Secondary, "#535F70", "#BBC7DB"),
            (Role::Tertiary, "#6B5778", "#D6BEE4"),
            (Role::Surface, "#F8F9FF", "#111418"),
            (Role::SurfaceContainerLow, "#F2F3FA", "#191C20"),
            (Role::SurfaceContainerHighest, "#E1E2E8", "#32353A"),
            (Role::Outline, "#73777F", "#8D9199"),
        ],
    },
    Candidate {
        tag: "C2",
        title: "② 冷调蓝（中性彩度 12）",
        seed_hex: "#2196F3",
        neutral_chroma: 12.0,
        pins: [
            (Role::Primary, "#36618E", "#A0CAFD"),
            (Role::OnPrimary, "#FFFFFF", "#003258"),
            (Role::PrimaryContainer, "#D1E4FF", "#194975"),
            (Role::Secondary, "#535F70", "#BBC7DB"),
            (Role::Tertiary, "#6B5778", "#D6BEE4"),
            (Role::Surface, "#F8F9FF", "#0C141E"),
            (Role::SurfaceContainerLow, "#EEF4FF", "#141C26"),
            (Role::SurfaceContainerHighest, "#DBE3F1", "#2D3540"),
            (Role::Outline, "#6D7887", "#8791A1"),
        ],
    },
    Candidate {
        tag: "C3",
        title: "③ 浅冷蓝（中性彩度 12 + 偏青天蓝种子）",
        seed_hex: "#87CEEB",
        neutral_chroma: 12.0,
        pins: [
            (Role::Primary, "#0C6780", "#89D0ED"),
            (Role::OnPrimary, "#FFFFFF", "#003544"),
            (Role::PrimaryContainer, "#BAEAFF", "#004D62"),
            (Role::Secondary, "#4C626B", "#B3CAD5"),
            (Role::Tertiary, "#5B5B7E", "#C4C3EB"),
            (Role::Surface, "#F3FAFF", "#07151B"),
            (Role::SurfaceContainerLow, "#E7F6FE", "#0F1D23"),
            (Role::SurfaceContainerHighest, "#D6E5ED", "#29373D"),
            (Role::Outline, "#687A82", "#81949C"),
        ],
    },
    Candidate {
        tag: "C4",
        title: "④ 天蓝（中性彩度 12 + 夹在中间的天蓝种子）",
        seed_hex: "#3C8DBC",
        neutral_chroma: 12.0,
        pins: [
            (Role::Primary, "#236488", "#93CDF6"),
            (Role::OnPrimary, "#FFFFFF", "#00344C"),
            (Role::PrimaryContainer, "#C8E6FF", "#004C6D"),
            (Role::Secondary, "#4F616E", "#B7C9D8"),
            (Role::Tertiary, "#63597C", "#CDC0E9"),
            (Role::Surface, "#F6FAFF", "#09151C"),
            (Role::SurfaceContainerLow, "#EBF5FF", "#111D25"),
            (Role::SurfaceContainerHighest, "#D8E4EF", "#2B363F"),
            (Role::Outline, "#6A7985", "#84939F"),
        ],
    },
];

/// 四档共同点：种子的 HCT 坐标（色相是候选之间**唯一**有效的差别）。
fn seed_hct(candidate: &Candidate) -> Hct {
    Hct::from_color(Color::from_hex(candidate.seed_hex).expect("候选种子必须是合法 hex"))
}

/// 按候选参数走**产品同一条**派生路径。
fn scheme(candidate: &Candidate, flavor: SchemeFlavor) -> Scheme {
    Scheme::from_palettes(
        &Palettes::from_seed_with_neutral_chroma(
            seed_hct(candidate).to_color(),
            candidate.neutral_chroma,
        ),
        flavor,
    )
}

#[test]
fn candidates_match_their_pins() {
    let mut drift = Vec::new();
    for candidate in &CANDIDATES {
        for (role, want_light, want_dark) in candidate.pins {
            for (flavor, want_hex) in [
                (SchemeFlavor::Light, want_light),
                (SchemeFlavor::Dark, want_dark),
            ] {
                let got = scheme(candidate, flavor).get(role).to_hex();
                if got != want_hex {
                    drift.push(format!(
                        "{}{} / {}/{} 实测 {got}，pin {want_hex}",
                        candidate.tag,
                        if flavor.is_dark() { "-dark" } else { "" },
                        candidate.title,
                        role.name()
                    ));
                }
            }
        }
    }
    assert!(
        drift.is_empty(),
        "候选色值漂了（换默认色后请照抄下面这些 hex 到 pin 表）：\n{}",
        drift.join("\n")
    );
}

#[test]
fn candidates_only_move_the_two_declared_inputs() {
    // 「不绕算法」的证据：四档的调色板彩度与官方 Tonal Spot 逐项一致，
    // 唯一的差别是中性彩度的入参（以及种子色相）。
    let official = Palettes::official_baseline();
    for candidate in &CANDIDATES {
        let p = Palettes::from_seed_with_neutral_chroma(
            seed_hct(candidate).to_color(),
            candidate.neutral_chroma,
        );
        assert_eq!(p.primary.chroma(), official.primary.chroma());
        assert_eq!(p.secondary.chroma(), official.secondary.chroma());
        assert_eq!(p.tertiary.chroma(), official.tertiary.chroma());
        assert_eq!(p.error, official.error, "error 族不该被候选影响");
        assert_eq!(p.neutral.chroma(), candidate.neutral_chroma);
        assert!(
            (p.tertiary.hue_deg() - (p.primary.hue_deg() + 60.0)).abs() < 1.0e-9,
            "{} 的 tertiary 不再是 +60°",
            candidate.tag
        );
    }
}

#[test]
fn the_candidates_are_actually_distinguishable() {
    // 「四档不像同一档」的机器判据。注意 light 的 surface（tone 98）在 8 bit 下已经贴顶，
    // 中性彩度 6→12 会量化到同一个 #F8F9FF——所以 ①② 的判据落在 container 系与 dark 模式上，
    // 这也是色卡上 ①② 的差别主要体现在卡片/浮层与深色模式的原因。
    let light = |c: &Candidate| scheme(c, SchemeFlavor::Light);
    let dark = |c: &Candidate| scheme(c, SchemeFlavor::Dark);
    let (c1, c2, c3, c4) = (
        light(&CANDIDATES[0]),
        light(&CANDIDATES[1]),
        light(&CANDIDATES[2]),
        light(&CANDIDATES[3]),
    );
    for role in [Role::SurfaceContainerLow, Role::SurfaceContainerHighest] {
        assert_ne!(
            c1.get(role),
            c2.get(role),
            "①与②的 {} 相同：中性彩度没起作用",
            role.name()
        );
        assert_ne!(c2.get(role), c3.get(role), "②与③的 {} 相同", role.name());
        // ③④ 的中性彩度相同，差别只允许来自色相
        assert_ne!(c3.get(role), c4.get(role), "③与④的 {} 相同", role.name());
    }
    assert_ne!(
        c3.get(Role::Primary),
        c4.get(Role::Primary),
        "③与④的主色必须真的不同（否则第 4 档白加）"
    );
    assert_ne!(
        dark(&CANDIDATES[0]).get(Role::Surface),
        dark(&CANDIDATES[1]).get(Role::Surface),
        "深色模式的 surface 应当能看出中性彩度差别"
    );
    let hue_gap = |a: &Candidate, b: &Candidate| {
        let (x, y) = (seed_hct(a).hue, seed_hct(b).hue);
        (x - y).abs().min(360.0 - (x - y).abs())
    };
    assert!(
        hue_gap(&CANDIDATES[0], &CANDIDATES[1]) < 0.01,
        "①② 应当只差中性彩度"
    );
    // ③ 偏青 / ④ 居中的分界靠色相：②→③ 与 ③→④ 都要肉眼可分的量级。
    for (i, j) in [(1, 2), (2, 3)] {
        assert!(
            (10.0..=40.0).contains(&hue_gap(&CANDIDATES[i], &CANDIDATES[j])),
            "{}→{} 的色相差应当明显（10°~40°），实测 {}",
            CANDIDATES[i].tag,
            CANDIDATES[j].tag,
            hue_gap(&CANDIDATES[i], &CANDIDATES[j])
        );
    }
}

#[test]
fn product_default_is_the_adopted_candidate() {
    // 用户 2026-11-01 拍板采用 **④ 天蓝**。这条用例就是「默认值 ↔ 候选表」的绑定器：
    // 谁改了 `DEFAULT_SEED`/`DEFAULT_NEUTRAL_CHROMA` 却没同步这里（或反过来），它会立刻红。
    // 断言一律比 **hex / 数值**，这样失败信息里直接能读到「期望 ④、实测几」。
    let adopted = &CANDIDATES[3];
    assert_eq!(
        DEFAULT_SEED.to_hex(),
        "#3C8DBC",
        "DEFAULT_SEED 必须仍是已采用的 ④ 天蓝：期望 #3C8DBC（④），实测 {}（{}）",
        DEFAULT_SEED.to_hex(),
        if DEFAULT_SEED.to_hex() == CANDIDATES[2].seed_hex {
            "③ 浅冷蓝——要改回 ③ 请连偏离登记与 pin 一起改"
        } else {
            "不是候选表里的任何一个种子"
        }
    );
    assert_eq!(
        DEFAULT_SEED.to_hex(),
        adopted.seed_hex,
        "DEFAULT_SEED（{}）与已采用档 {} 的种子（{}）不一致",
        DEFAULT_SEED.to_hex(),
        adopted.tag,
        adopted.seed_hex
    );
    assert_eq!(
        DEFAULT_NEUTRAL_CHROMA, 12.0,
        "DEFAULT_NEUTRAL_CHROMA 必须仍是已采用的 12，实测 {}",
        DEFAULT_NEUTRAL_CHROMA
    );
    assert_eq!(
        DEFAULT_NEUTRAL_CHROMA, adopted.neutral_chroma,
        "DEFAULT_NEUTRAL_CHROMA（{}）与已采用档 {} 的中性彩度（{}）不一致",
        DEFAULT_NEUTRAL_CHROMA, adopted.tag, adopted.neutral_chroma
    );
    for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
        let product = Scheme::baseline(flavor);
        let expected = scheme(adopted, flavor);
        for (role, got) in product.iter() {
            assert_eq!(
                got.to_hex(),
                expected.get(role).to_hex(),
                "产品默认与已采用档 {} 的 {}（{} 模式）不一致",
                adopted.tag,
                role.name(),
                flavor.name()
            );
        }
    }
}

#[test]
fn surface_tint_stays_a_tint() {
    // 门禁：中性彩度提高是「淡蓝调」不是「把窗口染成蓝纸」。surface 的 HCT 彩度必须
    // 明显低于 primary 的 36，且 surface 的 Y 仍 > 0.85（浅色模式还是浅底）。
    for candidate in &CANDIDATES {
        let light = scheme(candidate, SchemeFlavor::Light);
        let surface = light.get(Role::Surface);
        let chroma = Hct::from_color(surface).chroma;
        assert!(
            chroma <= 1.2 * candidate.neutral_chroma,
            "{} 的 surface 彩度 {chroma} 超过中性彩度入参的两倍",
            candidate.tag
        );
        assert!(surface.relative_luminance() > 0.85);
    }
}

#[test]
fn dump_candidates_for_the_color_card() {
    // 色卡脚本读的就是这两行格式（`SEED|` / `ROLE|`）。跑法：
    //   cargo test -p unlucky-design --test skyseed_candidates -- --nocapture
    for candidate in &CANDIDATES {
        let hct = seed_hct(candidate);
        println!(
            "SEED|{}|{}|{}|{}|hue={:.2}|chroma={:.2}",
            candidate.tag,
            candidate.title,
            candidate.seed_hex,
            candidate.neutral_chroma,
            hct.hue,
            hct.chroma
        );
        for flavor in [SchemeFlavor::Light, SchemeFlavor::Dark] {
            let s = scheme(candidate, flavor);
            for role in Role::ALL {
                println!(
                    "ROLE|{}|{}|{}|{}",
                    candidate.tag,
                    flavor.name(),
                    role.name(),
                    s.get(role).to_hex().trim_start_matches('#')
                );
            }
        }
    }
}
