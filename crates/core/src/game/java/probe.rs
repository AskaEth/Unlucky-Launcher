//! Java 运行时探测：跑一次 `java -version`、扫托管目录、列系统候选。
//!
//! 探测动作只有两种：读托管目录（`.ULL/data/java-managed/jdk-<major>-<vendor>/`）
//! 与跑 `java -version`（输出在 stderr，主版本号从引号里的版本号解析，
//! `1.8.0_402` 这种老形态换算成 8）。
//!
//! 「选哪个 Java 启动」不在这里，而在 [`super::select`]——那边是**纯函数**，
//! 候选运行时由调用方注入，测试因此不需要真的装三个 JDK。

use std::path::{Path, PathBuf};

use super::extract::{java_binary_name, read_stamp};
use super::vendor::JdkVendor;
use crate::platform::child::quiet;

/// 缺省 Java 主版本（探测不到任何东西时的保守值，D13：JDK 17 基准）。
pub const DEFAULT_MAJOR: u32 = 17;

/// 托管目录名前缀。
pub const MANAGED_PREFIX: &str = "jdk-";

/// 一个 Java 运行时的来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JavaOrigin {
    /// 实例覆盖（`--java` 参数或实例档案的 `javaPath`）。
    Instance,
    /// 托管目录（`ull java install` 装的）。
    Managed,
    /// `JAVA_HOME`。
    JavaHome,
    /// `PATH` 上的 `java`。
    Path,
    /// 标准安装位置扫出来的（MSI 装的 Temurin / 官方启动器的 `runtime/` / 注册表等）。
    System,
}

impl JavaOrigin {
    /// 稳定字符串 id（`--json` 的 `source` 字段）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Managed => "managed",
            Self::JavaHome => "JAVA_HOME",
            Self::Path => "PATH",
            Self::System => "system",
        }
    }

    /// 展示类别（`--json` 的 `source_category`）：GUI 用它分组，不必区分 `JAVA_HOME`/`PATH`。
    #[must_use]
    pub const fn category(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Managed => "managed",
            Self::JavaHome | Self::Path => "env",
            Self::System => "system",
        }
    }

    /// 同档内的优先序（小者优先）。
    ///
    /// 「环境变量」先于「扫盘」：用户显式设过的 `JAVA_HOME`/`PATH` 是意图表达，
    /// 盘上扫出来的只是候选（HMCL/PCL 也是这个顺序）。
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Instance => 0,
            Self::Managed => 1,
            Self::JavaHome => 2,
            Self::Path => 3,
            Self::System => 4,
        }
    }
}

/// 一个探测到的 Java 运行时。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaRuntime {
    /// 可执行文件。
    pub binary: PathBuf,
    /// 来源。
    pub origin: JavaOrigin,
    /// `java -version` 的首行（含 `version` 字样）。
    pub version: Option<String>,
    /// 主版本号（`1.8.0_402` → 8）。
    pub major: Option<u32>,
    /// 位数（32 / 64；探测不到时按宿主）。
    pub bits: u32,
    /// 托管目录里的供应商（`temurin`）；其它来源为 `None`。
    pub vendor: Option<String>,
}

impl JavaRuntime {
    /// 探测不出来时给个「版本未知」的占位。
    #[must_use]
    pub fn unknown(binary: impl Into<PathBuf>, origin: JavaOrigin) -> Self {
        Self {
            binary: binary.into(),
            origin,
            version: None,
            major: None,
            bits: host_bits(),
            vendor: None,
        }
    }

    /// 主版本号（未知时退回 [`DEFAULT_MAJOR`]）。
    #[must_use]
    pub fn major_or_default(&self) -> u32 {
        self.major.unwrap_or(DEFAULT_MAJOR)
    }
}

/// 跑一次 `java -version` 填满运行时信息（探测失败不报错，只留空字段）。
///
/// 失败不报错的理由：`ull java list` 要能列出「探测不到的项」而不是整体失败，
/// 而启动路径上探测失败会自然退化为系统默认 Java（见 [`super::select::resolve`]）。
#[must_use]
pub fn probe(binary: &Path, origin: JavaOrigin) -> JavaRuntime {
    let mut runtime = JavaRuntime::unknown(binary.to_path_buf(), origin);
    let Ok(output) = quiet(binary).arg("-version").output() else {
        return runtime;
    };
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if let Some((line, major)) = parse_version_output(&text) {
        runtime.major = Some(major);
        runtime.version = Some(line);
        runtime.bits = bits_of(&text);
    }
    runtime
}

/// 扫描一个安装根里的托管 JDK，逐个探测。
///
/// **两种布局都要认**（D23 之后同时存在）：
///
/// ```text
/// <root>/jdk-<major>-<vendor>/bin/java      Portable（v1 布局，M33 定下，永不改）
/// <root>/<vendor>/<version>/bin/java        Standard / Custom（D23 的 <发行方>/<版本>）
/// ```
///
/// 只认带戳记的目录：托管区是我们自己写的地方，但没有戳记的子目录可能是用户塞进来的
/// 别的东西，不去猜它。`major` 一律以 `java -version` 为准（戳记只用于幂等判定）。
#[must_use]
pub fn discover_managed(managed_root: &Path) -> Vec<JavaRuntime> {
    let mut found: Vec<JavaRuntime> = Vec::new();
    let Ok(entries) = std::fs::read_dir(managed_root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || name.starts_with('.') {
            continue;
        }
        if let Some(runtime) = probe_managed_dir(&path, parse_managed_dir(&name).map(|(_, v)| v)) {
            found.push(runtime);
            continue;
        }
        // 两层布局：`<vendor>/<version>`——vendor 名取自父目录名。
        if JdkVendor::parse(&name).is_some() {
            let Ok(children) = std::fs::read_dir(&path) else {
                continue;
            };
            for child in children.flatten() {
                let child_path = child.path();
                if let Some(runtime) = probe_managed_dir(&child_path, Some(&name)) {
                    found.push(runtime);
                }
            }
        }
    }
    sort_runtimes(&mut found);
    found
}

/// 扫多个安装根（Portable 数据根 + Standard/Custom 落点），按 `bin` 路径去重。
#[must_use]
pub fn discover_managed_roots(roots: &[PathBuf]) -> Vec<JavaRuntime> {
    let mut found: Vec<JavaRuntime> = Vec::new();
    for root in roots {
        for runtime in discover_managed(root) {
            if !found.iter().any(|known| known.binary == runtime.binary) {
                found.push(runtime);
            }
        }
    }
    sort_runtimes(&mut found);
    found
}

/// 探测一个候选托管目录（有戳记 + `bin/java` 才算数）。
fn probe_managed_dir(path: &Path, vendor: Option<&str>) -> Option<JavaRuntime> {
    if !path.is_dir() || read_stamp(path).is_none() {
        return None;
    }
    let binary = path.join("bin").join(java_binary_name());
    if !binary.is_file() {
        return None;
    }
    let mut runtime = probe(&binary, JavaOrigin::Managed);
    runtime.vendor = vendor.map(str::to_string);
    Some(runtime)
}

/// 大版本升序，输出稳定（同 major 按路径）。
fn sort_runtimes(found: &mut [JavaRuntime]) {
    found.sort_by(|left, right| {
        left.major
            .cmp(&right.major)
            .then_with(|| left.binary.cmp(&right.binary))
    });
}

/// 系统来源的候选：`JAVA_HOME` 与 `PATH` 上的 `java`（都探测一次）。
///
/// `PATH` 上的那一项会先**解析成绝对路径**再探测：否则它与扫盘结果（绝对路径）看起来是
/// 两个不同的文件，`java list` 会把同一个 JDK 列两次。
#[must_use]
pub fn system_candidates() -> Vec<JavaRuntime> {
    let mut found = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let binary = PathBuf::from(home).join("bin").join(java_binary_name());
        if binary.is_file() {
            found.push(probe(&binary, JavaOrigin::JavaHome));
        }
    }
    let name = java_binary_name();
    let binary = resolve_on_path(name).unwrap_or_else(|| PathBuf::from(name));
    found.push(probe(&binary, JavaOrigin::Path));
    found
}

/// 在 `PATH` 里找某个可执行文件的绝对路径（找不到返回 `None`）。
#[must_use]
pub fn resolve_on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// 托管目录名（`jdk-21-temurin`）。
#[must_use]
pub fn managed_dir_name(major: u32, vendor: &str) -> String {
    format!("{MANAGED_PREFIX}{major}-{vendor}")
}

/// 反解托管目录名 →（大版本，供应商）。
#[must_use]
pub fn parse_managed_dir(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix(MANAGED_PREFIX)?;
    let (major, vendor) = rest.split_once('-')?;
    let major: u32 = major.parse().ok()?;
    (!vendor.is_empty()).then_some((major, vendor))
}

/// 从 `java -version` 输出里取（含 `version` 的首行，主版本号）。
#[must_use]
pub fn parse_version_output(text: &str) -> Option<(String, u32)> {
    let line = text
        .lines()
        .find(|line| line.contains("version"))
        .or_else(|| text.lines().find(|line| !line.trim().is_empty()))?
        .trim();
    let version = quoted_version(line).or_else(|| bare_version(line))?;
    let major = major_of(&version)?;
    Some((line.to_string(), major))
}

/// 引号里的版本号（`openjdk version "21.0.13" 2026-01-20 LTS`）。
fn quoted_version(line: &str) -> Option<String> {
    let start = line.find('"')? + 1;
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// 无引号形态（`openjdk 25.0.3 2026-04-21`）。
///
/// 「含 `version` 的那一段之后逐个试」优先——避开 `Java(TM) SE Runtime` 这类噪声；
/// 没有 `version` 字样时从头试（部分精简发行版就这么打印）。
fn bare_version(line: &str) -> Option<String> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    let start = tokens
        .iter()
        .position(|token| token.contains("version"))
        .map_or(0, |position| position + 1);
    tokens[start..].iter().find_map(|token| {
        let candidate = token.trim_matches(['"', ',', '(', ')']);
        (candidate.contains('.') && major_of(candidate).is_some()).then(|| candidate.to_string())
    })
}

/// 版本号 → 主版本（`1.8.0_402` → 8，`21.0.13` → 21）。
#[must_use]
pub fn major_of(version: &str) -> Option<u32> {
    let mut parts = version.split(['.', '_', '-']);
    let first = parts.next()?.parse::<u32>().ok()?;
    if first == 1 {
        parts.next()?.parse::<u32>().ok()
    } else {
        Some(first)
    }
}

/// `java -version` 输出里的位数（`64-Bit` / `32-Bit`；读不到按宿主）。
#[must_use]
pub fn bits_of(text: &str) -> u32 {
    let lowered = text.to_ascii_lowercase();
    if lowered.contains("64-bit") {
        64
    } else if lowered.contains("32-bit") {
        32
    } else {
        host_bits()
    }
}

/// 宿主指针宽度（32 / 64）。
#[must_use]
pub const fn host_bits() -> u32 {
    if cfg!(target_pointer_width = "64") {
        64
    } else {
        32
    }
}

#[cfg(test)]
mod tests {
    use super::{bits_of, major_of, managed_dir_name, parse_managed_dir, parse_version_output};

    #[test]
    fn parses_modern_and_legacy_version_lines() {
        let modern =
            parse_version_output("openjdk version \"21.0.13\" 2026-01-20 LTS\nOpenJDK 64-Bit");
        assert_eq!(modern.map(|(_, major)| major), Some(21));
        let legacy = parse_version_output("java version \"1.8.0_402\"\nJava(TM) SE Runtime");
        assert_eq!(legacy.map(|(_, major)| major), Some(8));
        let bare = parse_version_output("openjdk 25.0.3 2026-04-21\nOpenJDK 64-Bit Server VM");
        assert_eq!(bare.map(|(_, major)| major), Some(25));
        assert!(parse_version_output("no java here").is_none());
    }

    #[test]
    fn parses_bit_width() {
        assert_eq!(bits_of("OpenJDK 64-Bit Server VM (build 25.0.3)"), 64);
        assert_eq!(bits_of("Java HotSpot(TM) Client VM (32-Bit)"), 32);
    }

    #[test]
    fn major_of_handles_legacy_scheme() {
        assert_eq!(major_of("1.8.0_402"), Some(8));
        assert_eq!(major_of("17.0.13+11"), Some(17));
        assert_eq!(major_of("not-a-version"), None);
    }

    #[test]
    fn managed_dir_names_round_trip() {
        assert_eq!(parse_managed_dir("jdk-21-temurin"), Some((21, "temurin")));
        assert_eq!(parse_managed_dir("jdk-8-temurin"), Some((8, "temurin")));
        assert_eq!(parse_managed_dir(".staging-jdk-21-temurin"), None);
        assert_eq!(parse_managed_dir("java-21-temurin"), None);
        assert_eq!(parse_managed_dir("jdk-x-temurin"), None);
        assert_eq!(managed_dir_name(25, "temurin"), "jdk-25-temurin");
    }
}
