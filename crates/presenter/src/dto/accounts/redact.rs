//! 凭据红线的**形状脱敏**：把 JWT / 不透明 token / `token=...` 这类串从任何要上屏的文案里抹掉。
//!
//! ## 为什么还要第二道脱敏器
//!
//! `core::launch::redact` 是**按已知明文值**替换的（拿着会话里的 token 原串去抹）。这里要挡的是
//! **没登记过的**字符串：服务端在错误响应里回显一个 token、HTTP 库把 header 拼进错误文案、
//! 某段调试串被画到面板上。两者互补，缺一不可：
//!
//! | 场景 | `core::launch::redact` | 本模块 |
//! |---|---|---|
//! | 已知会话 token 出现在日志里 | ✅ | ❌（不知道明文） |
//! | 服务端回包里的陌生 JWT 被画到错误面板 | ❌ | ✅ |
//!
//! 走形状识别（三段 base64url / 超长不透明串 / 敏感键名）就够拦住「一眼能看出是凭据」的串，
//! 而普通文案（URL、用户码 `ABCD-EFGH`、服务器名）**一个字都不动**——脱敏器本身不能变成噪音源。

/// 脱敏占位符（**故意刺眼**：宁可难看，也不许静默把凭据画到屏幕上）。
pub const REDACTED: &str = "⟪redacted⟫";

/// 把一段要上屏的文案里的凭据形状串抹掉（空白与其余字符**原样保留**）。
#[must_use]
pub fn redact(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in text.split_inclusive(char::is_whitespace) {
        let split = piece.find(char::is_whitespace).unwrap_or(piece.len());
        let (chunk, space) = piece.split_at(split);
        out.push_str(&redact_chunk(chunk));
        out.push_str(space);
    }
    out
}

/// 单个「词」的脱敏：① 敏感键名的值 ② 整个词本身就是凭据形状。
fn redact_chunk(chunk: &str) -> String {
    if let Some((key, _value)) = chunk.split_once('=') {
        if is_sensitive_key(key) || looks_opaque(chunk) {
            return format!("{key}={REDACTED}");
        }
    }
    if looks_opaque(chunk) {
        return REDACTED.to_string();
    }
    chunk.to_string()
}

/// 键名是否属于「它的值一定是凭据」（`accessToken` / `refresh_token` / `password` …）。
fn is_sensitive_key(key: &str) -> bool {
    let key = key
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .to_lowercase();
    [
        "token",
        "secret",
        "password",
        "passwd",
        "authorization",
        "bearer",
        "session",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

/// 词的形状是否像凭据：① 三段 base64url（JWT）② 长度 ≥40 的不透明串。
fn looks_opaque(chunk: &str) -> bool {
    let trimmed = chunk.trim_matches(|c: char| {
        !c.is_ascii_alphanumeric() && c != '_' && c != '-' && c != '.' && c != '+'
    });
    looks_like_jwt(trimmed) || (trimmed.len() >= 40 && trimmed.chars().all(is_base64url))
}

/// 是否是 JWT 形状：`header.payload.signature`，三段都非空且够长、全是 base64url 字符。
///
/// 公开是为了单测能直接钉这条判据（UI 侧「注入 JWT 必须被抹掉」的测试也用它）。
#[must_use]
pub fn looks_like_jwt(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| part.len() >= 8 && part.chars().all(is_base64url))
}

/// base64url 字母表（含 `=` 填充与 `+`/`/` 的宽容分支）。
fn is_base64url(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '/' | '=' | '.')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 标准 JWT 形状（2 段 header/payload + 1 段签名）。
    const JWT: &str =
        "eyJhbGciOiJSUzI1NiIsIng1dCI6IkFCQyJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVl";

    #[test]
    fn the_jwt_shape_judgement_is_not_a_length_heuristic() {
        assert!(looks_like_jwt(JWT));
        assert!(!looks_like_jwt("not.a.jwt"), "短段不算 JWT");
        assert!(!looks_like_jwt("a.b.c.d"), "四段不算 JWT");
        assert!(!looks_like_jwt("ABCD-EFGH"), "设备码不是 JWT");
    }

    #[test]
    fn bare_jwts_and_sensitive_pairs_are_redacted() {
        assert_eq!(redact(JWT), REDACTED);
        assert_eq!(
            redact(&format!("Bearer {JWT}")),
            format!("Bearer {REDACTED}")
        );
        assert_eq!(
            redact(&format!("accessToken={JWT}")),
            format!("accessToken={REDACTED}")
        );
        // 敏感键名 + 短值（真实 refresh token 常常很短，靠键名兜）
        assert_eq!(redact("password=hunter2"), format!("password={REDACTED}"));
        assert_eq!(
            redact("refresh_token=abc"),
            format!("refresh_token={REDACTED}")
        );
    }

    #[test]
    fn ordinary_copy_is_left_byte_for_byte() {
        for text in [
            "在浏览器打开 https://www.microsoft.com/link 并输入 ABCD-EFGH",
            "服务器不可达：dns error",
            "HTTP 403 Forbidden — 该服务器要求白名单",
            "",
        ] {
            assert_eq!(redact(text), text, "普通文案被改了：{text}");
        }
    }

    #[test]
    fn whitespace_and_line_breaks_survive() {
        let text = format!("第一行 token={JWT}\n\n第二行  缩进");
        let out = redact(&text);
        assert!(out.contains("\n\n"), "{out}");
        assert!(out.contains("  缩进"), "{out}");
        assert!(!out.contains(JWT), "{out}");
    }
}
