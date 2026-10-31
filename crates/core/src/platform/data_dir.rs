//! 三模式数据目录解析（Portable 默认 / Standard / Custom）。
//!
//! 设计出处：`DevDocs/PlanDoc/04-data-and-config.md` §1——
//! 用户原话定案：「1.默认在运行目录下生成 .ULL .minecraft 目录 2.设置内可以更改到标准的用户应用数据目录
//! 3.用户可手动指定数据目录」。
//!
//! 附加约束：`.ULL` 与 `.minecraft` 同根但**可分离**（`launcher_home` / `game_home` 覆盖项），
//! macOS `.app` 只读所以默认走 Standard。

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::error::{PlatformError, PlatformResult};
use super::layout::{GAME_HOME_DIR, LAUNCHER_HOME_DIR};
use super::paths::{self, EnvSource, Platform, ProcessEnv};
use super::redirect;

/// 数据目录三模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DataMode {
    /// exe 运行目录下生成 `.ULL/` 与 `.minecraft/`（首次运行的默认）。
    #[default]
    Portable,
    /// 各 OS 标准应用数据目录。
    Standard,
    /// 用户手动指定目录。
    Custom,
}

impl DataMode {
    /// 稳定名（写进 launcher.json 与日志）。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Portable => "portable",
            Self::Standard => "standard",
            Self::Custom => "custom",
        }
    }

    /// 从配置字符串解析。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "portable" => Some(Self::Portable),
            "standard" => Some(Self::Standard),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    /// 该平台的默认模式：macOS 的 `.app` 内是只读的 → 默认 Standard（§1 细节）。
    #[must_use]
    pub const fn default_for(platform: Platform) -> Self {
        match platform {
            Platform::MacOs => Self::Standard,
            _ => Self::Portable,
        }
    }
}

impl fmt::Display for DataMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 设置里关于数据目录的原始输入。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DataDirRequest {
    /// 三模式。
    pub mode: DataMode,
    /// Custom 模式的根目录（必须绝对路径）。
    pub custom_root: Option<PathBuf>,
    /// 高级：显式覆盖启动器数据目录（`.ULL` 的等价物）。
    pub launcher_home: Option<PathBuf>,
    /// 高级：游戏目录与数据目录分离指定（整合包用户：游戏在 D 盘，配置在用户目录）。
    pub game_home: Option<PathBuf>,
}

/// 解析结果：实际生效的两个 home。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedData {
    /// 生效模式。
    pub mode: DataMode,
    /// 启动器数据目录（四分区之家）。
    pub launcher_home: PathBuf,
    /// 游戏目录（`.minecraft`）。
    pub game_home: PathBuf,
    /// 经 `redirect.json` 跳转时的原始目录（未跳转为 `None`）。
    pub redirected_from: Option<PathBuf>,
}

impl ResolvedData {
    /// 生效的启动器布局视图。
    #[must_use]
    pub fn layout(&self) -> super::layout::LauncherLayout {
        super::layout::LauncherLayout::new(self.launcher_home.clone())
    }

    /// 是否和另一份解析结果指向同一组目录（迁移判定用）。
    #[must_use]
    pub fn same_target(&self, other: &Self) -> bool {
        self.launcher_home == other.launcher_home && self.game_home == other.game_home
    }
}

/// 三模式解析器。环境与 exe 目录都可注入，单测里能确定性地覆盖各 OS 落点。
pub struct Resolver<'env> {
    platform: Platform,
    exe_dir: Option<PathBuf>,
    env: &'env dyn EnvSource,
}

impl Resolver<'static> {
    /// 用真实进程环境构造。
    #[must_use]
    pub fn new(platform: Platform, exe_dir: Option<PathBuf>) -> Self {
        Self {
            platform,
            exe_dir,
            env: &ProcessEnv,
        }
    }
}

impl<'env> Resolver<'env> {
    /// 注入环境来源（测试 / 便携化封装）。
    #[must_use]
    pub fn with_env(
        platform: Platform,
        exe_dir: Option<PathBuf>,
        env: &'env dyn EnvSource,
    ) -> Self {
        Self {
            platform,
            exe_dir,
            env,
        }
    }

    /// 平台。
    #[must_use]
    pub const fn platform(&self) -> Platform {
        self.platform
    }

    /// 该平台的默认模式。
    #[must_use]
    pub const fn default_mode(&self) -> DataMode {
        DataMode::default_for(self.platform)
    }

    /// 按三模式解析出实际目录（不跟随 `redirect.json`）。
    ///
    /// # Errors
    /// Portable 拿不到 exe 目录、Standard 拿不到标准目录、Custom 没给绝对路径时返回错误。
    pub fn resolve(&self, request: &DataDirRequest) -> PlatformResult<ResolvedData> {
        let (launcher_home, game_home) = match request.mode {
            DataMode::Portable => {
                let exe_dir = self.exe_dir.clone().ok_or(PlatformError::NoExecutableDir)?;
                (exe_dir.join(LAUNCHER_HOME_DIR), exe_dir.join(GAME_HOME_DIR))
            }
            DataMode::Standard => {
                let launcher = paths::standard_launcher_home(self.platform, self.env).ok_or(
                    PlatformError::NoStandardDataDir {
                        platform: self.platform.as_str(),
                    },
                )?;
                let game = paths::standard_game_home(self.platform, self.env).ok_or(
                    PlatformError::NoStandardDataDir {
                        platform: self.platform.as_str(),
                    },
                )?;
                (launcher, game)
            }
            DataMode::Custom => {
                let root = request.custom_root.clone().ok_or_else(|| {
                    PlatformError::InvalidCustomRoot {
                        root: PathBuf::new(),
                    }
                })?;
                if !root.is_absolute() {
                    return Err(PlatformError::InvalidCustomRoot { root });
                }
                (root.join(LAUNCHER_HOME_DIR), root.join(GAME_HOME_DIR))
            }
        };

        Ok(ResolvedData {
            mode: request.mode,
            launcher_home: request.launcher_home.clone().unwrap_or(launcher_home),
            game_home: request.game_home.clone().unwrap_or(game_home),
            redirected_from: None,
        })
    }

    /// 解析并跟随 `<launcher_home>/redirect.json`（便携盘被搬走后旧目录不再自己长出新实例）。
    ///
    /// # Errors
    /// 解析失败、指针损坏、跳转成环/超限时返回错误。
    pub fn resolve_effective(
        &self,
        request: &DataDirRequest,
        max_hops: usize,
    ) -> PlatformResult<ResolvedData> {
        let resolved = self.resolve(request)?;
        let followed = redirect::follow(&resolved.launcher_home, max_hops)?;
        match followed {
            Some(followed) => Ok(ResolvedData {
                mode: resolved.mode,
                launcher_home: followed.pointer.launcher_home.clone(),
                game_home: followed.pointer.game_home.clone(),
                redirected_from: Some(resolved.launcher_home),
            }),
            None => Ok(resolved),
        }
    }
}

/// 默认跟随跳数上限（防成环）。
pub const DEFAULT_REDIRECT_MAX_HOPS: usize = 8;

impl<'env> Resolver<'env> {
    /// [`Self::resolve_effective`] 的默认跳数版本。
    ///
    /// # Errors
    /// 同 [`Self::resolve_effective`]。
    pub fn resolve_default(&self, request: &DataDirRequest) -> PlatformResult<ResolvedData> {
        self.resolve_effective(request, DEFAULT_REDIRECT_MAX_HOPS)
    }
}
