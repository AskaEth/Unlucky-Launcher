//! 环境快照与 mod 清单（analyse/02 §7.1 信号 E）。
//!
//! HMCL 只在崩溃窗口里散着展示这些信息；我们做成结构化数据，
//! 既进诊断输出（`environment.json` 一样的机器可读上下文），也让规则能按版本号做前置判断。
//! 由 `collectors.rs` 用 `#[path]` 声明并统一导出。

use std::path::PathBuf;

/// 一个已装组件的版本（Forge/Fabric/OptiFine…）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentVersion {
    /// 组件名（`forge`/`fabric`/`optifine`/`minecraft`…）。
    pub name: String,
    /// 版本串。
    pub version: String,
}

impl ComponentVersion {
    /// 构造。
    #[must_use]
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

/// 一个已装 mod（供交叉验证「冲突/mixin 失败」类结论，analyse/02 §8.4-10）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModRecord {
    /// modid（解析不出时为空）。
    pub id: String,
    /// 展示名。
    pub name: Option<String>,
    /// 版本。
    pub version: Option<String>,
    /// jar 文件名（诊断包里给人看）。
    pub file: Option<String>,
    /// 是否启用（`.disabled` 视为未启用）。
    pub enabled: bool,
}

impl ModRecord {
    /// 从文件名构造（不解 jar，P0 足够：冲突类结论要的是「装了哪些文件」）。
    #[must_use]
    pub fn from_file_name(file: &str) -> Self {
        let enabled = !file.ends_with(".disabled");
        let stem = file
            .trim_end_matches(".disabled")
            .trim_end_matches(".jar")
            .to_owned();
        let (name, version) = stem
            .rsplit_once('-')
            .filter(|(_, version)| version.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
            .map_or((stem.clone(), None), |(name, version)| {
                (name.to_owned(), Some(version.to_owned()))
            });
        Self {
            id: name.to_lowercase(),
            name: Some(name),
            version,
            file: Some(file.to_owned()),
            enabled,
        }
    }
}

/// 启动环境快照（Java/内存/OS/组件/mod 清单）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnvironmentSnapshot {
    /// 启动器版本（`ull 0.1.0`）。
    pub launcher: Option<String>,
    /// 实例 id。
    pub instance_id: Option<String>,
    /// 游戏版本。
    pub game_version: Option<String>,
    /// 操作系统名（`Windows 10.0.19045`）。
    pub os_name: Option<String>,
    /// 架构（`x86_64` / `aarch64`）。
    pub os_arch: Option<String>,
    /// 物理内存（MB）。
    pub physical_memory_mb: Option<u64>,
    /// Java 可执行文件。
    pub java_path: Option<PathBuf>,
    /// Java 大版本。
    pub java_major: Option<u32>,
    /// Java 位数（32 / 64）。
    pub java_bits: Option<u32>,
    /// `-Xmx`（MB）。
    pub xmx_mb: Option<u32>,
    /// `-Xms`（MB）。
    pub xms_mb: Option<u32>,
    /// 工作目录（跑游戏的 cwd）。
    pub run_dir: Option<PathBuf>,
    /// 已装组件版本。
    pub components: Vec<ComponentVersion>,
    /// 已装 mod。
    pub mods: Vec<ModRecord>,
}

impl EnvironmentSnapshot {
    /// 一行人类可读摘要（证据行 / UI 头部）。
    #[must_use]
    pub fn summary(&self) -> String {
        let os = format!(
            "{} {}",
            self.os_name.as_deref().unwrap_or("未知系统"),
            self.os_arch.as_deref().unwrap_or("")
        );
        let java = self.java_major.map_or_else(
            || "Java 未知".to_owned(),
            |major| match self.java_bits {
                Some(bits) => format!("Java {major}（{bits} 位）"),
                None => format!("Java {major}"),
            },
        );
        let memory = match (self.xmx_mb, self.physical_memory_mb) {
            (Some(xmx), Some(physical)) => format!("-Xmx {xmx} MB / 物理 {physical} MB"),
            (Some(xmx), None) => format!("-Xmx {xmx} MB"),
            (None, Some(physical)) => format!("物理 {physical} MB"),
            (None, None) => "内存未知".to_owned(),
        };
        let mods = self.mods.len();
        format!("{} / {} / {} / {} 个 mod", os.trim(), java, memory, mods)
    }

    /// 是不是 32 位 Java（`-Xmx` 超过 ~1.5 GB 必崩的直接证据）。
    #[must_use]
    pub fn is_java_32bit(&self) -> bool {
        self.java_bits == Some(32)
    }

    /// 组件版本（按名字查，`optifine` 之类的冲突规则用）。
    #[must_use]
    pub fn component(&self, name: &str) -> Option<&str> {
        self.components
            .iter()
            .find(|component| component.name.eq_ignore_ascii_case(name))
            .map(|component| component.version.as_str())
    }

    /// 是否装了某个 mod（按 id 或文件名模糊匹配）。
    #[must_use]
    pub fn has_mod(&self, id: &str) -> bool {
        let needle = id.to_ascii_lowercase();
        self.mods.iter().any(|record| {
            record.id.contains(&needle)
                || record
                    .file
                    .as_deref()
                    .is_some_and(|file| file.to_ascii_lowercase().contains(&needle))
        })
    }
}
