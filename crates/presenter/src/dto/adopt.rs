//! 「导入已有版本」的 IPC 面（M223 GUIADOPT）：扫描 → 预检 → 确认三段。
//!
//! 能力本身在 core（`unlucky_core::game::adopt`，M221 的契约见
//! `DevDocs/SubAgentsTalk/adoptcore-契约.md`）；本文件只有**可序列化的视图**，
//! 一个 `unlucky_core::*` 类型都不出现（AGENTS §6 的跨边界纪律）。
//!
//! ## 三段：前两段零写盘，第三段只写 ULL 自己的配置
//!
//! | 段 | 指令 | 回程 | 写盘 |
//! |---|---|---|---|
//! | 扫描 | [`AdoptCommand::Scan`] | [`AdoptEvent::Scan`] | 零（不建目录、不改 mtime） |
//! | 预检 | [`AdoptCommand::Preview`] | [`AdoptEvent::Planned`] / [`AdoptEvent::Failed`] | 零 |
//! | 确认 | [`AdoptCommand::Confirm`] | [`AdoptEvent::Done`] / [`AdoptEvent::Failed`] | 实例设置 + `profiles.json`（+ 缺省运行目录） |
//!
//! ## 三条硬纪律（契约 §5 的 1/2/3）
//!
//! 1. **运行目录只认回程**：[`AdoptPlanView::run_dir`] / [`AdoptDoneView::run_dir`] 是唯一来源。
//!    缺省档会带 `-2`/`-3` 后缀消歧（两个不同展示名可能落到同一个物理目录），**只有 core 知道
//!    最后用了哪个**；页面拿展示名自己派生一次就是错的（会与实例真正启动时用的目录分叉）。
//! 2. **后果话术只有一份**：`Shared` / `VersionDir` 的后果**不在本文件里另写**一句，
//!    由预检把 core `target_notes()` 的原文放进 [`AdoptPlanView::notes`]——于是界面上的
//!    「mods/saves/config 会落进别人的 `.minecraft`」与 CLI 打的是同一句。
//!    想显示后果就必须先预检（预检零写盘，所以这不是成本）。
//! 3. **问题逐条**：core 的结构化问题（[`AdoptProblemView`]，带稳定 `code`）与整次失败
//!    （[`AdoptFailureView`]）都原样搬过来，**不折叠成一句「导入失败」**。
//!
//! ## 缺省档为什么是 `InstanceDir` 而不是 `isolated`
//!
//! 全局缺省 `isolated` 的运行目录就是 `versions/<id>/`——那是**别人的版本目录**。沿用它会让第一次
//! 启动就在别人的 `.minecraft` 里长出 `mods/` `saves/` `logs/`。所以 [`AdoptTargetView`] 的
//! 缺省是 `InstanceDir`（ULL 数据区新建独立目录），另两档必须**明确点选**。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// app → host 的导入意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptCommand {
    /// 扫当前游戏根的 `versions/`（只读；顺带得到每个版本能不能登记、已登记给谁）
    Scan,
    /// **预检**：按请求算落点与后果（零写盘）。页面拿回 [`AdoptEvent::Planned`] 才画确认按钮。
    Preview(Box<AdoptRequestView>),
    /// 确认登记（唯一写盘入口）。
    Confirm(Box<AdoptRequestView>),
    /// 关掉面板（扫描结果 / 预检 / 结果卡都收起来）
    Dismiss,
}

/// host → app 的回程事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptEvent {
    /// 扫描结果（`versions/` 里的全部条目，逐项带问题）
    Scan(Box<AdoptScanView>),
    /// 预检结果（落点 + 后果原文 + 将写的展示名）
    Planned(Box<AdoptPlanView>),
    /// 失败：`code` 稳定、`message` 人读、`problems` 逐条（可能为空）
    Failed(Box<AdoptFailureView>),
    /// 登记完成
    Done(Box<AdoptDoneView>),
    /// 面板已关（状态复位）
    Cleared,
}

/// 登记后的**运行目录从哪来**（隔离档由它决定）。
///
/// GUI 只开三档：[`Self::InstanceDir`]（缺省）/[`Self::Shared`] / [`Self::VersionDir`]。
/// `custom`（用户手输绝对路径）留在 CLI：GUI 要它就得配目录选择器，不在本 mission 的交付面里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AdoptTargetView {
    /// 缺省：ULL 数据区新建独立运行目录（`custom` 档，与别人的 `.minecraft` 隔离）
    #[default]
    InstanceDir,
    /// 共用游戏根（别人的 `.minecraft`）——必须明确点选
    Shared,
    /// 版本目录 `versions/<id>/`（`isolated` 档）——会在别人的版本目录里长玩家数据
    VersionDir,
}

impl AdoptTargetView {
    /// 三档的声明顺序（= 面板上的先后，缺省在最前）。
    pub const ALL: [AdoptTargetView; 3] = [
        AdoptTargetView::InstanceDir,
        AdoptTargetView::Shared,
        AdoptTargetView::VersionDir,
    ];

    /// 稳定 slug（与 core 的 `AdoptTarget::as_str` 同词表；`instance_dir` 不进 CLI 词表）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::InstanceDir => "instance_dir",
            Self::Shared => "shared",
            Self::VersionDir => "version_dir",
        }
    }

    /// 从 slug 反解。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|target| target.slug() == slug)
    }

    /// 这一档会不会把玩家数据写进**别人的**目录（界面据此加警示标记）。
    #[must_use]
    pub const fn touches_foreign_dir(self) -> bool {
        matches!(self, Self::Shared | Self::VersionDir)
    }
}

/// 生效的隔离档（`core::game::install::instance::Isolation` 的视图形态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptIsolationView {
    /// `versions/<id>/` 当运行目录
    Isolated,
    /// 游戏根当运行目录
    Shared,
    /// 指定/新建目录当运行目录（缺省档就是这一档）
    Custom,
}

impl AdoptIsolationView {
    /// 稳定 slug（与 core 的 `Isolation::as_str` 同一套字面量）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Isolated => "isolated",
            Self::Shared => "shared",
            Self::Custom => "custom",
        }
    }

    /// 从 slug 反解（未知值返回 `None`，界面照原样上屏而不是猜）。
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "isolated" => Some(Self::Isolated),
            "shared" => Some(Self::Shared),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// 一次预检/登记的请求（页面表单 → host）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptRequestView {
    /// 要接进来的版本 id（= `versions/<id>/` 的目录名）
    pub version_id: String,
    /// 展示名（`None` / 空串 = 用版本 id）
    pub name: Option<String>,
    /// 运行目录选择（缺省 [`AdoptTargetView::InstanceDir`]）
    pub target: AdoptTargetView,
}

impl AdoptRequestView {
    /// 最小请求（缺省名 + 缺省运行目录）。
    #[must_use]
    pub fn new(version_id: impl Into<String>) -> Self {
        Self {
            version_id: version_id.into(),
            name: None,
            target: AdoptTargetView::default(),
        }
    }
}

/// 一条**结构化**的不合格原因（`code` 稳定，`message` 人读）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptProblemView {
    /// 稳定机读码（`manifest_missing` / `missing_parent` / `outside_game_root` …）
    pub code: String,
    /// 人读原因（core 的 `Display` 原文：带路径、行列号、缺哪个父版本）
    pub message: String,
}

/// 盘上的一个候选版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptCandidateView {
    /// 版本 id（= 目录名，登记后写进 `profiles.json`）
    pub version_id: String,
    /// 客户端 jar 在不在（只报事实，不校验内容）
    pub jar_present: bool,
    /// `inheritsFrom` 链（自身在前、最祖先在后）
    pub inherits: Vec<String>,
    /// 已经绑定这个版本的实例名（空 = 还没登记过）
    pub registered: Vec<String>,
    /// 问题清单（空 = 可登记）
    pub problems: Vec<AdoptProblemView>,
}

impl AdoptCandidateView {
    /// 能不能登记（没有任何问题）。
    #[must_use]
    pub fn adoptable(&self) -> bool {
        self.problems.is_empty()
    }

    /// 已登记过（再登记会得到第二个实例，共用同一份版本文件）。
    #[must_use]
    pub fn is_registered(&self) -> bool {
        !self.registered.is_empty()
    }
}

/// 一次扫描的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptScanView {
    /// 被扫的目录（`<游戏根>/versions`）
    pub versions_dir: PathBuf,
    /// 候选（按版本 id 字典序）
    pub candidates: Vec<AdoptCandidateView>,
}

impl AdoptScanView {
    /// 候选条数。
    #[must_use]
    pub fn total(&self) -> usize {
        self.candidates.len()
    }

    /// 可登记的条数。
    #[must_use]
    pub fn adoptable(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.adoptable())
            .count()
    }

    /// `versions/` 存在但一个候选都没有（新数据根，或只剩别的启动器的空目录）。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// 预检结果：落点 + 后果原文 + 将写的展示名。**零写盘**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptPlanView {
    /// 版本 id
    pub version_id: String,
    /// 将写进档案的展示名（空名已回落到版本 id）
    pub name: String,
    /// 运行目录选择
    pub target: AdoptTargetView,
    /// 生效隔离档
    pub isolation: AdoptIsolationView,
    /// **生效运行目录**（可能是顺延后的 `-2`/`-3`；页面不许自己派生）
    pub run_dir: PathBuf,
    /// 人读提示（含 `target_notes()` 的原文：共享档/版本目录档会把玩家数据写进别人的目录）
    pub notes: Vec<String>,
}

/// 登记完成。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptDoneView {
    /// 实例 id（稳定 UUID）
    pub instance_id: String,
    /// 展示名
    pub name: String,
    /// 绑定的版本 id
    pub version_id: String,
    /// 生效隔离档
    pub isolation: AdoptIsolationView,
    /// 生效运行目录（**唯一来源**：启动时用的就是它）
    pub run_dir: PathBuf,
    /// 是否新建了缺省运行目录（`false` = 目录已存在但为空、直接复用）
    pub dir_created: bool,
    /// 人读提示（与预检同一份）
    pub notes: Vec<String>,
}

/// 一次失败（整次调用，不是单条候选问题）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdoptFailureView {
    /// 稳定机读码（`adopt_not_adoptable` / `adopt_duplicate_name` / `adopt_target_not_empty` …）
    pub code: String,
    /// 人读原因（core 的 `Display` 原文）
    pub message: String,
    /// 候选问题清单（`adopt_not_adoptable` 时逐条给出；其它失败为空）
    pub problems: Vec<AdoptProblemView>,
}

/// 向导的**事件态**（`AppState::adopt` 的全部内容）。
///
/// 只有从 [`AdoptEvent`] 来的事实：扫描结果、预检结果、失败、完成。页面的表单字段
/// （选中的版本、展示名输入、目标选择、面板开没开）属于 UI 本地态，**不在这里**——
/// 状态只有一个写入点，才不会有「页面显示的落点和真正写盘的落点不是一回事」。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AdoptState {
    /// 最近一次扫描（`None` = 还没扫过 / 已被新一轮顶掉）
    pub scan: Option<AdoptScanView>,
    /// 最近一次预检（`None` = 还没预检 / 已过期）
    pub plan: Option<AdoptPlanView>,
    /// 最近一次失败
    pub failure: Option<AdoptFailureView>,
    /// 最近一次完成的登记
    pub done: Option<AdoptDoneView>,
}

impl AdoptState {
    /// 事件 → 状态（**唯一**写入点，与 `PackState::apply` 同一条纪律）。
    pub fn apply(&mut self, event: AdoptEvent) {
        match event {
            AdoptEvent::Scan(scan) => {
                // 新一轮扫描：上一轮的预检/失败/完成都作废（候选集合可能已经变了）
                self.scan = Some(*scan);
                self.plan = None;
                self.failure = None;
                self.done = None;
            }
            AdoptEvent::Planned(plan) => {
                self.plan = Some(*plan);
                self.failure = None;
                self.done = None;
            }
            AdoptEvent::Failed(failure) => {
                // 预检失败时上一份预检必须消失：留着会让确认按钮继续亮着
                self.plan = None;
                self.failure = Some(*failure);
            }
            AdoptEvent::Done(done) => {
                // 登记成功：预检与失败都过期（这一轮已经有结论）
                self.plan = None;
                self.failure = None;
                self.done = Some(*done);
            }
            AdoptEvent::Cleared => {
                self.scan = None;
                self.plan = None;
                self.failure = None;
                self.done = None;
            }
        }
    }

    /// 面板此刻该画的那一段（扫描 / 选版本 / 预检 / 结果）。
    ///
    /// 优先级：完成 > 失败 > 预检 > 扫描列表。写成一个方法而不是让页面各判一遍，
    /// 是为了「同一时刻只画一段」在单测里可断言。
    #[must_use]
    pub fn stage(&self) -> AdoptStage {
        if self.done.is_some() {
            return AdoptStage::Done;
        }
        if self.failure.is_some() {
            return AdoptStage::Failed;
        }
        if self.plan.is_some() {
            return AdoptStage::Preview;
        }
        AdoptStage::Pick
    }
}

/// 面板当前画哪一段（[`AdoptState::stage`] 的产物）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdoptStage {
    /// 扫出来的候选列表（选版本）
    Pick,
    /// 预检结果（落点 + 后果原文 + 确认按钮）
    Preview,
    /// 失败（原因逐条）
    Failed,
    /// 完成
    Done,
}

impl AdoptStage {
    /// 稳定 slug（进日志/测试断言用）。
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Pick => "pick",
            Self::Preview => "preview",
            Self::Failed => "failed",
            Self::Done => "done",
        }
    }
}

#[cfg(test)]
mod tests;
