//! 测试公用小工具（临时目录 + 文件读写）。
//!
//! 不引 `tempfile`：M11 的依赖面只有 serde/serde_json，测试自己管临时目录。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 进程内唯一的临时目录，`Drop` 时递归删除。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 建一个临时目录。
    pub fn new(label: &str) -> Self {
        let index = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-setting-{label}-{}-{index}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&path).ok();
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    /// 根路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 拼接子路径（不创建）。
    pub fn child(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// 写一个文件（自动建父目录）。
    pub fn write(&self, name: &str, contents: &str) -> PathBuf {
        let path = self.child(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(&path, contents).expect("写文件");
        path
    }

    /// 读一个文件。
    pub fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.child(name)).expect("读文件")
    }

    /// 子路径是否存在。
    pub fn exists(&self, name: &str) -> bool {
        self.child(name).exists()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).ok();
    }
}

/// 目录里的文件名（排序后返回，便于断言没留下临时文件）。
pub fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("读目录")
        .map(|entry| {
            entry
                .expect("目录项")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}
