//! 日志与事件流的脱敏（写入前执行，落盘即脱敏）。
//!
//! 规则见 `DevDocs/SubAgentsTalk/launch-接口约定.md` §4：
//! token 整串替换、用户主目录替换、长度 ≥64 的不透明串（base64/hex token）替换。
//! 依据 `analyse/02` §6.4 / §8.4-5（HMCL 只过滤 access token，我们要求导出前全量脱敏）。
//!
//! 第四条规则（M40 从 `auth::body` 上提）：**带点 token** 整串替换。JWT 形状是
//! `header.payload.signature`（XSTS/XBL 票据正是这个），点号把连续段切开，单看
//! 「连续不透明串 ≥64」这条会整串原样放过。规则只在这里实现一份，所有调用方
//! （launch 会话 / 诊断 / auth 错误摘要 …）共用，避免各自补一套后漂移。

use std::borrow::Cow;
use std::path::Path;

use serde_json::Value;

/// access token / session token 的替换标记。
pub const TOKEN_PLACEHOLDER: &str = "<redacted:token>";
/// 用户主目录的替换标记。
pub const HOME_PLACEHOLDER: &str = "<home>";
/// 形似 token 的长不透明串的替换标记。
pub const OPAQUE_PLACEHOLDER: &str = "<redacted:opaque>";

/// 判定“像 token”的最短连续长度。
const MAX_OPAQUE_RUN: usize = 64;
/// 「带点 token」（JWT 三段式）判定的最短总长。
const DOTTED_TOKEN_MIN_TOTAL: usize = 40;
/// 「带点 token」每一段的最短长度（挡住 `Service.Unavailable.today` 这类正常文案）。
const DOTTED_TOKEN_MIN_SEGMENT: usize = 8;

/// 脱敏器：持有需要抹掉的敏感值（token、主目录）。
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    secrets: Vec<String>,
    homes: Vec<String>,
}

impl Redactor {
    /// 空脱敏器（不做任何替换）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一个敏感值（access token、userhash、xuid 等）。
    ///
    /// 太短的值（< 6 字节）会被忽略：替换一个 3 字符串会把正常日志搅乱。
    pub fn with_secret(mut self, secret: impl Into<String>) -> Self {
        let secret = secret.into();
        if secret.trim().len() >= 6 && !self.secrets.contains(&secret) {
            self.secrets.push(secret);
        }
        self
    }

    /// 登记一个需要抹掉的主目录（替换成 `<home>`）。
    pub fn with_home(mut self, home: impl AsRef<Path>) -> Self {
        let home = home.as_ref().to_string_lossy().into_owned();
        if home.len() > 3 && !self.homes.contains(&home) {
            self.homes.push(home);
        }
        self
    }

    /// 登记一个主目录字符串。
    pub fn with_home_str(mut self, home: impl Into<String>) -> Self {
        let home = home.into();
        if home.len() > 3 && !self.homes.contains(&home) {
            self.homes.push(home);
        }
        self
    }

    /// 是否登记了任何规则。
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty() && self.homes.is_empty()
    }

    /// 对文本脱敏；无需修改时返回借用，避免无谓拷贝。
    pub fn scrub<'a>(&self, text: &'a str) -> Cow<'a, str> {
        let mut current: Cow<'a, str> = Cow::Borrowed(text);
        for secret in &self.secrets {
            if current.contains(secret.as_str()) {
                current = Cow::Owned(current.replace(secret.as_str(), TOKEN_PLACEHOLDER));
            }
        }
        for home in &self.homes {
            if current.contains(home.as_str()) {
                current = Cow::Owned(current.replace(home.as_str(), HOME_PLACEHOLDER));
            }
        }
        if has_dotted_token(&current) {
            current = Cow::Owned(mask_dotted_tokens(&current));
        }
        if has_opaque_run(&current) {
            current = Cow::Owned(mask_opaque(&current));
        }
        current
    }

    /// 对整条命令行脱敏（`cmd_built.argv` 必然含 accessToken，必须走这里）。
    pub fn scrub_all(&self, values: &[String]) -> Vec<String> {
        values
            .iter()
            .map(|value| self.scrub(value).into_owned())
            .collect()
    }

    /// 递归脱敏一个 JSON 值。
    ///
    /// **只改字符串**：数值/布尔/null 与字段名原样保留——满足「launch 只做脱敏、不改写字段」
    /// 的契约（`DevDocs/SubAgentsTalk/launch-接口约定.md` §9.3）。诊断结论里的证据行常带
    /// 崩溃报告/`hs_err` 的绝对路径，正是要过 `<home>` 规则的地方。
    pub fn scrub_json(&self, value: &Value) -> Value {
        match value {
            Value::String(text) => Value::String(self.scrub(text).into_owned()),
            Value::Array(items) => {
                Value::Array(items.iter().map(|item| self.scrub_json(item)).collect())
            }
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(key, value)| (key.clone(), self.scrub_json(value)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
}

/// 文本里是否含长度 ≥64 的“不透明串”（base64/hex token 的特征）。
pub fn has_opaque_run(text: &str) -> bool {
    let mut run = 0usize;
    for ch in text.chars() {
        if is_opaque_char(ch) {
            run += 1;
            if run >= MAX_OPAQUE_RUN {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// 把长度 ≥64 的不透明串替换成 [`OPAQUE_PLACEHOLDER`]。
pub fn mask_opaque(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut run_start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_opaque_char(ch) {
            if run_start.is_none() {
                run_start = Some(idx);
            }
        } else {
            if let Some(start) = run_start.take() {
                push_masked(&mut out, &text[start..idx]);
            }
            out.push(ch);
        }
    }
    if let Some(start) = run_start {
        push_masked(&mut out, &text[start..]);
    }
    out
}

fn push_masked(out: &mut String, run: &str) {
    if run.len() >= MAX_OPAQUE_RUN {
        out.push_str(OPAQUE_PLACEHOLDER);
    } else {
        out.push_str(run);
    }
}

fn is_opaque_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '=' | '_' | '-')
}

/// 文本里是否含「带点 token」（≥3 段、每段 ≥8、总长 ≥40）。
pub fn has_dotted_token(text: &str) -> bool {
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_dotted_token_char(ch) {
            start.get_or_insert(idx);
            continue;
        }
        if let Some(from) = start.take() {
            if is_dotted_token(&text[from..idx]) {
                return true;
            }
        }
    }
    start.is_some_and(|from| is_dotted_token(&text[from..]))
}

/// 把「带点 token」整串替换成 [`TOKEN_PLACEHOLDER`]（段外文本原样保留）。
pub fn mask_dotted_tokens(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if is_dotted_token_char(ch) {
            start.get_or_insert(idx);
            continue;
        }
        if let Some(from) = start.take() {
            push_dotted(&mut out, &text[from..idx]);
        }
        out.push(ch);
    }
    if let Some(from) = start {
        push_dotted(&mut out, &text[from..]);
    }
    out
}

fn push_dotted(out: &mut String, run: &str) {
    if is_dotted_token(run) {
        out.push_str(TOKEN_PLACEHOLDER);
    } else {
        out.push_str(run);
    }
}

/// 「带点 token」的字符集：比不透明串多一个 `.`（JWT 的分段符）。
fn is_dotted_token_char(ch: char) -> bool {
    is_opaque_char(ch) || ch == '.'
}

/// 单段连写（无空白）的串是否像 JWT：段数 ≥3、每段 ≥8 字节、总长 ≥40。
fn is_dotted_token(run: &str) -> bool {
    run.len() >= DOTTED_TOKEN_MIN_TOTAL
        && run.split('.').count() >= 3
        && run
            .split('.')
            .all(|segment| segment.len() >= DOTTED_TOKEN_MIN_SEGMENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实形状的 JWT（header.payload.signature，三段各 ≥8）。
    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJlLXZhbHVlLXRoYXQtaXMtbG9uZy1lbm91Z2g";

    #[test]
    fn jwt_token_is_masked_without_any_registered_secret() {
        let scrubbed = Redactor::new().scrub(JWT).into_owned();
        assert!(scrubbed.contains(TOKEN_PLACEHOLDER), "{scrubbed}");
        assert!(!scrubbed.contains("eyJhbGci"), "{scrubbed}");

        // 嵌在别的内容里也只换 token 本体。
        let masked = Redactor::new()
            .scrub(&format!("authorization: Bearer {JWT}, retry=1"))
            .into_owned();
        assert_eq!(
            masked,
            format!("authorization: Bearer {TOKEN_PLACEHOLDER}, retry=1")
        );
    }

    #[test]
    fn dotted_token_rule_does_not_eat_normal_text() {
        // 分段太短、总长不够、段数不够：三种都不算 token。
        for text in [
            "Service.Unavailable.today",
            "1.2.0",
            "xbl.xsts.entitlements.profile",
            "eyJhbGciOiJIUzI1NiJ9.short.abc",
        ] {
            assert!(!has_dotted_token(text), "{text}");
            assert_eq!(Redactor::new().scrub(text), text, "{text}");
        }
    }

    #[test]
    fn existing_rules_still_apply() {
        let redactor = Redactor::new()
            .with_secret("SECRET-TOKEN-123456")
            .with_home_str("/home/alice");
        let scrubbed = redactor
            .scrub("/home/alice/game.log: token=SECRET-TOKEN-123456")
            .into_owned();
        assert!(scrubbed.contains(HOME_PLACEHOLDER), "{scrubbed}");
        assert!(scrubbed.contains(TOKEN_PLACEHOLDER), "{scrubbed}");
        assert!(
            !scrubbed.contains("alice") && !scrubbed.contains("SECRET"),
            "{scrubbed}"
        );

        // 没有点号的长不透明串仍走 ≥64 规则，替换标记也不同。
        let long = "A".repeat(MAX_OPAQUE_RUN);
        assert!(has_opaque_run(&long));
        assert!(!has_dotted_token(&long));
        assert_eq!(
            Redactor::new().scrub(&long).into_owned(),
            OPAQUE_PLACEHOLDER.to_string()
        );
    }
}
