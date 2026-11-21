//! 报告里的**条目类型**：一条文件、一条冲突、一条缺失、一个 component、一个加载器目标。

use serde::Serialize;

use super::issue::ImportIssue;
use super::keys;
use crate::addon::loader::LoaderKind;
use crate::addon::pack::PackSide;
use crate::game::java::JavaOrigin;

/// 一条文件来自哪里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSource {
    /// 包内 `overrides/`（或 CF 清单点名的那个目录 / MultiMC 的 `.minecraft/`）。
    Overrides,
    /// 包内 `client-overrides/`。
    ClientOverrides,
    /// CurseForge `files[]` 条目（靠 projectID/fileID 补全）。
    CurseForge,
    /// Modrinth `files[]` 条目（清单自带 path + downloads）。
    Modrinth,
}

impl FileSource {
    /// 从覆盖目录的侧别取来源。
    #[must_use]
    pub const fn from_side(side: PackSide) -> Self {
        match side {
            PackSide::Both => Self::Overrides,
            PackSide::Client => Self::ClientOverrides,
            // 服务端覆盖在客户端导入里不会落地（见 `plan` 的侧别优先级）。
            PackSide::Server => Self::Overrides,
        }
    }
}

/// 一个文件本次被怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileAction {
    /// 新写入（磁盘上原本没有）。
    Installed,
    /// 覆盖写入（用户没改过，整合包升级）。
    Replaced,
    /// 磁盘内容与包内一致，没动。
    SkippedIdentical,
    /// 用户改过 / 删过，按策略保留了用户那一侧。
    KeptUser,
}

impl FileAction {
    /// 是否真的写了盘。
    #[must_use]
    pub const fn wrote(self) -> bool {
        matches!(self, Self::Installed | Self::Replaced)
    }
}

/// 冲突的成因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictReason {
    /// 基线里记过这个文件，磁盘内容与基线不同 ⇒ 用户改过。
    UserModified,
    /// 基线里记过这个文件，磁盘上却没了 ⇒ 用户删过。
    UserDeleted,
    /// 没有基线可判，磁盘上本来就有同名文件。
    PreExisting,
}

impl ConflictReason {
    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::UserModified => keys::CONFLICT_USER_MODIFIED,
            Self::UserDeleted => keys::CONFLICT_USER_DELETED,
            Self::PreExisting => keys::CONFLICT_PRE_EXISTING,
        }
    }
}

/// 冲突实际采用的处置（由 [`super::super::ConflictPolicy`] / [`super::super::ConflictDecisions`] 决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictChoice {
    /// 保留用户侧（不写盘）。
    KeepUser,
    /// 用包内内容覆盖（用户选了"覆盖"）。
    Overwrite,
}

impl Default for ConflictChoice {
    /// **缺省是保留**：没有决策时绝不覆盖用户已有文件（M160 的硬线，见 `import::decision`）。
    fn default() -> Self {
        Self::KeepUser
    }
}

/// 一条冲突（"无法判定时给可选择的冲突清单"就是这张表）。
#[derive(Debug, Clone, Serialize)]
pub struct ConflictFile {
    /// 实例内相对路径。
    pub path: String,
    /// 成因。
    pub reason: ConflictReason,
    /// 本次实际采用的一侧。
    pub choice: ConflictChoice,
    /// 文案 key（= `reason.message_key()`）。
    pub message_key: &'static str,
    /// 包内声明的 SHA-1。
    pub pack_sha1: String,
    /// 包内声明的字节数。
    pub pack_size: u64,
    /// 上次导入写下的 SHA-1（没有基线时 `None`）。
    pub base_sha1: Option<String>,
    /// 磁盘现状的 SHA-1（文件不存在时 `None`）。
    pub disk_sha1: Option<String>,
}

/// 一个"搞不到"的文件（**容忍**，但必须逐条列出）。
#[derive(Debug, Clone, Serialize)]
pub struct MissingFile {
    /// 实例内相对路径（mrpack 有；CF 补全失败时没有）。
    pub path: Option<String>,
    /// 人读标识（CF 条目是 `projectID:fileID`，其余是路径）。
    pub display: String,
    /// CF `projectID`。
    pub project_id: Option<i64>,
    /// CF `fileID`。
    pub file_id: Option<i64>,
    /// 为什么搞不到。
    pub reason: MissingReason,
    /// 文案 key。
    pub message_key: &'static str,
    /// 细节（错误原文）。
    pub detail: String,
}

/// 「离线」那一档缺文件的 `detail`（调用方没接 fetcher ⇒ 远端条目一律跳过）。
///
/// **这句话不许自称「离线」**（M325/P30 第 2 条）：CLI 的人读行是
/// `缺文件：mods/a.jar（离线导入：<这句>）`，GUI 的短标签也是「离线导入」——句子里再写一次，
/// 用户看到的就是「离线导入：…（离线），远端文件一律跳过」。标签已经说了那一半，`detail` 只
/// 补「为什么跳过 / 跳过了什么」。
///
/// 为什么是 `pub`（M325 r1 复审提问，已核）：**生产代码只有一处**用它——[`super::super::phase`]
/// 的 `missing_for`；其余四处都是**用例**（core 的 `failure_tests` 钉产出路径、cli 的 `m323`
/// 钉「detail 逐字仍在」、cli 的 `m325` 钉渲染）。cli 是另一个 crate，`pub(crate)` 会让那三条
/// 用例只能再抄一份字符串——抄一份就变成「改措辞不会红」，正是本常量要消灭的坏法。所以保留
/// `pub`，但它**不是**给界面/脚本的 API：`--json` 里看到的是 `missing[].detail` 的文本，
/// 不是这个常量名。
pub const MISSING_OFFLINE_DETAIL: &str = "本次导入没有接 fetcher，远端文件一律跳过";

/// "搞不到"的成因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingReason {
    /// 调用方没给 fetcher（离线）。
    Offline,
    /// 条目补不出下载地址（CF 缺 API 客户端 / `downloads` 为空）。
    NoSource,
    /// 尝试过但失败（404 / 网络 / 响应读不懂）。
    FetchFailed,
}

impl MissingReason {
    /// 文案 key。
    #[must_use]
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Offline => keys::MISSING_OFFLINE,
            Self::NoSource => keys::MISSING_NO_SOURCE,
            Self::FetchFailed => keys::MISSING_FETCH_FAILED,
        }
    }
}

/// 一个 MultiMC / Prism component 被识别为"本启动器不支持"（**逐项**列出，不静默丢）。
#[derive(Debug, Clone, Serialize)]
pub struct RejectedComponent {
    /// 原始 uid（`org.lwjgl3` / `net.fabricmc.intermediary` / 社区自定义）。
    pub uid: String,
    /// 该组件声明的版本。
    pub version: Option<String>,
    /// 文案 key（恒为 [`keys::REJECTED_COMPONENT`]）。
    pub message_key: &'static str,
}

/// 加载器安装计划的输入：**只需要这三个参数**就能接上 `addon::loader` 的既有能力。
#[derive(Debug, Clone)]
pub struct LoaderTarget {
    /// 加载器种类。
    pub kind: LoaderKind,
    /// 目标原版版本。
    pub mc_version: String,
    /// 加载器版本（已剥掉 CF 的 MC 版本前缀）。
    pub loader_version: String,
    /// 清单里的原始标识（诊断用）。
    pub raw: String,
    /// 实例绑定的版本 id（`1.20.1-fabric-0.15.11` 或纯原版 `1.20.1`）。
    pub version_id: String,
}

impl LoaderTarget {
    /// 直接喂给 `LoaderSource::resolve(kind, mc_version, Some(loader_version))`。
    #[must_use]
    pub fn resolve_args(&self) -> (LoaderKind, &str, &str) {
        (self.kind, &self.mc_version, &self.loader_version)
    }
}

impl Serialize for LoaderTarget {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;

        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("kind", self.kind.as_str())?;
        map.serialize_entry("label", self.kind.label())?;
        map.serialize_entry("mc_version", &self.mc_version)?;
        map.serialize_entry("loader_version", &self.loader_version)?;
        map.serialize_entry("version_id", &self.version_id)?;
        map.serialize_entry("raw", &self.raw)?;
        map.end()
    }
}

/// 一条落地的文件。
#[derive(Debug, Clone, Serialize)]
pub struct ImportedFile {
    /// 实例内相对路径（`.disabled` 后缀原样保留）。
    pub path: String,
    /// 来源。
    pub source: FileSource,
    /// 实际写入/比对用的 SHA-1。
    pub sha1: Option<String>,
    /// 字节数。
    pub bytes: u64,
    /// 是否禁用态（路径后缀表达，不附加语义）。
    pub disabled: bool,
    /// 处理方式。
    pub action: FileAction,
    /// 可选注记（`.disabled` / `optional` 之类）。
    pub note: Option<ImportIssue>,
}

/// 登记出来的实例（给 UI 显示"装到哪了"）。
#[derive(Debug, Clone, Serialize)]
pub struct RegisteredInstance {
    /// 实例 id（稳定 UUID）。
    pub id: String,
    /// 展示名。
    pub name: String,
    /// 绑定版本 id。
    pub version_id: String,
    /// 是否本次新建（`false` = 复用已有档案）。
    pub created: bool,
}

/// 一个 installer processor 步骤的结局（M321：**跑过的每一步都要留下这一条**）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessorStep {
    /// 序号（从 1 起，与 `ull install` 的 `--json` 同源）。
    pub index: usize,
    /// jar 的 maven 坐标（`net.minecraftforge:installertools:1.3.0` 之类）。
    pub label: String,
    /// 结局 id：`executed`（真起了一次 java）/ `skipped`（产物已就绪，整步跳过）。
    pub status: &'static str,
    /// 产物个数。
    pub outputs: usize,
    /// 耗时（毫秒；跳过时为 0）。
    pub elapsed_ms: u64,
}

/// 第六跳 processor 链的汇总（[`super::ImportReport::processors`]）。
///
/// 为什么是一份**自己的**结构而不是直接把 `ProcessorRun` 塞进报告：报告要能序列化给
/// CLI 的 `--json` 与 GUI 的 DTO 层（`app-iced` 只吃可序列化类型），而 `ProcessorRun` 里的
/// `JavaRuntime` 带着宿主路径与探测细节。这里只留「谁跑的、跑了几步、每步什么结局、有什么告警」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProcessorSummary {
    /// 实际用的 Java 可执行文件。
    pub java: String,
    /// Java 来源。
    ///
    /// 存**枚举**而不是 `&'static str`（M325）：人读那一行要按来源取一句目录文案，用 slug 去
    /// `match` 就没有穷尽性——core 哪天加一种来源，命令行那边悄悄印出没词的 slug 而不报错。
    /// 存枚举以后，壳层的 `match` 少一个臂就**编译不过**。
    ///
    /// `--json` 的形状不变：仍然序列化成原来那个稳定 id（`managed` / `JAVA_HOME` / `PATH` /
    /// `system` / `instance`），见本模块末尾的 `origin_id`。
    #[serde(serialize_with = "origin_id")]
    pub java_origin: JavaOrigin,
    /// Java 主版本（探测不到时 `None`）。
    pub java_major: Option<u32>,
    /// 真的起过 java 的步数。
    pub executed: usize,
    /// 产物已就绪而整步跳过的步数。
    pub skipped: usize,
    /// 逐步骤结果（顺序即执行顺序）。
    pub steps: Vec<ProcessorStep>,
    /// 不致命的告警（摘要不符、没有 outputs 的步骤等）。它们同时会进
    /// [`super::ImportReport::warnings`]（由 `note_processors` 铺进去）。
    pub warnings: Vec<String>,
}

impl ProcessorSummary {
    /// 从一次执行结果转成报告条目（唯一入口：谁都不许自己数一遍步数）。
    #[must_use]
    pub fn from_run(run: &crate::addon::loader::processors::ProcessorRun) -> Self {
        Self {
            java: run.java.binary.display().to_string(),
            java_origin: run.java.origin,
            java_major: run.java.major,
            executed: run.executed(),
            skipped: run.skipped(),
            steps: run
                .steps
                .iter()
                .map(|step| ProcessorStep {
                    index: step.index,
                    label: step.label.clone(),
                    status: step.status.as_str(),
                    outputs: step.outputs,
                    elapsed_ms: u64::try_from(step.elapsed_ms).unwrap_or(u64::MAX),
                })
                .collect(),
            warnings: run.warnings.clone(),
        }
    }
}

/// [`JavaOrigin`] 的序列化：`--json` 里仍是原来那个稳定 id（`managed` / `JAVA_HOME` / `PATH` /
/// `system` / `instance`），**形状一字未变**（M325 把字段类型从 `&'static str` 换成枚举时钉住的
/// 就是这一条：脚本读的 `processors.java_origin` 不许换形状，换的只是「人读那行怎么取词」）。
fn origin_id<S>(origin: &JavaOrigin, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(origin.as_str())
}
