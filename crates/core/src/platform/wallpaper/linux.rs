//! Linux 壁纸路径探测：GNOME → KDE Plasma → swaybg / hyprpaper → XFCE。
//!
//! 四步互相独立：`gsettings` 缺失、配置文件不存在、解析不到值都只记一条原因，探测继续往下
//! 走（Wayland 上一条配置都读不到是常态，此时应直接落到 D38 的第③级色卡）。
//!
//! 探测路径全部来自「桌面环境自己记在磁盘上的东西」，不需要 D-Bus 与 xdg-desktop-portal：
//!
//! 1. GNOME / Ubuntu 系：`gsettings get org.gnome.desktop.background picture-uri`，再看
//!    `picture-uri-dark`（深色壁纸是独立键）；
//! 2. KDE Plasma：`~/.config/plasma-org.kde.plasma.desktop-appletsrc` 的 `Image=` 行；
//! 3. swaybg / hyprpaper：`~/.config/hypr/hyprpaper.conf`（`wallpaper =` / `preload =`）与
//!    `~/.config/sway/config`（`output <name> bg <path>`）；
//! 4. XFCE：`~/.config/xfce4/xfconf/xfce-perchannel-xml/xfce4-desktop.xml` 的
//!    `last-image` / `image-path` 属性。
//!
//! 本文件不带 `cfg`：平台分流只在 [`super::detect_with`] 一处，任何宿主都能跑夹具用例。

use super::{Collect, SourceKind};

/// GNOME 壁纸 schema。
pub(super) const GNOME_SCHEMA: &str = "org.gnome.desktop.background";

/// 探测的键，顺序即优先级（浅色在前，深色壁纸在后）。
pub(super) const GNOME_KEYS: &[&str] = &["picture-uri", "picture-uri-dark"];

const KDE_CONFIG: &str = "plasma-org.kde.plasma.desktop-appletsrc";
const HYPRPAPER_CONFIG: &str = "hypr/hyprpaper.conf";
const SWAY_CONFIG: &str = "sway/config";
const XFCE_DESKTOP_XML: &str = "xfce4/xfconf/xfce-perchannel-xml/xfce4-desktop.xml";

/// 四步依次探测。
pub(super) fn collect(collect: &mut Collect<'_>) {
    gnome(collect);
    kde(collect);
    compositors(collect);
    xfce(collect);
}

/// 第一步：GNOME 的 `picture-uri` / `picture-uri-dark`。
fn gnome(collect: &mut Collect<'_>) {
    for key in GNOME_KEYS {
        let evidence = format!("gsettings get {GNOME_SCHEMA} {key}");
        match collect.io().run("gsettings", &["get", GNOME_SCHEMA, key]) {
            Some(text) => match parse_gsettings_value(&text) {
                Some(value) => {
                    collect.add(SourceKind::Gnome, &evidence, &value);
                }
                None => collect.note(
                    SourceKind::Gnome.as_str(),
                    format!("{evidence} = {}（没有设壁纸）", text.trim()),
                ),
            },
            None => collect.note(
                SourceKind::Gnome.as_str(),
                format!("{evidence} 失败（命令缺失 / 非零退出 / 空输出）"),
            ),
        }
    }
}

/// 第二步：KDE Plasma 的壁纸插件配置。
fn kde(collect: &mut Collect<'_>) {
    let Some(text) = read_config(collect, SourceKind::Kde, KDE_CONFIG) else {
        return;
    };
    let values = parse_kde_images(&text);
    if values.is_empty() {
        collect.note(
            SourceKind::Kde.as_str(),
            format!("{KDE_CONFIG} 里没有 Image= 行"),
        );
    }
    let evidence = format!("{KDE_CONFIG} 的 Image=");
    for value in values {
        collect.add(SourceKind::Kde, &evidence, &value);
    }
}

/// 第三步：swaybg（sway 配置）/ hyprpaper。
fn compositors(collect: &mut Collect<'_>) {
    for (relative, what) in [
        (HYPRPAPER_CONFIG, "wallpaper/preload 行"),
        (SWAY_CONFIG, "output ... bg 行"),
    ] {
        let Some(text) = read_config(collect, SourceKind::WlrootsCompositor, relative) else {
            continue;
        };
        let values = match relative {
            HYPRPAPER_CONFIG => parse_hyprpaper_wallpapers(&text),
            _ => parse_sway_backgrounds(&text),
        };
        if values.is_empty() {
            collect.note(
                SourceKind::WlrootsCompositor.as_str(),
                format!("{relative} 里没有 {what}"),
            );
        }
        let evidence = format!("{relative} 的 {what}");
        for value in values {
            collect.add(SourceKind::WlrootsCompositor, &evidence, &value);
        }
    }
}

/// 第四步：XFCE 的 xfconf XML。
fn xfce(collect: &mut Collect<'_>) {
    let Some(text) = read_config(collect, SourceKind::Xfce, XFCE_DESKTOP_XML) else {
        return;
    };
    let values = parse_xfce_last_image(&text);
    if values.is_empty() {
        collect.note(
            SourceKind::Xfce.as_str(),
            format!("{XFCE_DESKTOP_XML} 里没有 last-image/image-path 属性"),
        );
    }
    let evidence = format!("{XFCE_DESKTOP_XML} 的 last-image/image-path");
    for value in values {
        collect.add(SourceKind::Xfce, &evidence, &value);
    }
}

/// 读 `$XDG_CONFIG_HOME` 下的一个配置文件；读不到只记原因并返回 `None`。
fn read_config(collect: &mut Collect<'_>, kind: SourceKind, relative: &str) -> Option<String> {
    let path = collect.roots().config_home.join(relative);
    match collect.io().read_text(&path) {
        Some(text) => Some(text),
        None => {
            collect.note(kind.as_str(), format!("读不到 {}", path.display()));
            None
        }
    }
}

/// 解析 `gsettings get` 的输出：`'file:///home/u/a.png'`；`'none'` / 空串返回 `None`。
pub(super) fn parse_gsettings_value(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let value = trimmed
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .unwrap_or(trimmed)
        .trim();
    if value.is_empty() || value.eq_ignore_ascii_case("none") {
        return None;
    }
    Some(value.to_owned())
}

/// 取 Plasma 配置里的 `Image=` 值（壁纸插件是 `[org.kde.image][General]` 段）。
///
/// 只收绝对路径与 `file://` URI：Plasma 的其它插件也有 `Image=` 键，相对值当成非壁纸。
pub(super) fn parse_kde_images(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.trim().split_once('=')?;
            if !key.trim().eq_ignore_ascii_case("image") {
                return None;
            }
            let value = value.trim();
            let usable = value.starts_with('/')
                || value.starts_with("file://")
                || value
                    .get(..2)
                    .is_some_and(|head| head.eq_ignore_ascii_case("~/"));
            usable.then(|| value.to_owned())
        })
        .collect()
}

/// 取 hyprpaper 的 `wallpaper = <监视器>,<路径>` 与 `preload = <路径>` 行。
pub(super) fn parse_hyprpaper_wallpapers(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let value = value.trim();
            match key.trim().to_ascii_lowercase().as_str() {
                "preload" => non_empty(value),
                // `wallpaper = eDP-1,/path/a.png`（监视器可留空）；没写逗号时整段就是路径。
                "wallpaper" => match value.split_once(',') {
                    Some((_, path)) => non_empty(path),
                    None => non_empty(value),
                },
                _ => None,
            }
        })
        .collect()
}

/// 取 sway 配置里 `output <名字|*> bg <路径> [填充方式]` 的路径。
///
/// sway 允许用引号包住含空格的对象名/路径，这里把引号拼回来；解析不出来的行直接跳过。
pub(super) fn parse_sway_backgrounds(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let tokens: Vec<&str> = line.split_whitespace().collect();
            if tokens.len() < 4 || tokens[0] != "output" || tokens[2] != "bg" {
                return None;
            }
            let raw = tokens[3];
            let mut value = raw.trim_start_matches('"').to_owned();
            if raw.starts_with('"') && !raw.ends_with('"') {
                for token in &tokens[4..] {
                    value.push(' ');
                    match token.strip_suffix('"') {
                        Some(tail) => {
                            value.push_str(tail);
                            break;
                        }
                        None => value.push_str(token),
                    }
                }
            }
            non_empty(value.trim_end_matches('"'))
        })
        .collect()
}

/// 取 XFCE `xfce4-desktop.xml` 里 `last-image` / `image-path` 属性的值。
pub(super) fn parse_xfce_last_image(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.contains("last-image") || line.contains("image-path"))
        .filter_map(|line| non_empty(&attribute(line, "value")?))
        .collect()
}

/// 取 XML 一行里 `name="..."` 属性的值。
fn attribute(line: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::super::io::fake::FakeIo;
    use super::super::{detect_with, SourceKind};
    use super::*;
    use crate::platform::paths::{MapEnv, Platform};

    const CONFIG: &str = "/home/u/.config";

    fn env() -> MapEnv {
        MapEnv::new().with("HOME", "/home/u")
    }

    fn gsettings(key: &str) -> String {
        format!("gsettings get {GNOME_SCHEMA} {key}")
    }

    fn config(name: &str) -> String {
        format!("{CONFIG}/{name}")
    }

    #[test]
    fn gnome_picture_uri_hit_decodes_percent_escapes() {
        let io = FakeIo::new()
            .with_command(
                &gsettings("picture-uri"),
                "'file:///home/u/my%20wall.png'\n",
            )
            .with_image("/home/u/my wall.png");
        let probe = detect_with(&io, &env(), Platform::Linux);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        let best = probe.best().expect("有候选");
        assert_eq!(best.kind, SourceKind::Gnome);
        assert_eq!(best.path.display().to_string(), "/home/u/my wall.png");
    }

    #[test]
    fn four_steps_yield_ordered_candidates() {
        let io = ["a", "b", "k", "h", "h2", "s", "x"]
            .into_iter()
            .map(|name| format!("/home/u/{name}.png"))
            .fold(
                FakeIo::new()
                    .with_command(&gsettings("picture-uri"), "'file:///home/u/a.png'\n")
                    .with_command(&gsettings("picture-uri-dark"), "'file:///home/u/b.png'\n")
                    .with_file(config(KDE_CONFIG), "Image=file:///home/u/k.png\n")
                    .with_file(
                        config(HYPRPAPER_CONFIG),
                        "preload = /home/u/h.png\nwallpaper = eDP-1, /home/u/h2.png\n",
                    )
                    .with_file(config(SWAY_CONFIG), "output * bg /home/u/s.png fill\n")
                    .with_file(
                        config(XFCE_DESKTOP_XML),
                        "<property name=\"last-image\" value=\"/home/u/x.png\"/>\n",
                    ),
                FakeIo::with_image,
            );
        let probe = detect_with(&io, &env(), Platform::Linux);
        let seen: Vec<(SourceKind, String)> = probe
            .iter()
            .map(|source| (source.kind, source.path.display().to_string()))
            .collect();
        assert_eq!(
            seen,
            [
                (SourceKind::Gnome, "/home/u/a.png"),
                (SourceKind::Gnome, "/home/u/b.png"),
                (SourceKind::Kde, "/home/u/k.png"),
                (SourceKind::WlrootsCompositor, "/home/u/h.png"),
                (SourceKind::WlrootsCompositor, "/home/u/h2.png"),
                (SourceKind::WlrootsCompositor, "/home/u/s.png"),
                (SourceKind::Xfce, "/home/u/x.png"),
            ]
            .map(|(kind, path)| (kind, path.to_owned()))
        );
    }

    #[test]
    fn failing_step_does_not_stop_later_steps() {
        // gsettings 说「没设壁纸」，KDE 指向一个不存在的文件，XFCE 才是真的。
        let io = FakeIo::new()
            .with_command(&gsettings("picture-uri"), "'none'\n")
            .with_file(config(KDE_CONFIG), "Image=file:///home/u/gone.png\n")
            .with_file(
                config(XFCE_DESKTOP_XML),
                "<property name=\"image-path\" value=\"/home/u/x.png\"/>\n",
            )
            .with_image("/home/u/x.png");
        let probe = detect_with(&io, &env(), Platform::Linux);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        assert_eq!(probe.best().expect("有候选").kind, SourceKind::Xfce);
        assert!(
            probe.notes().iter().any(|note| note.contains("gone.png")),
            "落选原因要留：{:?}",
            probe.notes()
        );
    }

    #[test]
    fn everything_missing_returns_empty_with_reasons() {
        let probe = detect_with(&FakeIo::new(), &env(), Platform::Linux);
        assert!(probe.is_empty(), "全部失败要返回空列表");
        let notes = probe.notes().join("\n");
        assert!(notes.contains("gsettings"), "{notes}");
        assert!(notes.contains("xfce4-desktop.xml"), "{notes}");
        assert!(notes.contains("hyprpaper.conf"), "{notes}");
    }

    #[test]
    fn gsettings_output_parsing() {
        assert_eq!(
            parse_gsettings_value("'file:///home/u/a.png'\n"),
            Some("file:///home/u/a.png".to_owned())
        );
        assert_eq!(parse_gsettings_value("'none'\n"), None);
        assert_eq!(parse_gsettings_value("''"), None);
        assert_eq!(parse_gsettings_value(""), None);
    }

    #[test]
    fn config_line_parsing() {
        assert_eq!(
            parse_kde_images("Image=file:///home/u/a.png\nImage=relative/b.png\n"),
            vec!["file:///home/u/a.png".to_owned()]
        );
        assert_eq!(
            parse_hyprpaper_wallpapers(
                "# 注释行\npreload = /home/u/a.png\nwallpaper = eDP-1, /home/u/b.png\nwallpaper = ,/home/u/c.png\nsplash = false\n"
            ),
            vec![
                "/home/u/a.png".to_owned(),
                "/home/u/b.png".to_owned(),
                "/home/u/c.png".to_owned(),
            ]
        );
        assert_eq!(
            parse_sway_backgrounds(
                "output * bg /home/u/a.png fill\noutput eDP-1 bg \"/home/u/b c.png\" stretch\n"
            ),
            vec!["/home/u/a.png".to_owned(), "/home/u/b c.png".to_owned()]
        );
        assert_eq!(
            parse_xfce_last_image(
                "<property name=\"last-image\" value=\"/home/u/a.png\"/>\n<property name=\"image-style\" value=\"3\"/>\n"
            ),
            vec!["/home/u/a.png".to_owned()]
        );
    }
}
