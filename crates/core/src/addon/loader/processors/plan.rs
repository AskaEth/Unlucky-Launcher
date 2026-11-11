//! processor 执行计划：`data` 表展开 → 每步的 `java -cp …` 命令行 → 依赖库待下载清单。
//!
//! 纯计算（不下载、不起进程），因此 `--dry-run` 能直接把「将执行的 processor 清单 +
//! 所需 Java + 依赖库」打出来。唯一读盘的动作是 [`crate::game::java::requirement_for_version`]
//! 读原版版本 JSON 判 Java 要求——那是诊断信息，读不到就按静态表兜底。
//!
//! # 为什么依赖库与启动 classpath 要分开
//!
//! install profile 的 `libraries` 是 **processor 依赖**（installertools / jarsplitter /
//! srgutils…），它们不进游戏 classpath。M32 只把数量记进告警，M35 起真正下载它们
//! （否则 `java -cp` 一排文件都不在盘上），但仍然**绝不**并进 loader 版本 JSON。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::error::ProcessorError;
use super::spec::Processor;
use super::vars::{absolute, Variables, VariablesInput, MINECRAFT_JAR_NAME};
use crate::addon::loader::forge::InstallProfile;
use crate::addon::loader::{maven, LoaderKind};
use crate::download::ResourceKind;
use crate::game::arguments::Environment;
use crate::game::install::libraries;
use crate::game::install::{FileRole, PlanFile};
use crate::game::java::{requirement_for_version, JavaRequirement};
use crate::game::layout::GameLayout;
use crate::game::library::MavenCoordinate;

/// 一个 processor 的某个产物。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepOutput {
    /// 产物绝对路径。
    pub path: PathBuf,
    /// 期望 SHA-1（profile 没给或给的不是摘要时为 `None`，只能按存在性判定）。
    pub sha1: Option<String>,
}

/// 一个待执行的 processor（已展开参数，不再依赖 install profile）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessorStep {
    /// 序号（从 1 起，顺序即执行顺序）。
    pub index: usize,
    /// 人读标识（jar 的 maven 坐标）。
    pub label: String,
    /// 可执行 jar 的磁盘路径。
    pub jar: PathBuf,
    /// 主类（profile 没写就留 `None`，执行期读 jar 的 MANIFEST）。
    pub main_class: Option<String>,
    /// `-cp` 的依赖 jar（不含 `jar` 自己）。
    pub classpath: Vec<PathBuf>,
    /// 已展开的参数。
    pub args: Vec<String>,
    /// 产物与期望摘要。
    pub outputs: Vec<StepOutput>,
}

impl ProcessorStep {
    /// 一步的 `-cp` 全串（依赖在前、可执行 jar 在后，与 HMCL 一致）。
    ///
    /// # Errors
    ///
    /// 路径里含分隔符时返回 IO 错误（Windows 的 `;` 不可能出现在合法路径里，
    /// 真出现说明路径是拼错的）。
    pub fn classpath_join(&self) -> Result<String, ProcessorError> {
        let paths = self
            .classpath
            .iter()
            .chain(std::iter::once(&self.jar))
            .map(|path| path.as_path());
        std::env::join_paths(paths)
            .map(|joined| joined.to_string_lossy().into_owned())
            .map_err(|error| ProcessorError::io(format!("classpath 拼不起来：{error}")))
    }
}

/// [`ProcessorPlan::build`] 的输入。
pub struct ProcessorPlanInput<'a> {
    /// 加载器种类（只影响补库时的 maven 基址兜底）。
    pub kind: LoaderKind,
    /// 目标原版版本。
    pub mc_version: &'a str,
    /// loader 版本 id（依赖库 `hint=local` 的落点）。
    pub version_id: &'a str,
    /// installer 里读出来的 install profile。
    pub profile: &'a InstallProfile,
    /// 游戏目录布局。
    pub layout: &'a GameLayout,
    /// installer jar 的磁盘路径。
    pub installer: &'a Path,
    /// 临时目录（解包/`MINECRAFT_JAR` 副本都落这里）。
    pub scratch: &'a Path,
    /// 规则求值环境。
    pub environment: &'a Environment,
}

/// processor 执行计划。
#[derive(Debug, Clone)]
pub struct ProcessorPlan {
    /// loader 版本 id。
    pub version_id: String,
    /// installer jar 路径。
    pub installer: PathBuf,
    /// 临时目录。
    pub scratch: PathBuf,
    /// 原版 client jar（复制源）。
    pub minecraft_jar: PathBuf,
    /// 交给 processor 的 client jar 副本（`{MINECRAFT_JAR}`）。
    pub minecraft_jar_copy: PathBuf,
    /// 展开后的变量表。
    pub variables: Variables,
    /// 按顺序待执行的 processor。
    pub steps: Vec<ProcessorStep>,
    /// processor 依赖库（走 M27 引擎下载）。
    pub libraries: Vec<PlanFile>,
    /// 跑 installer 需要的 Java 要求。
    pub java_requirement: JavaRequirement,
    /// 规划期告警（不致命）。
    pub warnings: Vec<String>,
}

impl ProcessorPlan {
    /// 组计划。
    ///
    /// # Errors
    ///
    /// `data` 表变量缺定义、构件坐标不合法、processor 缺 `jar` 字段时返回规划错误。
    pub fn build(input: ProcessorPlanInput<'_>) -> Result<Self, ProcessorError> {
        let scratch = absolute(input.scratch);
        let variables = Variables::build(VariablesInput {
            data: &input.profile.data,
            layout: input.layout,
            mc_version: input.mc_version,
            installer: input.installer,
            scratch: &scratch,
        })?;

        let mut warnings = Vec::new();
        let mut steps = Vec::new();
        let mut needed: Vec<(String, PathBuf)> = Vec::new();
        for processor in input
            .profile
            .processors
            .iter()
            .filter(|processor| processor.applies_to_client())
        {
            let coordinate = processor.jar_coordinate().ok_or_else(|| {
                ProcessorError::plan("install profile 里有 processor 缺 `jar` 字段")
            })?;
            let jar = library_path(input.layout, coordinate)?;
            needed.push((coordinate.to_string(), jar.clone()));
            let mut classpath = Vec::with_capacity(processor.classpath.len());
            for entry in &processor.classpath {
                let path = library_path(input.layout, entry)?;
                needed.push((entry.trim().to_string(), path.clone()));
                classpath.push(path);
            }
            let mut args = Vec::with_capacity(processor.args.len());
            for arg in &processor.args {
                args.push(variables.expand(arg)?);
            }
            let outputs = step_outputs(&variables, processor, coordinate, &scratch, &mut warnings)?;
            steps.push(ProcessorStep {
                index: steps.len() + 1,
                label: coordinate.to_string(),
                jar,
                main_class: processor.main_class().map(str::to_string),
                classpath,
                args,
                outputs,
            });
        }

        let (mut libraries, library_warnings) = libraries::plan(
            input.layout,
            input.version_id,
            &input.profile.libraries,
            input.environment,
        );
        warnings.extend(library_warnings);
        let base = library_base(input.kind, input.profile);
        for (coordinate, dest) in &needed {
            if libraries.iter().any(|file| &file.dest == dest) {
                continue;
            }
            warnings.push(format!(
                "processor 依赖 {coordinate} 不在 install profile 的 libraries 节里，按 maven 基址补一条"
            ));
            let Ok(parsed) = MavenCoordinate::parse(coordinate) else {
                continue;
            };
            libraries.push(PlanFile {
                role: FileRole::Library,
                name: coordinate.clone(),
                url: join_url(&base, &parsed.relative_path()),
                sha1: None,
                size: None,
                dest: dest.clone(),
                resource: ResourceKind::Library,
            });
        }
        // NeoForge 的 install profile 会把同一条依赖**声明多次**（实测 21.1.80：
        // cli-utils / srgutils / jopt-simple / gson 各 2~3 次）。不下重：同一落点被下两次
        // 会在同一个 `.part` 中间文件上互相抢（真机现场：原子落盘 os error 2）。
        let (libraries, duplicated) = dedupe_by_dest(libraries);
        if duplicated > 0 {
            warnings.push(format!(
                "install profile 重复声明了 {duplicated} 个 processor 依赖，已按落点去重"
            ));
        }

        let java_requirement = requirement_for_version(input.layout, input.mc_version);
        let minecraft_jar_copy = variables
            .get("MINECRAFT_JAR")
            .map_or_else(|| scratch.join(MINECRAFT_JAR_NAME), PathBuf::from);
        Ok(Self {
            version_id: input.version_id.to_string(),
            installer: absolute(input.installer),
            scratch,
            minecraft_jar: input.layout.version_jar(input.mc_version),
            minecraft_jar_copy,
            variables,
            steps,
            libraries,
            java_requirement,
            warnings,
        })
    }

    /// 是否一个 processor 都没有。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// 汇总行数（CLI 人读输出用）。
    #[must_use]
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// `--dry-run` / 报告里的 processors 载荷。
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "count": self.steps.len(),
            "installer": self.installer.display().to_string(),
            "scratch": self.scratch.display().to_string(),
            "minecraft_jar": self.minecraft_jar.display().to_string(),
            "libraries": self.libraries.len(),
            "library_files": self.libraries.iter().map(|file| json!({
                "name": file.name,
                "url": file.url,
                "dest": file.dest.display().to_string(),
                "sha1": file.sha1,
                "size": file.size,
            })).collect::<Vec<_>>(),
            "variables": self.variables.len(),
            "embedded_files": self.variables.embedded().len(),
            "java_requirement": {
                "minimum": self.java_requirement.minimum,
                "maximum": self.java_requirement.maximum,
                "describe": self.java_requirement.describe(),
                "source": self.java_requirement.source.as_str(),
            },
            "steps": self.steps.iter().map(step_json).collect::<Vec<_>>(),
            "warnings": self.warnings,
        })
    }
}

/// 一条 processor 的 JSON 视图。
fn step_json(step: &ProcessorStep) -> Value {
    json!({
        "index": step.index,
        "jar": step.label,
        "jar_path": step.jar.display().to_string(),
        "main_class": step.main_class,
        "classpath": step.classpath.len(),
        "args": step.args,
        "outputs": step.outputs.iter().map(|output| json!({
            "path": output.path.display().to_string(),
            "sha1": output.sha1,
        })).collect::<Vec<_>>(),
    })
}

/// 展开一步的 `outputs` 表（路径 + 期望摘要）。
fn step_outputs(
    variables: &Variables,
    processor: &Processor,
    coordinate: &str,
    scratch: &Path,
    warnings: &mut Vec<String>,
) -> Result<Vec<StepOutput>, ProcessorError> {
    let mut outputs = Vec::with_capacity(processor.outputs.len());
    for (raw_path, raw_sha1) in &processor.outputs {
        let expanded = variables.expand(raw_path)?;
        let path = PathBuf::from(&expanded);
        let path = if path.is_absolute() {
            path
        } else {
            scratch.join(path)
        };
        let expanded_sha1 = variables.expand(raw_sha1).unwrap_or_default();
        let sha1 = is_sha1(&expanded_sha1).then(|| expanded_sha1.to_ascii_lowercase());
        if sha1.is_none() {
            warnings.push(format!(
                "processor {coordinate} 的输出 {} 没给 SHA-1，只能按存在性判定",
                path.display()
            ));
        }
        outputs.push(StepOutput { path, sha1 });
    }
    Ok(outputs)
}

/// 40 位十六进制（SHA-1 的样子）。
fn is_sha1(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// 坐标 → `libraries/` 下的路径。
fn library_path(layout: &GameLayout, raw: &str) -> Result<PathBuf, ProcessorError> {
    let coordinate = MavenCoordinate::parse(raw.trim()).map_err(|error| {
        ProcessorError::plan(format!("processor 依赖坐标不合法：`{raw}`（{error}）"))
    })?;
    Ok(layout.libraries_dir().join(coordinate.relative_path()))
}

/// 补库时用的 maven 基址：install profile 自己声明的 `url` 优先，否则按加载器兜底。
fn library_base(kind: LoaderKind, profile: &InstallProfile) -> String {
    profile
        .libraries
        .iter()
        .filter_map(|library| library.url.as_deref())
        .map(str::trim)
        .find(|url| !url.is_empty())
        .map_or_else(|| maven::base(kind).to_string(), str::to_string)
}

/// `基址 + 相对路径`（保证中间恰好一个 `/`）。
fn join_url(base: &str, relative: &Path) -> String {
    let path = relative.to_string_lossy().replace('\\', "/");
    if base.ends_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// 按落点去重（保留首次出现的条目），返回（去重后的清单，丢掉几条）。
fn dedupe_by_dest(files: Vec<PlanFile>) -> (Vec<PlanFile>, usize) {
    let total = files.len();
    let mut seen: Vec<PathBuf> = Vec::with_capacity(total);
    let mut kept: Vec<PlanFile> = Vec::with_capacity(total);
    for file in files {
        if seen.contains(&file.dest) {
            continue;
        }
        seen.push(file.dest.clone());
        kept.push(file);
    }
    let dropped = total - kept.len();
    (kept, dropped)
}
