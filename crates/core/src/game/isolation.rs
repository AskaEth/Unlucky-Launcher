//! 实例隔离档位的**派生**与**切换检测**（D20 定案）。
//!
//! 依据：`DevDocs/PlanDoc/research/04-launcher-configs.md` §5.1-§5.2（键草案 + 切换检测）、
//! `DevDocs/analyse/07-settings-packaging-update.md` §2.4（HMCL 用 `runningDirectory` 覆盖表达隔离）、
//! `DevDocs/PlanDoc/04-data-and-config.md` §4（实例模型）。
//!
//! # D20 的语义（用户定案，不可改）
//!
//! - **默认隔离**：全局 `instances.mode` 默认 `isolated`，用户可手动切 `shared`；
//! - **恒共享层不可配**：`versions/` `assets/` `libraries/` 永远在游戏根，隔离只改**运行目录**
//!   （= `--gameDir` 的取值 + 进程 CWD），见 [`SHARED_LAYERS`]；
//! - **切换不迁移 + 双侧内容提示**：切档只改路径语义，不动任何数据；两侧都有内容时提示
//!   （[`ModeChange::take_warning`]），避免「存档消失」错觉。
//!
//! # 覆盖链（三层，左优先）
//!
//! ```text
//! 临时启动参数（--game-dir 之类）  >  实例 isolation  >  全局 instances.mode
//! ```
//!
//! `isolation` 的 `inherit` 不是第四个档位，而是「键不存在」→ 落到全局（[`IsolationMode`]）；
//! 这样改全局对存量实例立即生效，不需要迁移（research/04 §5.3 的 PCL2 反面教材）。
//!
//! 本模块只做**纯路径计算 + 只读探测**：不建目录、不搬文件、不写配置。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::GameError;
use super::install::instance::{InstanceConfig, Isolation};
use super::layout::GameLayout;

/// 恒共享层（任何隔离档位都不按实例复制；natives 例外，按实例解包）。
pub const SHARED_LAYERS: &[&str] = &["versions", "assets", "libraries"];

/// 切换检测探测的三类内容目录（两家启动器一致：只关心玩家数据）。
pub const PROBED_CONTENT_DIRS: &[&str] = &["mods", "saves", "config"];

/// 全局默认隔离模式（`launcher.json` 的 `instances.mode`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IsolationMode {
    /// 每实例运行目录 = `versions/<versionId>/`（D20 默认）。
    #[default]
    Isolated,
    /// 所有实例共用 `.minecraft` 根（HMCL 传统单目录形态）。
    Shared,
}

impl IsolationMode {
    /// 稳定字符串 id（与 `setting::keys::INSTANCE_MODES` 同字面量）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Shared => "shared",
        }
    }

    /// 从字符串解析（未知值返回 `None`，由调用方决定报错还是取默认）。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "isolated" => Some(Self::Isolated),
            "shared" => Some(Self::Shared),
            _ => None,
        }
    }

    /// 折算成实例档位（全局模式只有两档，`inherit`/`custom` 不存在）。
    #[must_use]
    pub const fn as_isolation(self) -> Isolation {
        match self {
            Self::Isolated => Isolation::Isolated,
            Self::Shared => Isolation::Shared,
        }
    }
}

/// 派生结论的来源（诊断/日志用，`mode` 变了要能解释为什么）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsolationSource {
    /// 临时启动参数覆盖（最高优先）。
    Temporary,
    /// 实例文件里的 `isolation`。
    Instance,
    /// 全局 `instances.mode`（实例 `inherit` 或没有实例文件）。
    Global,
}

impl IsolationSource {
    /// 稳定名。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Temporary => "temporary",
            Self::Instance => "instance",
            Self::Global => "global",
        }
    }
}

/// 一次派生（覆盖链求值）的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Derivation {
    /// 生效档位（`inherit` 已被解析掉）。
    pub isolation: Isolation,
    /// 生效运行目录（喂 `--gameDir` 与进程 CWD）。
    pub game_dir: PathBuf,
    /// 档位来自哪一层。
    pub source: IsolationSource,
}

/// 按覆盖链派生运行目录（纯计算，不碰盘）。
///
/// - `temporary`：临时启动参数给的目录（空路径视为未给）；
/// - `instance`：实例设置；`isolation` 为 `None`（= `inherit`）或没有实例时落到 `global_mode`；
/// - `custom` 却没有 `gameDir` → [`GameError::CustomIsolationWithoutDir`]（不猜默认目录）。
///
/// # Errors
///
/// `custom` 档位缺 `gameDir` 时返回 [`GameError::CustomIsolationWithoutDir`]。
pub fn derive_game_dir(
    layout: &GameLayout,
    version_id: &str,
    global_mode: IsolationMode,
    instance: Option<&InstanceConfig>,
    temporary: Option<&Path>,
) -> Result<Derivation, GameError> {
    let (isolation, source) = match instance.and_then(|config| config.isolation) {
        Some(isolation) => (isolation, IsolationSource::Instance),
        None => (global_mode.as_isolation(), IsolationSource::Global),
    };
    if let Some(path) = temporary.filter(|path| !path.as_os_str().is_empty()) {
        return Ok(Derivation {
            isolation,
            game_dir: path.to_path_buf(),
            source: IsolationSource::Temporary,
        });
    }
    let game_dir = match isolation {
        Isolation::Isolated => layout.version_dir(version_id),
        Isolation::Shared => layout.root().to_path_buf(),
        Isolation::Custom => match instance.and_then(|config| config.game_dir.clone()) {
            Some(path) if !path.as_os_str().is_empty() => path,
            _ => {
                return Err(GameError::CustomIsolationWithoutDir {
                    instance: instance.map_or_else(String::new, |config| config.id.clone()),
                })
            }
        },
    };
    Ok(Derivation {
        isolation,
        game_dir,
        source,
    })
}

/// 一侧目录的内容探测结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideContent {
    /// 探测的目录（隔离侧 = `versions/<versionId>/`，共享侧 = `.minecraft/`）。
    pub path: PathBuf,
    /// `mods` `saves` `config` 三类内容的条目数合计。
    pub entries: u32,
}

impl SideContent {
    /// 是否有玩家数据（条目数 > 0）。
    #[must_use]
    pub const fn occupied(&self) -> bool {
        self.entries > 0
    }
}

/// 切换检测的双侧快照（落进 `lastModeChange.probe`）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeChangeProbe {
    /// 隔离侧条目数。
    pub isolated_side: u32,
    /// 共享侧条目数。
    pub shared_side: u32,
}

impl ModeChangeProbe {
    /// 两侧都有内容（切换后玩家会以为「存档消失」的条件）。
    #[must_use]
    pub const fn both_occupied(&self) -> bool {
        self.isolated_side > 0 && self.shared_side > 0
    }
}

/// 一次隔离档位切换的记录（实例文件的 `lastModeChange`，启动器写入）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModeChange {
    /// 切换时间（RFC-3339，由调用方给——core::game 不碰时钟格式）。
    pub at: String,
    /// 切换前生效档位。
    pub from: Isolation,
    /// 切换后生效档位。
    pub to: Isolation,
    /// 切换当时的双侧内容探测（只探测，不迁移）。
    #[serde(default)]
    pub probe: ModeChangeProbe,
    /// 提示是否已展示过（保证「切换后首次启动提示一次」而不是每次启动都刷）。
    #[serde(default)]
    pub warned: bool,
}

impl ModeChange {
    /// 是否是一次**真的**切换（`from != to`；首启建基线时两者相同）。
    #[must_use]
    pub fn is_real_change(&self) -> bool {
        self.from != self.to
    }

    /// 取走待展示的提示：真切换 + 两侧都有内容 + 还没提示过。
    ///
    /// 取走后把 `warned` 置真（调用方负责落盘），因此同一次切换只提示一次。
    pub fn take_warning(&mut self) -> Option<ModeChangeWarning> {
        if self.warned || !self.is_real_change() || !self.probe.both_occupied() {
            return None;
        }
        self.warned = true;
        Some(ModeChangeWarning {
            from: self.from,
            to: self.to,
            probe: self.probe,
        })
    }
}

/// 「切换后两侧都有内容」的提示（D20：**只提示不迁移**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeChangeWarning {
    /// 切换前档位。
    pub from: Isolation,
    /// 切换后档位。
    pub to: Isolation,
    /// 双侧探测结果。
    pub probe: ModeChangeProbe,
}

impl ModeChangeWarning {
    /// 一句给人看的文案（含两侧条目数与「不自动搬迁」的说明）。
    #[must_use]
    pub fn message(&self) -> String {
        format!(
            "隔离档位已从 {} 切到 {}：隔离侧 {} 条、共享侧 {} 条，两侧都有内容。\
             ULL 不会自动搬数据——旧内容仍在原来那一侧，需要的话请手动把 mods/saves/config 挪到新目录。",
            self.from.as_str(),
            self.to.as_str(),
            self.probe.isolated_side,
            self.probe.shared_side,
        )
    }
}

/// 探测一侧目录的内容条目数（`mods` `saves` `config` 的直接子项合计）。
///
/// 目录不存在按 0 计（新实例、纯新的隔离目录都是常态，不是错误）。
///
/// # Errors
///
/// 目录存在但**读不了**（权限/被占用）时返回 [`GameError::Io`]——这种情况不能当成「空」，
/// 否则切换检测会给出错误的「那侧没内容」结论。
pub fn probe_side(path: &Path) -> Result<SideContent, GameError> {
    let mut entries: u32 = 0;
    for name in PROBED_CONTENT_DIRS {
        let dir = path.join(name);
        let reader = match std::fs::read_dir(&dir) {
            Ok(reader) => reader,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(GameError::io(&dir, error)),
        };
        for entry in reader {
            entry.map_err(|error| GameError::io(&dir, error))?;
            entries = entries.saturating_add(1);
        }
    }
    Ok(SideContent {
        path: path.to_path_buf(),
        entries,
    })
}

/// 同时探测隔离侧（`versions/<versionId>/`）与共享侧（游戏根）。
///
/// # Errors
///
/// 任一侧读不了时返回 [`GameError::Io`]。
pub fn probe_both_sides(
    layout: &GameLayout,
    version_id: &str,
) -> Result<(SideContent, SideContent), GameError> {
    Ok((
        probe_side(&layout.version_dir(version_id))?,
        probe_side(layout.root())?,
    ))
}

/// 启动时刷新切换记录（幂等）：生效档位与记录里的 `to` 不同才记一次。
///
/// 返回 `true` 表示 `config` 被改动（调用方负责落盘——本模块不写文件）。
///
/// 首次观察（没有记录）会建一条基线：`from == to`，`warned = true`——这样老实例
/// 升级到 D20 后不会平白弹一次「两侧都有内容」。
///
/// # Errors
///
/// 探测读盘失败时返回 [`GameError::Io`]。
pub fn observe_mode(
    config: &mut InstanceConfig,
    layout: &GameLayout,
    at: &str,
    effective: Isolation,
) -> Result<bool, GameError> {
    if config
        .last_mode_change
        .as_ref()
        .is_some_and(|change| change.to == effective)
    {
        return Ok(false);
    }
    let from = config
        .last_mode_change
        .as_ref()
        .map_or(effective, |change| change.to);
    let (isolated_side, shared_side) = probe_both_sides(layout, &config.version_id)?;
    config.last_mode_change = Some(ModeChange {
        at: at.to_string(),
        from,
        to: effective,
        probe: ModeChangeProbe {
            isolated_side: isolated_side.entries,
            shared_side: shared_side.entries,
        },
        warned: from == effective,
    });
    Ok(true)
}
