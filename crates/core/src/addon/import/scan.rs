//! 导入**预检**（M160）：只读地算出「冲突清单」——**只列真正需要用户点头的文件**。
//!
//! # 什么进清单，什么不进（口径只此一处）
//!
//! 一个路径进清单**当且仅当**：目标实例里同名路径是一个普通文件，且它的内容与包内**不同**。
//! 等值判据是 **SHA-1**（包内那条来自 M110 读包时现算的摘要，磁盘那条现算）；体积只作展示，
//! 不参与判定——同一份内容不可能有两个哈希，而"体积相同"并不说明内容相同。
//!
//! 不进清单的四种情形，每一种都有理由：
//!
//! | 情形 | 为什么不必打扰用户 |
//! |------|--------------------|
//! | 目标里没有这个文件 | 全新文件，装就是了 |
//! | 内容与包内一致 | 已经是最新，写盘是浪费 |
//! | 用户删过（基线有、磁盘没有） | 删是用户的决定，**不偷偷装回来**（`merge::decide` 的 `UserDeleted`） |
//! | 包内被同包另一条声明遮蔽（`shadowed`） | 它不落地，用户看不到它，问了也无从选 |
//!
//! # 叠层优先级不在这层重判
//!
//! `client-overrides/` 胜 `overrides/`、`server-overrides/` 客户端不解压——这些**已由**
//! [`super::plan::ImportPlan::build`] 与 `plan::layer::resolve` 算定（一路径一行，胜者才在
//! `overrides` 表里且 `shadowed.is_none()`）。本模块只按胜者取 `source`，从不自己再排一次。
//!
//! # 远端条目的两种命运
//!
//! mrpack 的 `files[]` 自带 `path` + `hashes.sha1`，所以能在这里判；判的是"**声明的**摘要 vs
//! 磁盘"，与导入时"下载下来再校验"是同一份期望值。CurseForge 的 `files[]` 只有
//! `projectID/fileID`，补全前**没有路径**，无从判起 ⇒ 不猜、不进清单，只把条数记进
//! [`ImportScan::unknown_remote`]（界面据此说"还有 N 条补全后才能知道会不会撞车"）。计数与
//! 文案同一口径：**判不了**的两种来路（补全前没有路径 / 有路径但缺 `hashes.sha1`）都算。
//!
//! # 便宜在哪、贵在哪
//!
//! 预检不读包内字节（摘要来自 M110 的读包结果），但要给每条冲突填"目标现有体积与哈希"，
//! 所以**磁盘上那些同名文件要真读一遍**。于是 `scan` + `import` 会把它们读两次——这是本轮
//! 的已知代价（要省就得让导入直接吃预检的判定结果，那是另一件事，报告里登记）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::baseline::PackBaseline;
use super::decision::ConflictDecisions;
use super::error::ImportError;
use super::merge;
use super::pipeline::{load_baseline, probe_and_plan};
use super::plan::ImportPlan;
use super::report::{keys, ConflictReason, FileSource, ImportIssue, LoaderTarget};
use super::ImportRequest;
use crate::download::verify;
use crate::download::HashAlgo;

/// 一条**要用户点头**的冲突（目标里已有同名文件且内容不同）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportConflict {
    /// 实例内相对路径。
    pub path: String,
    /// 来源（包内 `overrides/` / `client-overrides/` / mrpack 或 CF 的 `files[]`）。
    pub source: FileSource,
    /// 成因（`PreExisting` = 没有基线可判；`UserModified` = 基线里记过、用户改过）。
    pub reason: ConflictReason,
    /// 包内那条是不是禁用态（`.disabled` 后缀原样保留）。
    pub disabled: bool,
    /// 包内声明的 SHA-1。
    pub pack_sha1: String,
    /// 包内声明的字节数。
    pub pack_size: u64,
    /// 目标现有文件的 SHA-1。
    pub disk_sha1: String,
    /// 目标现有文件的字节数。
    pub disk_size: u64,
    /// 上次导入写下的 SHA-1（没有基线时 `None`）。
    pub base_sha1: Option<String>,
}

impl ImportConflict {
    /// 覆盖它会丢掉多少字节（界面上"覆盖会丢掉 12.4 MB"就是它）。
    #[must_use]
    pub const fn lost_bytes(&self) -> u64 {
        self.disk_size
    }
}

/// 一次预检的结果。
///
/// `error` 为 `Some` 时其余字段是"没算出来"：探测/开包/组计划失败与导入管线同一条口径
/// （同一个 [`PackError`] → [`ImportError`] 的映射），不去新造一套。
#[derive(Debug, Clone)]
pub struct ImportScan {
    /// 目标实例目录。
    pub target: PathBuf,
    /// 命中的格式 id。
    pub format: Option<String>,
    /// 包名。
    pub pack_name: Option<String>,
    /// 包版本。
    pub pack_version: Option<String>,
    /// 游戏版本。
    pub game_version: Option<String>,
    /// 加载器计划输入（`None` = 纯原版）。
    pub loader: Option<LoaderTarget>,
    /// 冲突清单（按路径排序）。
    pub conflicts: Vec<ImportConflict>,
    /// 预检阶段**判不了**的远端条目数。
    ///
    /// 判不了有两种来路，计数把两者都算上（口径与注记文案一致，复审 r1 P3-4）：
    /// ①CurseForge `files[]` 补全前**没有路径**（常态）；②极少数 mrpack 条目**有路径但缺
    /// `hashes.sha1`**，没有声明摘要就没法与磁盘现状比对。
    pub unknown_remote: usize,
    /// 注记（`.disabled` / 可选条目 / 基线口径 / 被遮蔽的条目……与导入报告同源）。
    pub notes: Vec<ImportIssue>,
    /// 开包或组计划失败（`Failed` 口径，目标目录一个字节都没动）。
    pub error: Option<ImportError>,
}

impl ImportScan {
    fn empty(target: PathBuf) -> Self {
        Self {
            target,
            format: None,
            pack_name: None,
            pack_version: None,
            game_version: None,
            loader: None,
            conflicts: Vec::new(),
            unknown_remote: 0,
            notes: Vec::new(),
            error: None,
        }
    }

    /// 预检成功了吗（`false` = 看 [`Self::error`]）。
    #[must_use]
    pub const fn is_ok(&self) -> bool {
        self.error.is_none()
    }

    /// 有没有需要用户点头的项（`false` = 可以直接导入，不必打扰用户）。
    #[must_use]
    pub fn needs_decision(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// 「全部保留」的决策（= 缺省档，不覆盖任何用户文件）。
    #[must_use]
    pub fn keep_all(&self) -> ConflictDecisions {
        ConflictDecisions::keep_all()
    }

    /// 「全部覆盖」的决策（用户显式要求"整合包说了算"）。
    #[must_use]
    pub fn overwrite_all(&self) -> ConflictDecisions {
        ConflictDecisions::overwrite_all()
    }

    /// 覆盖会丢掉的字节总量（界面上的计数摘要）。
    #[must_use]
    pub fn lost_bytes(&self) -> u64 {
        self.conflicts
            .iter()
            .map(ImportConflict::lost_bytes)
            .fold(0, u64::saturating_add)
    }

    /// 包内那些条目声明了多大（= 覆盖后会占用的字节数）。
    #[must_use]
    pub fn incoming_bytes(&self) -> u64 {
        self.conflicts
            .iter()
            .map(|conflict| conflict.pack_size)
            .fold(0, u64::saturating_add)
    }
}

/// 跑一次预检：**只读**（不建目录、不写暂存、不改基线、**不联网**）。
#[must_use]
pub fn scan_pack(request: &ImportRequest<'_>) -> ImportScan {
    let mut scan = ImportScan::empty(request.target.to_path_buf());
    let plan = match probe_and_plan(request) {
        Ok((_, plan)) => plan,
        Err(error) => {
            scan.error = Some(error);
            return scan;
        }
    };
    fill(request, &plan, &mut scan);
    scan
}

/// 跑一次预检，**先补全再判**（M162）：缺 `hashes.sha1` 的 mrpack 条目按 `sha512` 反查 Modrinth。
///
/// 补出来的条目与清单里本来就带 `sha1` 的条目走**同一个** [`judge`]（判据一行没改）；
/// 补不出的仍进 [`ImportScan::unknown_remote`] 并留同一条注记——计数口径不变，只是「能判的变多了」。
///
/// 没有挂 fetcher（离线导入）时等价于 [`scan_pack`]：不发任何请求，计数照旧。
#[must_use]
pub async fn scan_pack_online(request: &ImportRequest<'_>) -> ImportScan {
    let mut scan = ImportScan::empty(request.target.to_path_buf());
    let mut plan = match probe_and_plan(request) {
        Ok((_, plan)) => plan,
        Err(error) => {
            scan.error = Some(error);
            return scan;
        }
    };
    let completion = super::complete::complete(request, &mut plan).await;
    fill(request, &plan, &mut scan);
    if let Some(note) = completion.note("预检前") {
        scan.notes.push(note);
    }
    scan
}

/// 预检的主体：把「已组好的计划」判成冲突清单（两个入口共用，判据只此一处）。
fn fill(request: &ImportRequest<'_>, plan: &ImportPlan, scan: &mut ImportScan) {
    let (baseline, warnings) = load_baseline(request.target, plan);
    scan.notes = warnings;
    scan.format = Some(plan.format.clone());
    scan.pack_name = Some(plan.manifest.name.clone());
    scan.pack_version = plan.manifest.version.clone();
    scan.game_version = Some(plan.mc_version.clone());
    scan.loader = plan.loader.clone();

    // 一路径一行：同一路径被覆盖条目与远端条目同时声明时，只留一条（决策是按路径给的）。
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for entry in plan
        .overrides
        .iter()
        .filter(|entry| entry.shadowed.is_none())
    {
        let Some(conflict) = judge(
            &entry.path,
            entry.source,
            &entry.sha1,
            entry.size,
            entry.disabled,
            &baseline,
            request.target,
        ) else {
            continue;
        };
        seen.insert(entry.path.clone(), ());
        scan.conflicts.push(conflict);
    }
    for entry in &plan.remote {
        let (Some(path), Some(declared)) = (entry.path.as_deref(), entry.file.sha1()) else {
            // 两种来路都算「判不了」：没有路径（CF 待补全）或没有声明摘要（缺 `hashes.sha1`）。
            scan.unknown_remote += 1;
            continue;
        };
        if seen.contains_key(path) {
            continue;
        }
        let Some(conflict) = judge(
            path,
            entry.source,
            declared,
            entry.file.size.unwrap_or(0),
            entry.disabled,
            &baseline,
            request.target,
        ) else {
            continue;
        };
        seen.insert(path.to_string(), ());
        scan.conflicts.push(conflict);
    }
    if scan.unknown_remote > 0 {
        scan.notes.push(ImportIssue::new(
            keys::NOTE_REMOTE_PATH_UNKNOWN,
            format!(
                "有 {} 条远端条目预检判不了：CurseForge files[] 补全前没有路径，个别条目有路径但没有 sha1 声明；这些条目会不会撞上已有文件只能等补全/下载后才知道，导入时仍按「缺省保留」处置",
                scan.unknown_remote
            ),
        ));
    }
    scan.conflicts
        .sort_by(|left, right| left.path.cmp(&right.path));
}

/// 判一个路径：`Some` = 要用户点头的冲突。
///
/// # 「内容相同」的判据**只此一处**：[`merge::decide`]
///
/// 这里**故意不再写** `if disk.sha1 == pack_sha1 { return None }`（复审 r1 P3-2 抓到的重复判据）：
/// 那种早退与 `merge::decide` 的 `SkipIdentical` 分支是同一件事的两次实现，改一处漏一处的后果
/// 就是"预检说没冲突、导入却判 SkipIdentical"或者反过来。
///
/// 现在只有一条链路：`decide` 给出 `MergeDecision`，**没有 `reason` 就不是冲突**
/// （`SkipIdentical` 与 `Install` 都是 `reason == None`），`?` 把它一步交代掉。
/// 于是"内容相同 ⇒ 不进清单"这条结论的源头是 `merge.rs` 的那张表——与导入用的是同一张。
fn judge(
    path: &str,
    source: FileSource,
    pack_sha1: &str,
    pack_size: u64,
    disabled: bool,
    baseline: &PackBaseline,
    target: &Path,
) -> Option<ImportConflict> {
    let disk = disk_state(&target.join(path))?;
    // 包内那条的摘要来自 M110 读包（`override_files` 边读边算），与导入时现算的是同一份字节。
    let base_sha1 = baseline.sha1(path).map(str::to_string);
    // 唯一判据：`reason` 为 `None` 就是"不是冲突"（内容相同 → `SkipIdentical`；
    // 磁盘上没有 → `Install`，而那一条 `disk_state` 已经先返回 `None` 了）。
    let decision = merge::decide(pack_sha1, base_sha1.as_deref(), Some(&disk.sha1));
    let reason = decision.reason?;
    Some(ImportConflict {
        path: path.to_string(),
        source,
        reason,
        disabled,
        pack_sha1: pack_sha1.to_string(),
        pack_size,
        disk_sha1: disk.sha1,
        disk_size: disk.size,
        base_sha1,
    })
}

/// 磁盘现状：`None` = 那里没有普通文件（不存在或是目录）。
struct DiskState {
    size: u64,
    sha1: String,
}

fn disk_state(path: &Path) -> Option<DiskState> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    let sha1 = verify::digest_file(path, HashAlgo::Sha1).ok()?;
    Some(DiskState {
        size: metadata.len(),
        sha1,
    })
}
