//! 可下载 JDK 大版本目录（M197）：设置页「下载一个 JDK」的来源表。
//!
//! 用户原话（2026-11-08）：「我们内置了那么多镜像源 甚至用户不能手动选择指定版本下载。」
//! 这一层只回答两个问题：**可选的有哪些大版本**、**每个大版本由哪几家发行版承接**。
//! 它不联网、不落盘、不认识 UI——真正查版本与下载仍在 [`super::install`] / [`super::source`]。
//!
//! ## 为什么目录要写在这里而不是 UI 里
//!
//! 「哪几家能装 8」是 [`JdkVendor::supports`] 的知识（Oracle 从 9 起、MS 只发 11/17/21/25），
//! 写进 UI 就是第二份真相：两边一旦不一致，界面会给出一个「点了必然失败」的按钮。
//! 所以本模块只做「目录 × 发行版能力」的组合，UI 拿到的就是回退链里真实会试的那几家。
//!
//! ## 基准版本
//!
//! [`BASELINE_MAJOR`]（17）是 D13 定的基准：缺 Java 时先装它。它同时也是目录的一员，
//! 但不是唯一一员——`26.1` 起 MC 要 25，`1.12.2` 只能吃 8，所以要能挑。
//!
//! ## 指定版本（M280）
//!
//! 「用户给的版本串是不是这一份」只有一个判据：[`matches_version`]。四家的版本串语义不同
//! （Temurin `21.0.13+9`、Oracle `21.0.2`、MS `21.0.13.0.2`、Zulu `21.0.13.0.3`），
//! 但「相等」这件事可以统一：先按 [`public_version`] 去掉构建号段（`+9`）再比。
//! 各家模块只负责**把候选版本喂进来**，不许自己再写一份前缀/包含判断。

use super::error::{JavaError, JavaErrorKind};
use super::vendor::{self, JdkVendor};

/// D13 的基准 JDK 大版本（没有上下文时「装哪个」的默认答案）。
pub const BASELINE_MAJOR: u32 = 17;

/// 一个可选的大版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JdkChoice {
    /// 大版本号（`17`）。
    pub major: u32,
    /// 是不是 LTS（展示用徽标；不参与任何判定）。
    pub lts: bool,
    /// 是不是 D13 基准版本。
    pub baseline: bool,
}

/// 目录（升序）：8 / 11 / 17 / 21 / 25。
///
/// 8 不是「顺便带上」的：1.12.2 及更早**配 LaunchWrapper 系加载器**（Forge / LiteLoader）时
/// 只能用 Java 8，而 25 是 `26.1` 起的下限（见 `mapping`）。少任何一端，用户在最老的实例上
/// 就只能手动去装 JDK。
pub const CHOICES: [JdkChoice; 5] = [
    JdkChoice {
        major: 8,
        lts: false,
        baseline: false,
    },
    JdkChoice {
        major: 11,
        lts: true,
        baseline: false,
    },
    JdkChoice {
        major: 17,
        lts: true,
        baseline: true,
    },
    JdkChoice {
        major: 21,
        lts: true,
        baseline: false,
    },
    JdkChoice {
        major: 25,
        lts: true,
        baseline: false,
    },
];

/// 全部可选大版本（升序）。
#[must_use]
pub const fn choices() -> &'static [JdkChoice] {
    &CHOICES
}

/// 目录里有没有这个大版本（`Some` = 这一档可选）。
#[must_use]
pub fn find(major: u32) -> Option<JdkChoice> {
    CHOICES.into_iter().find(|choice| choice.major == major)
}

/// 某个大版本能由哪几家发行版承接（顺序 = 回退链顺序）。
///
/// 与 [`super::install::install`] 的过滤同源——`vendor::order` 给出这条链，`supports` 去掉
/// 发不出这个大版本的那几家。于是 GUI 上「能选哪几家」与「真会试哪几家」不会各说一套。
#[must_use]
pub fn vendors_for(major: u32, priority: &[JdkVendor], domestic: bool) -> Vec<JdkVendor> {
    vendor::order(priority, domestic)
        .into_iter()
        .filter(|vendor| vendor.supports(major))
        .collect()
}

/// 「该装哪个大版本」：把当前实例的最低要求对齐到目录里的一档，没有上下文时给基准版本。
///
/// 对齐方向是**向上取整**（1.17 要 16 → 目录里的 17），因为装低一档一定跑不起来；
/// 要求超过目录上限（例如 26.1 的 25 已在目录里；将来 28 就超出）时给基准版本，
/// 由页面把「这个版本装什么都不对」说清楚——而不是悄悄装个用不上的。
#[must_use]
pub fn default_major(required: Option<u32>) -> u32 {
    let Some(required) = required else {
        return BASELINE_MAJOR;
    };
    CHOICES
        .iter()
        .map(|choice| choice.major)
        .find(|major| *major >= required)
        .unwrap_or(BASELINE_MAJOR)
}

/// 版本串的规范化：去首尾空白 + 去掉 `jdk-` 前缀（Adoptium 的 release 名是 `jdk-21.0.13+9`）。
#[must_use]
pub fn canonical_version(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix("jdk-")
        .or_else(|| trimmed.strip_prefix("JDK-"))
        .unwrap_or(trimmed)
        .trim()
}

/// 版本串的「公共部分」：`21.0.13+9` / `21.0.13+9.0.LTS` → `21.0.13`（`+` 之后是构建号）。
///
/// 目录 `/ 戳记 / --json` 用的是 vendor 原生整串，只有「是不是同一个版本」这件事看公共部分。
#[must_use]
pub fn public_version(text: &str) -> &str {
    let canonical = canonical_version(text);
    let end = canonical.find('+').unwrap_or(canonical.len());
    canonical[..end].trim_end()
}

/// 构建号的首段数字（`+9` / `+9.0.LTS` / `+9-LTS` 都归成 `9`；没有 `+` 段时 `None`）。
fn build_head(text: &str) -> Option<u32> {
    let canonical = canonical_version(text);
    let rest = canonical.get(canonical.find('+')? + 1..)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// 用户点名的版本就是这份候选吗（四家共用的**唯一**判据）。
///
/// 三种情形都算命中：整串相同（`21.0.13+9`）、只给公共部分（`21.0.13` ↔ `21.0.13+9`）、
/// 构建号首段相同（`21.0.13+9` ↔ `21.0.13+9.0.LTS`）。
/// **不做**前缀近似：`21.0` 不算 `21.0.13`，`21.0.13` 也不算 Microsoft 的 `21.0.13.0.2`
/// —— 少一位就是另一个版本，宁可报错并列出该大版本的可用版本。
#[must_use]
pub fn matches_version(requested: &str, candidate: &str) -> bool {
    let wanted = canonical_version(requested);
    let found = canonical_version(candidate);
    if wanted.is_empty() || found.is_empty() || public_version(wanted) != public_version(found) {
        return false;
    }
    match (build_head(wanted), build_head(found)) {
        // 只给公共部分：该公共部分的唯一产物就是它。
        (None, _) => true,
        (Some(wanted), Some(found)) => wanted == found,
        // 用户点名了构建号、候选里没给：不能当作命中（可能不是同一个构建）。
        (Some(_), None) => false,
    }
}

/// 兜底闸：实际拿到的版本必须真是用户点名的那一版（`None` = 没钉版本，永远通过）。
///
/// 这是「不静默回落」的最后一道保险：即使某家的实现哪天真回落到了 latest，
/// 调用方（`install` / `plan`）也会把它记成一次失败并继续走链，而不是把 latest
/// 当成用户要的版本装下去。
#[must_use]
pub fn version_mismatch(wanted: Option<&str>, found: &str) -> Option<String> {
    let wanted = wanted?;
    (!matches_version(wanted, found)).then(|| format!("返回的版本是 {found}，不是请求的 {wanted}"))
}

/// 可用版本清单的完整度：决定错误文案里那句「还有没有更多」怎么说。
///
/// 上游的版本清单有三种形态：Oracle 目录页一次就能看全、Microsoft 只有一份快照、
/// Zulu/Termux 压根没有清单（[`Complete`]）；Temurin 要走分页，翻到上限还没翻完时
/// 只能如实说「只查了前 N 条」——**不许把窗口当全集**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionListScope {
    /// 上游的清单已经全看到了。
    Complete,
    /// 只看了上游的前若干条（Temurin 翻页到上限）。
    Truncated,
}

/// 可用版本列表的人读形态（错误里列给人看；超过 `limit` 条就收口成「等 N 个」）。
///
/// `scope` 只影响尾巴那句：`Complete` 时「等 N 个」就是上游总数，`Truncated` 时补一句
/// 「只查了前 N 条」，因为这时 N 是**我们的窗口**而不是上游的总数。
#[must_use]
pub fn available_versions_text(
    versions: &[String],
    limit: usize,
    scope: VersionListScope,
) -> String {
    if versions.is_empty() {
        return "（上游一个都没列出来）".to_string();
    }
    let shown = versions
        .iter()
        .take(limit)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" / ");
    let listed = if versions.len() > limit {
        format!("{shown} 等 {} 个", versions.len())
    } else {
        shown
    };
    match scope {
        VersionListScope::Complete => listed,
        VersionListScope::Truncated => {
            format!("{listed}（只查了前 {} 条，后面可能还有）", versions.len())
        }
    }
}

/// 「这家装不了你要的那个版本」的错误：**四家文案互不相同**，各自说清原因 + 一句「哪家能做」。
///
/// `available` 是这家真正能给的版本串（Temurin/Oracle 给该大版本的清单，MS 给快照里那一个，
/// Zulu/Termux 给空数组——上游根本没有版本清单）。这条错误是用户唯一能看见的下一步，
/// 所以「不静默回落、也不假装支持」这句话只说一次，就在这儿说。
#[must_use]
pub fn version_pin_error(
    vendor: JdkVendor,
    major: u32,
    wanted: &str,
    available: &[String],
    scope: VersionListScope,
) -> JavaError {
    let listed = available_versions_text(available, 10, scope);
    let try_other = "要这个版本换一家：`--vendor temurin`（Adoptium 全量补丁）或 `--vendor oracle`";
    let vendor_id = vendor.id();
    match vendor {
        JdkVendor::Temurin | JdkVendor::Oracle => {
            // `source` 自己带连接词：拉丁文名与中文名需要的空格不一样，
            // 模板里再补一个空格就会拼出「Oracle 镜像目录 里可用」。
            let source = if vendor == JdkVendor::Temurin {
                "Adoptium 里"
            } else {
                "Oracle 镜像目录里"
            };
            JavaError::new(
                JavaErrorKind::Metadata,
                format!("{vendor_id} 的 Java {major} 没有版本 {wanted}（{source}可用：{listed}）"),
            )
            .with_hint("不给 --version 就装该大版本最新的 GA；指定版本要写上面列出的形态")
        }
        JdkVendor::Microsoft => JavaError::new(
            JavaErrorKind::Metadata,
            format!(
                "microsoft 的 Java {major} 只内置了 {listed} 这一份快照，没有 {wanted}（这家不能自选版本，要自选请换 temurin 或 oracle）"
            ),
        )
        .with_hint(format!(
            "微软这套版本号写死在快照里：装它就 `ull java install {major} --vendor microsoft`；{try_other}"
        )),
        JdkVendor::Zulu => JavaError::new(
            JavaErrorKind::Metadata,
            format!("zulu 只查最新一份（api 的 latest=true），不能指定版本 {wanted}（要自选请换 temurin 或 oracle）"),
        )
        .with_hint(format!(
            "只装该大版本最新的 Zulu 就去掉 --version；{try_other}"
        )),
        JdkVendor::Termux => JavaError::new(
            JavaErrorKind::Unsupported,
            format!(
                "termux 的 JDK 由系统包（pkg install openjdk-{major}）决定版本，不能指定 {wanted}（普通平台上要自选请换 temurin 或 oracle）"
            ),
        )
        .with_hint("去掉 --version 让系统包装它给的那份；要指定版本只能在普通平台上装归档"),
    }
}

#[cfg(test)]
mod tests;
