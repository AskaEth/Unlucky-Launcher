//! 凭据值包装：**打印出来永远是 `<redacted>`**。
//!
//! 依据 `DevDocs/PlanDoc/07-accounts.md` §3 与 `DevDocs/AGENTS.md` §6 的凭据红线：
//! token/密码永不进日志、事件流、stdout、测试快照。HMCL 靠「往日志过滤器注册 token」
//! （`analyse/04` §11 观察项 9：绕开 Logger 就失效），我们在**类型层**堵：
//! 只要某值被包成 [`Secret`]，`Debug`/`Display`/`{:?}`/`{}` 都拿不到明文，
//! 取明文必须显式写 `.expose()`（一眼可搜、CR 可审）。
//!
//! 落盘仍然需要明文（凭据文件），因此保留 serde 透明序列化——序列化不是打印。

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::launch::redact::{Redactor, TOKEN_PLACEHOLDER};

/// `Debug`/`Display` 与脱敏替换的统一标记。
pub const REDACTED: &str = "<redacted>";

/// 一个凭据值（access token / refresh token / client token / 密码）。
///
/// 相等性按明文比较（刷新后判断「token 有没有变」要用到），哈希同理。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    /// 包住一个明文值。
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// 取明文。**唯一取明文入口**：出现处都应能说清「这是发请求，不是打印」。
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// 是否为空（空 token 等价于「没有凭据」）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 明文字节长度（判空/判占位长度用，不泄露内容）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(REDACTED)
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for Secret {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

/// 生成 32 个十六进制字符的随机串（离线 accessToken 占位、条目 id）。
///
/// **不是密码学随机**（同 `game::ProfileId::generate` 的取舍）：混合纳秒时间戳、进程 id、
/// 进程内计数器后过 SplitMix64。这些值不参与任何服务端校验（HMCL 同样用
/// `UUID.randomUUID().toString()` 的紧凑形式当占位），唯一性够用即可。
#[must_use]
pub fn random_hex32() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos() as u64);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut seed = nanos ^ u64::from(std::process::id()).rotate_left(17) ^ seq.rotate_left(41);

    let mut out = String::with_capacity(32);
    let mut state = seed;
    while out.len() < 32 {
        // 两个 u64 混合出 16 个 hex 字符一轮。
        state = split_mix64(state);
        seed ^= split_mix64(seed.rotate_left(29) ^ state);
        out.push_str(&format!("{:016x}", seed));
    }
    out.truncate(32);
    out
}

fn split_mix64(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// 用一组凭据武装一个日志脱敏器（`launch::redact::Redactor` 的唯一构造入口）。
///
/// 太短的值（< 6 字节）会被 `Redactor` 自动忽略，避免把正常文本搅乱。
#[must_use]
pub fn redactor_for<'a>(secrets: impl IntoIterator<Item = &'a Secret>) -> Redactor {
    let mut redactor = Redactor::new();
    for secret in secrets {
        if !secret.is_empty() {
            redactor = redactor.with_secret(secret.expose().to_string());
        }
    }
    redactor
}

/// 对一段文本做凭据脱敏（走 `launch::redact` 的三条规则：已登记凭据、长度 ≥64 不透明串、
/// 带点 token/JWT）。
#[must_use]
pub fn scrub<'a>(text: &'a str, secrets: &[&Secret]) -> Cow<'a, str> {
    let owned: Vec<Secret> = secrets.iter().map(|secret| (*secret).clone()).collect();
    redactor_for(owned.iter()).scrub(text)
}

/// 占位常量转出（给调用方断言 / 拼提示用）。
#[must_use]
pub const fn token_placeholder() -> &'static str {
    TOKEN_PLACEHOLDER
}

#[cfg(test)]
mod tests {
    use super::{random_hex32, scrub, Secret, REDACTED, TOKEN_PLACEHOLDER};

    #[test]
    fn debug_and_display_never_leak() {
        let secret = Secret::new("SUPER-SECRET-TOKEN-VALUE");
        assert_eq!(format!("{secret:?}"), REDACTED);
        assert_eq!(format!("{secret}"), REDACTED);
        assert!(!format!("{secret:?}").contains("SUPER"));
        let bundle = format!("{:?}", vec![secret.clone()]);
        assert!(!bundle.contains("SECRET"), "{bundle}");
        assert_eq!(secret.expose(), "SUPER-SECRET-TOKEN-VALUE");
    }

    #[test]
    fn serde_roundtrip_keeps_plaintext() {
        let secret = Secret::new("abc123");
        let text = serde_json::to_string(&secret).expect("序列化");
        assert_eq!(text, "\"abc123\"");
        let back: Secret = serde_json::from_str(&text).expect("反序列化");
        assert_eq!(back, secret);
    }

    #[test]
    fn scrub_replaces_registered_secret() {
        let secret = Secret::new("0f8fad5bd9cb469fa16570867728950e");
        let text = format!("--accessToken {} --uuid abc", secret.expose());
        let scrubbed = scrub(&text, &[&secret]).into_owned();
        assert!(!scrubbed.contains("0f8fad"));
        assert!(scrubbed.contains(TOKEN_PLACEHOLDER));
    }

    #[test]
    fn random_hex32_is_32_hex_and_unique() {
        let first = random_hex32();
        let second = random_hex32();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));
        assert_ne!(first, second);
    }
}
