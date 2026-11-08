//! Maven 坐标与版本表（Forge / NeoForge）：版本列表地址、版本选择、installer 地址。
//!
//! 与 [`super::forge`] 的分工：这里只管"maven 上有什么版本、jar 在哪"，
//! 那边只管"installer 里的 profile 怎么变成版本 JSON"。
//!
//! 镜像：两家 maven 都在 D7 镜像表里有 BMCLAPI 规则（`bmclapi-neoforge` /
//! `bmclapi-forge-maven-net`），所以这里一律**写官方地址**，重写交给 `core::download`
//! 的镜像展开（与 M27 的"不拼镜像 URL"纪律一致）。

use super::LoaderKind;

/// NeoForge maven 根（规则 `bmclapi-neoforge` 把 `/releases` 重写到 `/maven`）。
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";
/// Forge maven 根（规则 `bmclapi-forge-maven-net`）。
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
/// NeoForge 唯一走 Forge 布局的原版版本（1.20.1 是"NeoForge 还叫 forge 的时候"）。
pub const NEOFORGE_FORGE_LAYOUT_MC: &str = "1.20.1";

/// maven 根地址。
#[must_use]
pub fn base(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::NeoForge => NEOFORGE_MAVEN,
        _ => FORGE_MAVEN,
    }
}

/// 该原版版本对应的 `maven-metadata.xml` 地址。
#[must_use]
pub fn metadata_url(kind: LoaderKind, mc_version: &str) -> String {
    let (root, path) = match kind {
        LoaderKind::NeoForge if mc_version == NEOFORGE_FORGE_LAYOUT_MC => {
            (NEOFORGE_MAVEN, "net/neoforged/forge")
        }
        LoaderKind::NeoForge => (NEOFORGE_MAVEN, "net/neoforged/neoforge"),
        _ => (FORGE_MAVEN, "net/minecraftforge/forge"),
    };
    format!("{root}/{path}/maven-metadata.xml")
}

/// installer jar 地址。
#[must_use]
pub fn installer_url(kind: LoaderKind, mc_version: &str, maven_version: &str) -> String {
    let (root, path, file) = match kind {
        LoaderKind::NeoForge if mc_version == NEOFORGE_FORGE_LAYOUT_MC => (
            NEOFORGE_MAVEN,
            "net/neoforged/forge",
            format!("forge-{maven_version}-installer.jar"),
        ),
        LoaderKind::NeoForge => (
            NEOFORGE_MAVEN,
            "net/neoforged/neoforge",
            format!("neoforge-{maven_version}-installer.jar"),
        ),
        _ => (
            FORGE_MAVEN,
            "net/minecraftforge/forge",
            format!("forge-{maven_version}-installer.jar"),
        ),
    };
    format!("{root}/{path}/{maven_version}/{file}")
}

/// 扫 maven-metadata.xml 里的 `<version>` 值。
///
/// 故意不引 XML 库：maven 元数据是**极简且稳定**的格式（只有一层 `<version>` 列表），
/// 为它拉一个 XML 解析器不划算。解析不出就返回空表，由上层报"没有可用版本"。
#[must_use]
pub fn parse_metadata_versions(xml: &str) -> Vec<String> {
    const OPEN: &str = "<version>";
    const CLOSE: &str = "</version>";
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(OPEN) {
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find(CLOSE) else {
            break;
        };
        let value = after[..end].trim();
        if !value.is_empty() {
            out.push(value.to_string());
        }
        rest = &after[end + CLOSE.len()..];
    }
    out
}

/// NeoForge 版本号 → 原版版本号（`21.1.80` → `1.21.1`，analyse/05 §1.3 的反解规则）。
///
/// - 主版本 `0` 是快照版（命名不带原版版本，返回 `None`）；
/// - 主版本 `≥26` 是 MC 新命名方案（`26.1.2.125` → `26.1`）；
/// - 其余是 `1.<主>.<次>`（`20.4.237` → `1.20.4`）。
#[must_use]
pub fn neoforge_mc_version(version: &str) -> Option<String> {
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let segments: Vec<u64> = core
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<_, _>>()
        .ok()?;
    let (major, minor) = (*segments.first()?, *segments.get(1)?);
    match major {
        0 => None,
        26.. => Some(format!("{major}.{minor}")),
        _ => Some(format!("1.{major}.{minor}")),
    }
}

/// maven 版本是否属于该原版版本。
#[must_use]
pub fn matches_mc(kind: LoaderKind, mc_version: &str, version: &str) -> bool {
    match kind {
        LoaderKind::NeoForge if mc_version == NEOFORGE_FORGE_LAYOUT_MC => {
            version.starts_with(&format!("{mc_version}-"))
        }
        LoaderKind::NeoForge => neoforge_mc_version(version).as_deref() == Some(mc_version),
        _ => version.starts_with(&format!("{mc_version}-")),
    }
}

/// 从 maven 版本表里挑出属于该原版版本的加载器版本（**升序**，末尾最好）。
#[must_use]
pub fn select_versions(kind: LoaderKind, mc_version: &str, versions: &[String]) -> Vec<String> {
    let mut picked: Vec<String> = versions
        .iter()
        .filter(|version| matches_mc(kind, mc_version, version))
        .cloned()
        .collect();
    picked.sort_by(|left, right| {
        numeric_key(left)
            .cmp(&numeric_key(right))
            .then_with(|| left.cmp(right))
    });
    picked
}

/// 用户给的版本号归一成 maven 版本号（Forge 的 maven 版本带原版前缀）。
///
/// `--loader-version 47.2.0` 与 `--loader-version 1.20.1-47.2.0` 都接受；
/// NeoForge（非 1.20.1 布局）本身就是完整 maven 版本，原样返回。
#[must_use]
pub fn normalize_maven_version(kind: LoaderKind, mc_version: &str, raw: &str) -> String {
    let raw = raw.trim();
    let needs_prefix = match kind {
        LoaderKind::NeoForge => mc_version == NEOFORGE_FORGE_LAYOUT_MC,
        _ => true,
    };
    if needs_prefix && !raw.starts_with(&format!("{mc_version}-")) {
        format!("{mc_version}-{raw}")
    } else {
        raw.to_string()
    }
}

/// 用户视角的加载器版本号（Forge 去掉原版前缀，NeoForge 原样）。
#[must_use]
pub fn display_loader_version(kind: LoaderKind, mc_version: &str, maven_version: &str) -> String {
    match kind {
        LoaderKind::NeoForge if mc_version != NEOFORGE_FORGE_LAYOUT_MC => maven_version.to_string(),
        _ => maven_version
            .strip_prefix(&format!("{mc_version}-"))
            .unwrap_or(maven_version)
            .to_string(),
    }
}

/// 版本号 → 可比较的数字键（非数字段按 0，保证不 panic）。
fn numeric_key(version: &str) -> Vec<u64> {
    version
        .split(['-', '.', '+'])
        .map(|segment| segment.parse::<u64>().unwrap_or(0))
        .collect()
}
