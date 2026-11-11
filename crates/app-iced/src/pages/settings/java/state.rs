//! 「Java」分组的运行期状态（M175）：扫描结论 + 手动名单 + 两个可编辑项。
//!
//! 状态住在 `pages::settings::Groups.java`（[`State`]），由 `App` 持有。三条边界：
//!
//! | 东西 | 住哪 | 为什么 |
//! |---|---|---|
//! | 扫描结论 / 进度 | 本结构（由后台线程的快照 1 拍一拍灌进来） | 用户切走再回来不该重扫 |
//! | 手动加的路径名单 | 本结构 + 一个纯文本文件（`store::manual_file`） | 它是「我加过哪些」的记忆，不是启动策略 |
//! | `java.selected_path` / `default_memory_mb` / `extra_args` | **`config/launcher.json`** | 真值只有一份，读回也读它 |
//!
//! ## 为什么内存草稿要记两层（`memory` 文本 + `saved_memory` 数值）
//!
//! 输入框里的是**文本**（用户可能刚敲了半截「819」），而「现在生效多少」是**落盘的真值**。
//! 两者混成一个字段就会重演「显示的就是生效的」这种假象：改到一半的数字不是生效值。
//! 于是草稿只影响「有未保存改动」与「保存按钮能不能点」，生效值一律来自 `saved_*`（落盘后读回的）。
//!
//! ## 版本门槛从哪来
//!
//! [`Context::required`] 由 **app 层**（`app/update/settings/java.rs`）算好塞进来：它要读实例快照
//! （`requirement_for(mc_version)`），而页面渲染器只拿得到 [`super::States`]。
//! 页面自己不去读盘、也不复制版本表。

pub use super::row::RuntimeRow;

/// M197 起 `Requirement` / `Layer` / `Context` / `Fit` 住在 [`super::context`]（本文件加了三键状态
/// 与下载槽之后越过 400 行硬线），三键的草稿与生效值住在 [`super::prefs`]——这里一并转出去，
/// 调用方仍然只认识 `pages::settings::java::{Context, Layer, Requirement}` 这一组名字。
pub use super::context::{Context, Fit, Layer, Requirement};

use unlucky_i18n::Key;

/// `java.default_memory_mb` 的下限（MB）。
///
/// **必须**等于 `core::setting::keys` 里那个键登记的下限（512）：UI 放行一个 schema 会拒的值，
/// 用户点保存只会得到一条「schema 拒绝」的错。一致性由 `tests::the_memory_bounds_mirror_the_schema` 守。
pub const MIN_MEMORY_MB: u32 = 512;
/// `java.default_memory_mb` 的上限（MB）。
pub const MAX_MEMORY_MB: u32 = 65536;
/// `java.default_memory_mb` 的出厂默认值（MB）：与 schema 的默认值同源，
/// 一致性由 `tests::the_memory_bounds_mirror_the_schema` 守。
pub const DEFAULT_MEMORY_MB: u32 = 2048;
/// `java.extra_args` 的长度上限（字符）：**必须**等于 schema 登记的 `Text{max_len: 1024}`。
///
/// 为什么页面要自己再判一次（M199）：schema 那一道是在 `store.set` 里拒的，用户得到的是
/// `Notice::WriteFailed`（「写不进去」）+ 一句带具体长度的中文只进 stderr——粘了一大段参数的
/// 人不知道是长度问题。一致性由 `tests::the_jvm_length_mirrors_the_schema` 守。
pub const MAX_JVM_ARGS: usize = 1024;

/// 扫描进度（`None` = 没在扫）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    /// 已探测的候选数
    pub done: usize,
    /// 候选总数
    pub total: usize,
}

/// 保存 / 操作的回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// 已落盘并读回
    Saved,
    /// 本地校验拒绝（没写盘）
    Refused(Refusal),
    /// 设置文件层面拒绝：只读 / 不可信 / 没有改动（i18n key，不拼字符串）
    Unwritable(Unwritable),
    /// 写盘失败（schema / 磁盘）：core 的 `Display` 是中文，不能直出给英文界面，
    /// 所以这里只带一个页面自己的 key；原始句子进 stderr 日志。
    WriteFailed,
    /// 没有可写文件（内存态运行）
    NoStorage,
    /// 默认 JDK 已写进 `java.selected_path`
    DefaultSet(String),
    /// 手动路径已加入名单
    Added(String),
    /// 三个设置键已落盘并读回（M197）
    PrefsSaved,
}

/// 设置文件层面拒绝的三档（`SettingStore::save` 的 `Skipped` 三因）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unwritable {
    /// 没有任何脏键（我们以为改了、其实没有）
    NotDirty,
    /// 文件只读（被更新版本的启动器写过 / 被标记不可信）
    ReadOnly,
    /// 文件不可信（加载时就降级成只读）
    Unreadable,
}

impl Unwritable {
    /// 文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::NotDirty => Key::SettingsJavaSkipNotDirty,
            Self::ReadOnly => Key::SettingsJavaSkipReadOnly,
            Self::Unreadable => Key::SettingsJavaSkipUnreadable,
        }
    }
}

/// 本地校验的拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// 内存草稿为空
    MemoryEmpty,
    /// 内存草稿不是整数
    MemoryNotNumber,
    /// 内存越界
    MemoryRange,
    /// JVM 参数里的引号没配成对
    JvmQuotes,
    /// `java.extra_args` 超过 schema 登记的长度上限（1024）
    JvmTooLong,
    /// 手动路径为空
    AddEmpty,
    /// 手动路径在盘上找不到
    AddMissing,
    /// 有这个东西，但不是 java 可执行文件
    AddNotJava,
    /// 跑不出 `-version`
    AddNoVersion,
    /// 路径太长（粘了一整段日志）
    AddTooLong,
    /// 名单文件写不进去（磁盘 / 权限）
    AddWriteFailed,
    /// 已经加过了
    AddDuplicate,
    /// `java.installRoot` 超过 schema 登记的长度上限（512）
    InstallRootTooLong,
}

impl Refusal {
    /// 文案 key。
    #[must_use]
    pub const fn label(self) -> Key {
        match self {
            Self::MemoryEmpty => Key::SettingsJavaReasonMemoryEmpty,
            Self::MemoryNotNumber => Key::SettingsJavaReasonMemoryNotNumber,
            Self::MemoryRange => Key::SettingsJavaReasonMemoryRange,
            Self::JvmQuotes => Key::SettingsJavaReasonJvmQuotes,
            Self::JvmTooLong => Key::SettingsJavaReasonJvmTooLong,
            Self::AddEmpty => Key::SettingsJavaReasonAddEmpty,
            Self::AddMissing => Key::SettingsJavaReasonAddMissing,
            Self::AddNotJava => Key::SettingsJavaReasonAddNotJava,
            Self::AddNoVersion => Key::SettingsJavaReasonAddNoVersion,
            Self::AddTooLong => Key::SettingsJavaReasonAddTooLong,
            Self::AddWriteFailed => Key::SettingsJavaReasonAddWriteFailed,
            Self::AddDuplicate => Key::SettingsJavaReasonAddDuplicate,
            Self::InstallRootTooLong => Key::SettingsJavaReasonInstallRootTooLong,
        }
    }
}

/// 「Java」分组的运行期状态。
#[derive(Debug)]
pub struct State {
    /// 扫到的运行时（顺序 = 大版本升序，探测不到主版本的排最后）
    pub runtimes: Vec<RuntimeRow>,
    /// 至少完整扫过一次（区分「未检测到」与「还没扫」）
    pub scanned: bool,
    /// 扫描进度
    pub scanning: Option<Progress>,
    /// 上一次扫描被用户取消（界面上要说一句，否则「列表没变」会被当成没扫）
    pub cancelled: bool,
    /// 扫描状态槽（快照 + 取消位；线程与界面共享同一份）
    pub scan: super::store::ScanSlot,
    /// 用户手动加的路径（原样，持久化在 `manual_file`）
    pub manual: Vec<String>,
    /// `java.selected_path` 的真值（空 = 没指定，启动时自动挑）
    pub selected: String,
    /// 内存草稿（MB 文本）
    pub memory: String,
    /// 落盘的内存真值（MB）
    pub saved_memory: u32,
    /// JVM 参数草稿
    pub jvm: String,
    /// 落盘的 JVM 参数真值
    pub saved_jvm: String,
    /// 手动路径输入框
    pub add_draft: String,
    /// 有未保存的改动
    pub dirty: bool,
    /// 这一次运行有没有可写的设置文件
    pub storage: bool,
    /// 上一次操作的回执
    pub notice: Option<Notice>,
    /// 当前实例的上下文（版本门槛 / 生效内存层级）
    pub context: Context,
    /// 三个设置键的草稿 + 落盘真值（M197：`installRoot` / `vendorPriority` / `auto_install`）
    pub prefs: super::prefs::Prefs,
    /// 上述三键的**生效值 + 来源层**（app 层每次 hydrate 重算）
    pub effective: super::prefs::Effective,
    /// 「下载一个 JDK」的作业槽（后台线程写、界面读）
    pub downloads: super::downloads::Slot,
    /// 上一次「下载按钮被拒」的原因（点了没反应是禁止的）
    pub download_notice: Option<super::downloads::Refusal>,
    /// 已经处理过的那份下载快照（用来判「刚成功」——装完要重扫一次并把新 JDK 收进来）
    pub download_seen: super::downloads::Report,
}

impl Default for State {
    fn default() -> Self {
        Self {
            runtimes: Vec::new(),
            scanned: false,
            scanning: None,
            cancelled: false,
            scan: super::store::ScanSlot::default(),
            manual: Vec::new(),
            selected: String::new(),
            memory: String::new(),
            saved_memory: DEFAULT_MEMORY_MB,
            jvm: String::new(),
            saved_jvm: String::new(),
            add_draft: String::new(),
            dirty: false,
            storage: false,
            notice: None,
            context: Context::default(),
            prefs: super::prefs::Prefs::default(),
            effective: super::prefs::Effective::default(),
            downloads: super::downloads::Slot::default(),
            download_notice: None,
            download_seen: super::downloads::Report::default(),
        }
    }
}

/// 从 `launcher.json` 读回来的三键真值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// `java.selected_path`
    pub selected: String,
    /// `java.default_memory_mb`
    pub memory_mb: u32,
    /// `java.extra_args`
    pub jvm: String,
}

impl State {
    /// 用真值刷新（**脏草稿不覆盖**，否则外部一改文件就冲掉用户正在输入的内容）。
    ///
    /// 返回是否改了「生效值」那一半（调用方据此决定要不要重建视图模型）。
    pub fn hydrate(&mut self, snapshot: Snapshot) -> bool {
        self.storage = true;
        let Snapshot {
            selected,
            memory_mb,
            jvm,
        } = snapshot;
        let changed =
            self.selected != selected || self.saved_memory != memory_mb || self.saved_jvm != jvm;
        self.selected = selected;
        self.saved_memory = memory_mb;
        if !self.dirty {
            self.memory = memory_mb.to_string();
            self.jvm = jvm.clone();
        }
        self.saved_jvm = jvm;
        changed
    }

    /// 灌一次扫描结论（后台线程的快照）。返回是否变了。
    pub fn apply_scan(&mut self, report: &super::store::ScanReport) -> bool {
        let scanning = report.running.then_some(Progress {
            done: report.done,
            total: report.total,
        });
        let changed = self.scanning != scanning
            || self.cancelled != report.cancelled
            || (report.finished && !self.scanned)
            || (report.finished && self.runtimes != report.runtimes);
        self.scanning = scanning;
        self.cancelled = report.cancelled;
        if report.finished {
            self.scanned = true;
            self.runtimes = report.runtimes.clone();
        }
        changed
    }

    /// 某个大版本够不够当前实例用（`None` = 没有实例 / 那个版本探测不出版本号）。
    ///
    /// 返回的是**带方向**的三态：老版本 MC 只能用 Java 8，「Java 21 太高」与「Java 8 太低」
    /// 在界面上是两句话（M197 的第三条任务不许静默失败）。
    #[must_use]
    pub fn fit(&self, major: Option<u32>) -> Option<Fit> {
        let (Some(required), Some(major)) = (self.context.required.as_ref(), major) else {
            return None;
        };
        Some(required.fit(major))
    }

    /// 保存前的本地校验（过不了就不写盘）。
    ///
    /// # Errors
    /// 内存不是整数 / 越界，JVM 参数引号没配成对，或 JVM 参数超过 schema 登记的长度上限时
    /// 返回原因。
    pub fn validate(&self) -> Result<u32, Refusal> {
        let text = self.memory.trim();
        if text.is_empty() {
            return Err(Refusal::MemoryEmpty);
        }
        let Ok(mb) = text.parse::<u32>() else {
            return Err(Refusal::MemoryNotNumber);
        };
        if !(MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&mb) {
            return Err(Refusal::MemoryRange);
        }
        if unbalanced_quotes(&self.jvm) {
            return Err(Refusal::JvmQuotes);
        }
        // 与 schema 同一个口径（`spec.rs` 的 `ValueKind::Text` 数 `chars()`，写盘前 `trim()`）。
        if self.jvm.trim().chars().count() > MAX_JVM_ARGS {
            return Err(Refusal::JvmTooLong);
        }
        Ok(mb)
    }

    /// 界面上「现在生效多少」的那句话用哪一层（没有上下文时按全局）。
    #[must_use]
    pub fn layer(&self) -> Layer {
        self.context.layer.clone().unwrap_or(Layer::Global)
    }

    /// 生效的（落盘的）发行版优先级，给「这一档能由哪几家承接」用。
    ///
    /// 用**落盘值**而不是草稿：列表边上那句「发行版：…」描述的是真跑起来会试的那条链；
    /// 草稿只影响「保存之后」。
    #[must_use]
    pub fn priority_vendors(&self) -> Vec<unlucky_core::game::java::JdkVendor> {
        self.prefs
            .saved_priority
            .iter()
            .filter_map(|id| unlucky_core::game::java::JdkVendor::parse(id))
            .collect()
    }

    /// 现在能不能点保存。
    #[must_use]
    pub fn can_save(&self) -> bool {
        self.storage && self.dirty
    }

    /// 当前默认运行时那一行（`selected` 指向的）。
    #[must_use]
    pub fn default_row(&self) -> Option<&RuntimeRow> {
        self.runtimes
            .iter()
            .find(|row| same_path(&row.binary, &self.selected))
    }

    /// 取证开关 `ULL_JAVA_STATE`（只给 `--screenshot` 用；三档的实现在 `java/seed.rs`）。
    ///
    /// 三档：`empty`（未检测到）/ `two`（两个 JDK 并选中其一）/ `editing`（改了内存与 JVM 参数、
    /// 还没保存）。与 `ULL_AI_STATE` / `ULL_UPDATE_STATE` 同一族，只影响这一次进程。
    #[must_use]
    pub fn screenshot_seed(raw: &str) -> Option<Self> {
        super::seed::build(raw)
    }
}

/// 两条路径是不是同一个（`/` 与 `\` 视为同一分隔符；Windows 折大小写）。
#[must_use]
pub fn same_path(left: &str, right: &str) -> bool {
    let key = |text: &str| {
        let text = text.replace('\\', "/");
        if cfg!(windows) {
            text.to_ascii_lowercase()
        } else {
            text
        }
    };
    !left.trim().is_empty() && key(left.trim()) == key(right.trim())
}

/// 引号（`"` 与 `'`）数量是不是奇数——奇数就意味着用户漏了一半。
#[must_use]
pub fn unbalanced_quotes(text: &str) -> bool {
    text.matches('"').count() % 2 == 1 || text.matches('\'').count() % 2 == 1
}
