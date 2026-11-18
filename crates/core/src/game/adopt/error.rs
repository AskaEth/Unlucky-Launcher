//! 一次登记的失败形状（CLI 据此给稳定错误码 + 结构化 details）。
//!
//! 与 [`AdoptProblem`] 的分工：`AdoptProblem` 是「这个版本为什么不能登记」（可以多条、逐条列出），
//! [`AdoptError`] 是「这次调用为什么没成功」（一条，喂给 `--json` 的 `error.code`）。
//! 候选不合格是后者的一个变体，但问题清单原样带出来——不许把原因折叠成一句「导入失败」。

use std::fmt;
use std::path::PathBuf;

use super::candidate::AdoptProblem;
use crate::game::error::GameError;

/// 一次登记失败（CLI 据此给稳定错误码 + 结构化 details）。
#[derive(Debug)]
pub enum AdoptError {
    /// `core::game` 的解析/路径/IO 失败。
    Game(GameError),
    /// 版本 id 在盘上不存在（没有 `versions/<id>/`）。
    UnknownVersion {
        /// 请求的版本 id。
        version_id: String,
        /// 期望的目录。
        version_dir: PathBuf,
    },
    /// 候选不合格（`problems` 就是全部原因，一条都不吞）。
    NotAdoptable {
        /// 版本 id。
        version_id: String,
        /// 全部原因。
        problems: Vec<AdoptProblem>,
    },
    /// 展示名已被别的实例占用（**显式报错**，不静默复用一条别的版本）。
    DuplicateName {
        /// 冲突的展示名。
        name: String,
        /// 占用它的实例 id。
        instance_id: String,
    },
    /// 这个版本已经以同名实例登记过了（重复执行 = 无副作用失败）。
    AlreadyRegistered {
        /// 版本 id。
        version_id: String,
        /// 已存在的实例名。
        name: String,
        /// 已存在的实例 id。
        instance_id: String,
    },
    /// 缺省运行目录已存在且非空（不猜「合并进已有实例」，也不覆盖）。
    TargetNotEmpty {
        /// 那个目录。
        path: PathBuf,
    },
    /// 运行目录选择本身不成立（`custom` 缺目录 / 相对路径）。
    TargetInvalid {
        /// 人读原因。
        detail: String,
    },
    /// 缺省运行目录的后缀全被占用（`-2` … `-33` 都撞上了）——不无限顺延，直接报错。
    NoFreeRunDir {
        /// 派生用的基准目录。
        base: PathBuf,
    },
    /// 配置写面（`config/`、`config/instances/`、`profiles.json`）的真实落点不可信：
    /// 跑出 `<.ULL>` 或落进游戏根（符号链接 / 目录联接）。
    ConfigSurfaceUnsafe {
        /// 出问题的目标路径（`config/`、`config/instances/` 或 `profiles.json`）。
        path: PathBuf,
        /// 人读原因（含解析后的真实位置）。
        detail: String,
    },
    /// 默认写盘面（`.ULL`/`data`/配置/缺省运行目录）解析后落在**受保护游戏树**之内（r2 R2.2）。
    ///
    /// 与 [`Self::ConfigSurfaceUnsafe`] 的分工：那条管「配置跑出 `.ULL`」，这条管「写盘面与游戏树重叠」。
    WriteSurfaceInGameTree {
        /// 出问题的写盘面（`.ULL`、`data/`、配置目录、缺省运行目录…）。
        path: PathBuf,
        /// 人读原因（含解析后的真实位置与「请换不重叠的数据目录」的指引）。
        detail: String,
    },
    /// `versions` 入口存在但读不出来（断链的符号链接/目录联接、目标被移走、祖先断链）。
    ///
    /// 与「`versions` 真不存在」严格区分：后者才是空结果，前者必须具名报错。
    VersionsDirUnreadable {
        /// 出问题的路径（`versions/` 或游戏根）。
        path: PathBuf,
        /// 人读原因。
        detail: String,
    },
}

impl AdoptError {
    /// 稳定机读码（`--json` 的 `error.code`）。
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Game(_) => "adopt_io",
            Self::UnknownVersion { .. } => "adopt_version_not_found",
            Self::NotAdoptable { .. } => "adopt_not_adoptable",
            Self::DuplicateName { .. } => "adopt_duplicate_name",
            Self::AlreadyRegistered { .. } => "adopt_already_registered",
            Self::TargetNotEmpty { .. } => "adopt_target_not_empty",
            Self::TargetInvalid { .. } => "adopt_target_invalid",
            Self::NoFreeRunDir { .. } => "adopt_no_free_run_dir",
            Self::ConfigSurfaceUnsafe { .. } => "adopt_config_surface_unsafe",
            Self::WriteSurfaceInGameTree { .. } => "adopt_write_surface_in_game_tree",
            Self::VersionsDirUnreadable { .. } => "adopt_versions_unreadable",
        }
    }

    /// 候选不合格时的问题清单（其它错误为空）。
    #[must_use]
    pub fn problems(&self) -> &[AdoptProblem] {
        match self {
            Self::NotAdoptable { problems, .. } => problems,
            _ => &[],
        }
    }
}

impl fmt::Display for AdoptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Game(error) => write!(f, "{error}"),
            Self::UnknownVersion {
                version_id,
                version_dir,
            } => write!(
                f,
                "版本 {version_id} 不在盘上（没有 {}）",
                version_dir.display()
            ),
            Self::NotAdoptable {
                version_id,
                problems,
            } => {
                write!(f, "版本 {version_id} 不能登记：")?;
                for (index, problem) in problems.iter().enumerate() {
                    if index > 0 {
                        f.write_str("；")?;
                    }
                    write!(f, "{problem}")?;
                }
                Ok(())
            }
            Self::DuplicateName { name, instance_id } => write!(
                f,
                "已有同名实例 `{name}`（id {instance_id}）：导入时请用 --name 换一个展示名"
            ),
            Self::AlreadyRegistered {
                version_id,
                name,
                instance_id,
            } => write!(
                f,
                "版本 {version_id} 已经登记过（实例 `{name}`，id {instance_id}）：没有改动任何文件"
            ),
            Self::TargetNotEmpty { path } => {
                write!(f, "运行目录已存在且非空，不覆盖：{}", path.display())
            }
            Self::TargetInvalid { detail } => write!(f, "运行目录选择不成立：{detail}"),
            Self::NoFreeRunDir { base } => write!(
                f,
                "为这个实例派生独立运行目录失败：{} 及其 -2…-33 后缀都被别的实例占着",
                base.display()
            ),
            Self::ConfigSurfaceUnsafe { path, detail } => {
                write!(f, "配置写面不可信：{} {detail}", path.display())
            }
            Self::WriteSurfaceInGameTree { path, detail } => {
                write!(f, "写盘面与游戏树重叠：{} {detail}", path.display())
            }
            Self::VersionsDirUnreadable { path, detail } => write!(
                f,
                "versions 入口存在但读不出来 {}：{detail}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AdoptError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Game(error) => Some(error),
            _ => None,
        }
    }
}

impl From<GameError> for AdoptError {
    fn from(error: GameError) -> Self {
        Self::Game(error)
    }
}
