//! 只读扫描与逐项检查：`versions/` 下哪些版本能登记、不能登记的原因是什么。
//!
//! # 三条不许偷懒的判据
//!
//! 1. **坏东西不能当空列表**：`versions/` 读不了是错误（[`AdoptError::Game`]），一个版本的清单坏了是
//!    一条 [`AdoptProblem`]；两条都不会变成「这里什么都没有」。
//! 2. **继承链要走到头**：缺父、成环、父清单坏了、父的清单 id 与目录名不一致，逐条报清楚是哪个文件。
//! 3. **不跟随越界符号链接**：`versions/<id>` 或清单是符号链接时，解析出的真实路径必须仍在游戏根内
//!    （与导出侧「符号链接不跟随」同一政策）。
//!
//! 全程**只读**：不建目录、不写文件、不改 mtime。

use std::path::{Path, PathBuf};

use super::super::error::GameError;
use super::super::inherit;
use super::super::install::instance::read_profiles;
use super::super::layout::GameLayout;
use super::super::manifest::VersionJson;
use super::super::profiles::ProfileList;
use super::candidate::{AdoptCandidate, AdoptProblem, RegisteredInstance};
use super::error::AdoptError;
use super::paths::{ensure_inside, root_of, validate_version_id};
use crate::platform::layout::LauncherLayout;

/// 一次扫描的结果（`versions/` 里的全部条目，逐项带问题）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdoptScan {
    /// 被扫的目录（`<游戏根>/versions`）。
    pub versions_dir: PathBuf,
    /// 候选版本（按版本 id 字典序）。
    pub candidates: Vec<AdoptCandidate>,
}

/// 扫当前游戏根的 `versions/`，逐个检查。
///
/// `versions/` **真的不存在** ⇒ 空结果（新装的数据根，没有可登记的东西，不是错误）；
/// `versions/` 入口**存在但读不出来**（断链的符号链接/目录联接、目标被移走、祖先断链）⇒
/// [`AdoptError::VersionsDirUnreadable`]，绝不降级成空列表；其它读目录失败 ⇒ [`AdoptError::Game`]。
///
/// # Errors
///
/// 读目录 / 读 `profiles.json` 失败；条目元数据读不出来（权限）时也直接失败——那属于
/// 「环境坏了」而不是「这个版本不能登记」。
pub fn scan(game: &GameLayout, launcher: &LauncherLayout) -> Result<AdoptScan, AdoptError> {
    let profiles = read_profiles(launcher)?;
    let versions_dir = game.versions_dir();
    let root = root_of(game);
    let entries = match std::fs::read_dir(&versions_dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // **入口还在但读不出来**不是「新数据根」：Windows 上断链的目录联接也只会给 NotFound，
            // 必须自己用不跟随链接的元数据分辨（复审 r1 的实测：lstat 成功而命令报「空」）。
            //
            // r2 R2.3：断链可能发生在**更高的祖先**上（例如 `--data-dir` 指向迁移联接下面的 nested
            // 目录，联接目标被搬走后 `versions` 与游戏根本身都 NotFound）。所以沿祖先找**最近的
            // 已存在入口**：它 lstat 得到但 `canonicalize` 不出来 ⇒ 具名报错；一路到盘根都不存在
            // ⇒ 才是「真正的新数据根」，空结果是诚实的。
            if let Some(entry) = nearest_existing_entry(&versions_dir) {
                if entry.canonicalize().is_err() {
                    return Err(AdoptError::VersionsDirUnreadable {
                        path: entry,
                        detail: "最近的已存在入口解析失败（断链的符号链接/目录联接，或目标被移走）"
                            .to_owned(),
                    });
                }
            }

            return Ok(AdoptScan {
                versions_dir,
                candidates: Vec::new(),
            });
        }
        Err(error) => return Err(GameError::io(&versions_dir, error).into()),
    };
    let mut candidates: Vec<AdoptCandidate> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| GameError::io(&versions_dir, error))?;
        let path = entry.path();
        let Some(version_id) = path.file_name().and_then(|name| name.to_str()) else {
            // 非 UTF-8 目录名不能当版本 id（进不了 profiles.json），如实记一条问题而不是丢掉。
            candidates.push(unreadable_id_candidate(&path, &versions_dir));
            continue;
        };
        match std::fs::metadata(&path) {
            Ok(meta) if meta.is_dir() => {}
            Ok(_) => continue, // `versions/` 里的普通文件不是版本目录
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // 断链符号链接：元数据跟不过去，但链接本身在。
                if std::fs::symlink_metadata(&path).is_ok() {
                    candidates.push(broken_link_candidate(version_id, &path));
                }
                continue;
            }
            Err(error) => return Err(GameError::io(&path, error).into()),
        }
        candidates.push(check(game, &root, version_id, &profiles));
    }
    candidates.sort_by(|a, b| a.version_id.cmp(&b.version_id));
    Ok(AdoptScan {
        versions_dir,
        candidates,
    })
}

/// 检查单个版本。
///
/// 与 [`scan`] 用同一套判据，但**先**校验 id 能不能当路径用（非法 id 连存在性都不该去探，
/// 否则 `versions/../../外面的/1.20.1` 这种形状会先撞上「目录真的存在」再报出别的结论）。
/// 候选不合格一律 [`AdoptError::NotAdoptable`] 且把问题清单原样带出，只有环境级失败才 `Err`。
///
/// # Errors
///
/// - [`AdoptError::NotAdoptable`]：id 非法，或候选有问题（坏清单 / 缺父 / 环 / 越界…）；
/// - [`AdoptError::UnknownVersion`]：id 合法但 `versions/<id>/` 不存在；
/// - [`AdoptError::Game`]：读 `profiles.json` 失败。
pub fn inspect(
    game: &GameLayout,
    launcher: &LauncherLayout,
    version_id: &str,
) -> Result<AdoptCandidate, AdoptError> {
    if let Err(problem) = validate_version_id(version_id) {
        return Err(AdoptError::NotAdoptable {
            version_id: version_id.to_owned(),
            problems: vec![problem],
        });
    }
    if !game.version_dir(version_id).is_dir() {
        let version_dir = game.version_dir(version_id);
        // 条目存在但不是可读目录（断链链接/联接、或同名的是个文件）⇒ 具名报错，别混成「没装」。
        if std::fs::symlink_metadata(&version_dir).is_ok() {
            return Err(AdoptError::NotAdoptable {
                version_id: version_id.to_owned(),
                problems: vec![AdoptProblem::DirectoryUnreadable {
                    path: version_dir,
                    message: "条目存在但不是可读目录（断链 / 联接目标被移走 / 同名的是文件）"
                        .to_owned(),
                }],
            });
        }
        return Err(AdoptError::UnknownVersion {
            version_id: version_id.to_owned(),
            version_dir,
        });
    }
    let profiles = read_profiles(launcher)?;
    let candidate = check(game, &root_of(game), version_id, &profiles);
    if candidate.adoptable() {
        Ok(candidate)
    } else {
        Err(AdoptError::NotAdoptable {
            version_id: candidate.version_id.clone(),
            problems: candidate.problems.clone(),
        })
    }
}

/// 逐项检查一个版本目录。
fn check(
    game: &GameLayout,
    root: &Path,
    version_id: &str,
    profiles: &ProfileList,
) -> AdoptCandidate {
    let version_dir = game.version_dir(version_id);
    let manifest_path = game.version_json(version_id);
    let registered = registered_for(profiles, version_id);
    let mut candidate = AdoptCandidate {
        version_id: version_id.to_owned(),
        version_dir: version_dir.clone(),
        manifest_path: manifest_path.clone(),
        jar_present: false,
        manifest_id: None,
        inherits: Vec::new(),
        registered,
        problems: Vec::new(),
    };

    // ① id 能不能当路径用：不能的话后面的 join 全不可信，直接结束。
    if let Err(problem) = validate_version_id(version_id) {
        candidate.problems.push(problem);
        return candidate;
    }
    // ② 版本目录解析到根外（符号链接 / 目录联接 / 祖先链接）⇒ 立刻停手：
    //    **一个字节都不去读**那个目录（读它就等于读别人的启动器数据）。
    if let Some(problem) = ensure_inside(&version_dir, root) {
        candidate.problems.push(problem);
        return candidate;
    }
    // ③ 清单必须在（`installed_versions` 的判据同源）。
    if !manifest_path.is_file() {
        candidate.problems.push(AdoptProblem::ManifestMissing {
            version_id: version_id.to_owned(),
            path: manifest_path,
        });
        return candidate;
    }
    if let Some(problem) = ensure_inside(&manifest_path, root) {
        candidate.problems.push(problem);
        return candidate;
    }
    candidate.jar_present = game.version_jar(version_id).is_file();

    // ④ 清单读得出来、id 对得上。
    let manifest = match read_manifest(&manifest_path) {
        Ok(manifest) => manifest,
        Err(problem) => {
            candidate.problems.push(problem);
            return candidate;
        }
    };
    let manifest_id = manifest.id.clone();
    candidate.manifest_id = manifest_id.clone();
    match manifest_id.as_deref() {
        None => {
            candidate.problems.push(AdoptProblem::ManifestMissingId {
                path: manifest_path.clone(),
            });
            return candidate;
        }
        Some(id) if id != version_id => {
            candidate.problems.push(AdoptProblem::ManifestIdMismatch {
                path: manifest_path.clone(),
                manifest_id: id.to_owned(),
                directory_id: version_id.to_owned(),
            });
        }
        Some(_) => {}
    }

    // ⑤ 继承链走到底（缺父 / 环 / 父清单坏 / 父 id 不匹配都在这条路上报）。
    match walk_chain(game, root, &manifest_path, manifest) {
        Ok(chain) => {
            candidate.inherits = chain;
            // 走查通过后再让 `resolve` 做一次权威判定：两处判据不许漂。
            if let Some(problem) = resolve_verdict(game, &candidate.manifest_path) {
                candidate.problems.push(problem);
            }
        }
        Err(problem) => candidate.problems.push(problem),
    }
    candidate
}

/// 走 `inheritsFrom` 链，返回 [自身, 父, 祖父, …]；任一层不合格就返回那条问题。
fn walk_chain(
    game: &GameLayout,
    root: &Path,
    manifest_path: &Path,
    manifest: VersionJson,
) -> Result<Vec<String>, AdoptProblem> {
    let Some(self_id) = manifest.id.clone() else {
        return Err(AdoptProblem::ManifestMissingId {
            path: manifest_path.to_path_buf(),
        });
    };
    let mut chain = vec![self_id];
    let mut current = manifest;
    while let Some(parent) = current.inherits_from.clone() {
        if chain.iter().any(|id| id == &parent) {
            let mut loop_chain = chain.clone();
            loop_chain.push(parent);
            return Err(AdoptProblem::CircularInheritance { chain: loop_chain });
        }
        validate_version_id(&parent)?;
        let required_by = chain.last().cloned().unwrap_or_default();
        let parent_dir = game.version_dir(&parent);
        let parent_path = game.version_json(&parent);
        // 父版本同样先过边界（越界就不读），再判存在。
        if let Some(problem) = ensure_inside(&parent_dir, root) {
            return Err(problem);
        }
        if !parent_path.is_file() {
            return Err(AdoptProblem::MissingParent {
                parent,
                required_by,
            });
        }
        if let Some(problem) = ensure_inside(&parent_path, root) {
            return Err(problem);
        }
        let parent_manifest = read_manifest(&parent_path)?;
        match parent_manifest.id.as_deref() {
            None => return Err(AdoptProblem::ManifestMissingId { path: parent_path }),
            Some(id) if id != parent => {
                return Err(AdoptProblem::ManifestIdMismatch {
                    path: parent_path,
                    manifest_id: id.to_owned(),
                    directory_id: parent,
                })
            }
            Some(_) => {}
        }
        chain.push(parent);
        current = parent_manifest;
    }
    Ok(chain)
}

/// 用 `core::game::resolve` 判一次继承链（权威结论；失败就如实报，不猜）。
fn resolve_verdict(game: &GameLayout, manifest_path: &Path) -> Option<AdoptProblem> {
    let manifest = read_manifest(manifest_path).ok()?;
    let source = |id: &str| -> Option<VersionJson> {
        let bytes = std::fs::read(game.version_json(id)).ok()?;
        VersionJson::from_json_slice(&bytes).ok()
    };
    match inherit::resolve(&source, &manifest) {
        Ok(_) => None,
        Err(error) => Some(AdoptProblem::InheritanceInvalid {
            detail: error.to_string(),
        }),
    }
}

/// 读一份清单（错误带路径与 serde 的行列号）。
fn read_manifest(path: &Path) -> Result<VersionJson, AdoptProblem> {
    let bytes = std::fs::read(path).map_err(|error| AdoptProblem::ManifestUnreadable {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    VersionJson::from_json_slice(&bytes).map_err(|error| match error {
        GameError::Json { source, .. } => AdoptProblem::ManifestInvalidJson {
            path: path.to_path_buf(),
            message: source.to_string(),
        },
        other => AdoptProblem::ManifestInvalidJson {
            path: path.to_path_buf(),
            message: other.to_string(),
        },
    })
}

/// 绑定这个版本 id 的全部实例档案。
fn registered_for(profiles: &ProfileList, version_id: &str) -> Vec<RegisteredInstance> {
    profiles
        .profiles
        .iter()
        .filter(|profile| profile.version_id == version_id)
        .map(|profile| RegisteredInstance {
            id: profile.id.as_str().to_owned(),
            name: profile.name.clone(),
        })
        .collect()
}

/// 非 UTF-8 目录名：当成一条不可登记的候选（丢掉它就等于「静默少一行」）。
fn unreadable_id_candidate(path: &Path, versions_dir: &Path) -> AdoptCandidate {
    let shown = path
        .strip_prefix(versions_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();
    AdoptCandidate {
        version_id: shown,
        version_dir: path.to_path_buf(),
        manifest_path: path.to_path_buf(),
        jar_present: false,
        manifest_id: None,
        inherits: Vec::new(),
        registered: Vec::new(),
        problems: vec![AdoptProblem::DirectoryUnreadable {
            path: path.to_path_buf(),
            message: "目录名不是合法 UTF-8，不能作为版本 id".to_owned(),
        }],
    }
}

/// 断链符号链接：`versions/` 里有这个条目，但指不到任何东西。
fn broken_link_candidate(version_id: &str, path: &Path) -> AdoptCandidate {
    AdoptCandidate {
        version_id: version_id.to_owned(),
        version_dir: path.to_path_buf(),
        manifest_path: path.to_path_buf(),
        jar_present: false,
        manifest_id: None,
        inherits: Vec::new(),
        registered: Vec::new(),
        problems: vec![AdoptProblem::SymlinkUnresolvable {
            path: path.to_path_buf(),
            message: "符号链接的目标不存在（断链）".to_owned(),
        }],
    }
}

/// 沿祖先找**最近的已存在入口**（自身优先；一路到盘根都没有 ⇒ `None`）。
///
/// 用途：`read_dir` 的 `NotFound` 不代表「这个数据根是空的」——断链可能发生在更高的祖先上
/// （迁移联接的目标被搬走时，`versions` 和游戏根都 NotFound，但入口还在）。判据是**存在性**
/// （`symlink_metadata`，不跟随），能不能解析由调用方另行判定。
fn nearest_existing_entry(path: &Path) -> Option<PathBuf> {
    let mut probe = path.to_path_buf();
    loop {
        if std::fs::symlink_metadata(&probe).is_ok() {
            return Some(probe);
        }
        if !probe.pop() {
            return None;
        }
    }
}
