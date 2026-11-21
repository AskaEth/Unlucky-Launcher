//! MC 版本 ↔ Java 版本映射（平移 HMCL 约束体系）；不符判定与文案见
//! [`crate::game::java::mismatch`]（其中的 `check` / `JavaMismatch` 仍从本模块转出，
//! 调用方的 `mapping::check` 写法不变）。
//!
//! 依据 `DevDocs/analyse/02-launch-pipeline-p1.md` §5.4（HMCL `GameJavaVersion.getMinimumJavaVersion`）
//! 与 §3.1 的强制约束清单（`JavaVersionConstraint` 的 `isMandatory` 项）。两条权威叠加：
//!
//! | MC 版本 | 最低 | 最高 | 出处 |
//! | --- | --- | --- | --- |
//! | ≥ 26.1 | 25 | — | `java-runtime-epsilon` |
//! | ≥ 1.20.5 | 21 | — | `java-runtime-delta` |
//! | ≥ 1.18 | 17 | — | `java-runtime-beta` |
//! | ≥ 1.17 | 16 | — | `java-runtime-alpha` |
//! | ≥ 1.13 | 8 | — | `jre-legacy`（`VANILLA` 约束只给下限） |
//! | < 1.13，原版 | 8 | **—** | `jre-legacy`：Mojang 自己也只声明最低版本 |
//! | < 1.13，LaunchWrapper 系加载器 | 8 | **8** | `LAUNCH_WRAPPER`：判据是「主类是 LaunchWrapper」 |
//!
//! **上限只属于加载器，不属于原版**（M348）。HMCL 那条强制约束的条件是
//! 「≤1.12 **且主类是 `net.minecraft.launchwrapper.Launch`** 且 launchwrapper < 1.13」
//! （`JavaVersionConstraint.LAUNCH_WRAPPER`）——它管的是 LaunchWrapper 的类加载器假设
//! （Forge / LiteLoader / OptiFine 那一代），原版的 `net.minecraft.client.main.Main`
//! 从来不在这条约束里。M347 实测：原版 1.12.2 在 Java 17 上走到主菜单（**进世界未验**）。
//! 所以 `<1.13` 原版的要求只有下限 8，高版本 Java 的取舍交给调用方提示（[`legacy_advisory_facts`]）。
//!
//! ⚠️ 未纳入的既有知识（如实记着，别当成已处理）：HMCL 还有一条 `VANILLA_LINUX_JAVA_8`
//! 强制约束——Linux x86_64 且 ≤1.12 时 JDK 9+ 载入 32 位原生库会失败。本模块不分平台，
//! 所以 Linux 上这一档的放宽没有收窄（Windows 上 M347 实测到主菜单）。
//!
//! **版本 JSON 优先**：装了 1.17 的玩家看的是 Mojang 自己写的 `javaVersion.majorVersion`
//! （1.17 就是 16），静态表只在清单没写时兜底——这也是 HMCL 的 `GAME_JSON` 强制项语义。

use std::cmp::Ordering;

use super::super::inherit::resolve;
use super::super::layout::GameLayout;
use super::super::loader::{InstanceLoader, LoaderKind};
use super::super::manifest::{JavaVersion, VersionJson};

pub use super::advisory::legacy_advisory_facts;
pub use super::mismatch::{check, JavaMismatch};

/// LaunchWrapper 系加载器的主类（`LAUNCH_WRAPPER` 约束唯一的判据，见模块文档）。
pub const LAUNCH_WRAPPER_MAIN_CLASS: &str = "net.minecraft.launchwrapper.Launch";

/// `<1.13` + LaunchWrapper 系加载器时的最高主版本号。
pub const LAUNCH_WRAPPER_MAXIMUM: u32 = 8;

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
    /// 最高主版本号（`Some(8)` = 只能用 8 及以下；原版老版本是 `None`）。
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

/// 静态表：MC 版本 → Java 要求（**原版口径**，不给上限）。
///
/// 无法解析的版本（周快照 `20w14a`、魔改 id）只给下限 `8`、不设上限——
/// 宁可漏判一次，也不要把「Java 21 + 1.20 快照」误锁成「必须降到 8」。
///
/// 上限不在这张表里（M348）：`<1.13` 的上限来自 **LaunchWrapper 系加载器**，
/// 由 [`requirement_for_version`] 按清单主类（或版本 id 的加载器标记）补上。
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
    JavaRequirement::table(8, None)
}

/// 静态表的最低位（`recommended_major("1.20.5") == 21`）。
#[must_use]
pub fn recommended_major(mc_version: &str) -> u32 {
    requirement_for(mc_version).minimum
}

/// 版本 JSON 的 `javaVersion` → 要求（字段缺失或为 0 时返回 `None`）。
///
/// Mojang 这个字段给的是**最低**要求（`majorVersion: 21` 表示 21 起跳），
/// 不声明上限——所以这里 `maximum` 恒为 `None`；上限由 [`requirement_for_version`]
/// 按加载器补（原版没有上限，见模块文档）。
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
/// **下限与上限来源不同**：Mojang 的 `javaVersion` 只声明「最低要多少」，而上限根本不来自
/// Mojang——它来自 HMCL 的 `LAUNCH_WRAPPER` 约束，而那条约束只管**主类是 LaunchWrapper**
/// 的版本（Forge / LiteLoader 那一代，见 [`uses_launch_wrapper`]）。所以合成时下限取清单、
/// 上限按加载器补：原版 `<1.13` 不设上限（M348，M347 实测到主菜单），LaunchWrapper 系仍锁 8。
///
/// 版本 JSON 读不到（未安装 / 损坏）时直接走静态表，不报错——这是「启动前提示」的路径，
/// 不该因为清单缺件就拦住用户（真正拦人的是 `snapshot::prepare`）；这种情形下加载器只能按
/// 版本 id 的标记猜（[`uses_launch_wrapper`] 的兜底支路）。
#[must_use]
pub fn requirement_for_version(layout: &GameLayout, version_id: &str) -> JavaRequirement {
    let table = requirement_for(version_id);
    let read = |id: &str| -> Option<VersionJson> {
        let text = std::fs::read_to_string(layout.version_json(id)).ok()?;
        VersionJson::from_json_str(&text).ok()
    };
    let Some(manifest) = read(version_id) else {
        return with_launch_wrapper_bound(table, version_id, None);
    };
    let resolved = resolve(&read, &manifest).unwrap_or(manifest);
    let main_class = resolved.main_class.clone();
    let Some(declared) = resolved
        .java_version
        .as_ref()
        .and_then(requirement_from_manifest)
    else {
        // 清单没写 javaVersion：按 id 的静态表（继承来的 id 可能是魔改名）。
        let table = match resolved.id.as_deref() {
            Some(id) => requirement_for(id),
            None => table,
        };
        return with_launch_wrapper_bound(table, version_id, main_class.as_deref());
    };
    let requirement = JavaRequirement {
        minimum: declared.minimum,
        maximum: None,
        source: RequirementSource::Manifest,
    };
    with_launch_wrapper_bound(requirement, version_id, main_class.as_deref())
}

/// 这个版本是不是 **LaunchWrapper 系**（`LAUNCH_WRAPPER` 约束的作用域）。
///
/// 判据优先级与项目里其它「认加载器」的地方相反，这里是有意的：
///
/// 1. **清单写了主类** ⇒ 只看主类是不是 [`LAUNCH_WRAPPER_MAIN_CLASS`]。Forge 1.12.2 的
///    清单主类就是它（`tests/liteloader/support.rs` 里有真清单样本），而 1.13+ 的 Forge 换成
///    `cpw.mods.modlauncher.Launcher` / `cpw.mods.bootstraplauncher.BootstrapLauncher`
///    ——那些**不**在这条约束里，它们的下限由 Mojang 的 `javaVersion` 管。
/// 2. **清单没有主类 / 读不到** ⇒ 退回版本 id 的加载器标记（[`InstanceLoader::parse`]），
///    认 Forge / LiteLoader / OptiFine 三家。这条兜底是往「拦」的一侧错：id 里带 `forge`
///    却其实不是 LaunchWrapper 的实例，会多收一次硬拦——用户能用 `--java` 显式指定绕过，
///    而反过来（漏拦）的代价是点启动才发现崩。
///
/// OptiFine 为什么算：`DevDocs/analyse/02-launch-pipeline-p1.md:204` 记的是 HMCL 按主类分派
/// 修补的那张表——`net.minecraft.launchwrapper.Launch` 覆盖「**≤1.12 的
/// Forge/LiteLoader/OptiFine**」三家，即 ≤1.12 的 OptiFine 也在 LaunchWrapper 上。它只影响
/// `<1.13`（`legacy_band`），所以 1.13+ 的 OptiFine（ModLauncher 那一代）不会被这条收窄。
#[must_use]
pub fn uses_launch_wrapper(version_id: &str, main_class: Option<&str>) -> bool {
    match main_class {
        Some(main_class) => main_class.trim() == LAUNCH_WRAPPER_MAIN_CLASS,
        None => matches!(
            InstanceLoader::parse(version_id).kind,
            LoaderKind::Forge | LoaderKind::LiteLoader | LoaderKind::OptiFine
        ),
    }
}

/// `<1.13` + LaunchWrapper ⇒ 补上 `maximum = 8`（其余情形原样返回）。
fn with_launch_wrapper_bound(
    mut requirement: JavaRequirement,
    version_id: &str,
    main_class: Option<&str>,
) -> JavaRequirement {
    if uses_launch_wrapper(version_id, main_class) && legacy_band(version_id) {
        requirement.maximum = Some(LAUNCH_WRAPPER_MAXIMUM);
    }
    requirement
}

/// `<1.13`，或版本 id 根本解析不出数字（魔改名的整合包）——只在上限这一处用得上。
fn legacy_band(mc_version: &str) -> bool {
    !at_least(&version_parts(mc_version), &[1, 13])
}

/// 版本串是不是**能解析出来的** `<1.13` 正式版（周快照与魔改名都是 `false`）。
///
/// 与 `legacy_band` 的差别：这里给「要不要提示」用（[`legacy_advisory_facts`]），认不出年代就
/// 不提示——不该对 `20w14a` 说「这是老版本，进世界可能崩」。
#[must_use]
pub fn is_pre_1_13_release(mc_version: &str) -> bool {
    let parts = version_parts(mc_version);
    !parts.is_empty() && !at_least(&parts, &[1, 13])
}

/// 版本串 → 数字分量（`1.20.5-pre1` → `[1, 20, 5]`；周快照 → 空）。
///
/// `pub(crate)`：`advisory` 模块要拿它判「这是不是我们实测过的那个版本」——**同一套解析**，
/// 不在别处再写一遍（写成第二份就会出现 `1.12.2-pre1` 两边认法不同的缝）。
pub(crate) fn version_parts(raw: &str) -> Vec<u32> {
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
    use super::{
        recommended_major, requirement_for, uses_launch_wrapper, JavaRequirement,
        LAUNCH_WRAPPER_MAIN_CLASS,
    };

    #[test]
    fn table_covers_the_hmcl_boundaries() {
        let cases: &[(&str, u32, Option<u32>)] = &[
            ("1.6.4", 8, None),
            ("1.12.2", 8, None),
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
    fn describe_spells_out_the_interval() {
        assert_eq!(requirement_for("1.12.2").describe(), "Java 8+");
        assert_eq!(requirement_for("1.20.5").describe(), "Java 21+");
        assert_eq!(JavaRequirement::table(8, Some(16)).describe(), "Java 8–16");
    }

    /// 上限只属于 LaunchWrapper（M348）：主类（清单权威）与 id 标记（兜底）各认一次。
    #[test]
    fn launch_wrapper_is_a_main_class_question_not_a_version_question() {
        assert!(uses_launch_wrapper(
            "1.12.2",
            Some(LAUNCH_WRAPPER_MAIN_CLASS)
        ));
        assert!(!uses_launch_wrapper(
            "1.12.2",
            Some("net.minecraft.client.main.Main")
        ));
        // 1.13+ 的 Forge 是 ModLauncher / BootstrapLauncher：主类说了算，不在这条约束里。
        assert!(!uses_launch_wrapper(
            "1.16.5-forge-36.2.0",
            Some("cpw.mods.modlauncher.Launcher")
        ));
        // 清单读不到 / 没写主类时才看 id 标记，认 Forge / LiteLoader / OptiFine 三家
        // （`02-launch-pipeline-p1.md:204`：≤1.12 的 OptiFine 也在 LaunchWrapper 上）。
        assert!(uses_launch_wrapper("1.12.2-forge-14.23.5.2860", None));
        assert!(uses_launch_wrapper("1.12.2-LiteLoader", None));
        assert!(uses_launch_wrapper("1.12.2-OptiFine_HD_U_C7", None));
        assert!(!uses_launch_wrapper("1.12.2", None));
        // 1.13+ 的 OptiFine 属于 ModLauncher 那一代；就算 id 认出来，`legacy_band` 也不会收窄它
        // （那一段由 `requirement_for_version` 的用例钉住）。
        assert!(!uses_launch_wrapper("1.20.1-fabric-0.16.5", None));
    }
}
