//! 语义化版本：解析、比较、预发布阶段识别。
//!
//! 排序规则严格按 semver 2.0 §11：
//!
//! 1. 主 / 次 / 修订号按**数值**比（不是字符串比，所以 `0.9.0 < 0.10.0`）；
//! 2. 带预发布标识的版本**小于**同号正式版（`1.0.0-rc.1 < 1.0.0`）；
//! 3. 预发布标识逐段比：纯数字段按数值比、其余按 ASCII 字典序比；纯数字段**小于**任何
//!    非数字段；前缀相同时段数少的更小（`1.0.0-alpha < 1.0.0-alpha.1`）；
//! 4. 构建元数据（`+` 之后）**既不参与比较也不参与相等判定**。
//!
//! `alpha < beta < rc` 不是特例：ASCII 里 `a < b < r`，规则 3 自然得出
//! `1.0.0-alpha.1 < 1.0.0-beta.1 < 1.0.0-rc.1 < 1.0.0`。**别**在这里加「阶段权重表」，
//! 一旦加了，`1.0.0-preview` 与 `1.0.0-rc` 的相对顺序就和 semver 生态不一致了。
//!
//! 拒绝的写法（宁可解析失败，也别让「看起来像版本」的字符串混进清单）：段数不是三段、
//! 前导零（`01.2.3`）、空标识段（`1.0.0-`、`1.0.0-alpha..1`）、非 ASCII 标识字符、
//! 留空的构建元数据。唯一放宽的是可选前缀 `v`（`v1.4.2`），因为 git tag 普遍这么写。

use std::cmp::Ordering;
use std::fmt;

/// 版本号解析失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionError {
    input: String,
    reason: String,
}

impl VersionError {
    /// 原始输入（已 trim，但保留 `v` 前缀这种细节）。
    #[must_use]
    pub fn input(&self) -> &str {
        &self.input
    }

    /// 具体原因。
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "版本号 `{}` 不合法：{}", self.input, self.reason)
    }
}

impl std::error::Error for VersionError {}

fn invalid(input: &str, reason: impl Into<String>) -> VersionError {
    VersionError {
        input: input.trim().to_string(),
        reason: reason.into(),
    }
}

/// 预发布标识的一段。
///
/// 刻意不对外暴露：外部能构造出 `Alpha("01")` 这种违反 semver 不变量的值，
/// 排序与相等就自相矛盾了。读方向只给 [`Version::prerelease`] 的字符串形式。
#[derive(Debug, Clone, PartialEq, Eq)]
enum Identifier {
    Numeric(u64),
    Alpha(String),
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric(value) => write!(f, "{value}"),
            Self::Alpha(text) => f.write_str(text),
        }
    }
}

impl Ord for Identifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(left), Self::Numeric(right)) => left.cmp(right),
            (Self::Numeric(_), Self::Alpha(_)) => Ordering::Less,
            (Self::Alpha(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::Alpha(left), Self::Alpha(right)) => left.cmp(right),
        }
    }
}

impl PartialOrd for Identifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 预发布阶段（渠道准入判定用，见 [`crate::update::Channel::admits_stage`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Stage {
    /// 正式版（没有预发布标识）。
    Stable,
    /// `rc` / `pre` / `preview`。
    ReleaseCandidate,
    /// `beta` / `b`。
    Beta,
    /// `alpha` / `a`。
    Alpha,
    /// 认不出的预发布标识（只有 `nightly` 渠道收）。
    Unknown,
}

impl Stage {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::ReleaseCandidate => "rc",
            Self::Beta => "beta",
            Self::Alpha => "alpha",
            Self::Unknown => "unknown",
        }
    }
}

/// 版本号。
///
/// 字段私有：构造只走 [`Version::parse`] 与 [`Version::new`]，不变量（前导零、空标识段）
/// 因此只需要守一处。
#[derive(Debug, Clone)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<Identifier>,
    build: Option<String>,
}

impl Version {
    /// 正式版（`major.minor.patch`，无预发布、无构建元数据）。
    #[must_use]
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            pre: Vec::new(),
            build: None,
        }
    }

    /// 解析 `MAJOR.MINOR.PATCH[-PRE][+BUILD]`（可选前缀 `v`）。
    ///
    /// # Errors
    ///
    /// 任何不符合 semver 的写法都返回 [`VersionError`]，错误里带原始输入与原因。
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let trimmed = text.trim();
        let body = trimmed
            .strip_prefix('v')
            .or_else(|| trimmed.strip_prefix('V'))
            .unwrap_or(trimmed);

        let (body, build) = match body.split_once('+') {
            Some((head, build)) => (head, Some(build)),
            None => (body, None),
        };
        if let Some(build) = build {
            if build.split('.').any(|segment| !is_identifier(segment)) {
                return Err(invalid(text, "构建元数据含非法字符或空段"));
            }
        }

        let (core, pre) = match body.split_once('-') {
            Some((head, pre)) => (head, Some(pre)),
            None => (body, None),
        };

        let mut numbers = core.split('.');
        let major = segment_number(text, numbers.next(), "主版本号")?;
        let minor = segment_number(text, numbers.next(), "次版本号")?;
        let patch = segment_number(text, numbers.next(), "修订号")?;
        if numbers.next().is_some() {
            return Err(invalid(text, "只接受三段数字（主.次.修订）"));
        }

        let pre = match pre {
            Some(pre) => parse_prerelease(text, pre)?,
            None => Vec::new(),
        };

        Ok(Self {
            major,
            minor,
            patch,
            pre,
            build: build.map(str::to_string),
        })
    }

    /// 主版本号。
    #[must_use]
    pub const fn major(&self) -> u64 {
        self.major
    }

    /// 次版本号。
    #[must_use]
    pub const fn minor(&self) -> u64 {
        self.minor
    }

    /// 修订号。
    #[must_use]
    pub const fn patch(&self) -> u64 {
        self.patch
    }

    /// 预发布标识的原始写法（`None` = 正式版）。
    #[must_use]
    pub fn prerelease(&self) -> Option<String> {
        if self.pre.is_empty() {
            return None;
        }
        Some(
            self.pre
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("."),
        )
    }

    /// 构建元数据（`+` 之后，不参与比较）。
    #[must_use]
    pub fn build_metadata(&self) -> Option<&str> {
        self.build.as_deref()
    }

    /// 是否预发布。
    #[must_use]
    pub fn is_prerelease(&self) -> bool {
        !self.pre.is_empty()
    }

    /// 预发布阶段（正式版为 [`Stage::Stable`]）。
    #[must_use]
    pub fn stage(&self) -> Stage {
        match self.pre.first() {
            None => Stage::Stable,
            Some(Identifier::Numeric(_)) => Stage::Unknown,
            Some(Identifier::Alpha(name)) => match name.as_str() {
                "alpha" | "a" => Stage::Alpha,
                "beta" | "b" => Stage::Beta,
                "rc" | "pre" | "preview" => Stage::ReleaseCandidate,
                _ => Stage::Unknown,
            },
        }
    }
}

fn segment_number(whole: &str, segment: Option<&str>, label: &str) -> Result<u64, VersionError> {
    let Some(segment) = segment else {
        return Err(invalid(whole, "段数不足：需要 `主.次.修订` 三段"));
    };
    if segment.is_empty() {
        return Err(invalid(whole, format!("{label}为空")));
    }
    if !segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid(whole, format!("{label}含非数字字符")));
    }
    if segment.len() > 1 && segment.starts_with('0') {
        return Err(invalid(whole, format!("{label}不得有前导零")));
    }
    segment
        .parse::<u64>()
        .map_err(|_| invalid(whole, format!("{label}超出 u64 范围")))
}

fn is_identifier(segment: &str) -> bool {
    !segment.is_empty()
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn parse_prerelease(whole: &str, pre: &str) -> Result<Vec<Identifier>, VersionError> {
    let mut out = Vec::new();
    for segment in pre.split('.') {
        if !is_identifier(segment) {
            return Err(invalid(whole, "预发布标识含非法字符或空段"));
        }
        if segment.bytes().all(|byte| byte.is_ascii_digit()) {
            if segment.len() > 1 && segment.starts_with('0') {
                return Err(invalid(whole, "预发布数字段不得有前导零"));
            }
            let value = segment
                .parse::<u64>()
                .map_err(|_| invalid(whole, "预发布数字段超出 u64 范围"))?;
            out.push(Identifier::Numeric(value));
        } else {
            out.push(Identifier::Alpha(segment.to_string()));
        }
    }
    Ok(out)
}

fn compare_prerelease(left: &[Identifier], right: &[Identifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => left.cmp(right),
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| compare_prerelease(&self.pre, &other.pre))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.pre.is_empty() {
            f.write_str("-")?;
            for (index, identifier) in self.pre.iter().enumerate() {
                if index > 0 {
                    f.write_str(".")?;
                }
                write!(f, "{identifier}")?;
            }
        }
        if let Some(build) = &self.build {
            write!(f, "+{build}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Version {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}
