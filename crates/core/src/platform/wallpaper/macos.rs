//! macOS 壁纸路径探测：`com.apple.wallpaper` 的 `Index.plist` → 老式 `com.apple.desktop` 域。
//!
//! 两条通道：
//!
//! 1. `~/Library/Application Support/com.apple.wallpaper/Store/Index.plist`——Big Sur 之后
//!    壁纸设置的真实落点，**二进制 plist（`bplist00`）**，里面记着
//!    `file:///…/xxx.heic` 与裸绝对路径；
//! 2. `defaults read com.apple.desktop Background`——Catalina 及更早的每屏壁纸配置
//!    （`ImageFilePath = "/Library/Desktop Pictures/…"`），新系统上该域通常已不存在。
//!
//! # 未在真机验证
//!
//! 本机（Windows）与 CI（Linux）都没有 macOS 通道：**下列代码没有在 macOS 真机上跑过**。
//! 因此这里刻意不引入 plist 解析依赖（`plist` crate 会为「读几个路径串」多背一个依赖），
//! 改成手写的字符串扫描：plist 里的路径串本身是可读 ASCII，按「可打印 ASCII 连续段」切出来
//! 再按「行首/分隔符后的 `/`」认路径开头即可，二进制 plist 的二进制部分天然成为分隔符。
//! 扫出的碎片串由 [`super::Collect`] 的图片头校验兜掉，不会喂给取色器。
//! 真机行为待后续 mission 在 macOS 上补充验证（`Index.plist` 的真实结构、`.heic` 壁纸路径）。

use super::{Collect, SourceKind};

/// Big Sur 之后的壁纸设置存储（相对用户主目录）。
pub(super) const STORE_INDEX: &str =
    "Library/Application Support/com.apple.wallpaper/Store/Index.plist";

/// 老式壁纸配置域。
pub(super) const DESKTOP_DOMAIN: &str = "com.apple.desktop";

/// 一次扫描最多收多少个候选串（防止异常文件把内存打满；正常情况下只有个位数）。
const MAX_SCANNED: usize = 64;

/// 两步依次探测。
pub(super) fn collect(collect: &mut Collect<'_>) {
    store_index(collect);
    desktop_defaults(collect);
}

/// 第一步：壁纸 Store 的 `Index.plist`。
fn store_index(collect: &mut Collect<'_>) {
    let path = collect.roots().home.join(STORE_INDEX);
    let Some(text) = collect.io().read_text(&path) else {
        collect.note(
            SourceKind::MacOsStore.as_str(),
            format!("读不到 {}（Catalina 及更早没有这个文件）", path.display()),
        );
        return;
    };
    let values = scan_string_paths(&text);
    if values.is_empty() {
        collect.note(
            SourceKind::MacOsStore.as_str(),
            format!("{} 里没有路径串", path.display()),
        );
        return;
    }
    let evidence = "Library/Application Support/com.apple.wallpaper/Store/Index.plist 的路径串";
    for value in values {
        collect.add(SourceKind::MacOsStore, evidence, &value);
    }
}

/// 第二步：`defaults read com.apple.desktop Background`（老系统）。
fn desktop_defaults(collect: &mut Collect<'_>) {
    let evidence = format!("defaults read {DESKTOP_DOMAIN} Background");
    match collect
        .io()
        .run("defaults", &["read", DESKTOP_DOMAIN, "Background"])
    {
        Some(text) => {
            let values = scan_defaults_background(&text);
            if values.is_empty() {
                collect.note(
                    SourceKind::MacOsDesktopDefaults.as_str(),
                    format!("{evidence} 里没有 ImageFilePath"),
                );
                return;
            }
            for value in values {
                collect.add(SourceKind::MacOsDesktopDefaults, &evidence, &value);
            }
        }
        None => collect.note(
            SourceKind::MacOsDesktopDefaults.as_str(),
            format!("{evidence} 失败（命令缺失 / 非零退出 / 该域不存在）"),
        ),
    }
}

/// 从 plist 原文里扫出候选串：`file://` URL 与裸绝对路径。
///
/// 输入既可能是 XML plist（`<string>file:///…</string>`）也可能是二进制 plist 的 lossy 文本
/// （可读 ASCII 串之间夹着不可打印字节）。因此先按「可打印 ASCII 连续段」切，再在每段里认：
///
/// - 任意位置的 `file://` 开头；
/// - 「段首 / 分隔符之后 / `~` 之后」的 `/` 开头（路径中间的 `/` 不算新起点，避免同一路径
///   被切成好几段）。
#[must_use]
pub(super) fn scan_string_paths(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for run in printable_runs(text) {
        let mut cursor = 0;
        while let Some(offset) = run[cursor..].find("file://") {
            let start = cursor + offset;
            push_candidate(&mut found, &run[start..]);
            cursor = start + "file://".len();
        }
        for (position, _) in run.match_indices('/') {
            let Some(start) = path_start(run, position) else {
                continue;
            };
            push_candidate(&mut found, &run[start..]);
        }
        if found.len() >= MAX_SCANNED {
            break;
        }
    }
    found.truncate(MAX_SCANNED);
    found
}

/// 解析 `defaults read com.apple.desktop Background` 的文本：只认 `ImageFilePath` / `NewPath`。
#[must_use]
pub(super) fn scan_defaults_background(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let key = key.trim().trim_matches('"');
            if !key.eq_ignore_ascii_case("ImageFilePath") && !key.eq_ignore_ascii_case("NewPath") {
                return None;
            }
            let value = value.trim().trim_end_matches(';').trim().trim_matches('"');
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

/// 可打印 ASCII 连续段（二进制 plist 的二进制字节在这里变成分隔符）。
fn printable_runs(text: &str) -> Vec<&str> {
    text.split(|ch: char| !(ch.is_ascii_graphic() || ch == ' '))
        .map(str::trim)
        .filter(|run| !run.is_empty())
        .collect()
}

/// 判断 `run[position]` 的 `/` 是不是一个路径起点；是则返回该路径在 `run` 里的起点。
fn path_start(run: &str, position: usize) -> Option<usize> {
    let next = run[position + 1..].chars().next()?;
    if !(next.is_ascii_alphanumeric() || next == '_') {
        return None;
    }
    match run[..position].chars().next_back() {
        // 段首：路径直接跟在二进制分隔符后面。
        None => Some(position),
        // `~/Library/...`：连 `~` 一起收，交给路径归一化展开。
        Some('~') => Some(position - 1),
        // 闭合标签 `</string>` 里的那个斜杠不是路径。
        Some('<') => None,
        // 路径中间的 `/`（前面还是路径字符）不当新起点。
        Some(prev) if is_path_char(prev) => None,
        Some(_) => Some(position),
    }
}

fn is_path_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | '%' | '+' | '@' | '~' | ' ')
}

/// plist/XML 序列化里的终止字符（`<string>` 的尖括号、引号、实体）切掉。
fn push_candidate(found: &mut Vec<String>, raw: &str) {
    let end = raw.find(['<', '>', '"', '&', '\'']).unwrap_or(raw.len());
    let candidate = raw[..end].trim_end();
    if !candidate.is_empty() && !found.iter().any(|seen| seen.as_str() == candidate) {
        found.push(candidate.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::super::io::fake::FakeIo;
    use super::super::{detect_with, SourceKind};
    use super::*;
    use crate::platform::paths::{MapEnv, Platform};

    const STORE_PATH: &str =
        "/Users/me/Library/Application Support/com.apple.wallpaper/Store/Index.plist";
    const DEFAULTS_KEY: &str = "defaults read com.apple.desktop Background";

    fn env() -> MapEnv {
        MapEnv::new().with("HOME", "/Users/me")
    }

    fn xml_plist(body: &str) -> String {
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n{body}\n</plist>\n")
    }

    #[test]
    fn store_index_hit_decodes_file_url() {
        let io = FakeIo::new()
            .with_file(
                STORE_PATH,
                &xml_plist("<string>file:///Users/me/Pictures/my%20wall.heic</string>"),
            )
            .with_image("/Users/me/Pictures/my wall.heic");
        let probe = detect_with(&io, &env(), Platform::MacOs);
        assert_eq!(probe.len(), 1, "{:?}", probe.notes());
        let best = probe.best().expect("有候选");
        assert_eq!(best.kind, SourceKind::MacOsStore);
        assert_eq!(
            best.path,
            std::path::PathBuf::from("/Users/me/Pictures/my wall.heic")
        );
    }

    #[test]
    fn multiple_candidates_from_both_channels_keep_order() {
        let io = FakeIo::new()
            .with_file(
                STORE_PATH,
                &xml_plist(
                    "<array>\n\
                     <string>file:///System/Library/Desktop Pictures/a.heic</string>\n\
                     <string>/Users/me/Library/Application Support/com.apple.wallpaper/Store/b.heic</string>\n\
                     </array>",
                ),
            )
            .with_image("/System/Library/Desktop Pictures/a.heic")
            .with_image(
                "/Users/me/Library/Application Support/com.apple.wallpaper/Store/b.heic",
            )
            .with_image("/Library/Desktop Pictures/legacy.jpg")
            .with_command(
                DEFAULTS_KEY,
                "{\n    \"Display Identifier\" = \"Main\";\n    \
                 ImageFilePath = \"/Library/Desktop Pictures/legacy.jpg\";\n}\n",
            );
        let probe = detect_with(&io, &env(), Platform::MacOs);
        let kinds: Vec<SourceKind> = probe.iter().map(|source| source.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SourceKind::MacOsStore,
                SourceKind::MacOsStore,
                SourceKind::MacOsDesktopDefaults,
            ]
        );
        assert_eq!(
            probe.best().expect("有候选").path,
            std::path::PathBuf::from("/System/Library/Desktop Pictures/a.heic")
        );
    }

    #[test]
    fn everything_missing_returns_empty_with_reasons() {
        let probe = detect_with(&FakeIo::new(), &env(), Platform::MacOs);
        assert!(probe.is_empty(), "全部失败要返回空列表");
        let notes = probe.notes().join("\n");
        assert!(notes.contains("Index.plist"), "{notes}");
        assert!(notes.contains("defaults"), "{notes}");
    }

    #[test]
    fn binary_plist_strings_are_scanned_between_binary_bytes() {
        // bplist00 的形态：控制字符/长度字节把可读 ASCII 串隔开。
        let raw = "\u{0}\u{1}\u{2}/Users/me/Pictures/a.heic\u{0}\u{3}file:///Users/me/b.jpg\u{0}";
        let found = scan_string_paths(raw);
        assert_eq!(
            found,
            vec![
                "/Users/me/Pictures/a.heic".to_owned(),
                "file:///Users/me/b.jpg".to_owned(),
            ]
        );
    }

    #[test]
    fn xml_tags_are_not_part_of_the_candidate() {
        let found = scan_string_paths(&xml_plist(
            "<key>Content</key>\n<string>/Users/me/a.png</string>",
        ));
        assert_eq!(found, vec!["/Users/me/a.png".to_owned()]);
    }

    #[test]
    fn defaults_output_parsing() {
        assert_eq!(
            scan_defaults_background(
                "{\n    \"Display Identifier\" = \"Main\";\n    \
                 ImageFilePath = \"/Library/Desktop Pictures/a.jpg\";\n    \
                 NewPath = \"/Users/me/b.png\";\n}\n"
            ),
            vec![
                "/Library/Desktop Pictures/a.jpg".to_owned(),
                "/Users/me/b.png".to_owned(),
            ]
        );
        assert!(scan_defaults_background("{\n    BackgroundColor = \"0 0 0\";\n}\n").is_empty());
    }

    #[test]
    fn tilde_paths_keep_their_prefix() {
        let found = scan_string_paths("<string>~/Library/Desktop Pictures/a.heic</string>");
        assert_eq!(found, vec!["~/Library/Desktop Pictures/a.heic".to_owned()]);
    }
}
