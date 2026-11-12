//! 登记的**数据形状**：候选、结构化失败、运行目录选择。
//!
//! 三种形状刻意分开：
//! - [`AdoptCandidate`]：盘上的一个 `versions/<id>/` 是不是能登记的版本（含逐项问题）；
//! - [`AdoptProblem`]：一条**结构化**的不合格原因（有稳定 `code`，人读 `message`）；
//! - [`AdoptTarget`]：登记后运行目录从哪来（隔离档由它决定）；
//! - 整次调用的失败形状在 [`super::error::AdoptError`]。
//!
//! 为什么不把问题塞进 [`crate::game::GameError`]：`core::game` 的错误是「解析/路径/IO」这一层，
//! 而这里是「这个版本对我们而言能不能登记」的业务判据（缺父、越界、同名冲突…）。
//! 混进 `GameError` 会让所有现存 `match` 多出一批只在本模块有意义的变体。

use std::fmt;
use std::path::PathBuf;

use super::super::install::instance::Isolation;

/// 候选版本的**结构化**不合格原因（每一条都带稳定机读 `code`）。
///
/// 判据来源：`DevDocs/artifacts/m218/SURVEY-p4.md` §3 的 copy/引用/拒绝表。任一 `Err` 都意味着
/// 「登记了也启动不了」或「越界/踩别人的目录」，因此登记前一律拒绝，并把原因原样说给用户。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdoptProblem {
    /// `versions/<id>/` 存在但没有同名清单（只剩 natives / 上个启动器的残留）。
    ManifestMissing {
        /// 版本 id（目录名）。
        version_id: String,
        /// 期望的清单路径 `versions/<id>/<id>.json`。
        path: PathBuf,
    },
    /// 版本 id 本身不能当路径用（空 / 含分隔符 / 盘符 / NUL / 结尾点号空格）。
    VersionIdInvalid {
        /// 原始 id。
        version_id: String,
        /// 人读原因。
        reason: &'static str,
    },
    /// 清单文件读不出来（权限 / 被占用 / 不是 UTF-8）。
    ManifestUnreadable {
        /// 清单路径。
        path: PathBuf,
        /// 底层错误文本（含 Win32 错误码或 serde 行列号）。
        message: String,
    },
    /// 清单不是合法 JSON（**带行号列号**，别只说「坏了」）。
    ManifestInvalidJson {
        /// 清单路径。
        path: PathBuf,
        /// serde_json 的错误文本（含 `line L column C`）。
        message: String,
    },
    /// 清单里没有 `id`。
    ManifestMissingId {
        /// 清单路径。
        path: PathBuf,
    },
    /// 清单里的 `id` 与目录名不一致（启动时按清单 id 找 jar，必然找不到）。
    ManifestIdMismatch {
        /// 清单路径。
        path: PathBuf,
        /// 清单里的 id。
        manifest_id: String,
        /// 目录名。
        directory_id: String,
    },
    /// 目录遍历/元数据读不出来（权限、坏链接）。
    DirectoryUnreadable {
        /// 出问题的路径。
        path: PathBuf,
        /// 底层错误文本。
        message: String,
    },
    /// 已有条目解析到游戏根之外（符号链接 / 目录联接 / 祖先链接；不跟随）。
    OutsideGameRoot {
        /// 链接本身。
        path: PathBuf,
        /// 解析出来的真实目标。
        target: PathBuf,
        /// 比较基准（真实化后的游戏根）。
        root: PathBuf,
    },
    /// 是符号链接但解析不出来（断链 / 权限）。
    SymlinkUnresolvable {
        /// 链接本身。
        path: PathBuf,
        /// 底层错误文本。
        message: String,
    },
    /// `inheritsFrom` 指向的父版本在盘上找不到。
    MissingParent {
        /// 缺失的父版本 id。
        parent: String,
        /// 谁在要它。
        required_by: String,
    },
    /// `inheritsFrom` 成环。
    CircularInheritance {
        /// 从根到环的 id 链（首尾同一个 id）。
        chain: Vec<String>,
    },
    /// 继承链在 `core::game::resolve` 那里失败（走查通过、权威解析不通过的兜底）。
    InheritanceInvalid {
        /// `GameError` 的人读文本。
        detail: String,
    },
}

impl AdoptProblem {
    /// 稳定机读码（`--json` 的 `problems[].code`）。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ManifestMissing { .. } => "manifest_missing",
            Self::VersionIdInvalid { .. } => "version_id_invalid",
            Self::ManifestUnreadable { .. } => "manifest_unreadable",
            Self::ManifestInvalidJson { .. } => "manifest_invalid_json",
            Self::ManifestMissingId { .. } => "manifest_missing_id",
            Self::ManifestIdMismatch { .. } => "manifest_id_mismatch",
            Self::DirectoryUnreadable { .. } => "directory_unreadable",
            Self::OutsideGameRoot { .. } => "outside_game_root",
            Self::SymlinkUnresolvable { .. } => "symlink_unresolvable",
            Self::MissingParent { .. } => "missing_parent",
            Self::CircularInheritance { .. } => "circular_inheritance",
            Self::InheritanceInvalid { .. } => "inheritance_invalid",
        }
    }
}

impl fmt::Display for AdoptProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestMissing { version_id, path } => write!(
                f,
                "版本 {version_id} 没有清单文件（期望 {}）",
                path.display()
            ),
            Self::VersionIdInvalid { version_id, reason } => {
                write!(f, "版本 id 不能当路径用（{reason}）：{version_id:?}")
            }
            Self::ManifestUnreadable { path, message } => {
                write!(f, "清单读不出来 {}：{message}", path.display())
            }
            Self::ManifestInvalidJson { path, message } => {
                write!(f, "清单不是合法 JSON {}：{message}", path.display())
            }
            Self::ManifestMissingId { path } => {
                write!(f, "清单缺少 id 字段：{}", path.display())
            }
            Self::ManifestIdMismatch {
                path,
                manifest_id,
                directory_id,
            } => write!(
                f,
                "清单 id（{manifest_id}）与目录名（{directory_id}）不一致：{}——启动时会按清单 id 找 jar，必然找不到",
                path.display()
            ),
            Self::DirectoryUnreadable { path, message } => {
                write!(f, "目录读不出来 {}：{message}", path.display())
            }
            Self::OutsideGameRoot { path, target, root } => write!(
                f,
                "条目解析到游戏根之外，不跟随：{} -> {}（游戏根 {}）",
                path.display(),
                target.display(),
                root.display()
            ),
            Self::SymlinkUnresolvable { path, message } => {
                write!(f, "符号链接解析失败 {}：{message}", path.display())
            }
            Self::MissingParent { parent, required_by } => {
                write!(f, "版本 {required_by} 继承的父版本 {parent} 不在盘上")
            }
            Self::CircularInheritance { chain } => {
                write!(f, "版本继承成环：{}", chain.join(" -> "))
            }
            Self::InheritanceInvalid { detail } => write!(f, "继承链解析失败：{detail}"),
        }
    }
}

/// 已经绑定某个版本 id 的实例档案（只带展示需要的最小信息，不泄露内部结构）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredInstance {
    /// 实例 id。
    pub id: String,
    /// 实例展示名。
    pub name: String,
}

/// 盘上的一个候选版本（扫出来的一个 `versions/<id>/`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptCandidate {
    /// 版本 id（= 目录名，登记后写进 `profiles.json` 的 `version_id`）。
    pub version_id: String,
    /// 版本目录 `versions/<id>/`。
    pub version_dir: PathBuf,
    /// 清单路径 `versions/<id>/<id>.json`。
    pub manifest_path: PathBuf,
    /// 客户端 jar `versions/<id>/<id>.jar` 是否存在（只报事实，不校验内容）。
    pub jar_present: bool,
    /// 清单里声明的 id（读不出来时为 `None`）。
    pub manifest_id: Option<String>,
    /// `inheritsFrom` 链（自身在前、最祖先在后；读不出来时是已走通的那一段）。
    pub inherits: Vec<String>,
    /// 已有实例绑定这个版本（空 = 还没登记过）。
    pub registered: Vec<RegisteredInstance>,
    /// 问题清单（空 = 可登记）。
    pub problems: Vec<AdoptProblem>,
}

impl AdoptCandidate {
    /// 是否可登记（没有任何问题）。
    #[must_use]
    pub fn adoptable(&self) -> bool {
        self.problems.is_empty()
    }

    /// 已登记过（至少有一个实例绑定这个版本 id）。
    #[must_use]
    pub fn is_registered(&self) -> bool {
        !self.registered.is_empty()
    }
}

/// 登记后实例的**运行目录**从哪来（隔离档由它决定，见 [`AdoptTarget::isolation`]）。
///
/// 缺省是 [`Self::InstanceDir`]——**不是** `isolated`：理由见模块文档。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AdoptTarget {
    /// 缺省：ULL 数据区里新建独立运行目录 `<data>/instances/<实例目录名>`（写 `custom` 档）。
    #[default]
    InstanceDir,
    /// 共用当前游戏根（别人的 `.minecraft`）——必须显式选，且必须告知后果。
    Shared,
    /// 版本目录 `versions/<id>/`（`isolated` 档）——会在别人的版本目录里长玩家数据。
    VersionDir,
    /// 用户指定的目录（`custom` 档，必须绝对路径）。
    Custom(PathBuf),
}

impl AdoptTarget {
    /// 稳定机读 id（`--json` 的 `run_dir_source`）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InstanceDir => "instance_dir",
            Self::Shared => "shared",
            Self::VersionDir => "version_dir",
            Self::Custom(_) => "custom",
        }
    }

    /// 生效隔离档。
    #[must_use]
    pub const fn isolation(&self) -> Isolation {
        match self {
            Self::InstanceDir | Self::Custom(_) => Isolation::Custom,
            Self::Shared => Isolation::Shared,
            Self::VersionDir => Isolation::Isolated,
        }
    }

    /// 命令行/配置里的词表（`shared` / `isolated` / `custom`，与 [`Isolation`] 同一套字面量）。
    ///
    /// `instance_dir` 有意**不接受**输入：它是缺省值，不是一个用户可以手写选中的档位。
    #[must_use]
    pub fn parse(raw: &str, game_dir: Option<PathBuf>) -> Option<Self> {
        match raw.trim() {
            "shared" => Some(Self::Shared),
            "isolated" | "per_version" => Some(Self::VersionDir),
            "custom" => game_dir.map(Self::Custom),
            _ => None,
        }
    }
}
