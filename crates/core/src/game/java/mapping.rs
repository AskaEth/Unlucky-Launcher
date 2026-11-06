//! MC 版本 ↔ Java 版本映射（平移 HMCL 约束体系）+ 不符检查。
//!
//! 依据 `DevDocs/analyse/02-launch-pipeline.md` §5.4（HMCL `GameJavaVersion.getMinimumJavaVersion`）
//! 与 §3.1 的强制约束清单（`JavaVersionConstraint` 的 `isMandatory` 项）。两条权威叠加：
//!
//! | MC 版本 | 最低 | 最高 | 出处 |
//! | --- | --- | --- | --- |
//! | ≥ 26.1 | 25 | — | `java-runtime-epsilon` |
//! | ≥ 1.20.5 | 21 | — | `java-runtime-delta` |
//! | ≥ 1.18 | 17 | — | `java-runtime-beta` |
//! | ≥ 1.17 | 16 | — | `java-runtime-alpha` |
//! | ≥ 1.13 | 8 | — | `jre-legacy` |
//! | < 1.13 | 8 | **8** | `LAUNCH_WRAPPER`：LaunchWrapper 的 `URLClassLoader` 只能吃 Java 8 |
//!
//! **版本 JSON 优先**：装了 1.17 的玩家看的是 Mojang 自己写的 `javaVersion.majorVersion`
//! （1.17 就是 16），静态表只在清单没写时兜底——这也是 HMCL 的 `GAME_JSON` 强制项语义。
//!
//! 上限（`maximum`）只对老版本存在：1.12.2 用 Java 17 会直接
//! `UnsupportedClassVersionError`-级别的失败（启动器侧是 LaunchWrapper 假设），
//! 所以「1.12.2 + Java 21」要被判成不符，而不是「19 以上都能跑」。

use std::cmp::Ordering;

use super::super::inherit::resolve;
use super::super::layout::GameLayout;
use super::super::manifest::{JavaVersion, VersionJson};

/// 要求来源（`--json` 的 `requirement_source`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequirementSource {
    /// 版本 JSON 的 `javaVersion`（含继承链解析）。
    Manifest,
    /// 静态表（HMCL 约束体系的平移）。
    Table,
}

impl RequirementSource {
    /// 稳定字符串 id。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Table => "table",
        }
    }
}

/// 一个版本对 Java 的要求（闭区间）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRequirement {
    /// 最低主版本号（`21`）。
    pub minimum: u32,
    /// 最高主版本号（`Some(8)` = 只能用 8 及以下）。
    pub maximum: Option<u32>,
    /// 来源。
    pub source: RequirementSource,
}

impl JavaRequirement {
    /// 构造。
    #[must_use]
    pub const fn new(minimum: u32, maximum: Option<u32>, source: RequirementSource) -> Self {
        Self {
            minimum,
            maximum,
            source,
        }
    }

    /// 静态表来源的构造。
    #[must_use]
    pub const fn table(minimum: u32, maximum: Option<u32>) -> Self {
        Self::new(minimum, maximum, RequirementSource::Table)
    }

    /// 某主版本号是否满足要求。
    #[must_use]
    pub fn satisfied_by(&self, major: u32) -> bool {
        major >= self.minimum && self.maximum.is_none_or(|maximum| major <= maximum)
    }

    /// 人读描述（`Java 21+` / `Java 8` / `Java 8–16`）。
    #[must_use]
    pub fn describe(&self) -> String {
        match self.maximum {
            Some(maximum) if maximum == self.minimum => format!("Java {}", self.minimum),
            Some(maximum) => format!("Java {}–{}", self.minimum, maximum),
            None => format!("Java {}+", self.minimum),
        }
    }
}

/// 不符的两种形态（D13：只提示，不阻断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JavaMismatch {
    /// Java 太低。
    TooOld {
        /// 需要的最低主版本。
        required: u32,
        /// 实际主版本。
        actual: u32,
    },
    /// Java 太高（老版本只能用 8）。
    TooNew {
        /// 允许的最高主版本。
        maximum: u32,
        /// 实际主版本。
        actual: u32,
    },
}

impl JavaMismatch {
    /// 稳定字符串 id（`--json` 的 `mismatch.code`）。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::TooOld { .. } => "java_too_old",
            Self::TooNew { .. } => "java_too_new",
        }
    }

    /// 人读说明（GUI 弹窗 P1 后直接复用这句）。
    #[must_use]
    pub fn message(self) -> String {
        match self {
            Self::TooOld { required, actual } => format!(
                "该版本需要 Java {required} 及以上，当前用的是 Java {actual}（大概率起不来）"
            ),
            Self::TooNew { maximum, actual } => format!(
                "该版本只能用 Java {maximum} 及以下，当前用的是 Java {actual}（老加载器吃不了新 class 文件）"
            ),
        }
    }
}

/// 判需求与实际是否相符（`None` = 相符）。
#[must_use]
pub fn check(actual: u32, required: &JavaRequirement) -> Option<JavaMismatch> {
    if actual < required.minimum {
        return Some(JavaMismatch::TooOld {
            required: required.minimum,
            actual,
        });
    }
    if let Some(maximum) = required.maximum {
        if actual > maximum {
            return Some(JavaMismatch::TooNew { maximum, actual });
        }
    }
    None
}

/// 静态表：MC 版本 → Java 要求。
///
/// 无法解析的版本（周快照 `20w14a`、魔改 id）只给下限 `8`、不设上限——
/// 宁可漏判一次，也不要把「Java 21 + 1.20 快照」误锁成「必须降到 8」。
#[must_use]
pub fn requirement_for(mc_version: &str) -> JavaRequirement {
    let parts = version_parts(mc_version);
    if parts.is_empty() {
        return JavaRequirement::table(8, None);
    }
    if at_least(&parts, &[26, 1]) {
        return JavaRequirement::table(25, None);
    }
    if at_least(&parts, &[1, 20, 5]) {
        return JavaRequirement::table(21, None);
    }
    if at_least(&parts, &[1, 18]) {
        return JavaRequirement::table(17, None);
    }
    if at_least(&parts, &[1, 17]) {
        return JavaRequirement::table(16, None);
    }
    if at_least(&parts, &[1, 13]) {
        return JavaRequirement::table(8, None);
    }
    JavaRequirement::table(8, Some(8))
}

/// 静态表的最低位（`recommended_major("1.20.5") == 21`）。
#[must_use]
pub fn recommended_major(mc_version: &str) -> u32 {
    requirement_for(mc_version).minimum
}

/// 版本 JSON 的 `javaVersion` → 要求（字段缺失或为 0 时返回 `None`）。
///
/// Mojang 这个字段给的是**最低**要求（`majorVersion: 21` 表示 21 起跳），
/// 不声明上限——所以这里 `maximum` 恒为 `None`；上限由 [`requirement_for`] 补
/// （见 [`requirement_for_version`] 的合成说明）。
#[must_use]
pub fn requirement_from_manifest(java: &JavaVersion) -> Option<JavaRequirement> {
    let major = java.major_version.filter(|major| *major > 0)?;
    Some(JavaRequirement::new(
        major,
        None,
        RequirementSource::Manifest,
    ))
}

/// 已安装版本的 Java 要求：解析继承链 → 版本 JSON 的 `javaVersion` 优先 → 静态表兜底。
///
/// **下限与上限来源不同**：Mojang 的 `javaVersion` 只声明「最低要多少」，
/// 而「1.12.2 及更早吃不了 Java 9+」这条知识来自 HMCL 的 `LAUNCH_WRAPPER` 约束，
/// 不在清单里。所以合成时下限取清单、上限取静态表——否则「1.12.2 + Java 21」
/// 会被判成完全合法（真机上必崩）。
///
/// 版本 JSON 读不到（未安装 / 损坏）时直接走静态表，不报错——这是「启动前提示」的路径，
/// 不该因为清单缺件就拦住用户（真正拦人的是 `snapshot::prepare`）。
#[must_use]
pub fn requirement_for_version(layout: &GameLayout, version_id: &str) -> JavaRequirement {
    let table = requirement_for(version_id);
    let read = |id: &str| -> Option<VersionJson> {
        let text = std::fs::read_to_string(layout.version_json(id)).ok()?;
        VersionJson::from_json_str(&text).ok()
    };
    let Some(manifest) = read(version_id) else {
        return table;
    };
    let resolved = resolve(&read, &manifest).unwrap_or(manifest);
    let Some(declared) = resolved
        .java_version
        .as_ref()
        .and_then(requirement_from_manifest)
    else {
        // 清单没写 javaVersion：按 id 的静态表（继承来的 id 可能是魔改名）。
        return match resolved.id.as_deref() {
            Some(id) => requirement_for(id),
            None => table,
        };
    };
    JavaRequirement {
        minimum: declared.minimum,
        maximum: table.maximum,
        source: RequirementSource::Manifest,
    }
}

/// 版本串 → 数字分量（`1.20.5-pre1` → `[1, 20, 5]`；周快照 → 空）。
fn version_parts(raw: &str) -> Vec<u32> {
    let trimmed = raw.trim();
    if is_weekly_snapshot(trimmed) {
        return Vec::new();
    }
    let mut parts = Vec::new();
    for piece in trimmed.split('.') {
        let digits: String = piece.chars().take_while(char::is_ascii_digit).collect();
        match digits.parse::<u32>() {
            Ok(value) => parts.push(value),
            Err(_) => break,
        }
    }
    parts
}

/// `20w14a` / `26w14a` 形态（与 `launch::game_version` 同一套判据）。
fn is_weekly_snapshot(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    bytes.len() == 6
        && bytes[..2].iter().all(u8::is_ascii_digit)
        && bytes[2].eq_ignore_ascii_case(&b'w')
        && bytes[3..5].iter().all(u8::is_ascii_digit)
        && bytes[5].is_ascii_alphabetic()
}

/// `parts >= floor`（缺位按 0：`1.20` < `1.20.5`）。
fn at_least(parts: &[u32], floor: &[u32]) -> bool {
    let len = parts.len().max(floor.len());
    for index in 0..len {
        let left = parts.get(index).copied().unwrap_or(0);
        let right = floor.get(index).copied().unwrap_or(0);
        match left.cmp(&right) {
            Ordering::Greater => return true,
            Ordering::Less => return false,
            Ordering::Equal => {}
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{check, recommended_major, requirement_for, JavaMismatch, JavaRequirement};

    #[test]
    fn table_covers_the_hmcl_boundaries() {
        let cases: &[(&str, u32, Option<u32>)] = &[
            ("1.6.4", 8, Some(8)),
            ("1.12.2", 8, Some(8)),
            ("1.13", 8, None),
            ("1.15.2", 8, None),
            ("1.16.5", 8, None),
            ("1.17", 16, None),
            ("1.17.1", 16, None),
            ("1.18", 17, None),
            ("1.19.4", 17, None),
            ("1.20.1", 17, None),
            ("1.20.4", 17, None),
            ("1.20.5", 21, None),
            ("1.21.4", 21, None),
            ("26.1", 25, None),
            ("26.3", 25, None),
            ("26.3-snapshot-1", 25, None),
        ];
        for (version, minimum, maximum) in cases {
            let requirement = requirement_for(version);
            assert_eq!(
                (requirement.minimum, requirement.maximum),
                (*minimum, *maximum),
                "{version}"
            );
            assert_eq!(recommended_major(version), *minimum, "{version}");
        }
    }

    #[test]
    fn weekly_snapshots_get_no_upper_bound() {
        // 20w14a 的前导数字是 20，绝不能被当成「26.x」或误锁上限。
        let requirement = requirement_for("20w14a");
        assert_eq!(requirement.minimum, 8);
        assert_eq!(requirement.maximum, None);
        assert!(requirement.satisfied_by(21));
    }

    #[test]
    fn pre_release_suffixes_do_not_change_the_floor() {
        assert_eq!(requirement_for("1.20.5-pre1").minimum, 21);
        assert_eq!(requirement_for("1.20.4-rc1").minimum, 17);
    }

    #[test]
    fn check_reports_both_directions() {
        let modern = JavaRequirement::table(21, None);
        assert_eq!(check(21, &modern), None);
        assert_eq!(check(25, &modern), None);
        assert_eq!(
            check(17, &modern),
            Some(JavaMismatch::TooOld {
                required: 21,
                actual: 17
            })
        );
        let legacy = JavaRequirement::table(8, Some(8));
        assert_eq!(check(8, &legacy), None);
        assert_eq!(
            check(21, &legacy),
            Some(JavaMismatch::TooNew {
                maximum: 8,
                actual: 21
            })
        );
        assert_eq!(legacy.describe(), "Java 8");
        assert_eq!(modern.describe(), "Java 21+");
        assert_eq!(JavaRequirement::table(8, Some(16)).describe(), "Java 8–16");
    }

    #[test]
    fn mismatch_messages_name_both_sides() {
        let message = JavaMismatch::TooOld {
            required: 21,
            actual: 8,
        }
        .message();
        assert!(message.contains("Java 21"));
        assert!(message.contains("Java 8"));
        assert_eq!(
            JavaMismatch::TooNew {
                maximum: 8,
                actual: 21
            }
            .code(),
            "java_too_new"
        );
    }
}
