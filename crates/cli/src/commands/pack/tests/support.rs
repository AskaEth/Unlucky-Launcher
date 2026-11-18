//! `pack import` 那几条用例共用的支撑件。
//!
//! M318 写进 `tests.rs`，M346 r3 拆到这里（`tests.rs` 贴着 400 行硬线，且下一轮任何一行
//! 改动都会顶线）。**纯搬移**：五个项（`KEY_ENV_LOCK` / `TempDir` / `KeyEnv` / `ctx_at` /
//! `write_key` / `fetcher_debug`）逐字未变，只有可见性从「同模块私有」改成 `pub(crate)`，
//! 由父模块 `pub(crate) use support::*;` 再导出——四个兄弟用例文件（`m321` / `m323` /
//! `m325` / `m346`）的 `use super::*` 因此一字不用改。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::json;
use unlucky_core::addon::curseforge::auth::API_KEY_ENV;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

use crate::cli::Globals;
use crate::context::Ctx;

use super::super::{build_engine, fetcher_for, CF_KEY};

/// 环境变量是**进程级**的：动它的用例串行跑。
pub(crate) static KEY_ENV_LOCK: Mutex<()> = Mutex::new(());

/// 一次性临时数据根（`Drop` 即删；cli 没有 tempfile 依赖）。
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    pub(crate) fn new(tag: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ull-pack-key-{tag}-{}-{seq}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 环境变量的值守卫：离开作用域（含 panic 展开）时**恢复原值**，不污染同进程的其它用例。
pub(crate) struct KeyEnv {
    previous: Option<String>,
}

impl KeyEnv {
    /// `Some` = 设成这个值，`None` = 删掉这个变量。
    pub(crate) fn set(value: Option<&str>) -> Self {
        let previous = std::env::var(API_KEY_ENV).ok();
        match value {
            Some(value) => std::env::set_var(API_KEY_ENV, value),
            None => std::env::remove_var(API_KEY_ENV),
        }
        Self { previous }
    }
}

impl Drop for KeyEnv {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(API_KEY_ENV, value),
            None => std::env::remove_var(API_KEY_ENV),
        }
    }
}

pub(crate) fn ctx_at(dir: &Path) -> Ctx {
    Ctx::resolve_in(&Globals::default(), Some(dir.to_path_buf())).expect("解析数据根")
}

/// 把设置写进 CLI 自己那份 `launcher.json`（走真实的 `SettingStore`，不手搓 JSON）。
pub(crate) fn write_key(ctx: &Ctx, value: &str) {
    if let Some(parent) = ctx.launcher_config().parent() {
        std::fs::create_dir_all(parent).expect("建配置目录");
    }
    let mut store = SettingStore::load(ctx.launcher_config(), launcher_schema());
    store.set(CF_KEY, json!(value)).expect("写 CF Key");
    store.save().expect("落盘");
}

/// 构造出来的 fetcher 到底带没带 Key（`ApiKey` 的 `Debug` 恒打 `<redacted>`，所以这条断言
/// 既看得见「有没有」，又印不出明文）。**这是 `with_curse_key` 那一行的行为级判据**：
/// 把它改回 `new()` ⇒ `curse_key: None` ⇒ 本用例红。
pub(crate) fn fetcher_debug(ctx: &Ctx) -> String {
    let engine = build_engine(ctx);
    format!("{:?}", fetcher_for(&engine, ctx))
}
