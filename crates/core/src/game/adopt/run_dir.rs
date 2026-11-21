//! 运行目录与**写盘边界**：缺省目录派生（含碰撞消歧）+ 数据区/配置写面的真实路径判据。
//!
//! # 两条铁律（都来自复审 r1 的实测缺陷）
//!
//! 1. **两个实例不许悄悄共用同一个运行目录**（R1）：展示名不重复不代表派生目录不重复
//!    （`Better MC` 与 `Better-MC` 经 slug 同名；Windows 上 `Foo` 与 `foo` 是同一个目录）。
//!    所以「目录是空的」**不足以**判可复用——还要看**已有实例已经占着哪个真实目录**；
//!    撞上了就顺延到 `<base>-2`、`<base>-3`…（原因写进 notes），而不是让两个实例共用一个。
//! 2. **只写 ULL 自己的配置**：配置写面与「默认写盘面不得与受保护游戏树重叠」两条判据在
//!    [`super::fspaths`]（`ensure_config_write_surface` / `ensure_surfaces_clear_of_game_tree`），
//!    本模块只负责**运行目录派生**与「缺省目录必须落在数据区」这一条。
//!
//! # 判据的基准（为什么不拿 `<data>/instances` 或 `<.ULL>/config` 当基准）
//!
//! 被保护的**边界自己是链接**时，拿它当基准等于把越界当正常。所以基准一律是**解析后的父级**：
//! 数据写面看 `<data>`（本模块只负责这一条）；配置写面（`<.ULL>` 内、并额外排除游戏根）已移交
//! [`super::fspaths`] 的 `ensure_config_write_surface` / `ensure_surfaces_clear_of_game_tree`，
//! 不在本模块的判据里。用户把整棵树迁到别的盘（`<.ULL>` 或 `<data>` 本身是联接/符号链接）
//! 仍然允许——判据是落点，不是路径长得像不像。
//!
//! # 已知边界
//!
//! 判据是「检查那一刻」的快照：检查与 `create_dir_all`/写盘之间被人换掉组件（TOCTOU）看不到。
//! 本命令不是并发/对抗入口，不做跨进程锁。

use std::path::{Path, PathBuf};

use super::super::install::instance::read_instance_config;
use super::super::install::pack_dir;
use super::super::profiles::ProfileList;
use super::error::AdoptError;
use super::fspaths;
use crate::platform::layout::LauncherLayout;

/// 派生目录撞车时最多顺延多少个后缀（`-2` … `-33`）。撞满即报错，不无限找。
const MAX_SUFFIX: u32 = 32;

/// 一个「已有实例已经占着的运行目录」。
///
/// 只收 `gameDir`（现役设置文件里的，或 M27 前档案的遗留 `game_dir`）：`shared` / `isolated`
/// 档的运行目录是**派生**的（游戏根 / `versions/<id>`），与缺省落点 `<data>/instances/<名>`
/// 不可能相等，收进来只会制造噪音。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunDirClaim {
    /// 占着这个目录的实例 id。
    pub(super) instance_id: String,
    /// 实例展示名（写进提示给人看）。
    pub(super) name: String,
    /// 声明/绑定的运行目录（原样，未真实化）。
    pub(super) dir: PathBuf,
    /// 比较用的键（真实化路径 + 大小写归一，见 [`dir_key`]）。
    pub(super) key: String,
}

/// 收集现有实例占着的运行目录（读 `config/instances/<id>.json`；读不了就是错误，不猜）。
///
/// # Errors
///
/// 实例设置文件读不出来 / JSON 坏：`AdoptError::Game`。
pub(super) fn claims(
    launcher: &LauncherLayout,
    profiles: &ProfileList,
) -> Result<Vec<RunDirClaim>, AdoptError> {
    let mut out = Vec::new();
    for profile in &profiles.profiles {
        let config = read_instance_config(launcher, profile.id.as_str())?;
        let dir = config
            .and_then(|config| config.game_dir)
            .or_else(|| profile.game_dir.clone());
        let Some(dir) = dir.filter(|dir| !dir.as_os_str().is_empty()) else {
            continue;
        };
        out.push(RunDirClaim {
            instance_id: profile.id.as_str().to_owned(),
            name: profile.name.clone(),
            key: dir_key(&dir),
            dir,
        });
    }
    Ok(out)
}

/// 缺省运行目录：`<data>/instances/<实例目录名>`，撞车/占用时顺延后缀。
///
/// 判据顺序（三条都要过）：
/// 1. 路径上的已有祖先解析后必须仍在数据根内（[`ensure_inside_data`]）；
/// 2. **不能**是已有实例占着的目录（按真实路径比较，[`dir_key`]）——撞上就顺延；
/// 3. 目录必须不存在或为空（非空且无人认领 = 用户数据，拒绝而不是换目录）。
///
/// 返回派生目录 + 人读提示（顺延过就有一条，没顺延就是空）。
///
/// # Errors
///
/// 数据区越界（`TargetInvalid`）、非空目录（`TargetNotEmpty`）、后缀撞满（`NoFreeRunDir`）、
/// 读实例设置失败（`Game`）。
pub(super) fn derive_instance_dir(
    launcher: &LauncherLayout,
    name: &str,
    claims: &[RunDirClaim],
) -> Result<(PathBuf, Vec<String>), AdoptError> {
    let base = pack_dir::instance_dir(launcher, name);
    let mut bumped: Vec<RunDirClaim> = Vec::new();
    let mut attempt = base.clone();
    let mut suffix = 1;
    loop {
        ensure_inside_data(launcher, &attempt)?;
        let key = dir_key(&attempt);
        match claims.iter().find(|claim| claim.key == key) {
            Some(claim) => {
                bumped.push(claim.clone());
                suffix += 1;
                if suffix > MAX_SUFFIX {
                    return Err(AdoptError::NoFreeRunDir { base });
                }
                attempt = with_suffix(&base, suffix);
            }
            None if directory_occupied(&attempt) => {
                // 非空且没有实例认领 ⇒ 用户自己的数据：不覆盖，也不偷偷换个目录把它晾在一边。
                return Err(AdoptError::TargetNotEmpty { path: attempt });
            }
            None => {
                let mut notes = Vec::new();
                if let Some(claim) = bumped.first() {
                    notes.push(format!(
                        "派生目录 {} 与已有实例 `{}`（id {}）的运行目录 {} 是同一个物理目录，\
                         已改用 {} —— 两个实例不会共用同一个运行目录",
                        base.display(),
                        claim.name,
                        claim.instance_id,
                        claim.dir.display(),
                        attempt.display()
                    ));
                }
                return Ok((attempt, notes));
            }
        }
    }
}

/// `<base>-2` / `<base>-3` …（只动最后一段）。
fn with_suffix(base: &Path, suffix: u32) -> PathBuf {
    let name = base
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    base.with_file_name(format!("{name}-{suffix}"))
}

/// 缺省运行目录必须**真的**落在启动器数据根之下。
///
/// 边界是 `<data>`（`<root>/.ULL/data`）而不是 `<data>/instances`：后者本身可能就是链接，
/// 拿它当基准等于把「越界」当成「正常」。
///
/// # Errors
///
/// 解析后跑出数据根（`TargetInvalid`）。
pub(super) fn ensure_inside_data(launcher: &LauncherLayout, dir: &Path) -> Result<(), AdoptError> {
    let Some(data_real) = fspaths::existing_real(&launcher.data()) else {
        // 连数据根都还不存在 ⇒ 不可能已经有指向外面的链接。
        return Ok(());
    };
    if let Some(dir_real) = fspaths::existing_real(dir) {
        if !dir_real.starts_with(&data_real) {
            return Err(AdoptError::TargetInvalid {
                detail: format!(
                    "运行目录 {} 解析到 {}，落在数据根 {} 之外：数据区里有指向外部的符号链接/目录联接，拒绝沿它写盘",
                    dir.display(),
                    dir_real.display(),
                    data_real.display()
                ),
            });
        }
    }
    Ok(())
}

/// `custom` 运行目录的用法判据（绝对路径 / 非空）。
///
/// # Errors
///
/// 相对路径或空路径：`TargetInvalid`。
pub(super) fn resolve_custom_dir(dir: &Path) -> Result<PathBuf, AdoptError> {
    if dir.as_os_str().is_empty() {
        return Err(AdoptError::TargetInvalid {
            detail: "运行目录为空".to_owned(),
        });
    }
    if !dir.is_absolute() {
        return Err(AdoptError::TargetInvalid {
            detail: format!("--game-dir 需要绝对路径，收到 `{}`", dir.display()),
        });
    }
    Ok(dir.to_path_buf())
}

/// 目录是否「有人在用」（存在且非空）。空目录是上次失败的残留，可复用。
pub(super) fn directory_occupied(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some())
}

/// **比较用**的真实路径键：真实化 + 平台别名归一（Win32 尾点/尾空格，见 [`fspaths`]）+ 大小写归一。
///
/// 大小写归一只在大小写不敏感的平台做（Windows / 默认 macOS）：`Foo` 与 `foo` 是同一个目录，
/// 不归一会让「两个实例共用目录」这一条在 Windows 上漏掉（复审 r1 的实测之一）。
/// **Windows 上还要折叠尾点/尾空格**（`Foo.` / `Foo ` 都是 `Foo`）——否则「尚不存在的认领目录」
/// 会从这条缝里绕过碰撞判定（复审 r2 的 R2.1）。verbatim 路径（`\?\…`）不折叠，交给平台语义。
///
/// **策略是保守的**：macOS 也可能是大小写敏感的 APFS 卷，那时 `Foo` 与 `foo` 本来是**不同**目录，
/// 归一后会判成碰撞 ⇒ 顺延 `-2`。代价是目录名可能多一个后缀，换来的是「绝不共用」这条不变量
/// 在任何卷上都成立（判错的方向只能是「多一次顺延」，不能是「共用一个目录」）。
///
/// **目标可能还不存在**：这里不要求路径存在（[`fspaths::comparison_real`] 会拼回尚不存在的尾段），
/// 因为「实例已经认领了、但目录还没被创建」（`custom` 档就是这种）同样必须参与比较。
fn dir_key(path: &Path) -> String {
    match fspaths::comparison_real(path) {
        Some(real) => fspaths::normalize_key(&real),
        None => fspaths::normalize_key(path),
    }
}
