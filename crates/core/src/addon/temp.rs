//! 测试用的**一次性临时根**（`Drop` 即删；panic 展开时同样删）。
//!
//! # 为什么它必须在 `addon` 里而不是某个测试目录里
//!
//! `addon::pack` 与 `addon::tests` 两族用例都建临时目录，而 `pack/tests/fixtures.rs` 被**两边
//! 同时**通过 `#[path]` 编译（同一份手造包只维护一处）。守卫因此要放在两边的公共祖先上，否则
//! `fixtures.rs` 里的 `super::TempRoot` 在其中一侧解析不到。
//!
//! # 为什么不能靠"系统会清 `%TEMP%`"
//!
//! 那种清理要等时间或外部工具，而 `zip64` 那族用例历史上每个要造 4 GiB 的"稀疏"文件验阈值
//! （**实测在 NTFS 上 `set_len` 真占空间**，见 `pack/tests/zip64.rs` 的模块文档）：
//! M160 r2 实测 `%TEMP%/ull-pack-*` 堆到 **1651 个目录 / 110 GB**，把整台机器（含别的 agent
//! 的构建）一起顶死。所以测试自己负责收尾，并且有自证用例
//! （`pack/tests/mod.rs` 的 `the_temp_root_is_gone_once_dropped` /
//! `a_panicking_test_still_removes_its_temp_root`）。
//!
//! # 形状来自仓库既有写法
//!
//! 与 [`crate::update::tests::support::TempDir`] 同一套 `new` / `join` / `Drop` 三件套
//! （不引 `tempfile`、不新增范式），只多了两件事：`Deref<Target = Path>` 让调用点能直接当
//! `&Path` 用；[`TempRoot::under`] 让"子树落点、删父目录"这件事不留下空壳。

use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 一次性临时根目录（**`Drop` 即删**）。
pub(crate) struct TempRoot {
    /// 真正要删的根（[`Self::under`] 造的子树也删它，不留空壳父目录）。
    root: PathBuf,
    /// 本夹具的落点（通常等于 `root`；`under` 时是 `root/<sub>`）。
    path: PathBuf,
}

impl TempRoot {
    /// 建一个新的临时根（同名残留先清掉：`COUNTER` 在同一进程内唯一，跨进程靠 pid）。
    pub(crate) fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("ull-pack-{}-{tag}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("建临时根");
        Self {
            path: root.clone(),
            root,
        }
    }

    /// 在临时根下开一个子目录当落点（`Drop` 删的是**根**，父目录不会剩空壳）。
    pub(crate) fn under(tag: &str, sub: &str) -> Self {
        let base = Self::new(tag);
        let path = base.path.join(sub);
        std::fs::create_dir_all(&path).expect("建临时子目录");
        Self {
            root: base.root.clone(),
            path,
        }
    }

    /// 建一个临时根，并让落点直接是根内的某个文件（`Drop` 删的仍是**根**）。
    ///
    /// 给"夹具函数要返回一个包路径、而包路径必须活到测试用完"的场景用：返回值 `Deref` 到那个
    /// 文件，根由它持有。`root.parent()` 拿得到根目录。
    pub(crate) fn at(tag: &str, file: &str) -> Self {
        let mut base = Self::new(tag);
        base.path = base.path.join(file);
        base
    }

    /// 本夹具的落点。
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// 落点下的一条路径（`&str` 与 `Path` 都收，免得把调用点写成两套）。
    pub(crate) fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.path.join(name)
    }
}

impl Deref for TempRoot {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for TempRoot {
    /// 让 `&TempRoot` 能直接喂给 `impl AsRef<Path>` 形态的 API（`fs::read_dir` / `remove_dir_all`
    /// / `PackArchive::open_default` …）。`Deref` 只解决"要 `&Path`"的场景，两者都要。
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        // 尽力删：删不掉留一行日志，别在析构里 panic（二次 panic 会 abort）。
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[temp] 删 {} 失败：{error}", self.root.display());
            }
        }
    }
}
