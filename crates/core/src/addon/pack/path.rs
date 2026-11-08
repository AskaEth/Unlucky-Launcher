//! 包内路径规整：路径穿越**第一检**（第二检是 `zip` crate 的 `enclosed_name()`，见 `archive`）。
//!
//! 判据不是"看起来安全"，而是一条条写死的规则。任一条不满足就报
//! `crate::addon::pack::error::PackErrorKind::Traversal`：
//!
//! 1. 空名字拒绝（`""` 与全 `/` 都不成路径）；
//! 2. `\0` 拒绝（底层会被截断，产生"看得见查不着"的名字）；
//! 3. `\` 拒绝——zip 规范要求 `/` 分隔，但 Win32 把 `\` 也当分隔符，于是
//!    `mods\..\..\evil` 在"按 `/` 切"的检查里是一个普通名字、在落盘时却跳三层；
//! 4. `:` 拒绝——盘符（`C:/x`）与 NTFS 交换数据流（`file:Zone.Identifier`）的共同记号；
//! 5. 前导 `/` 拒绝（绝对路径）；
//! 6. 任何 `..` 段拒绝（不"抵消"后放行：`a/../b` 与 `../b` 一律拒，避免归一化差异）；
//! 7. `.` 段与空段（`a//b`）**丢弃**（纯噪声，不是穿越手法）。
//!
//! 规整后的名字一律 `/` 分隔、无前导斜杠、无 `.` / `..` 段，因此可以安全地
//! `root.join(name)`——但**写盘前仍要再查一次**（`archive` 的 `sha1`/`read` 与 `format` 的
//! `write` 都重新过一遍本模块），因为模型可能来自外部构造而非本模块的读者。

use super::error::{PackError, PackResult};

/// 规整一条**文件**条目名（目录条目请用 [`normalize_dir`]）。
///
/// 结尾的 `/` 视为目录标记并被去掉；去掉后为空则拒绝。
pub fn normalize_file(raw: &str) -> PackResult<String> {
    let text = normalize_text(raw)?;
    if text.is_empty() {
        return Err(PackError::traversal(format!("包内路径为空：{raw:?}")));
    }
    Ok(text)
}

/// 规整一条**目录**前缀（例如 CF 的 `overrides`、mrpack 的 `client-overrides`）。
pub fn normalize_dir(raw: &str) -> PackResult<String> {
    let text = normalize_text(raw)?;
    if text.is_empty() {
        return Err(PackError::traversal(format!("包内目录为空：{raw:?}")));
    }
    Ok(text)
}

/// 文件名是否以 `/` 结尾（zip 的目录条目约定）。
#[must_use]
pub fn has_trailing_slash(raw: &str) -> bool {
    raw.ends_with('/')
}

/// 规则 1–7 的实现。返回可能为空的规整串（由调用方判定"空"是否合法）。
fn normalize_text(raw: &str) -> PackResult<String> {
    if raw.is_empty() {
        return Err(PackError::traversal("包内路径为空"));
    }
    if raw.contains('\0') {
        return Err(PackError::traversal(format!("包内路径含 NUL：{raw:?}")));
    }
    if raw.contains('\\') {
        return Err(PackError::traversal(format!(
            "包内路径含反斜杠（Win32 会当分隔符）：{raw:?}"
        )));
    }
    if raw.contains(':') {
        return Err(PackError::traversal(format!(
            "包内路径含冒号（盘符 / NTFS 数据流）：{raw:?}"
        )));
    }
    if raw.starts_with('/') {
        return Err(PackError::traversal(format!("包内路径是绝对路径：{raw:?}")));
    }

    let mut segments: Vec<&str> = Vec::new();
    for segment in raw.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                return Err(PackError::traversal(format!(
                    "包内路径含 `..` 上跳：{raw:?}"
                )));
            }
            other => segments.push(other),
        }
    }
    Ok(segments.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addon::pack::error::PackErrorKind;

    #[test]
    fn keeps_plain_relative_names() {
        assert_eq!(normalize_file("mods/jei.jar").unwrap(), "mods/jei.jar");
        assert_eq!(
            normalize_file("config/a/b.toml").unwrap(),
            "config/a/b.toml"
        );
        // `.` 段与空段是噪声，丢掉而不是报错。
        assert_eq!(normalize_file("./mods//jei.jar").unwrap(), "mods/jei.jar");
        assert_eq!(normalize_dir("overrides/").unwrap(), "overrides");
        assert_eq!(
            normalize_dir("client-overrides//").unwrap(),
            "client-overrides"
        );
    }

    #[test]
    fn rejects_traversal_shapes() {
        for raw in [
            "../evil.jar",
            "mods/../../evil.jar",
            "a/../b",
            "/etc/passwd",
            "//server/share",
            "C:/Windows/evil.dll",
            "mods\\..\\..\\evil.jar",
            "mods/jei.jar:Zone.Identifier",
            "mods/\0jei.jar",
            "",
            "/",
            "./",
        ] {
            let error = normalize_file(raw).expect_err(raw);
            assert_eq!(
                error.kind(),
                PackErrorKind::Traversal,
                "应判为穿越，实际 {raw:?} → {error}"
            );
        }
    }

    #[test]
    fn trailing_slash_marks_directories() {
        assert!(has_trailing_slash("overrides/"));
        assert!(!has_trailing_slash("overrides"));
        // 目录标记不改变规整结果：`overrides/` 与 `overrides` 是同一个前缀。
        assert_eq!(
            normalize_dir("overrides/").unwrap(),
            normalize_dir("overrides").unwrap()
        );
    }
}
