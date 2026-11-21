//! 路径归一化与图片头嗅探：把各平台读出的「原始串」变成可用的绝对路径候选。
//!
//! 三平台读出来的东西形状都不一样（gsettings 的 `'file:///x'`、KDE 的 `file:///x`、
//! Windows `.theme` 的 `%SystemRoot%\x`、plist 里裸的 `/x`），全部在这里收口，
//! 上层子模块只管「从哪读」。

use std::path::PathBuf;

use super::io::HostRoots;

/// 图片文件头识别（不用解码库，只看魔数与文本 SVG 头）。
///
/// 认得的：PNG / JPEG / GIF / BMP / WebP / HEIF-AVIF / TIFF，外加头部出现 `<svg` 的文本。
/// 认不出的一律不收——壁纸探测宁可少一个候选，也不要把随便一个文件喂给取色器。
#[must_use]
pub(super) fn looks_like_image(head: &[u8]) -> bool {
    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
    if head.starts_with(PNG) {
        return true;
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return true; // JPEG
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return true;
    }
    if head.len() >= 14 && head.starts_with(b"BM") {
        return true; // BMP
    }
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return true;
    }
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        // ISO-BMFF 家族：HEIC / HEIF 是 macOS 的默认壁纸容器（`.heic`），AVIF 是新格式。
        let brand = &head[8..12];
        return matches!(
            brand,
            b"avif" | b"avis" | b"heic" | b"heix" | b"mif1" | b"msf1"
        );
    }
    if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        return true; // TIFF
    }
    contains_ignore_ascii_case(head, b"<svg")
}

/// 把一条原始串归一化成绝对路径；不像壁纸路径的返回 `None`（具体原因由调用方记）。
///
/// 处理顺序：去引号 → 认 `file://` URI（含百分号解码与 `/C:/` 盘符还原）→ 展开
/// `%USERPROFILE%` 之类的 Windows 变量 → 展开 `~/` → 要求绝对路径。
#[must_use]
pub(super) fn normalize_path(raw: &str, roots: &HostRoots) -> Option<PathBuf> {
    let unquoted = unquote(raw).trim();
    if unquoted.is_empty() {
        return None;
    }
    // GNOME 的「无壁纸」与 macOS `defaults` 的「未设置」占位值。
    if unquoted.eq_ignore_ascii_case("none") || unquoted.eq_ignore_ascii_case("default") {
        return None;
    }
    let text = match strip_prefix_ignore_ascii_case(unquoted, "file://") {
        Some(rest) => strip_uri_drive_prefix(&percent_decode(rest)),
        None => expand_vars(unquoted, roots),
    };
    let text = text.trim();
    if let Some(rest) = text.strip_prefix("~/") {
        return Some(roots.home.join(rest));
    }
    if !looks_absolute(text) {
        return None;
    }
    Some(PathBuf::from(text))
}

/// 去掉成对的单/双引号（`gsettings get` 的输出是 `'file:///x'`）。
fn unquote(raw: &str) -> &str {
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        if (first == b'\'' || first == b'"') && bytes[bytes.len() - 1] == first {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn strip_prefix_ignore_ascii_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &text[prefix.len()..])
}

/// `file://` URI 的路径部分：`/C:/Users/me/a.jpg` 在 Windows 上是 `C:/Users/me/a.jpg`。
///
/// 非 Windows 上这条分支不会命中（Unix 路径不会以 `/C:` 开头），统一处理不影响正确性。
fn strip_uri_drive_prefix(text: &str) -> String {
    let bytes = text.as_bytes();
    if bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':' {
        text[1..].to_owned()
    } else {
        text.to_owned()
    }
}

/// 百分号解码（GNOME 会把空格编码成 `%20`；非法转义原样保留）。
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            let hex = &text[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 展开 Windows 路径里的 `%VAR%`（`.theme` 的 `Wallpaper=%SystemRoot%\web\...`）。
///
/// 认不出的变量原样保留，让后面的图片校验把它判掉——这里不猜路径。
fn expand_vars(text: &str, roots: &HostRoots) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let Some(offset) = rest[start + 1..].find('%') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + offset];
        let value = match name.to_ascii_lowercase().as_str() {
            "userprofile" | "home" => Some(roots.home.to_string_lossy()),
            "appdata" => Some(roots.app_data.to_string_lossy()),
            // Win10+ 的「当前主题」`Custom.theme` 在 `%LOCALAPPDATA%` 一侧。
            "localappdata" => Some(roots.local_app_data.to_string_lossy()),
            "systemroot" | "windir" => roots
                .system_root
                .as_deref()
                .map(std::path::Path::to_string_lossy),
            _ => None,
        };
        match value {
            Some(value) => {
                out.push_str(&rest[..start]);
                out.push_str(&value);
                rest = &rest[start + offset + 2..];
            }
            None => {
                out.push_str(&rest[..=start]);
                rest = &rest[start + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 字符串层面判绝对路径：`/x`、`\\server\share`、`C:/x`、`C:\x`。
///
/// 不用 `Path::is_absolute` 是因为它按**编译目标**判，Windows 夹具在 Linux CI 上会被判成
/// 相对路径，夹具就不跨宿主了。
fn looks_absolute(text: &str) -> bool {
    let bytes = text.as_bytes();
    if text.starts_with('/') || text.starts_with('\\') {
        return true;
    }
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::paths::MapEnv;

    fn roots() -> HostRoots {
        let env = MapEnv::new()
            .with("HOME", "/home/u")
            .with("APPDATA", "C:/Users/u/AppData/Roaming")
            .with("SYSTEMROOT", "C:/Windows");
        HostRoots::from_env(&env).expect("HOME 已给")
    }

    #[test]
    fn image_sniff_accepts_common_wallpaper_containers() {
        assert!(looks_like_image(b"\x89PNG\r\n\x1a\n\x00\x00"));
        assert!(looks_like_image(b"\xFF\xD8\xFF\xE0\x00\x10JFIF"));
        assert!(looks_like_image(b"GIF89a---"));
        assert!(looks_like_image(
            b"BM\x36\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00"
        ));
        assert!(looks_like_image(b"RIFF\x24\x00\x00\x00WEBPVP8 "));
        assert!(looks_like_image(
            b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00heicmif1"
        ));
        assert!(looks_like_image(b"II*\x00\x08\x00\x00\x00"));
        assert!(looks_like_image(
            b"<?xml version=\"1.0\"?><svg xmlns=\"...\""
        ));
    }

    #[test]
    fn image_sniff_rejects_non_images_and_short_heads() {
        assert!(!looks_like_image(b""));
        assert!(!looks_like_image(b"BM"));
        assert!(!looks_like_image(
            b"[Control Panel\\Desktop]\r\nWallpaper=x\r\n"
        ));
        assert!(!looks_like_image(b"RIFF\x24\x00\x00\x00AVI LIST"));
        assert!(!looks_like_image(b"\x00\x00\x00\x18ftypmp42"));
    }

    #[test]
    fn normalize_handles_file_uris_quotes_and_percent_encoding() {
        let roots = roots();
        assert_eq!(
            normalize_path("'file:///home/u/Pictures/my%20wall.png'", &roots),
            Some(PathBuf::from("/home/u/Pictures/my wall.png"))
        );
        // Windows 的 URI 会多一个前导斜杠，盘符要还原。
        assert_eq!(
            normalize_path("file:///C:/Users/u/a.jpg", &roots),
            Some(PathBuf::from("C:/Users/u/a.jpg"))
        );
        assert_eq!(
            normalize_path("\"C:\\Users\\u\\a.jpg\"", &roots),
            Some(PathBuf::from("C:\\Users\\u\\a.jpg"))
        );
        // 非法百分号转义原样保留，不 panic（多字节字符也要安全）。
        assert_eq!(
            normalize_path("file:///home/u/100%zz%日.png", &roots),
            Some(PathBuf::from("/home/u/100%zz%日.png"))
        );
    }

    #[test]
    fn normalize_expands_windows_vars_and_home() {
        let roots = roots();
        assert_eq!(
            normalize_path(r"%SystemRoot%\web\wallpaper\a.jpg", &roots),
            Some(PathBuf::from("C:/Windows\\web\\wallpaper\\a.jpg"))
        );
        assert_eq!(
            normalize_path(
                "%APPDATA%\\Microsoft\\Windows\\Themes\\TranscodedWallpaper",
                &roots
            ),
            Some(PathBuf::from(
                "C:/Users/u/AppData/Roaming\\Microsoft\\Windows\\Themes\\TranscodedWallpaper"
            ))
        );
        // Win10+ 的当前主题在 %LOCALAPPDATA%，展开失败就会白丢最可靠的候选。
        assert_eq!(
            normalize_path(
                "%LOCALAPPDATA%\\Microsoft\\Windows\\Themes\\Custom.theme",
                &roots
            ),
            Some(PathBuf::from(
                "/home/u/AppData/Local\\Microsoft\\Windows\\Themes\\Custom.theme"
            ))
        );
        assert_eq!(
            normalize_path("~/Library/Desktop Pictures/a.heic", &roots),
            Some(PathBuf::from("/home/u/Library/Desktop Pictures/a.heic"))
        );
        // 认不出的变量不猜：整串保留，随后被「必须绝对路径」这条校验判掉。
        assert_eq!(normalize_path("%NOPE%\\a.jpg", &roots), None);
    }

    #[test]
    fn normalize_rejects_placeholders_relative_and_empty() {
        let roots = roots();
        for raw in [
            "",
            "   ",
            "none",
            "'none'",
            "default",
            "relative/a.png",
            "./a.png",
        ] {
            assert_eq!(
                normalize_path(raw, &roots),
                None,
                "{raw:?} 不该被当成壁纸路径"
            );
        }
    }

    #[test]
    fn absolute_detection_is_host_independent() {
        assert!(looks_absolute("/home/u/a.png"));
        assert!(looks_absolute(r"\\server\share\a.png"));
        assert!(looks_absolute("C:/Users/u/a.png"));
        assert!(looks_absolute(r"C:\Users\u\a.png"));
        assert!(!looks_absolute("a.png"));
        assert!(!looks_absolute("C:"));
    }
}
