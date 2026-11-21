//! 路径语义与**写盘面**判据：真实化、Win32 别名归一、配置写面、与受保护游戏树的重叠。
//!
//! # 为什么单独一层
//!
//! 「这个路径到底是哪个物理位置」在 Windows 上不是字符串比较：普通路径的**尾点/尾空格会被 Win32
//! 折叠**（`Foo.` 与 `Foo ` 都落到 `Foo`），而 verbatim 路径（`\\?\…`）**不**折叠。`canonicalize`
//! 只能回答「已存在的东西在哪」，尚不存在的尾段（`custom` 档认领的目录、派生出来还没建的缺省目录）
//! 必须自己按平台语义归一，否则「两个实例共用同一个物理目录」会从这个缝里漏过去（复审 r2 的 R2.1）。
//!
//! 另一件事是**写盘面**：登记承诺「只写 ULL 自己的配置、外部游戏树零写入」。判据分两条：
//! 1. 配置落点必须在 `<.ULL>` 内（[`ensure_config_write_surface`]）；
//! 2. 全部默认写盘面（`.ULL` 自身、`config/`、`config/instances/`、`profiles.json`、`data/`
//!    以及缺省运行目录）**不得与受保护游戏树重叠**（[`ensure_surfaces_clear_of_game_tree`]）。
//!    用户把整个 `.ULL`/`data` 迁到游戏树外是合法迁移，照样通过；重叠则具名拒绝——
//!    不能写完之后再补一句 warning（复审 r2 的 R2.2）。
//!
//! # 已知边界
//!
//! 判据都是「检查那一刻」的快照（TOCTOU 看不到）；Win32 的 8.3 短名别名不在归一范围内
//! （未就绪的短名无法可靠推导），非 junction 的重解析点也未逐个识别。

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::super::layout::GameLayout;
use super::error::AdoptError;
use crate::platform::layout::LauncherLayout;

/// **最近**的已存在祖先/自身解析出的真实路径 + 尚不存在的尾段（`None` = 整条链都不存在）。
///
/// 用于**位置判据**（越界比较、写面重叠）：这里不做别名折叠——问的是「物理上在哪」。
pub(super) fn existing_real(path: &Path) -> Option<PathBuf> {
    resolve(path, false)
}

/// **比较键**用的路径：已存在部分真实化，尚不存在的尾段按平台语义归一（Windows 折叠尾点/尾空格，
/// verbatim 路径除外）。两个不同写法指向同一个物理目录时，键必须相等。
pub(super) fn comparison_real(path: &Path) -> Option<PathBuf> {
    resolve(path, true)
}

fn resolve(path: &Path, fold_tail: bool) -> Option<PathBuf> {
    let verbatim = is_verbatim(path);
    let mut probe = path.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if std::fs::symlink_metadata(&probe).is_ok() {
            let real = probe.canonicalize().ok()?;
            let mut full = real;
            for part in tail.iter().rev() {
                if fold_tail {
                    full.push(platform_component(part, verbatim));
                } else {
                    full.push(part);
                }
            }
            return Some(full);
        }
        let name = probe.file_name().map(OsString::from)?;
        tail.push(name);
        if !probe.pop() {
            return None;
        }
    }
}

/// 整条路径是不是 verbatim 形式（`\\?\…` / `\\.\…`）：那种写法**不**折叠尾点/尾空格。
fn is_verbatim(path: &Path) -> bool {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        text.starts_with(r"\\?\") || text.starts_with(r"\\.\")
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// 尚不存在的一段名字按**平台语义**归一。
///
/// Windows（非 verbatim）：去掉尾部的 `.` 与空格——Win32 建目录/解析路径时就这么做，所以
/// `Foo.`、`Foo `、`Foo` 是同一个物理目录。全是点/空格的名字保持原样（那种名字本身建不出来，
/// 折叠成空串只会制造新的歧义）。其它平台原样返回（`Foo.` 与 `Foo` 是不同目录）。
fn platform_component(name: &std::ffi::OsStr, verbatim: bool) -> OsString {
    #[cfg(windows)]
    {
        if !verbatim {
            let text = name.to_string_lossy();
            let trimmed = text.trim_end_matches(['.', ' ']);
            if !trimmed.is_empty() && trimmed.len() != text.len() {
                return OsString::from(trimmed.to_string());
            }
        }
    }
    let _ = verbatim;
    name.to_os_string()
}

/// 路径 → 比较键（`/` 分隔 + 大小写归一；大小写只在大小写不敏感的平台折叠）。
pub(super) fn normalize_key(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text
    }
}

/// 配置写面（`config/`、`config/instances/`、`profiles.json`）必须在 `<.ULL>` 之内。
///
/// 只管「在不在自己家里」；「有没有落进受保护游戏树」由
/// [`ensure_surfaces_clear_of_game_tree`] 统一负责（两条判据各有单一职责，不再互相豁免）。
///
/// # Errors
///
/// 落点跑出 `<.ULL>`：`ConfigSurfaceUnsafe`。
pub(super) fn ensure_config_write_surface(launcher: &LauncherLayout) -> Result<(), AdoptError> {
    let Some(home_real) = existing_real(launcher.home()) else {
        // `<.ULL>` 都还不存在 ⇒ 还不可能有链接可言。
        return Ok(());
    };
    let targets = [
        launcher.config(),
        launcher.config().join("instances"),
        launcher.profiles_config(),
    ];
    for target in targets {
        let Some(real) = existing_real(&target) else {
            continue;
        };
        if !real.starts_with(&home_real) {
            return Err(AdoptError::ConfigSurfaceUnsafe {
                path: target,
                detail: format!(
                    "解析到 {}，落在启动器目录 {} 之外（有指向外部的符号链接/目录联接）",
                    real.display(),
                    home_real.display()
                ),
            });
        }
    }
    Ok(())
}

/// 默认写盘面**不得**与受保护游戏树重叠（`.ULL`/`data`/配置/缺省运行目录解析后都不许落进游戏根）。
///
/// 为什么要它：用户可能预先在数据根里放过目录联接（不是 adopt 建的）。这时「默认独立目录」写着写着
/// 就写进了别人的 `.minecraft`，输出却还说「与别人的 .minecraft 隔离」——那是**假承诺**。
/// 判据在**写入之前**给出具名拒绝，并让用户把数据目录挪到游戏树之外；整体迁移到游戏树**之外**照旧允许。
///
/// `run_dir` 只在缺省档（`InstanceDir`）传：`shared` / `version_dir` / `custom` 是用户显式选择的
/// 运行目录，允许与游戏树重叠（那是选择的语义，不是偷偷写的）。
///
/// # Errors
///
/// 任一面解析后落进游戏根：`WriteSurfaceInGameTree`。
pub(super) fn ensure_surfaces_clear_of_game_tree(
    game: &GameLayout,
    launcher: &LauncherLayout,
    run_dir: Option<&Path>,
) -> Result<(), AdoptError> {
    let Some(game_real) = existing_real(game.root()) else {
        // 游戏根还不存在 ⇒ 没有「受保护游戏树」可重叠。
        return Ok(());
    };
    let mut surfaces = vec![
        launcher.home().to_path_buf(),
        launcher.config(),
        launcher.config().join("instances"),
        launcher.profiles_config(),
        launcher.data(),
    ];
    if let Some(run_dir) = run_dir {
        surfaces.push(run_dir.to_path_buf());
    }
    for surface in surfaces {
        let Some(real) = existing_real(&surface) else {
            continue;
        };
        if real.starts_with(&game_real) {
            return Err(AdoptError::WriteSurfaceInGameTree {
                path: surface,
                detail: format!(
                    "解析到 {}，落在受保护游戏树 {} 之内：默认登记不会把实例数据写进别人的 .minecraft；\
                     请把 .ULL（或 --data-dir）指到游戏树之外的目录再登记（整体迁移本身是合法的）",
                    real.display(),
                    game_real.display()
                ),
            });
        }
    }
    Ok(())
}
