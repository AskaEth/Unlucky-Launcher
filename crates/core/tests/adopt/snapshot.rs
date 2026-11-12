//! 目录树**全节点**快照：目录 / 文件 / 链接各记自己的类型与适用 mtime。
//!
//! # 为什么不是「只记文件」
//!
//! 复审 r1 的 R5：早先版本遇到目录只递归、不记录，于是「登记过程凭空建了一个空目录」
//! （例如错误地在 `<game>/versions/<v>/mods` 里 `create_dir_all`）在快照上**完全看不出来**。
//! 「只读外部」这条承诺必须能抓住新增/删除的**目录节点**，所以这里：
//!
//! - 根自身、每个目录、每个文件、每个链接都进快照（相对路径 → [`Node`]）；
//! - 目录记 mtime（条目增删会改它），文件记字节数 + mtime + 内容哈希；
//! - **链接不跟随**：只记目标路径（跟随会把根外的内容算进「外部树」，让越界写入看起来像「没变化」）；
//!   Windows 的目录联接（junction）不是 `is_symlink`，这里额外用重解析点属性识别它；
//! - 任何读盘失败都**直接失败**（`panic`），不吞成「这项没变」——吞错就是假绿。
//!
//! # 为什么 CLI 测试也用这一份
//!
//! 以前 core 与 CLI 各写一份 walk，两份都漏了目录节点（同一处缺口两遍）。现在 CLI 的
//! `adopt_test/fixture.rs` 用 `#[path = "../../../core/tests/adopt/snapshot.rs"]` 复用本文件：
//! **判据只有一份**，不会再次漂移。

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// 快照里的一个节点。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Node {
    /// 目录（只记 mtime：条目增删会改它）。
    Dir {
        /// mtime（纳秒；取不到记 0）。
        mtime_ns: u128,
    },
    /// 文件（字节数 + mtime + 内容哈希）。
    File {
        /// 字节数。
        len: u64,
        /// mtime（纳秒）。
        mtime_ns: u128,
        /// 内容哈希（稳定即可，不要密码学强度）。
        hash: u64,
    },
    /// 符号链接 / Windows 目录联接：只记目标，**不跟随**。
    Link {
        /// 链接目标（原样，可能是相对路径；读不出来时记 `<unreadable-link>`）。
        target: PathBuf,
    },
}

/// 一次快照：相对路径（根自身是 `.`）→ 节点。
pub type Snapshot = BTreeMap<String, Node>;

/// 拍一次全节点快照（目标不存在 = 空快照，见 [`snapshot_opt`] 的区分）。
///
/// # Panics
///
/// 任何目录/元数据/文件读不出来都 panic：读不出来不能当成「没变化」。
pub fn snapshot(root: &Path) -> Snapshot {
    snapshot_opt(root).unwrap_or_default()
}

/// 同 [`snapshot`]，但目标**不存在**时返回 `None`（用于区分「空树」与「根本没有」）。
///
/// # Panics
///
/// 同上（存在但读不出来即 panic）。
pub fn snapshot_opt(root: &Path) -> Option<Snapshot> {
    match std::fs::symlink_metadata(root) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => panic!("快照根读不出来 {}：{error}", root.display()),
    }
    let mut out = Snapshot::new();
    out.insert(".".to_string(), node_of(root));
    walk(root, root, &mut out);
    Some(out)
}

fn walk(root: &Path, dir: &Path, out: &mut Snapshot) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("目录应可读 {}：{error}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("目录项应可读 {}：{error}", dir.display()));
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let node = node_of(&path);
        let is_dir = matches!(node, Node::Dir { .. });
        out.insert(relative, node);
        if is_dir {
            walk(root, &path, out);
        }
    }
}

/// 单个节点的形状（链接/联接**不跟随**）。
fn node_of(path: &Path) -> Node {
    let meta = std::fs::symlink_metadata(path)
        .unwrap_or_else(|error| panic!("元数据应可读 {}：{error}", path.display()));
    if link_like(&meta) {
        let target =
            std::fs::read_link(path).unwrap_or_else(|_| PathBuf::from("<unreadable-link>"));
        return Node::Link { target };
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_nanos());
    if meta.file_type().is_dir() {
        return Node::Dir { mtime_ns };
    }
    let bytes = std::fs::read(path)
        .unwrap_or_else(|error| panic!("文件应可读 {}：{error}", path.display()));
    Node::File {
        len: bytes.len() as u64,
        mtime_ns,
        hash: hash(&bytes),
    }
}

/// 符号链接（所有平台）或 Windows 重解析点（junction / mount point）。
///
/// `FileType::is_symlink` 对 junction 返回 `false`，所以 Windows 上另看
/// `FILE_ATTRIBUTE_REPARSE_POINT`：不识别它就会把 junction 当普通目录**递归进去**，
/// 等于跟随链接读根外内容。
fn link_like(meta: &std::fs::Metadata) -> bool {
    if meta.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 内容哈希（只要稳定、不需要密码学强度）。
fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// 断言两次快照**完全一致**：路径集合（含目录/链接）与每个节点的形状都不许变。
///
/// 抓得住：
/// - 新增/删除**空目录**（目录节点进快照）、目录条目变化（目录 mtime 变）；
/// - 文件被改写（哈希变、mtime 变）或被删（路径集合变）；
/// - 链接目标被改（`Link.target` 变）。
///
/// 抓不住：只读打开（无可观测痕迹）、文件内容相同且 mtime 未被改写的「伪写」、
/// 以及检查期间的 TOCTOU。
///
/// # Panics
///
/// 快照不一致即 panic（消息里点名第一个不同的路径）。
pub fn assert_unchanged(before: &Snapshot, after: &Snapshot, what: &str) {
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "{what}：路径集合（含目录/链接节点）变了"
    );
    for (path, node) in before {
        assert_eq!(
            node,
            after.get(path).expect("同一路径"),
            "{what}：节点被改动 -> {path}"
        );
    }
}
