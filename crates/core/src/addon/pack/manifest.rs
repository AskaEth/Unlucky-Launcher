//! 与格式无关的中间模型 [`PackManifest`]。
//!
//! # 为什么要中间模型
//!
//! CF 的 `manifest.json` 与 Modrinth 的 `modrinth.index.json` 表达同一件事的方式不同：
//!
//! | 概念 | CurseForge | Modrinth `.mrpack` | 模型字段 |
//! |------|-----------|--------------------|----------|
//! | 包名 | `name` | `name` | [`PackManifest::name`] |
//! | 包版本 | `version` | `versionId` | [`PackManifest::version`] |
//! | 作者 | `author` | **没有** | [`PackManifest::author`] |
//! | 简介 | **没有** | `summary` | [`PackManifest::summary`] |
//! | 游戏版本 | `minecraft.version` | `dependencies.minecraft` | [`PackManifest::minecraft`] |
//! | 加载器 | `minecraft.modLoaders[].id`（`forge-47.2.0`） | `dependencies["forge"] = "47.2.0"` | [`PackManifest::loaders`] |
//! | 远端文件 | `files[].projectID/fileID`（**无路径、无 URL**） | `files[].path/hashes/downloads/env` | [`PackManifest::files`] |
//! | 覆盖目录 | `overrides`（目录名由清单给，可任意） | `overrides` + `client-overrides` + `server-overrides` | [`PackManifest::overrides`] |
//! | 禁用位 | 无（Prism 借 `required: false` 表达） | 路径 `.disabled` 后缀（官方，本仓库采这条） | [`PackFile::disabled`] |
//! | 多 MC 实例（MultiMC） | — | — | 同上（`.minecraft` 当覆盖目录） |
//!
//! 方向不同造成的缺口**不靠"假装字段都在"抹平**，而是显式记录：
//!
//! - CF 的 `files[]` 没有路径 → [`PackFile::path`] 是 `Option`，`None` 表示"要靠远端 API 补全"
//!   （D10 §2 的两阶段补全就是补这个）；写 mrpack 时缺路径即
//!   `crate::addon::pack::error::PackErrorKind::Unsupported`。
//! - CF 只有一个 `required` 布尔 → 读成 `client`/`server` 同值，写回时 `client == Optional` 即
//!   `required = false`；`client`/`server` 不同的条目不适用于 CF 导出（文档写明，不静默降级）。
//!
//! # 未知字段不丢（"社区标准加字段不炸"的具体手段）
//!
//! 三处 `extra` 用 `serde(flatten)` 接住不认识的 JSON 字段，**原样 Value 保存**，
//! 各格式的 `render` 再把它们并回去（已知字段优先，冲突时丢弃 extra 里的同名键）：
//!
//! - [`PackManifest::extra`]：清单顶层；
//! - [`PackManifest::minecraft_extra`]：CF `minecraft` 对象内部（mrpack 没有这一层）；
//! - [`PackFile::extra`] / [`PackFile::env_extra`] / [`PackLoader::extra`]：条目、`env`、加载器条目内部。
//!
//! 哈希不按"sha1/sha512 两个字段"建模，而是 [`PackFile::hashes`] 整张表：mrpack 允许
//! `sha1`/`sha256`/`sha512`/`md5` 同时出现，只留两个字段就会丢信息。

mod entries;

pub use entries::{merge_unknown, OverrideFile, PackFile, PackOverrides};

use std::collections::HashMap;

use serde_json::{Map, Value};

use super::path::normalize_file;
use crate::addon::LoaderKind;

/// 客户端 / 服务端环境标记（mrpack `env` 的三值，CF 只有 `required` 布尔）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnvSupport {
    /// 必需（mrpack `required` / CF `required: true`）。
    #[default]
    Required,
    /// 可选（mrpack `optional` / CF `required: false`）。
    ///
    /// **不是禁用位**：真实语义是"安装时用户可取消勾选"，装回来默认仍然启用。
    /// 禁用是两回事，在 mrpack 里表达为路径后缀 `.disabled`（见 [`super::PackFile::disabled`]）。
    Optional,
    /// 本侧不支持（mrpack `unsupported`；CF 表达不了）。
    Unsupported,
}

impl EnvSupport {
    /// 解析 mrpack 的字符串形式；不认识的返回 `None`（调用方决定报错还是忽略）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "required" => Some(Self::Required),
            "optional" => Some(Self::Optional),
            "unsupported" => Some(Self::Unsupported),
            _ => None,
        }
    }

    /// mrpack 的字符串形式。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
            Self::Unsupported => "unsupported",
        }
    }

    /// 是否"可以不要"。
    #[must_use]
    pub const fn is_optional(self) -> bool {
        matches!(self, Self::Optional)
    }
}

/// 覆盖目录的目标侧（mrpack 用三个目录区分，CF 只有一个）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PackSide {
    /// 客户端与服务端都要（`overrides` / `.minecraft`）。
    #[default]
    Both,
    /// 只要客户端（`client-overrides`）。
    Client,
    /// 只要服务端（`server-overrides`）。
    Server,
}

impl PackSide {
    /// 人读名称（进日志/报告）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Client => "client",
            Self::Server => "server",
        }
    }

    /// mrpack 里这一侧的标准目录名。
    #[must_use]
    pub const fn mrpack_dir(self) -> &'static str {
        match self {
            Self::Both => "overrides",
            Self::Client => "client-overrides",
            Self::Server => "server-overrides",
        }
    }

    /// 该侧的覆盖内容对客户端是否生效。
    #[must_use]
    pub const fn applies_to_client(self) -> bool {
        matches!(self, Self::Both | Self::Client)
    }

    /// 该侧的覆盖内容对服务端是否生效。
    #[must_use]
    pub const fn applies_to_server(self) -> bool {
        matches!(self, Self::Both | Self::Server)
    }
}

/// 一个加载器依赖。
///
/// `raw` 保留原始标识，读写两侧因此无损：
/// - CF：`raw = "forge-47.2.0"`，`kind = Forge`，`version = "47.2.0"`；
/// - mrpack：`raw = "fabric-loader"`，`kind = Fabric`，`version = "0.15.11"`；
/// - MultiMC：`raw = "net.fabricmc.fabric-loader"`，`kind = Fabric`，`version = "0.15.11"`。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackLoader {
    /// 原始标识（按格式原样保存）。
    pub raw: String,
    /// 规范化加载器种类；不认识的加载器是 `None`（仍然保留 `raw` 与 `version`）。
    pub kind: Option<LoaderKind>,
    /// 加载器版本。
    pub version: Option<String>,
    /// 是否主加载器（CF `primary`；其它格式取"第一条能认出来的"）。
    pub primary: bool,
    /// 该加载器条目的未知字段。
    pub extra: Map<String, Value>,
}

impl PackLoader {
    /// 从 CF 的 `modLoaders[].id`（`forge-47.2.0`）解析。
    #[must_use]
    pub fn parse_curse(id: &str, primary: bool) -> Self {
        let (kind, version) = split_curse_id(id);
        Self {
            raw: id.to_string(),
            kind,
            version,
            primary,
            extra: Map::new(),
        }
    }

    /// 从 mrpack `dependencies` 的一条键值解析（`fabric-loader` → `0.15.11`）。
    #[must_use]
    pub fn parse_dependency(key: &str, version: &str, primary: bool) -> Self {
        Self {
            raw: key.to_string(),
            kind: parse_kind_key(key),
            version: Some(version.to_string()),
            primary,
            extra: Map::new(),
        }
    }

    /// 从 MultiMC 组件 `uid` + `version` 解析（`net.fabricmc.fabric-loader`）。
    #[must_use]
    pub fn parse_multimc(uid: &str, version: &str, primary: bool) -> Self {
        let kind = match uid {
            "net.fabricmc.fabric-loader" => Some(LoaderKind::Fabric),
            "org.quiltmc.quilt-loader" => Some(LoaderKind::Quilt),
            "net.minecraftforge" => Some(LoaderKind::Forge),
            "net.neoforged" => Some(LoaderKind::NeoForge),
            _ => None,
        };
        Self {
            raw: uid.to_string(),
            kind,
            version: Some(version.to_string()),
            primary,
            extra: Map::new(),
        }
    }

    /// 写成 CF 的 `id`。
    ///
    /// `raw` 非空时**逐字回写**（CF 的 id 是不透明字符串，不认识的加载器也得原样带过去）；
    /// 只有从别的格式转过来的 `raw` 为空的情况才用 `kind`+`version` 拼。等于
    /// `curse_id_for(None)`——跨格式导出请用带游戏版本的那个，见 [`Self::curse_id_for`]。
    #[must_use]
    pub fn curse_id(&self) -> Option<String> {
        self.curse_id_for(None)
    }

    /// 写成 CF 的 `id`，并按 CF 官方导出器的写法处理 **NeoForge 的 MC 前缀特例**。
    ///
    /// 证据（A 级 `FlamePackExportTask.cpp:404-410`，转述见 research/18 §8.2）：Prism 的 CF
    /// 导出在 MC **1.20.1** 上写 `neoforge-1.20.1-<ver>`，其它版本写 `neoforge-<ver>`。
    /// 原因是 NeoForge 在 1.20.1 上沿用 Forge 的 `47.x` 版本号，不写 MC 版本就分不清代际。
    /// 读侧（`split_curse_id`）不剥这个前缀——那是 M111 的 `strip_mc_prefix` 的活。
    #[must_use]
    pub fn curse_id_for(&self, mc_version: Option<&str>) -> Option<String> {
        if !self.raw.is_empty() {
            return Some(self.raw.clone());
        }
        let kind = self.kind?;
        let version = self.version.as_deref()?;
        if kind == LoaderKind::NeoForge && mc_version == Some("1.20.1") {
            return Some(format!("{}-1.20.1-{version}", kind.as_str()));
        }
        Some(format!("{}-{version}", kind.as_str()))
    }

    /// 写成 mrpack `dependencies` 的键。
    ///
    /// 认得种类时用 Modrinth 的规范拼法（`fabric-loader` / `quilt-loader` / `forge` /
    /// `neoforge`）——这是 Modrinth 自己的键名约定，CF 的 `forge-47.2.0` 与 MultiMC 的
    /// `net.fabricmc.fabric-loader` 都靠它翻译过去。种类未知时（社区新加载器）逐字回写 `raw`。
    #[must_use]
    pub fn dependency_key(&self) -> String {
        match self.kind {
            Some(LoaderKind::Fabric) => "fabric-loader".to_string(),
            Some(LoaderKind::Quilt) => "quilt-loader".to_string(),
            Some(kind) => kind.as_str().to_string(),
            None => self.raw.clone(),
        }
    }
}

/// CF 的 `modLoaders[].id` 里认得上哪些前缀。
fn split_curse_id(id: &str) -> (Option<LoaderKind>, Option<String>) {
    for kind in LoaderKind::all() {
        let prefix = format!("{}-", kind.as_str());
        if let Some(rest) = id.strip_prefix(&prefix) {
            if !rest.is_empty() {
                return (Some(*kind), Some(rest.to_string()));
            }
        }
    }
    (None, None)
}

/// mrpack `dependencies` 的键 → 加载器种类（`fabric-loader` / `neo-forge` 两种拼法都认）。
fn parse_kind_key(key: &str) -> Option<LoaderKind> {
    let lowered = key.trim().to_ascii_lowercase();
    let bare = lowered.strip_suffix("-loader").unwrap_or(&lowered);
    LoaderKind::parse(bare)
}

/// 与格式无关的整合包模型。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PackManifest {
    /// 包名（CF `name` / mrpack `name`）。
    pub name: String,
    /// 包版本（CF `version` / mrpack `versionId`）。
    pub version: Option<String>,
    /// 作者（CF `author`；mrpack 没有这一项）。
    pub author: Option<String>,
    /// 简介（mrpack `summary`；CF 没有这一项）。
    pub summary: Option<String>,
    /// 清单 schema 版本（CF `manifestVersion` / mrpack `formatVersion` / MultiMC `formatVersion`）。
    pub format_version: Option<i64>,
    /// 平台标识（mrpack `game`，恒为 `minecraft`；CF 隐式，`None`）。
    pub game: Option<String>,
    /// 游戏版本（`1.20.1`）。
    pub minecraft: Option<String>,
    /// 加载器依赖集合。
    pub loaders: Vec<PackLoader>,
    /// 远端文件条目（不含覆盖目录里的文件）。
    pub files: Vec<PackFile>,
    /// 覆盖目录清单。
    pub overrides: Vec<PackOverrides>,
    /// 清单顶层的未知字段（原样保存、导出时回写）。
    pub extra: Map<String, Value>,
    /// CF `minecraft` 对象里的未知字段（mrpack/MultiMC 没有这一层）。
    pub minecraft_extra: Map<String, Value>,
}

impl PackManifest {
    /// 建一个只有包名的空清单。
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// 按目标侧找覆盖目录。
    #[must_use]
    pub fn overrides_for(&self, side: PackSide) -> Option<&PackOverrides> {
        self.overrides.iter().find(|group| group.side == side)
    }

    /// 覆盖目录里的全部文件，按 `(side, path)` 展平（三方合并要按实例路径查）。
    pub fn override_files(&self) -> impl Iterator<Item = (PackSide, &OverrideFile)> {
        self.overrides
            .iter()
            .flat_map(|group| group.files.iter().map(move |file| (group.side, file)))
    }

    /// 某个加载器的依赖（有就返回第一条）。
    #[must_use]
    pub fn loader(&self, kind: LoaderKind) -> Option<&PackLoader> {
        self.loaders.iter().find(|loader| loader.kind == Some(kind))
    }

    /// 有远端来源的条目（M111 的补全目标）。
    pub fn remote_files(&self) -> impl Iterator<Item = &PackFile> {
        self.files.iter().filter(|file| file.is_remote_reachable())
    }

    /// 校验模型里所有路径安全（导出前必过：模型可能来自外部构造）。
    ///
    /// 覆盖目录前缀与文件路径都要过 [`normalize_file`] / [`normalize_dir`] 的同一条规则。
    pub fn check_paths(&self) -> super::error::PackResult<()> {
        use super::path::normalize_dir;

        for group in &self.overrides {
            normalize_dir(&group.dir)?;
            for file in &group.files {
                normalize_file(&file.path)?;
            }
        }
        for file in &self.files {
            if let Some(path) = &file.path {
                normalize_file(path)?;
            }
        }
        Ok(())
    }

    /// 撞同一实例路径的覆盖条目：`(路径, 涉及的侧, 条目份数)`，按路径排序。
    ///
    /// **两种冲突都要报**（r2 p2-2：上一版按侧去重，把"同侧撞车"这条能力换掉了）：
    ///
    /// - **跨侧**：`overrides/`（Both）与 `client-overrides/`（Client）写同一个实例路径——
    ///   mrpack 唯一的真实冲突。按 `(side, path)` 计数会恒返回空表（两侧键天然不同）。
    /// - **同侧**：两个同侧组撞同一路径，或一个组里同一路径列了两次（模型被合并时才出现）。
    ///   按侧去重会把这份重复折叠掉，于是同样"查不出来"。
    ///
    /// 判据：`侧数 > 1 || 条份数 > 侧数`（后者即同侧重复）。冲突由导入器决定优先级，
    /// 本模块只报告、不选边。
    #[must_use]
    pub fn duplicate_override_paths(&self) -> Vec<(String, Vec<PackSide>, usize)> {
        struct Slot {
            sides: Vec<PackSide>,
            occurrences: usize,
        }

        let mut slots: HashMap<&str, Slot> = HashMap::new();
        for (side, file) in self.override_files() {
            let slot = slots.entry(file.path.as_str()).or_insert(Slot {
                sides: Vec::new(),
                occurrences: 0,
            });
            slot.occurrences += 1;
            if !slot.sides.contains(&side) {
                slot.sides.push(side);
            }
        }

        let mut duplicated: Vec<(String, Vec<PackSide>, usize)> = slots
            .into_iter()
            .filter(|(_, slot)| slot.sides.len() > 1 || slot.occurrences > slot.sides.len())
            .map(|(path, mut slot)| {
                slot.sides.sort_by_key(|side| side.as_str());
                (path.to_string(), slot.sides, slot.occurrences)
            })
            .collect();
        duplicated.sort_by(|left, right| left.0.cmp(&right.0));
        duplicated
    }
}
