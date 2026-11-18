//! 原子落盘：临时文件守卫 + `tmp → rename`。
//!
//! 设计文档 §4 的 HMCL 坑之一：没有「先校验后落盘」，崩溃/取消会留下「看起来存在但内容坏了」
//! 的文件，之后所有「存在即跳过」的判断全部失效。这里的规矩：
//!
//! * 下载永远写进同级中间文件，校验通过才 `rename` 到目标路径；
//! * `PartFile` 在 `Drop` 时自动清理（除非已 `commit`），取消/early-return 都不会留垃圾；
//! * 小文件（索引、健康度）走 `write_atomic`，同样先写临时文件再 `rename`。
//!
//! # 中间文件的身份
//!
//! 中间文件名是 `<name>.ullpart-<session>-<seq>`：`session` 是进程号，`seq` 是进程内
//! 自增序号（一个 `PartFile` 一个）。旧版命名只由目标路径决定（`<name>.ullpart`），
//! 同一数据根同时跑两个启动器进程时两边共用一个中间文件：先提交的一方 `rename` 走，
//! 另一方就拿到 `os error 2`（M35 真机：两个 `ull install` 并行装同一数据根，
//! 各失败 819 / 1116 个资源）。
//!
//! 单进程语义（幂等 / 断点续传 / `current_bytes` 跳过判定）刻意保持不变：
//!
//! * [`PartFile::for_dest`] 先在目标目录里找这个目标的旧中间文件并**收养**（本进程的、
//!   旧版无主的、以及外进程但静置超过 [`ADOPT_MIN_AGE`] 的），改名成本次路径；后续
//!   续传判定完全照旧，跨进程/跨版本中断的下载仍然续得上。
//! * 孤儿清理只碰两类：本会话留下又没人在用的副本，和无主（旧版命名）且超过
//!   [`ORPHAN_MAX_AGE`] 的残留。本进程正在用的路径登记在活跃表里一律让开；外进程的
//!   中间文件除了「静置超过 [`ORPHAN_MAX_AGE`] 且同一个目标」的，一概不删——没有进程
//!   存活性探测就无法证明那边已经死了，宁可再生点垃圾也不重犯 `os error 2`。

use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// 下载中间文件后缀（中间文件家族的前缀：`<name>.ullpart` 或 `<name>.ullpart-<session>-<seq>`）。
pub const PART_SUFFIX: &str = ".ullpart";
/// 续传元数据后缀（挂在中间文件路径后面）。
pub const META_SUFFIX: &str = ".ullmeta.json";
/// 收养阈值：别的进程留下的中间文件静置这么久才敢接管。
///
/// 活着的下载每读到一个块就写一次（读超时默认 30s 就报错退出），所以「10 分钟没写」
/// 基本等价于「那边已经死了」。
pub const ADOPT_MIN_AGE: Duration = Duration::from_secs(10 * 60);
/// 孤儿阈值：无主（旧版命名）中间文件超过这个岁数才清。
pub const ORPHAN_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// 同进程内中间文件/临时文件的去重序号。
static SEQ: AtomicU64 = AtomicU64::new(0);

fn sibling(dest: &Path, suffix: &str) -> PathBuf {
    let mut name = dest.file_name().map_or_else(OsString::new, OsString::from);
    name.push(suffix);
    match dest.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// 本进程序号（中间文件与 `write_atomic` 的临时文件共用）。
fn next_seq() -> u64 {
    SEQ.fetch_add(1, Ordering::Relaxed)
}

/// 本进程此刻仍在用的中间文件路径（清理与收养都必须让开）。
fn live_parts() -> &'static Mutex<HashSet<PathBuf>> {
    static LIVE: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashSet::new()))
}

fn mark_live(path: &Path, live: bool) {
    let mut set = live_parts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if live {
        set.insert(path.to_path_buf());
    } else {
        set.remove(path);
    }
}

fn is_live(path: &Path) -> bool {
    live_parts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(path)
}

/// 中间文件属主（从文件名解析）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    /// 旧版命名：没有进程标识（加标识之前留下的续传材料）。
    Legacy,
    /// 某个进程（会话）留下的中间文件。
    Session(u32),
}

fn strip_meta(name: &str) -> &str {
    name.strip_suffix(META_SUFFIX).unwrap_or(name)
}

/// 名字里指向的中间文件属主；不是我们认识的名字形状 → `None`（一概不碰）。
fn owner_of(file_name: &str) -> Option<Owner> {
    let base = strip_meta(file_name);
    let index = base.rfind(PART_SUFFIX)?;
    if index == 0 {
        return None;
    }
    let tail = &base[index + PART_SUFFIX.len()..];
    if tail.is_empty() {
        return Some(Owner::Legacy);
    }
    let mut pieces = tail.strip_prefix('-')?.split('-');
    let session = pieces.next()?.parse::<u32>().ok()?;
    pieces.next()?.parse::<u64>().ok()?;
    if pieces.next().is_some() {
        return None;
    }
    Some(Owner::Session(session))
}

/// 本次会话的中间文件路径：`<name>.ullpart-<session>-<seq>`。
fn part_path(dest: &Path, session: u32, seq: u64) -> PathBuf {
    let mut name = dest.file_name().map_or_else(OsString::new, OsString::from);
    name.push(format!("{PART_SUFFIX}-{session}-{seq}"));
    match dest.parent() {
        Some(parent) => parent.join(name),
        None => PathBuf::from(name),
    }
}

/// 文件距上次修改多久（取不到就当刚写过：清理与收养一律保守）。
fn age_of(path: &Path) -> Duration {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .unwrap_or_default()
}

/// 删掉一个中间文件连同它挂的续传元数据。
fn remove_family(part: &Path) {
    let _ = fs::remove_file(part);
    let _ = fs::remove_file(sibling(part, META_SUFFIX));
}

/// 目录扫描的三类结果。
#[derive(Default)]
struct Scan {
    /// 可收养的旧中间文件（最年轻的那个）。
    adopted: Option<PathBuf>,
    /// 本目标的全部中间文件副本（含被收养者）。
    leftovers: Vec<(PathBuf, Owner, Duration)>,
    /// 无主且超龄的孤儿。
    orphans: Vec<PathBuf>,
}

fn scan(dest: &Path, session: u32) -> Scan {
    let mut scan = Scan::default();
    let Some(dir) = dest
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return scan;
    };
    let Some(dest_name) = dest.file_name() else {
        return scan;
    };
    let family = format!("{}{PART_SUFFIX}", dest_name.to_string_lossy());
    let Ok(entries) = fs::read_dir(dir) else {
        return scan;
    };
    let mut newest: Option<Duration> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(META_SUFFIX) {
            continue; // 元数据跟着它挂的中间文件一起处理
        }
        let Some(owner) = owner_of(&name) else {
            continue;
        };
        let file = entry.path();
        if !file.is_file() {
            continue;
        }
        let age = age_of(&file);
        if !name.starts_with(&family) {
            if owner == Owner::Legacy && age >= ORPHAN_MAX_AGE {
                scan.orphans.push(file);
            }
            continue;
        }
        let adoptable = match owner {
            // 本会话的旧副本：没人在用就能接管（保留「同一进程内重跑可续传」）。
            Owner::Session(owner_session) if owner_session == session => !is_live(&file),
            // 外进程的：静置够久才敢动手，否则可能正在被写。
            Owner::Session(_) => age >= ADOPT_MIN_AGE,
            Owner::Legacy => true,
        };
        if adoptable && newest.is_none_or(|best| age < best) {
            newest = Some(age);
            scan.adopted = Some(file.clone());
        }
        scan.leftovers.push((file, owner, age));
    }
    scan
}

/// 为本次下载准备中间文件：收养可续传的旧文件，清掉本会话多余副本与无主孤儿。
///
/// 全程尽力而为：清理失败绝不影响下载本身。
fn prepare(dest: &Path, path: &Path, session: u32) {
    let scan = scan(dest, session);

    if let Some(found) = scan.adopted {
        // 同一目录内改名，跨进程也是原子的：谁先动手谁拿到这份续传材料。
        if found != *path && fs::rename(&found, path).is_ok() {
            let meta = sibling(&found, META_SUFFIX);
            if meta.is_file() {
                let _ = fs::rename(&meta, sibling(path, META_SUFFIX));
            }
        }
    }

    for (leftover, owner, age) in scan.leftovers {
        if leftover == *path || is_live(&leftover) {
            continue;
        }
        // 本会话落败的副本留着没意义；外进程的只有超龄才敢清。
        if owner == Owner::Session(session) || age >= ORPHAN_MAX_AGE {
            remove_family(&leftover);
        }
    }

    for orphan in scan.orphans {
        remove_family(&orphan);
    }
}

/// 下载中间文件的守卫。
#[derive(Debug)]
pub struct PartFile {
    path: PathBuf,
    committed: bool,
    preserved: bool,
}

impl PartFile {
    /// 为目标路径派生中间文件（名字带进程标识，并顺手收养旧的续传材料）。
    #[must_use]
    pub fn for_dest(dest: &Path) -> Self {
        let session = std::process::id();
        let path = part_path(dest, session, next_seq());
        mark_live(&path, true);
        prepare(dest, &path, session);
        Self {
            path,
            committed: false,
            preserved: false,
        }
    }

    /// 中间文件路径。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 续传元数据路径。
    #[must_use]
    pub fn meta_path(&self) -> PathBuf {
        sibling(&self.path, META_SUFFIX)
    }

    /// 中间文件是否存在。
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path.is_file()
    }

    /// 已写入的字节数（不存在时为 0）。
    #[must_use]
    pub fn len(&self) -> u64 {
        fs::metadata(&self.path).map_or(0, |meta| meta.len())
    }

    /// 是否为空（续传判定要区分「文件不存在」和「文件存在但 0 字节」）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 以追加方式打开（续传）。
    pub fn open_append(&self) -> io::Result<File> {
        ensure_parent(&self.path)?;
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
    }

    /// 以截断方式打开（从头下载）。
    pub fn open_truncate(&self) -> io::Result<File> {
        ensure_parent(&self.path)?;
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)
    }

    /// 校验通过后提交：把中间文件 `rename` 到目标路径（同一目录，跨文件系统不会发生）。
    pub fn commit(mut self, dest: &Path) -> io::Result<()> {
        ensure_parent(dest)?;
        fs::rename(&self.path, dest)?;
        self.committed = true;
        // rename 后中间文件已不存在，元数据是纯冗余，顺手清掉。
        let _ = fs::remove_file(self.meta_path());
        Ok(())
    }

    /// 主动丢弃中间文件（换源/校验失败）。
    pub fn discard(&mut self) {
        remove_family(&self.path);
    }

    /// 确保中间文件的父目录存在（异步版）。
    ///
    /// 单连接路径由 [`Self::open_append`] / [`Self::open_truncate`] 顺手建目录，
    /// 但**分块路径**是先 `tokio::fs::OpenOptions` 预分配整文件、再并发写段——
    /// 它不走那两个入口，所以得显式补这一步，否则「下到还不存在的目录」会全部源失败
    /// （现场故障：`data/java-managed/.cache/` 尚未创建时 6 个候选源瞬间全挂）。
    pub(crate) async fn ensure_parent_async(&self) -> io::Result<()> {
        let Some(parent) = self.path.parent() else {
            return Ok(());
        };
        if parent.as_os_str().is_empty() {
            return Ok(());
        }
        tokio::fs::create_dir_all(parent).await
    }

    /// 保留中间文件（跨会话续传用）：`Drop` 时不再清理。
    pub fn preserve(&mut self) {
        self.preserved = true;
    }
}

impl Drop for PartFile {
    fn drop(&mut self) {
        if !self.committed && !self.preserved {
            remove_family(&self.path);
        }
        mark_live(&self.path, false);
    }
}

/// 原子写小文件：`<path>.tmp-<session>-<seq>` → `rename(path)`。
///
/// 临时名同样带进程标识：两个进程同时写同一份小文件（健康度、续传元数据）时
/// 不会互相把对方的临时文件 `rename` 走。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    ensure_parent(path)?;
    let tmp = sibling(path, &format!(".tmp-{}-{}", std::process::id(), next_seq()));
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })
}

fn ensure_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}
