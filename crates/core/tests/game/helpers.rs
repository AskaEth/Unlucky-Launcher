//! 测试公用件：fixture 解析、内存父版本表、临时目录守卫。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use unlucky_core::game::manifest::VersionJson;

/// 解析 fixture JSON（fixture 是测试常量，解析失败即 panic）。
pub fn parse(json: &str) -> VersionJson {
    VersionJson::from_json_str(json).expect("fixture 必须可解析")
}

/// 用 `(id, json)` 对建内存父版本表，喂给 `inherit::resolve`。
pub fn source(pairs: &[(&str, &str)]) -> BTreeMap<String, VersionJson> {
    pairs
        .iter()
        .map(|(id, json)| ((*id).to_owned(), parse(json)))
        .collect()
}

/// 唯一临时目录守卫：`drop` 时递归删除。
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// 建一个唯一临时目录。
    pub fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-game-{tag}-{}-{nanos}-{seq}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("临时目录应可创建");
        Self { path }
    }

    /// 目录路径。
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 建一个游戏根布局（纯路径计算，不建目录）。
pub fn layout(root: &Path) -> unlucky_core::game::GameLayout {
    unlucky_core::game::GameLayout::new(root)
}

/// 造 `dir/<name>` 并写一个文件进去（模拟玩家内容：`mods`/`saves`/`config` 的条目）。
pub fn seed(dir: &Path, name: &str) {
    let target = dir.join(name);
    std::fs::create_dir_all(&target).expect("建目录");
    std::fs::write(target.join("entry.txt"), b"x").expect("写文件");
}
