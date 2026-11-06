//! `fonts` 的单元测试（拆出来只为「src 文件 ≤400 行」的门禁）。

use super::cjk::CjkReport;
use super::*;
use unlucky_design::typography::WeightCoverage;
use unlucky_i18n::font::Strategy;
use unlucky_i18n::lang::Detection;

fn only(installed: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |family| installed.contains(&family)
}

#[test]
fn windows_stack_falls_back_to_classic_segoe_when_variable_is_absent() {
    // Win10 没有 Segoe UI Variable Text
    let family = pick(
        Platform::Windows,
        FontFamily::Text,
        &only(&["Segoe UI", "Microsoft YaHei UI"]),
    );
    assert_eq!(family, "Segoe UI");
}

#[test]
fn windows_stack_prefers_the_variable_face_on_win11() {
    let family = pick(
        Platform::Windows,
        FontFamily::Text,
        &only(&["Segoe UI Variable Text", "Segoe UI"]),
    );
    assert_eq!(family, "Segoe UI Variable Text");
}

#[test]
fn generic_family_names_are_never_reported_as_the_resolved_family() {
    // 一个都没装：返回栈头，且栈头不是通用族名
    let family = pick(Platform::Linux, FontFamily::Text, &only(&[]));
    assert_eq!(family, "Roboto");
    assert!(!is_generic(family));
}

#[test]
fn missing_cjk_fonts_do_not_change_the_latin_pick() {
    // 装了苹方但没装 SF：macOS 栈应该选 Helvetica Neue 而不是中文族
    let family = pick(
        Platform::MacOs,
        FontFamily::Text,
        &only(&["PingFang SC", "Helvetica Neue"]),
    );
    assert_eq!(family, "Helvetica Neue");
}

#[test]
fn weight_reports_use_the_token_degradation_path() {
    // 没有系统字面表时回落到令牌纯函数：Windows 栈里 Segoe UI Variable Text 是可变 300..700
    let medium = weight_report(
        Platform::Windows,
        "Segoe UI Variable Text",
        Weight::Medium,
        None,
    );
    assert_eq!(medium.requested, 500);
    assert_eq!(medium.quality, WeightMatch::Native);

    // 手工构造一个「只有 400/700」的族（= 经典 Segoe UI），500 应该落到最近档
    let faces = [
        Face {
            family: "Segoe UI".to_owned(),
            weight: 400,
        },
        Face {
            family: "Segoe UI".to_owned(),
            weight: 700,
        },
    ];
    let medium = weight_report(Platform::Windows, "Segoe UI", Weight::Medium, Some(&faces));
    assert_eq!(medium.available, 400, "400 比 700 更近");
    assert_eq!(medium.quality, WeightMatch::Nearest);

    // 只有 400 却要 700 → 合成加粗
    let only_regular = [Face {
        family: "Narrow".to_owned(),
        weight: 400,
    }];
    let bold = weight_report(
        Platform::Windows,
        "Narrow",
        Weight::Bold,
        Some(&only_regular),
    );
    assert_eq!(bold.quality, WeightMatch::Synthetic);

    // 原生有 500 就用 500
    let has_medium = [Face {
        family: "Noto".to_owned(),
        weight: 500,
    }];
    assert_eq!(
        weight_report(Platform::Linux, "Noto", Weight::Medium, Some(&has_medium)).quality,
        WeightMatch::Native
    );

    let narrow = WeightCoverage::Static(&[400]);
    assert_eq!(narrow.nearest(700), 400);
}

#[test]
fn describe_is_one_line_and_mentions_all_three_stacks() {
    let resolved = Resolved {
        platform: Platform::Windows,
        families: [
            FamilyReport {
                stack: "text",
                family: "Segoe UI",
                installed: true,
            },
            FamilyReport {
                stack: "display",
                family: "Segoe UI Variable Display",
                installed: false,
            },
            FamilyReport {
                stack: "mono",
                family: "Cascadia Mono",
                installed: true,
            },
        ],
        weights: [
            WeightReport {
                requested: 400,
                available: 400,
                quality: WeightMatch::Native,
            },
            WeightReport {
                requested: 500,
                available: 400,
                quality: WeightMatch::Nearest,
            },
            WeightReport {
                requested: 700,
                available: 400,
                quality: WeightMatch::Synthetic,
            },
        ],
        cjk: CjkReport {
            detection: Detection {
                lang: unlucky_i18n::Lang::En,
                raw: None,
                origin: unlucky_i18n::lang::ORIGIN_DEFAULT,
            },
            strategy: Strategy::NotNeeded,
        },
    };

    let line = resolved.describe();
    assert_eq!(line.lines().count(), 1);
    assert!(line.contains("text=\"Segoe UI\""), "{line}");
    assert!(line.contains("未装机"), "{line}");
    assert!(line.contains("w700=400(synthetic)"), "{line}");
    assert!(line.contains("cjk=not-needed"), "英文环境要进日志：{line}");
    assert!(resolved.has_synthetic());
    assert_eq!(resolved.families().text, "Segoe UI");
}

#[test]
fn type_roles_pick_the_display_stack_for_big_text() {
    use unlucky_design::typography::TypeRole;

    assert_eq!(
        TypeRole::TitleLarge.style().family,
        FontFamily::Display,
        "22px 标题走 Display 栈"
    );
    assert_eq!(TypeRole::BodyMedium.style().family, FontFamily::Text);
}
