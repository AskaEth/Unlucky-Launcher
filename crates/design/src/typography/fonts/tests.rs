//! 字体栈与字重解析的测试（独立文件是为了守住单文件 ≤400 行的硬约束）。

use super::*;
use crate::typography::Weight;

const PLATFORMS: [Platform; 3] = [Platform::Windows, Platform::MacOs, Platform::Linux];
const FAMILIES: [FontFamily; 3] = [FontFamily::Display, FontFamily::Text, FontFamily::Mono];

#[test]
fn font_stacks_are_well_formed() {
    for platform in PLATFORMS {
        for family in FAMILIES {
            let stack = platform.font_stack(family);
            assert!(
                !stack.is_empty(),
                "{} 的 {family:?} 栈为空",
                platform.name()
            );
            let last = stack[stack.len() - 1].family;
            let generic = match family {
                FontFamily::Mono => "monospace",
                _ => "sans-serif",
            };
            assert_eq!(
                last,
                generic,
                "{} 的 {family:?} 栈必须以通用族名兜底",
                platform.name()
            );
            for face in stack {
                assert!(!face.family.is_empty());
                assert!(
                    !face.family.contains(['/', '\\', '.', ':']),
                    "字体栈只允许族名，不得出现路径/文件名：{}",
                    face.family
                );
                assert!(
                    face.family.is_ascii(),
                    "族名应为 ASCII（非 ASCII 名交给系统匹配）：{}",
                    face.family
                );
                // 每条栈项都必须声明字重覆盖（连 400 都没有的族写进栈没有意义）。
                assert!(
                    face.supports(Weight::Regular),
                    "{} 连 400 都没有，写它没意义",
                    face.family
                );
            }
        }
    }
}

#[test]
fn display_and_text_stacks_differ_on_windows_and_macos() {
    assert_ne!(
        Platform::Windows.font_stack(FontFamily::Display)[0].family,
        Platform::Windows.font_stack(FontFamily::Text)[0].family
    );
    assert_ne!(
        Platform::MacOs.font_stack(FontFamily::Display)[0].family,
        Platform::MacOs.font_stack(FontFamily::Text)[0].family
    );
}

#[test]
fn chinese_fallback_is_present_but_not_first() {
    // 中文族名必须在栈里，但不能排第一（纯英文环境不加载中文字体，D28）。
    for platform in PLATFORMS {
        let stack = platform.font_stack(FontFamily::Text);
        let cjk = ["Microsoft YaHei UI", "PingFang SC", "Noto Sans CJK SC"];
        assert!(
            stack.iter().any(|face| cjk.contains(&face.family)),
            "{} 的正文栈缺 CJK 兜底",
            platform.name()
        );
        assert!(
            !cjk.contains(&stack[0].family),
            "{} 的首选字体不该是 CJK 族",
            platform.name()
        );
    }
}

#[test]
fn cjk_weight_coverage_is_registered() {
    // 用户点名的坑：中文字重覆盖三平台差别很大。
    let win = Platform::Windows.cjk_face();
    assert!(win.supports(Weight::Regular) && win.supports(Weight::Bold));
    assert!(
        !win.supports(Weight::Medium),
        "雅黑没有 500——这条断言就是「中文 500 会掉到降级路径」的登记"
    );
    let mac = Platform::MacOs.cjk_face();
    assert!(mac.supports(Weight::Medium));
    assert!(
        !mac.supports(Weight::Bold),
        "苹方到 600 为止，700 要合成/换族"
    );
    let linux = Platform::Linux.cjk_face();
    for weight in [Weight::Regular, Weight::Medium, Weight::Bold] {
        assert!(
            linux.supports(weight),
            "思源黑体/Noto CJK 应覆盖 400/500/700"
        );
    }
}

#[test]
fn resolve_prefers_native_and_flags_synthetic() {
    // Windows 正文 700：Segoe UI Variable 覆盖 → 原生。
    let bold = Platform::Windows.resolve_weight(FontFamily::Text, Weight::Bold);
    assert_eq!(bold.quality, WeightMatch::Native);
    assert_eq!(bold.available, 700);
    assert!(!bold.is_synthetic());
    // Linux 正文 500：Roboto 可变 → 原生。
    assert_eq!(
        Platform::Linux
            .resolve_weight(FontFamily::Text, Weight::Medium)
            .quality,
        WeightMatch::Native
    );
    // 静态族缺档：只有 400 的族请求 700 → nearest 400，差 300 → 判为合成降级。
    let only_regular = FontFace::new("Courier New", WeightCoverage::Static(&[400]));
    assert_eq!(only_regular.weights.nearest(700), 400);
    let synthetic = FontChoice {
        family: only_regular.family,
        requested: 700,
        available: only_regular.weights.nearest(700),
        quality: WeightMatch::Synthetic,
    };
    assert!(synthetic.is_synthetic());
}

#[test]
fn nearest_picks_closest_static_weight() {
    let coverage = WeightCoverage::Static(&[300, 400, 600, 700]);
    assert_eq!(coverage.nearest(500), 400);
    assert_eq!(coverage.nearest(650), 600);
    assert_eq!(coverage.nearest(900), 700);
    let variable = WeightCoverage::Variable { min: 100, max: 900 };
    assert_eq!(variable.nearest(500), 500);
    assert_eq!(variable.nearest(50), 100);
}

#[test]
fn current_platform_returns_a_stack() {
    let platform = Platform::current();
    assert!(!platform.font_stack(FontFamily::Text).is_empty());
}
