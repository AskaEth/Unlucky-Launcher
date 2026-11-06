//! Windows 壁纸路径探测：四步，可信度从高到低。
//!
//! Win10+ 的真机事实（Windows 11 24H2 实测）：`%APPDATA%\Microsoft\Windows\Themes\` 里
//! **通常只有** `TranscodedWallpaper`，一个 `.theme` 都没有；当前生效的主题记在
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes` 的 `CurrentTheme` 值里，指向
//! `%LOCALAPPDATA%\Microsoft\Windows\Themes\Custom.theme`。所以「扫 `%APPDATA%` 下的
//! `*.theme`」只能当最后一档兜底：那里的主题包是用户另存的，**可能从未启用**，拿它取色会
//! 取到与当前桌面无关的图（宁可少给一个候选，也不给错路径）。
//!
//! 1. `%APPDATA%\Microsoft\Windows\Themes\TranscodedWallpaper`——系统**当前**壁纸转码后的
//!    副本，没有扩展名（部分版本还有 `TranscodedWallpaper.jpg` 别名），所以「像不像图片」
//!    只看文件头魔数，不看后缀；
//! 2. `CurrentTheme` 指向的 `.theme`——当前生效主题里的 `Wallpaper=` 行（Win10+ 在
//!    `%LOCALAPPDATA%` 一侧，支持 `%LOCALAPPDATA%` / `%SystemRoot%` 之类变量展开）；
//! 3. `HKCU\Control Panel\Desktop` 的 `WallPaper` 值——注册表权威来源；
//! 4. `%APPDATA%\...\Themes\*.theme`——用户另存的主题包，`evidence` 里如实标「可能未启用」。
//!
//! 第 2、3 步走 `reg.exe` 子进程，不用 win32 API（core 是 `#![forbid(unsafe_code)]`，
//! 见 [`super`] 模块文档）；命令缺失 / 非零退出 / 空输出都只记一条原因，探测继续。
//!
//! 本文件不带 `cfg`：Linux CI 上同样要能跑夹具用例，平台分流只在
//! [`super::detect_with`] 一处。

use std::path::{Path, PathBuf};

use self::parse::{parse_reg_sz_value, parse_theme_wallpaper};
use super::io::HostRoots;
use super::parse::normalize_path;
use super::{Collect, SourceKind};

mod parse;

/// `reg query` 用的桌面键（`HKCU\Control Panel\Desktop`，壁纸值名 `WallPaper`）。
pub(super) const DESKTOP_KEY: &str = r"HKCU\Control Panel\Desktop";

/// `reg query` 用的主题键（Win10+ 当前主题记录在这里）。
pub(super) const THEMES_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes";

/// 壁纸值名（注册表里是 `WallPaper`，`.theme` 里是 `Wallpaper=`，比对时都忽略大小写）。
pub(super) const WALLPAPER_VALUE: &str = "WallPaper";

/// 当前主题文件名（值是指向 `.theme` 的绝对路径）。
pub(super) const CURRENT_THEME_VALUE: &str = "CurrentTheme";

/// `%APPDATA%` 下的主题目录（分段拼，避免 Windows 上出现混用的分隔符）。
const THEMES_PARTS: &[&str] = &["Microsoft", "Windows", "Themes"];

/// 转码副本的两个已知文件名（无扩展名的是主力）。
const TRANSCODED_NAMES: &[&str] = &["TranscodedWallpaper", "TranscodedWallpaper.jpg"];

/// 主题文件后缀。
const THEME_EXTENSION: &str = "theme";

/// 四步依次探测。
pub(super) fn collect(collect: &mut Collect<'_>) {
    transcoded(collect);
    current_theme(collect);
    registry(collect);
    saved_theme_packages(collect);
}

fn themes_dir(roots: &HostRoots) -> PathBuf {
    let mut dir = roots.app_data.clone();
    for part in THEMES_PARTS {
        dir.push(part);
    }
    dir
}

/// 第一步：系统转码后的当前壁纸副本（第一个名字命中后就不再试别名）。
fn transcoded(collect: &mut Collect<'_>) {
    let dir = themes_dir(collect.roots());
    for name in TRANSCODED_NAMES {
        let path = dir.join(name);
        let evidence = format!("%APPDATA%\\Microsoft\\Windows\\Themes\\{name}");
        if collect.add_path(SourceKind::WindowsTranscoded, &evidence, path) {
            return;
        }
    }
}

/// 第二步：当前生效主题（`CurrentTheme` 指向的 `.theme`）里的 `Wallpaper=`。
fn current_theme(collect: &mut Collect<'_>) {
    let Some(raw) = query_reg(
        collect,
        SourceKind::WindowsCurrentTheme,
        THEMES_KEY,
        CURRENT_THEME_VALUE,
    ) else {
        return;
    };
    let Some(theme) = normalize_path(&raw, collect.roots()) else {
        collect.note(
            SourceKind::WindowsCurrentTheme.as_str(),
            format!("{CURRENT_THEME_VALUE} 不是可用的绝对路径（{raw}）"),
        );
        return;
    };
    let label = format!(
        "reg query {THEMES_KEY} /v {CURRENT_THEME_VALUE} → {}",
        theme.display()
    );
    collect_theme_file(collect, SourceKind::WindowsCurrentTheme, &theme, &label);
}

/// 第三步：注册表里的权威壁纸值。
fn registry(collect: &mut Collect<'_>) {
    let Some(value) = query_reg(
        collect,
        SourceKind::WindowsRegistry,
        DESKTOP_KEY,
        WALLPAPER_VALUE,
    ) else {
        return;
    };
    let label = format!("reg query {DESKTOP_KEY} /v {WALLPAPER_VALUE}");
    collect.add(SourceKind::WindowsRegistry, &label, &value);
}

/// 第四步（兜底）：`%APPDATA%\...\Themes\*.theme`——用户另存的主题包，**可能未启用**。
fn saved_theme_packages(collect: &mut Collect<'_>) {
    let dir = themes_dir(collect.roots());
    let mut themes: Vec<PathBuf> = collect
        .io()
        .list_dir(&dir)
        .into_iter()
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case(THEME_EXTENSION))
        })
        .collect();
    themes.sort();
    if themes.is_empty() {
        collect.note(
            SourceKind::WindowsThemeFile.as_str(),
            format!("{} 下没有 .theme 文件", dir.display()),
        );
        return;
    }
    for theme in themes {
        let label = format!("{}（用户另存的主题包，可能未启用）", theme.display());
        collect_theme_file(collect, SourceKind::WindowsThemeFile, &theme, &label);
    }
}

/// 读一个 `.theme` 文件，把里面的 `Wallpaper=` 收成候选；读不到 / 没值只记原因。
fn collect_theme_file(collect: &mut Collect<'_>, kind: SourceKind, file: &Path, label: &str) {
    let Some(text) = collect.io().read_text(file) else {
        collect.note(kind.as_str(), format!("读不到 {}", file.display()));
        return;
    };
    let values = parse_theme_wallpaper(&text);
    if values.is_empty() {
        collect.note(
            kind.as_str(),
            format!("{} 里没有 Wallpaper= 行", file.display()),
        );
        return;
    }
    let evidence = format!("{label} 的 Wallpaper=");
    for value in values {
        collect.add(kind, &evidence, &value);
    }
}

/// 跑一次 `reg query <键> /v <值名>` 并取出值；失败或缺值都只记原因、返回 `None`。
fn query_reg(
    collect: &mut Collect<'_>,
    kind: SourceKind,
    key: &str,
    value_name: &str,
) -> Option<String> {
    let query = format!("reg query {key} /v {value_name}");
    match collect
        .io()
        .run("reg.exe", &["query", key, "/v", value_name])
    {
        Some(text) => match parse_reg_sz_value(&text, value_name) {
            Some(value) => Some(value),
            None => {
                collect.note(
                    kind.as_str(),
                    format!("{query} 输出里没有 {value_name} 值（可能被策略禁用或壁纸为纯色）"),
                );
                None
            }
        },
        None => {
            collect.note(
                kind.as_str(),
                format!("{query} 失败（命令缺失 / 非零退出 / 空输出）"),
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::io::fake::FakeIo;
    use super::super::{detect_with, SourceKind};
    use super::*;
    use crate::platform::paths::{MapEnv, Platform};

    const APP_DATA: &str = "C:/Users/u/AppData/Roaming";
    const LOCAL_APP_DATA: &str = "C:/Users/u/AppData/Local";
    const DESKTOP_QUERY: &str = "reg.exe query HKCU\\Control Panel\\Desktop /v WallPaper";
    const THEMES_QUERY: &str =
        "reg.exe query HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes /v CurrentTheme";
    const CUSTOM_THEME: &str = r"C:\Users\u\AppData\Local\Microsoft\Windows\Themes\Custom.theme";

    fn env() -> MapEnv {
        MapEnv::new()
            .with("HOME", "C:/Users/u")
            .with("APPDATA", APP_DATA)
            .with("LOCALAPPDATA", LOCAL_APP_DATA)
            .with("SYSTEMROOT", "C:/Windows")
    }

    fn roots() -> HostRoots {
        HostRoots::from_env(&env()).expect("HOME 已给")
    }

    /// 夹具用**生产代码同一个拼法**造路径，所以 Windows / Linux 宿主上都能对上
    /// （另有一条字面量断言盯着这个拼法本身，见 `transcoded_wallpaper_hit_comes_first`）。
    fn themes() -> PathBuf {
        themes_dir(&roots())
    }

    #[test]
    fn transcoded_wallpaper_hit_comes_first() {
        let io = FakeIo::new().with_image(themes().join("TranscodedWallpaper"));
        let probe = detect_with(&io, &env(), Platform::Windows);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        let best = probe.best().expect("有候选");
        assert_eq!(best.kind, SourceKind::WindowsTranscoded);
        assert_eq!(best.path, themes().join("TranscodedWallpaper"));
        // 字面量：夹具路径是拿生产代码的构造函数造出来的，这条断言防止两边一起拼错。
        assert_eq!(
            best.path,
            PathBuf::from(
                "C:/Users/u/AppData/Roaming/Microsoft/Windows/Themes/TranscodedWallpaper"
            )
        );
        assert!(best.evidence.contains("TranscodedWallpaper"));
    }

    #[test]
    fn current_theme_is_read_from_local_app_data() {
        // Win10+ 的真机形态：%APPDATA% 下没有 .theme，当前主题在 %LOCALAPPDATA%。
        let io = FakeIo::new()
            .with_command(
                THEMES_QUERY,
                &format!("    CurrentTheme    REG_SZ    {CUSTOM_THEME}\r\n"),
            )
            .with_file(
                CUSTOM_THEME,
                "[Control Panel\\Desktop]\r\nWallpaper=%SystemRoot%\\web\\wallpaper\\Windows\\img0.jpg\r\n",
            )
            .with_image("C:/Windows\\web\\wallpaper\\Windows\\img0.jpg");
        let probe = detect_with(&io, &env(), Platform::Windows);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        let best = probe.best().expect("有候选");
        assert_eq!(best.kind, SourceKind::WindowsCurrentTheme);
        assert_eq!(
            best.path,
            PathBuf::from("C:/Windows\\web\\wallpaper\\Windows\\img0.jpg")
        );
        assert!(best.evidence.contains("CurrentTheme"), "{}", best.evidence);
        assert!(best.evidence.contains("Windows\\Themes\\Custom.theme"));
    }

    #[test]
    fn current_theme_expands_local_app_data_env_var() {
        let io = FakeIo::new()
            .with_command(
                THEMES_QUERY,
                "    CurrentTheme    REG_SZ    %LOCALAPPDATA%\\Microsoft\\Windows\\Themes\\Custom.theme\r\n",
            )
            .with_file(
                CUSTOM_THEME,
                "Wallpaper=C:\\Users\\u\\Pictures\\a.jpg\r\n",
            )
            .with_image(r"C:\Users\u\Pictures\a.jpg");
        let probe = detect_with(&io, &env(), Platform::Windows);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        assert_eq!(
            probe.best().expect("有候选").kind,
            SourceKind::WindowsCurrentTheme
        );
    }

    #[test]
    fn authority_order_puts_saved_packages_last() {
        // 四步全在场，路径互不相同：顺序必须是 转码副本 > 当前主题 > 注册表 > 另存包。
        let package = themes().join("a.theme");
        let io = FakeIo::new()
            .with_image(themes().join("TranscodedWallpaper"))
            .with_command(
                THEMES_QUERY,
                &format!("    CurrentTheme    REG_SZ    {CUSTOM_THEME}\r\n"),
            )
            .with_file(
                CUSTOM_THEME,
                "Wallpaper=C:\\Users\\u\\Pictures\\one.jpg\r\n",
            )
            .with_command(
                DESKTOP_QUERY,
                "    WallPaper    REG_SZ    C:\\Users\\u\\Pictures\\two.jpg\r\n",
            )
            .with_dir(themes(), &["a.theme", "b.theme", "notes.txt"])
            .with_file(
                package.clone(),
                "Wallpaper=%SystemRoot%\\web\\wallpaper\\three.jpg\r\n",
            )
            .with_image(r"C:\Users\u\Pictures\one.jpg")
            .with_image(r"C:\Users\u\Pictures\two.jpg")
            .with_image("C:/Windows\\web\\wallpaper\\three.jpg");
        let probe = detect_with(&io, &env(), Platform::Windows);
        let seen: Vec<(SourceKind, String)> = probe
            .iter()
            .map(|source| (source.kind, source.path.display().to_string()))
            .collect();
        assert_eq!(seen.len(), 4, "{:?}", probe.notes());
        assert_eq!(seen[0].0, SourceKind::WindowsTranscoded);
        assert_eq!(seen[1].0, SourceKind::WindowsCurrentTheme);
        assert_eq!(seen[2].0, SourceKind::WindowsRegistry);
        assert_eq!(seen[3].0, SourceKind::WindowsThemeFile);
        // 兜底档在 evidence 里如实标明「可能未启用」，调用方据此可降权。
        assert!(
            probe.sources()[3].evidence.contains("可能未启用"),
            "{}",
            probe.sources()[3].evidence
        );
    }

    #[test]
    fn everything_missing_returns_empty_with_reasons() {
        let probe = detect_with(&FakeIo::new(), &env(), Platform::Windows);
        assert!(probe.is_empty(), "全部失败要返回空列表而不是错误");
        let notes = probe.notes().join("\n");
        assert!(notes.contains("reg query"), "注册表失败要留原因：{notes}");
        assert!(
            notes.contains("CurrentTheme"),
            "当前主题失败要留原因：{notes}"
        );
        assert!(
            notes.contains("TranscodedWallpaper"),
            "转码副本缺失要留原因：{notes}"
        );
    }

    #[test]
    fn same_path_from_two_steps_is_deduped() {
        let transcoded = themes().join("TranscodedWallpaper");
        let registry_value = transcoded.display().to_string().to_ascii_uppercase();
        let io = FakeIo::new()
            .with_image(&transcoded)
            // 注册表指向同一个文件，大小写还不同（Windows 路径大小写不敏感）。
            .with_command(
                DESKTOP_QUERY,
                &format!("    WallPaper    REG_SZ    {registry_value}\r\n"),
            );
        let probe = detect_with(&io, &env(), Platform::Windows);
        assert_eq!(probe.len(), 1, "{:?}", probe.sources());
        assert_eq!(
            probe.best().expect("有候选").kind,
            SourceKind::WindowsTranscoded
        );
    }
}
