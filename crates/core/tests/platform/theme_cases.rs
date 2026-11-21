//! 系统主题探测的解析与判定（`core::platform::theme_probe`）。
//!
//! 解析函数跨平台编译，所以本用例在任意宿主上都能跑；真实探测只是把同一份解析套到
//! `reg query` / `defaults` / `gsettings` 的输出上（`probe()` 的非确定性不进断言）。

use unlucky_core::platform::theme_probe::{
    self, parse_apps_use_light_theme, parse_gsettings_color_scheme, parse_gtk_theme,
    parse_macos_interface_style, SystemTheme, WINDOWS_PERSONALIZE_KEY,
};

#[test]
fn windows_registry_apps_use_light_theme_maps_dword() {
    // 真实 `reg query` 输出（REG_DWORD 以 0x 十六进制打印）。
    let light = "HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\n    \
                 AppsUseLightTheme    REG_DWORD    0x1\n    SystemUsesLightTheme    REG_DWORD    0x0\n";
    assert_eq!(parse_apps_use_light_theme(light), Some(SystemTheme::Light));

    let dark = "    AppsUseLightTheme    REG_DWORD    0x0\n";
    assert_eq!(parse_apps_use_light_theme(dark), Some(SystemTheme::Dark));

    // 十进制形式的输出（某些区域设置/工具会把 DWORD 打成十进制）。
    assert_eq!(
        parse_apps_use_light_theme("    AppsUseLightTheme    REG_DWORD    1"),
        Some(SystemTheme::Light)
    );

    // 键不存在（老系统 / 被策略删掉）与别的值都不猜。
    assert_eq!(parse_apps_use_light_theme(""), None);
    assert_eq!(
        parse_apps_use_light_theme("    SystemUsesLightTheme    REG_DWORD    0x0"),
        None
    );
    assert_eq!(
        parse_apps_use_light_theme("    AppsUseLightTheme    REG_SZ    yes"),
        None
    );
}

#[test]
fn windows_key_points_at_personalize_hive() {
    assert!(WINDOWS_PERSONALIZE_KEY.contains("Themes\\Personalize"));
}

#[test]
fn macos_interface_style_maps_dark_and_empty() {
    assert_eq!(
        parse_macos_interface_style("Dark\n"),
        Some(SystemTheme::Dark)
    );
    assert_eq!(
        parse_macos_interface_style("Light\n"),
        Some(SystemTheme::Light)
    );
    // 浅色时 `AppleInterfaceStyle` 键不存在，defaults 无输出。
    assert_eq!(parse_macos_interface_style(""), None);
}

#[test]
fn gsettings_color_scheme_prefers_explicit_preference() {
    assert_eq!(
        parse_gsettings_color_scheme("'prefer-dark'\n"),
        Some(SystemTheme::Dark)
    );
    assert_eq!(
        parse_gsettings_color_scheme("'prefer-light'\n"),
        Some(SystemTheme::Light)
    );
    // `default` = 未表态，交给 gtk-theme 名字兜底。
    assert_eq!(parse_gsettings_color_scheme("'default'\n"), None);
}

#[test]
fn gtk_theme_name_falls_back_to_dark_suffix() {
    assert_eq!(parse_gtk_theme("'Yaru-dark'\n"), Some(SystemTheme::Dark));
    assert_eq!(parse_gtk_theme("'Adwaita-dark'\n"), Some(SystemTheme::Dark));
    assert_eq!(parse_gtk_theme("'Yaru'\n"), Some(SystemTheme::Light));
    assert_eq!(parse_gtk_theme("''\n"), None);
}

#[test]
fn system_theme_tristate_contract() {
    assert_eq!(SystemTheme::Light.is_dark(), Some(false));
    assert_eq!(SystemTheme::Dark.is_dark(), Some(true));
    assert_eq!(
        SystemTheme::Unknown.is_dark(),
        None,
        "Unknown 必须让调用方自己决定回落（不假装浅色）"
    );
    assert_eq!(SystemTheme::Light.as_str(), "light");
    assert_eq!(SystemTheme::Dark.as_str(), "dark");
    assert_eq!(SystemTheme::Unknown.as_str(), "unknown");
}

#[test]
fn probe_never_panics_and_reports_one_of_three_states() {
    // 真实探测（Windows 读注册表，其余平台问 DE）；不断言具体值，只要求落在三态内。
    let probed = theme_probe::probe();
    assert!(matches!(
        probed,
        SystemTheme::Light | SystemTheme::Dark | SystemTheme::Unknown
    ));
}
