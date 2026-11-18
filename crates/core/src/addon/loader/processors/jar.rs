//! 从 processor jar 的 `MANIFEST.MF` 里取 `Main-Class`。
//!
//! 官方 install profile **不写主类**（`processors[].jar` 只给坐标），主类在 jar 的清单里；
//! 少数 fork 会额外给 `mainClass`（那条路走 [`super::plan::ProcessorStep::main_class`]，
//! 不到这里）。清单按 RFC 822 折行：续行以**单个空格**开头。

use std::path::Path;

use super::error::ProcessorError;

/// 读 jar 的 `Main-Class`。
///
/// # Errors
///
/// jar 打不开/不是 zip（IO/规划错误）、没有 `META-INF/MANIFEST.MF` 或清单里没有
/// `Main-Class`（规划错误）。
pub(super) fn read_main_class(jar: &Path) -> Result<String, ProcessorError> {
    use std::io::Read;

    let file = std::fs::File::open(jar).map_err(|error| {
        ProcessorError::io(format!(
            "打开 processor jar 失败（{}）：{error}",
            jar.display()
        ))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        ProcessorError::plan(format!("{} 不是合法 jar：{error}", jar.display()))
    })?;
    let mut manifest = String::new();
    {
        let mut entry = archive.by_name("META-INF/MANIFEST.MF").map_err(|error| {
            ProcessorError::plan(format!(
                "{} 里没有 META-INF/MANIFEST.MF（拿不到主类）：{error}",
                jar.display()
            ))
        })?;
        entry.read_to_string(&mut manifest).map_err(|error| {
            ProcessorError::io(format!("读 {} 的 MANIFEST 失败：{error}", jar.display()))
        })?;
    }
    parse_main_class(&manifest).ok_or_else(|| {
        ProcessorError::plan(format!(
            "{} 的 MANIFEST 里没有 Main-Class（install profile 也没给 mainClass）",
            jar.display()
        ))
    })
}

/// 从 MANIFEST 文本里取 `Main-Class`（续行以单个空格开头）。
fn parse_main_class(manifest: &str) -> Option<String> {
    let mut lines = manifest.lines().peekable();
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("Main-Class:") else {
            continue;
        };
        let mut value = rest.trim().to_string();
        while let Some(next) = lines.peek() {
            let Some(continuation) = next.strip_prefix(' ') else {
                break;
            };
            value.push_str(continuation.trim());
            lines.next();
        }
        return (!value.is_empty()).then_some(value);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::parse_main_class;

    #[test]
    fn parses_main_class_with_continuation_lines() {
        let manifest = "Manifest-Version: 1.0\r\nMain-Class: com.example.\r\n Tool\r\n\r\n";
        assert_eq!(
            parse_main_class(manifest).as_deref(),
            Some("com.example.Tool")
        );
        assert!(parse_main_class("Manifest-Version: 1.0\n").is_none());
        assert!(parse_main_class("Main-Class:   \n").is_none());
    }
}
