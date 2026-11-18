//! 规则库加载：YAML 分组文件 + 内嵌兜底 + 热重载。
//!
//! 对应 PlanDoc/06 §2.2「规则文件 YAML 外置可热更」（修 HMCL 编译期枚举不可热更的债）：
//! 1. 内嵌一份（`include_str!`）：任何情况下都能诊断，不依赖安装目录完整；
//! 2. 用户目录 `data/diagnose/rules.d/*.yaml` 覆盖内嵌的同名文件（缺哪个用内嵌补哪个）；
//! 3. 热重载是 **mtime+size 指纹轮询**（[`RuleLoader::refresh`]），不引 `notify` 依赖；
//!    解析失败时**保留上一份好规则**并返回错误列表——用户手改错 YAML 不会让诊断能力归零。

use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::rules::{CompiledRule, Rule, RuleCategory};

/// 规则目录（相对数据根，由 platform 模块解析）。
pub const RULES_DIR: &str = "diagnose/rules.d";

/// 规则 schema 版本（文件里 `version: 1`）。
pub const SCHEMA_VERSION: u32 = 1;

/// 内嵌规则库：`(文件名, 内容)`。按分类拆文件，每文件守 400 行上限。
pub const EMBEDDED_RULES: &[(&str, &str)] = &[
    ("jvm.yaml", include_str!("rules.d/jvm.yaml")),
    ("memory.yaml", include_str!("rules.d/memory.yaml")),
    ("graphics.yaml", include_str!("rules.d/graphics.yaml")),
    ("integrity.yaml", include_str!("rules.d/integrity.yaml")),
    ("fabric.yaml", include_str!("rules.d/fabric.yaml")),
    ("forge.yaml", include_str!("rules.d/forge.yaml")),
    ("mod_runtime.yaml", include_str!("rules.d/mod_runtime.yaml")),
    (
        "mod_conflict.yaml",
        include_str!("rules.d/mod_conflict.yaml"),
    ),
    ("world.yaml", include_str!("rules.d/world.yaml")),
    ("other.yaml", include_str!("rules.d/other.yaml")),
];

/// 一个 YAML 文件的 schema。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    #[serde(default)]
    version: u32,
    category: RuleCategory,
    rules: Vec<Rule>,
}

/// 编译好的规则集（字段私有，只能经 loader 构造，保证「id 唯一 + 正则可编译」）。
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<CompiledRule>,
}

impl RuleSet {
    /// 规则表。
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        &self.rules
    }

    /// 规则条数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// 是否空规则集。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 按 id 查规则（测试 / 调试用）。
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CompiledRule> {
        self.rules.iter().find(|rule| rule.id() == id)
    }

    /// 从 `(文件名, 内容)` 列表编译；有任何错误就整体失败（调用方决定是否回退）。
    pub fn from_sources(sources: &[(String, String)]) -> Result<Self, Vec<LoadError>> {
        let mut errors = Vec::new();
        let mut rules: Vec<CompiledRule> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (file, text) in sources {
            let parsed = match serde_yaml_ng::from_str::<RuleFile>(text) {
                Ok(parsed) => parsed,
                Err(error) => {
                    errors.push(LoadError::yaml(file, &error));
                    continue;
                }
            };
            if parsed.version > SCHEMA_VERSION {
                errors.push(LoadError {
                    file: file.clone(),
                    kind: LoadErrorKind::UnsupportedVersion,
                    message: format!(
                        "规则文件 version={} 高于本程序支持的 {}",
                        parsed.version, SCHEMA_VERSION
                    ),
                });
                continue;
            }
            for rule in parsed.rules {
                if !seen.insert(rule.id.clone()) {
                    errors.push(LoadError {
                        file: file.clone(),
                        kind: LoadErrorKind::DuplicateId,
                        message: format!("规则 id 重复：{}", rule.id),
                    });
                    continue;
                }
                match CompiledRule::compile(rule, parsed.category) {
                    Ok(compiled) => rules.push(compiled),
                    Err(error) => errors.push(LoadError {
                        file: file.clone(),
                        kind: LoadErrorKind::Regex,
                        message: error.to_string(),
                    }),
                }
            }
        }
        if errors.is_empty() {
            Ok(Self { rules })
        } else {
            Err(errors)
        }
    }
}

/// 加载错误分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadErrorKind {
    /// 读盘失败。
    Io,
    /// YAML 语法/schema 错误（`serde_yaml_ng` 的报错带行号）。
    Yaml,
    /// 正则编译失败。
    Regex,
    /// 规则 id 跨文件重复。
    DuplicateId,
    /// 规则文件版本高于本程序。
    UnsupportedVersion,
}

impl LoadErrorKind {
    /// 稳定字符串。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Yaml => "yaml",
            Self::Regex => "regex",
            Self::DuplicateId => "duplicate_id",
            Self::UnsupportedVersion => "unsupported_version",
        }
    }
}

/// 一条加载错误（带文件名，用户改 YAML 时能定位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadError {
    /// 文件名（磁盘目录加载时是完整路径）。
    pub file: String,
    /// 分类。
    pub kind: LoadErrorKind,
    /// 报错原文（YAML 错误含行号）。
    pub message: String,
}

impl LoadError {
    fn yaml(file: &str, error: &serde_yaml_ng::Error) -> Self {
        Self {
            file: file.to_owned(),
            kind: LoadErrorKind::Yaml,
            message: error.to_string(),
        }
    }

    fn io(file: &Path, error: &std::io::Error) -> Self {
        Self {
            file: file.display().to_string(),
            kind: LoadErrorKind::Io,
            message: error.to_string(),
        }
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}] {}：{}",
            self.kind.as_str(),
            self.file,
            self.message
        )
    }
}

impl std::error::Error for LoadError {}

/// 热重载结果。
#[derive(Debug, Clone, PartialEq)]
pub enum ReloadOutcome {
    /// 指纹没变，规则未动。
    Unchanged,
    /// 重新加载成功。
    Reloaded {
        /// 新规则条数。
        rules: usize,
    },
    /// 新文件有错：**保留旧规则**，错误交给调用方展示。
    Rejected {
        /// 错误列表。
        errors: Vec<LoadError>,
    },
}

/// 规则库加载器（持有当前生效的规则集 + 目录指纹）。
#[derive(Debug, Clone)]
pub struct RuleLoader {
    dir: Option<PathBuf>,
    set: RuleSet,
    fingerprint: u64,
    last_errors: Vec<LoadError>,
}

impl Default for RuleLoader {
    fn default() -> Self {
        Self::embedded()
    }
}

impl RuleLoader {
    /// 只用内嵌规则（无磁盘目录）。
    #[must_use]
    pub fn embedded() -> Self {
        let sources = embedded_sources();
        let (set, errors) = compile_sources(&sources);
        Self {
            dir: None,
            set,
            fingerprint: 0,
            last_errors: errors,
        }
    }

    /// 以磁盘目录为准、内嵌补齐缺文件。
    ///
    /// 磁盘规则有任何错误时**退回内嵌规则**并记录错误（保证永远有可用规则集）。
    #[must_use]
    pub fn from_dir(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let (sources, mut errors) = merged_sources(Some(&dir));
        let set = match RuleSet::from_sources(&sources) {
            Ok(set) => set,
            Err(mut load_errors) => {
                errors.append(&mut load_errors);
                RuleSet::from_sources(&embedded_sources()).unwrap_or_default()
            }
        };
        Self {
            fingerprint: dir_fingerprint(&dir),
            dir: Some(dir),
            set,
            last_errors: errors,
        }
    }

    /// 当前生效的规则。
    #[must_use]
    pub fn rules(&self) -> &[CompiledRule] {
        self.set.rules()
    }

    /// 当前生效的规则集。
    #[must_use]
    pub fn rule_set(&self) -> &RuleSet {
        &self.set
    }

    /// 上次加载留下的错误（UI 可提示「你的规则文件有错，已用内置规则」）。
    #[must_use]
    pub fn last_errors(&self) -> &[LoadError] {
        &self.last_errors
    }

    /// 规则目录（内嵌模式为 `None`）。
    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// 目录指纹（测试用：确认 mtime/size 变化真的改变了指纹）。
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    /// 轮询热重载：指纹变了才重新解析。
    pub fn refresh(&mut self) -> ReloadOutcome {
        let Some(dir) = self.dir.clone() else {
            return ReloadOutcome::Unchanged;
        };
        let fingerprint = dir_fingerprint(&dir);
        if fingerprint == self.fingerprint {
            return ReloadOutcome::Unchanged;
        }
        self.fingerprint = fingerprint;
        let (sources, mut errors) = merged_sources(Some(&dir));
        match RuleSet::from_sources(&sources) {
            Ok(set) => {
                self.set = set;
                self.last_errors = errors;
                ReloadOutcome::Reloaded {
                    rules: self.set.len(),
                }
            }
            Err(mut load_errors) => {
                errors.append(&mut load_errors);
                self.last_errors = errors.clone();
                ReloadOutcome::Rejected { errors }
            }
        }
    }
}

fn embedded_sources() -> Vec<(String, String)> {
    EMBEDDED_RULES
        .iter()
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect()
}

/// 磁盘文件覆盖内嵌：磁盘有的用磁盘，没的用内嵌。
fn merged_sources(dir: Option<&Path>) -> (Vec<(String, String)>, Vec<LoadError>) {
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for (name, embedded) in EMBEDDED_RULES {
        let path = dir.map(|dir| dir.join(name));
        match path {
            Some(path) if path.is_file() => match fs::read_to_string(&path) {
                Ok(text) => sources.push((path.display().to_string(), text)),
                Err(error) => {
                    errors.push(LoadError::io(&path, &error));
                    sources.push(((*name).to_owned(), (*embedded).to_owned()));
                }
            },
            _ => sources.push(((*name).to_owned(), (*embedded).to_owned())),
        }
    }
    (sources, errors)
}

fn compile_sources(sources: &[(String, String)]) -> (RuleSet, Vec<LoadError>) {
    match RuleSet::from_sources(sources) {
        Ok(set) => (set, Vec::new()),
        Err(errors) => (RuleSet::default(), errors),
    }
}

/// 目录指纹：所有 `*.yaml` 的（文件名, 长度, mtime）之和。
///
/// 长度 + mtime 足以发现「保存了但秒级 mtime 未变」之外的一切编辑；漏检时用户点刷新即可。
fn dir_fingerprint(dir: &Path) -> u64 {
    let mut entries = fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("yaml"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    entries.sort();
    let mut hasher = DefaultHasher::new();
    for path in entries {
        path.file_name().unwrap_or_default().hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .hash(&mut hasher);
        }
    }
    hasher.finish()
}
