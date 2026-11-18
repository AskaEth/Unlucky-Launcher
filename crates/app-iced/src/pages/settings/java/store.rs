//! 「Java」分组的真读写（M175）：设置三键 + 手动名单 + 后台扫描。
//!
//! ## 写盘走 `SettingStore`，与 `ai/store.rs` 同一条通路
//!
//! `java.selected_path` / `java.default_memory_mb` / `java.extra_args` 都是
//! `config/launcher.json` 里的 schema 键（`core::setting::keys` 已登记，本 mission 不动 schema）。
//! 于是这里只做「读一份 → 改几个键 → 存 → **再从磁盘读回来**」：`save()` 有 `Skipped` 一档
//! （只读 / 文件被更新版本写过 / 没有脏键），把它当成功就会出现「界面说已保存、重启回旧值」。
//! 失败一律走 [`WriteError`]：**要么是页面自己的拒绝原因（i18n key），要么是 core 给的原始句子**，
//! 不再把 Rust 枚举名（`"MemoryRange"`）当人话往上抛（M175 r1 复审 P2-2）。
//!
//! ## 扫描为什么是「进程级快照 + 后台线程」，不是 `Task::perform`
//!
//! 与 `fonts::install` 同一条理由（那里记了完整推导）：本 crate 的执行器是 `thread-pool`，
//! 而扫描要的是**进度（3/7）+ 取消 + 不卡 UI**。快照方案的代价是一把短锁 + 1 拍一次比较，
//! 好处是窗口关掉之后不会往死掉的 channel 里写。界面的刷新节奏见 `app/update/clock.rs`
//! 与 `app/subscription.rs`（扫描期间按帧跑，扫描结束回到 1 Hz）。
//!
//! ### 三条与「取消」有关的硬约束（M175 r1 复审 P2-5）
//!
//! 1. **每个候选都有超时**：卡住的 `java -version` 由 `core::game::java::manual` 的
//!    `run_with_timeout` kill 掉，扫描线程不会永远不结束；
//! 2. **取消打断的是「正在跑的那一个」**：取消位一路传进探测函数，不必等它自己结束；
//! 3. **同时只允许一个扫描线程**：[`ScanReport::running`] 的含义就是「线程还活着」，
//!    取消**不**翻这个位（只置 `cancelled`），于是取消到线程真退出之间点「立即扫描」
//!    不会起第二个线程——界面上那一小段显示的是「正在取消…」。
//!
//! ## 手动名单为什么是纯文本文件
//!
//! 它记的是「我手动加过哪些路径」，不是启动策略（真正决定用哪个 Java 的是 `java.selected_path`）。
//! 放进 `launcher.json` 要新登记一个 schema 键并推 minor（会让旧版本整份文件变只读），
//! 代价远大于收益；而 `core::game::java::manual` 的读写是纯文本、一行一个，坏了顶多丢名单。

use std::path::{Path, PathBuf};

use unlucky_core::game::java::{discover, manual, JavaRuntime};
use unlucky_core::platform::paths::{Platform, ProcessEnv};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::{SaveOutcome, SettingStore};

use super::state::{Refusal, RuntimeRow, Snapshot, State, Unwritable};

// M204 r1：扫描槽与生产作业搬到了 [`super::scan`]（本文件加 `ScanSlot::seed_report` 之后
// 408 行，越过 400 行硬线）。这里再导出，`store::ScanSlot` / `store::ScanReport` 这些既有路径
// **一个都不用改**；`Job` 只被 `scan` 内部用，不外抛（外抛会让本文件多一条 unused 警告）。
pub use super::scan::{ScanReport, ScanSlot};

/// 该不该**自动**发起一次扫描（首屏 / 配置文件刚被外部改过时）。
///
/// 四条都要成立：①不是截图通道（D30：同一份代码出同一张图，截图不许读本机装了什么）；
/// ②没有在扫；③**没被取消过**（一次取消 = 本进程不再自动扫：被取消的扫描 `finished` 仍是
/// `false`，而 `launcher.json` 每改一次（存内存、改主题、改语言、存 AI 配置）心跳就会走一次
/// `hydrate_java_settings`，不看这一条就会把用户的取消无声撤销）；④还没扫完过（一次就够——
/// 用户手动点「立即扫描」走的是 `start`，不看这里）。
///
/// 单独抽出来是因为它是「用户实测的头一条抱怨」（设置页从来自动检测本机 Java）的开关：
/// 判据写在这里，`tests/cmdline.rs` 逐档断言，改坏立刻红。
#[must_use]
pub fn auto_scan_due(report: &ScanReport, screenshot: bool) -> bool {
    !screenshot && !report.running && !report.cancelled && !report.finished
}

/// 托管安装根（`<launcher home>/data/java-managed` + `java.installRoot` + 标准落点）。
///
/// 从**我们正在写的那份 `launcher.json`** 的上一级推 launcher home：这比重新解析一次数据根模式
/// 更可信（三模式解析已经在 `SettingsBridge::open` 里做过一遍了）。
#[must_use]
pub fn managed_roots(config: &Path) -> Vec<PathBuf> {
    let Some(home) = config.parent().and_then(Path::parent) else {
        return Vec::new();
    };
    let layout = unlucky_core::platform::layout::LauncherLayout::new(home);
    let override_root = SettingStore::load(config, launcher_schema())
        .doc()
        .effective_str("java.installRoot")
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(PathBuf::from);
    unlucky_core::game::java::location::scan_roots(
        unlucky_core::platform::data_dir::DataMode::Portable,
        &layout,
        override_root.as_deref(),
        Platform::current(),
        &ProcessEnv,
    )
}

/// 手动名单文件（与 `launcher.json` 同一个 `config/`）。
#[must_use]
pub fn manual_file(config: &Path) -> PathBuf {
    config.parent().map_or_else(
        || PathBuf::from("java-extra-paths.txt"),
        |dir| dir.join("java-extra-paths.txt"),
    )
}

/// 读三键真值。
#[must_use]
pub fn load(config: &Path) -> Snapshot {
    snapshot(SettingStore::load(config, launcher_schema()).doc())
}

/// 写盘失败的三种来源（都是「没写进去」，但对用户要说的不是同一句话）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteError {
    /// 页面自己的校验拒绝（i18n key）
    Refused(Refusal),
    /// 设置文件层面拒绝：只读 / 不可信 / 没有改动（i18n key）
    Unwritable(Unwritable),
    /// schema / 磁盘给的原始句子（core 的文案，不属于本页）
    Schema(String),
}

/// 保存内存 + JVM 参数，返回**落盘后读回来**的真值。
///
/// # Errors
/// 本地校验不过（[`WriteError::Refused`]）、文件只读/不可信（[`WriteError::Unwritable`]）
/// 或 schema 拒绝（[`WriteError::Schema`]）时返回原因。
pub fn save(config: &Path, state: &State) -> Result<Snapshot, WriteError> {
    let mb = state.validate().map_err(WriteError::Refused)?;
    let mut store = SettingStore::load(config, launcher_schema());
    store
        .set("java.default_memory_mb", i64::from(mb).into())
        .and_then(|()| store.set("java.extra_args", state.jvm.trim().to_string().into()))
        .map_err(|error| WriteError::Schema(format!("{error}")))?;
    finish(store, config)
}

/// 写 `java.selected_path`（选定默认 JDK），返回落盘后读回来的真值。
///
/// # Errors
/// 文件只读/不可信或 schema 拒绝时返回原因。
pub fn set_selected(config: &Path, binary: &str) -> Result<Snapshot, WriteError> {
    let mut store = SettingStore::load(config, launcher_schema());
    store
        .set("java.selected_path", binary.to_string().into())
        .map_err(|error| WriteError::Schema(format!("{error}")))?;
    finish(store, config)
}

/// 落盘 + 判定 + 读回（`save` / `set_selected` 共用：只认 `Saved` 一档）。
fn finish(mut store: SettingStore, config: &Path) -> Result<Snapshot, WriteError> {
    match store.save() {
        Ok(SaveOutcome::Saved(_)) => Ok(load(config)),
        Ok(SaveOutcome::Skipped(reason)) => Err(WriteError::Unwritable(unwritable(reason))),
        Err(error) => Err(WriteError::Schema(format!("{error}"))),
    }
}

/// `SkipReason` → 页面原因（**不**再用中文字符串：那是 r1 复审 P2-2 的第一条）。
pub(super) fn unwritable(reason: unlucky_core::setting::store::SkipReason) -> Unwritable {
    use unlucky_core::setting::store::SkipReason;
    match reason {
        SkipReason::NotDirty => Unwritable::NotDirty,
        SkipReason::ReadOnly => Unwritable::ReadOnly,
        SkipReason::Unreadable => Unwritable::Unreadable,
    }
}

/// 文档 → 快照。
fn snapshot(doc: &unlucky_core::setting::document::SettingDocument) -> Snapshot {
    Snapshot {
        selected: doc
            .effective_str("java.selected_path")
            .unwrap_or_default()
            .trim()
            .to_string(),
        memory_mb: doc
            .effective_i64("java.default_memory_mb")
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or(super::state::DEFAULT_MEMORY_MB),
        jvm: doc
            .effective_str("java.extra_args")
            .unwrap_or_default()
            .to_string(),
    }
}

/// 读手动名单。
#[must_use]
pub fn load_manual(config: &Path) -> Vec<String> {
    manual::load_paths(&manual_file(config))
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

/// 手动加一个路径：严格校验（真跑 `java -version`）→ 进名单 → 落盘。
///
/// 拒绝时**什么都不写**（名单不变、设置不变）——这是「非法路径不污染设置」那条守卫的落点。
///
/// # Errors
/// 校验不过（见 [`Refusal`]）或名单写盘失败时返回原因。
pub fn add_manual(config: &Path, input: &str) -> Result<(String, RuntimeRow), Refusal> {
    add_manual_with(config, input, &manual::probe_manual)
}

/// 同 [`add_manual`]，但探测函数可注入（测试用：不依赖本机装没装 JDK）。
///
/// # Errors
/// 探测被拒或名单写盘失败时返回原因。
pub fn add_manual_with(
    config: &Path,
    input: &str,
    probe: &dyn Fn(&str) -> Result<JavaRuntime, manual::ManualFailure>,
) -> Result<(String, RuntimeRow), Refusal> {
    let runtime = probe(input.trim()).map_err(failure_refusal)?;
    let binary = runtime.binary.display().to_string();
    let mut paths = manual::load_paths(&manual_file(config));
    let key = discover::path_key(&runtime.binary);
    if paths.iter().any(|known| discover::path_key(known) == key) {
        return Err(Refusal::AddDuplicate);
    }
    paths.push(runtime.binary.clone());
    manual::save_paths(&manual_file(config), &paths).map_err(|_| Refusal::AddWriteFailed)?;
    Ok((
        binary.clone(),
        RuntimeRow {
            binary,
            version: runtime.version.clone().unwrap_or_default(),
            major: runtime.major,
            vendor: runtime.vendor.clone(),
            source: "manual",
        },
    ))
}

/// `core` 的拒绝原因 → 页面原因。
fn failure_refusal(failure: manual::ManualFailure) -> Refusal {
    match failure {
        manual::ManualFailure::Empty => Refusal::AddEmpty,
        manual::ManualFailure::TooLong => Refusal::AddTooLong,
        manual::ManualFailure::Missing => Refusal::AddMissing,
        manual::ManualFailure::NotJava => Refusal::AddNotJava,
        manual::ManualFailure::NoVersion => Refusal::AddNoVersion,
    }
}

/// 设置文件路径（数据根解析不出来时没有）。
#[must_use]
pub fn config_of(bridge: &unlucky_presenter::settings_bridge::SettingsBridge) -> Option<PathBuf> {
    bridge.path().map(Path::to_path_buf)
}
