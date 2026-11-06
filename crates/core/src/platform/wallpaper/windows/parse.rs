//! Windows 侧的纯解析函数：`.theme` 的 `Wallpaper=` 行与 `reg query` 输出的取值。
//!
//! 单独一个文件只为守住「单文件 ≤400 行」（`guards.yml` 的 file-size 闸）：这里没有任何
//! IO，全部是纯函数，跨平台编译，夹具用例覆盖在文件尾部。

/// 取 `.theme` 文本里所有 `Wallpaper=` 行的值（键名大小写不敏感，值去首尾空白）。
///
/// `.theme` 是 INI：`[Control Panel\Desktop]` 段下的 `Wallpaper=...`。`Wallpaper` 这个键名
/// 本身不随系统语言变（Windows 只本地化少数键），所以直接按 ASCII 比对。
pub(super) fn parse_theme_wallpaper(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            if !key.trim().eq_ignore_ascii_case("wallpaper") {
                return None;
            }
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_owned())
        })
        .collect()
}

/// 取 `reg query <键> /v <值名>` 输出里某个值的原文。
///
/// 形如 `    WallPaper    REG_SZ    C:\Users\me\Pictures\a.jpg`；值里的空格要保留，所以
/// 「值名 + 类型」之后的部分整段取原文（不能 `split_whitespace` 再拼，会吃掉连续空格）。
/// 值名为 `WallPaper` 时是壁纸路径，`CurrentTheme` 时是 `.theme` 文件路径。
pub(super) fn parse_reg_sz_value(text: &str, value_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let name_end = trimmed.find(char::is_whitespace)?;
        if !trimmed[..name_end].eq_ignore_ascii_case(value_name) {
            return None;
        }
        let after_name = trimmed[name_end..].trim_start();
        let kind_end = after_name.find(char::is_whitespace)?;
        let value = after_name[kind_end..].trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::super::{CURRENT_THEME_VALUE, WALLPAPER_VALUE};
    use super::*;

    const CUSTOM_THEME: &str = r"C:\Users\u\AppData\Local\Microsoft\Windows\Themes\Custom.theme";

    #[test]
    fn theme_text_yields_only_wallpaper_values() {
        let text = "[Control Panel\\Desktop]\r\n\
                    Wallpaper=C:\\Users\\u\\a.jpg\r\n\
                    wallpaper = %SystemRoot%\\web\\wallpaper\\b.jpg\r\n\
                    [Slideshow]\r\n\
                    ImagesRootPath=C:\\Users\\u\\Pictures\r\n\
                    Wallpaper=\r\n";
        assert_eq!(
            parse_theme_wallpaper(text),
            vec![
                r"C:\Users\u\a.jpg".to_owned(),
                r"%SystemRoot%\web\wallpaper\b.jpg".to_owned(),
            ]
        );
        assert!(parse_theme_wallpaper("").is_empty());
    }

    #[test]
    fn reg_output_parses_value_and_keeps_spaces() {
        assert_eq!(
            parse_reg_sz_value(
                "HKEY_CURRENT_USER\\Control Panel\\Desktop\r\n    \
                 WallPaper    REG_SZ    C:\\Users\\Jak e\\Desktop Pictures\\a.jpg\r\n",
                WALLPAPER_VALUE
            ),
            Some(r"C:\Users\Jak e\Desktop Pictures\a.jpg".to_owned())
        );
        // 纯色壁纸 = 空值。
        assert_eq!(
            parse_reg_sz_value("    WallPaper    REG_SZ    \r\n", WALLPAPER_VALUE),
            None
        );
        assert_eq!(parse_reg_sz_value("", WALLPAPER_VALUE), None);
        assert_eq!(
            parse_reg_sz_value("    TileWallpaper    REG_SZ    0\r\n", WALLPAPER_VALUE),
            None
        );
        // 当前主题：值名换成 CurrentTheme，取到的是 `.theme` 的绝对路径。
        assert_eq!(
            parse_reg_sz_value(
                &format!("    CurrentTheme    REG_SZ    {CUSTOM_THEME}\r\n"),
                CURRENT_THEME_VALUE
            ),
            Some(CUSTOM_THEME.to_owned())
        );
        assert_eq!(
            parse_reg_sz_value("    CurrentTheme    REG_SZ    \r\n", CURRENT_THEME_VALUE),
            None
        );
    }
}
