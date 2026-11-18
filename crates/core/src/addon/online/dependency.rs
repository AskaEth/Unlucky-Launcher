//! 依赖图谱的**形状**：一次解析要什么（[`ResolveRequest`]）、出什么（[`DependencyPlan`]）。
//!
//! 算法在 [`super::resolve`]；本文件只放类型与不变量，于是「两档失败从类型上可分辨」这件事
//! 不必读算法也能核对。
//!
//! # 三份名单，一份都不许丢
//!
//! | 名单 | 含义 | 上层该做什么 |
//! |------|------|--------------|
//! | [`DependencyPlan::nodes`] | 要下的（BFS 顺序、父子关系齐全） | 逐个下载 |
//! | [`DependencyPlan::skipped`] | 目标目录里已经有同名文件 | 提示「已存在，跳过」 |
//! | [`DependencyPlan::failures`] | 装不了的，**每条自带 [`FailureKind`]** | 两档分开说；进可重试名单 |
//!
//! `truncated` 是第四样东西：撞到 [`DEFAULT_MAX_DEPTH`] 而没展开的项目。它既不是成功也不是
//! 失败（站点没拒绝，我们主动停），但**必须**露出来——静默截断会让用户看着「装好了」而缺前置。
//!
//! # 两档为什么不能合成一句
//!
//! 「所选版本/加载器没有匹配项」与「网络异常 / 项目不存在」要用户做的事完全不同：
//! 前者换版本或换加载器，后者检查网络再试。字符串错误只能靠 `contains("网络")` 猜，
//! 一改文案就失效；[`FailureKind`] 把判据放在类型上，叶子成因（404 / 连不上 / 5xx）
//! 另存在 [`DependencyFailure::cause`] 上，允许上层再细分而不用重新判。

use super::model::{VersionFile, VersionQuery};

/// 默认最大深度（根 = 0）。六层足够覆盖真实套娃（前置的前置的前置……），
/// 同时给环与异常链一个硬边界——撞上就记 [`DependencyPlan::truncated`]，**不静默截断**。
pub const DEFAULT_MAX_DEPTH: u8 = 6;

/// 目标目录里已存在的文件（调用方扫描后给；core 不读盘）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PresentFile {
    /// 文件名（可以带路径，比对时会砍掉）
    pub name: String,
}

impl PresentFile {
    /// 记一个已存在的文件名。
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    /// 归一化后的比对键（见 [`present_key`]）。
    #[must_use]
    pub fn key(&self) -> String {
        present_key(&self.name)
    }
}

/// 文件名 → 比对键：只留文件名、小写、去掉 `.disabled` 后缀。
///
/// 为什么带 `.disabled`：本仓用**路径后缀**表示禁用（`mods/x.jar.disabled`，见
/// `core::addon::export`），禁用中的 jar 也是「装过」，再下一份会和它并存。
#[must_use]
pub fn present_key(name: &str) -> String {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .to_ascii_lowercase();
    base.strip_suffix(".disabled").unwrap_or(&base).to_string()
}

/// 一次解析请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveRequest {
    /// 起点（项目 id 或 slug）；同层处理时按项目键字典序，与这里给的顺序无关
    pub roots: Vec<String>,
    /// 目标游戏版本（选版本用；`None` = 不限）
    pub game_version: Option<String>,
    /// 目标加载器（选版本用；`None` = 不限）
    pub loader: Option<String>,
    /// 最大深度（根 = 0，见 [`DEFAULT_MAX_DEPTH`]）
    pub max_depth: u8,
    /// 目标目录里已存在的文件（盘上去重的依据；空 = 目录是空的）
    pub present: Vec<PresentFile>,
}

impl Default for ResolveRequest {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            game_version: None,
            loader: None,
            max_depth: DEFAULT_MAX_DEPTH,
            present: Vec::new(),
        }
    }
}

impl ResolveRequest {
    /// 单根请求（内容库「装进实例」的入口形状）。
    #[must_use]
    pub fn single(project: impl Into<String>) -> Self {
        Self {
            roots: vec![project.into()],
            ..Self::default()
        }
    }

    /// 这个项目的版本请求（过滤条件相同，只换项目）。
    #[must_use]
    pub fn version_query(&self, project: &str) -> VersionQuery {
        VersionQuery {
            project: project.to_string(),
            game_version: self.game_version.clone(),
            loader: self.loader.clone(),
        }
    }
}

/// 计划里的一条「要装什么」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedNode {
    /// 规范项目 id（站点回的 `project_id`；空则用请求时的键）
    pub project: String,
    /// 当初请求它用的键（slug 或 id）——重试要用同一个
    pub requested_as: String,
    /// 版本 id
    pub version_id: String,
    /// 版本名
    pub version_name: String,
    /// 版本号
    pub version_number: String,
    /// 要下载的那个文件（`primary` 优先，见 `Version::primary_file`）
    pub file: VersionFile,
    /// 距根的层数（根 = 0）
    pub depth: u8,
    /// 谁要求装它（规范项目 id，排序去重；根为空 = 用户自己点的）
    pub required_by: Vec<String>,
}

impl PlannedNode {
    /// 是用户点的那个项目吗（根 = 层数 0）。
    ///
    /// 判据是 `depth` 而不是 `required_by.is_empty()`：环里根也会被别人「要求」
    /// （A ⇄ B 时 A 的 `required_by` 里有 B），但它在计划里仍是起点。
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.depth == 0
    }
}

/// 为什么跳过。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 目标目录里已经有同名文件（含 `.disabled`）
    AlreadyPresent,
}

impl SkipReason {
    /// 稳定字符串 id（日志 / DTO 用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AlreadyPresent => "already_present",
        }
    }
}

/// 一条被跳过的（本来要下载的）项目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedNode {
    /// 规范项目 id
    pub project: String,
    /// 请求它用的键
    pub requested_as: String,
    /// 计划里的文件名（在目录里找到了同名的那个）
    pub file_name: String,
    /// 距根的层数
    pub depth: u8,
    /// 谁要求装它
    pub required_by: Vec<String>,
    /// 跳过原因
    pub reason: SkipReason,
}

/// 失败的两档（**类型上就分开，不许拼成一句「下载失败」**）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    /// 站点有这个项目，但所选游戏版本 / 加载器下没有可装的条目
    /// （版本列表为空，或选中的版本没有可下载的主文件）。
    ///
    /// 用户该做的动作是**换版本或换加载器**；原样重试得到的还是这一档。
    Incompatible,
    /// 取不到：没联网 / 项目不存在 / 站点出错 / 响应坏。
    ///
    /// 用户该做的动作是**检查网络后再试**；具体叶子成因保留在
    /// [`DependencyFailure::cause`] 上（`NotFound` 与 `NotConnected` 不能混为一谈）。
    Unreachable,
}

impl FailureKind {
    /// 稳定字符串 id（日志 / 命令行输出用，**不要改名**）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incompatible => "incompatible",
            Self::Unreachable => "unreachable",
        }
    }
}

/// 一条解析失败（进「可重试名单」的就是它们）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyFailure {
    /// 请求它用的键（重试也用它）
    pub requested_as: String,
    /// 当初因为谁而被要求（规范项目 id；根为空）
    pub required_by: Vec<String>,
    /// 距根的层数
    pub depth: u8,
    /// 哪一档
    pub kind: FailureKind,
    /// 叶子成因（只有 [`FailureKind::Unreachable`] 有）
    pub cause: Option<super::transport::OnlineErrorKind>,
    /// 人读说明（站点/引擎原话 + 上下文，不是界面文案）
    pub message: String,
}

/// 声明了 `required`、但**只给了 `version_id`**（没有 `project_id`）的依赖。
///
/// BFS 需要项目 id 才能取版本（`/v2/project/{id}/version`），所以这条既装不了、也不该被
/// 假装没看见：留在计划里，上层才能说「有 N 条前置没法解析」而不是默默少装一个。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedRequired {
    /// 声明它的项目（规范 id）
    pub parent: String,
    /// 站点给的版本 id（`None` = 连版本 id 都没有，只有文件名）
    pub version_id: Option<String>,
    /// 站点给的文件名
    pub file_name: Option<String>,
}

/// 一次 BFS 的完整结果（成功、跳过、失败三份名单，都**不丢**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyPlan {
    /// 这次用的请求（重试要沿用同样的过滤条件与同一份 `present`）
    pub request: ResolveRequest,
    /// 要装的（BFS 顺序：层号小的在前，同层按项目键字典序）
    pub nodes: Vec<PlannedNode>,
    /// 目录里已经有了、不用再下的
    pub skipped: Vec<SkippedNode>,
    /// 装不了的（两档混在同一个列表里，但每条自带 [`FailureKind`]）
    pub failures: Vec<DependencyFailure>,
    /// 撞到 `max_depth` 而没展开的项目键（排序去重）
    pub truncated: Vec<String>,
    /// 有 `required` 但没有项目 id、解析不了的前置（排序去重）
    pub unresolved_required: Vec<UnresolvedRequired>,
}

impl DependencyPlan {
    /// 空计划（`resolve` 从这里开始填）。
    #[must_use]
    pub fn new(request: &ResolveRequest) -> Self {
        Self {
            request: request.clone(),
            nodes: Vec::new(),
            skipped: Vec::new(),
            failures: Vec::new(),
            truncated: Vec::new(),
            unresolved_required: Vec::new(),
        }
    }

    /// 一条失败都没有吗（空循环也算干净）。
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.failures.is_empty()
    }

    /// 某一档的失败（两档分开看时用）。
    pub fn failures_of(&self, kind: FailureKind) -> impl Iterator<Item = &DependencyFailure> {
        self.failures.iter().filter(move |item| item.kind == kind)
    }

    /// 记一条失败——**按 `requested_as` 去重**，父边合并。
    ///
    /// 为什么要有这道去重：同一个项目可能被两条路径要求（A→GONE、B→C→GONE）。若不去重，
    /// 「可只重试这 N 个」会写出 `GONE、GONE`，用户按名单重试等于发两次一样的请求。
    /// 解析阶段与下载阶段的失败都走这里，于是名单里一个项目只出现一次。
    pub fn record_failure(&mut self, failure: DependencyFailure) {
        if let Some(existing) = self
            .failures
            .iter_mut()
            .find(|item| item.requested_as == failure.requested_as)
        {
            existing.required_by.extend(failure.required_by);
            existing.required_by.sort();
            existing.required_by.dedup();
            // 深度取更浅的那条：越靠近根越该先修
            existing.depth = existing.depth.min(failure.depth);
            return;
        }
        self.failures.push(failure);
    }

    /// 「可重试名单」：失败项原样重启一次解析的请求。
    ///
    /// 只含失败项 ⇒ 用户只重试这一部分，**不重下**已经装好的文件（`present` 沿用）。
    /// 两档都在名单里，但每条自带 [`FailureKind`]：不兼容那一档重试前得换版本/加载器，
    /// 换完再调它才有意义。**这不是「后台自动重试」**——本模块只给请求，什么时候发由上层决定。
    ///
    /// 调用方如果刚跑完一轮下载，重试前应**重新扫一遍目录**再填 `present`：这一轮新落盘的
    /// 文件不在旧的 `present` 里，不重扫就会把它们当「还没有」再下一份。
    #[must_use]
    pub fn retry_request(&self) -> ResolveRequest {
        let mut roots: Vec<String> = Vec::new();
        for failure in &self.failures {
            if !roots.contains(&failure.requested_as) {
                roots.push(failure.requested_as.clone());
            }
        }
        ResolveRequest {
            roots,
            ..self.request.clone()
        }
    }

    /// 这个项目为什么在计划里（规范 id → 要求装它的项目）。`None` = 不在计划里。
    #[must_use]
    pub fn required_by(&self, project: &str) -> Option<&[String]> {
        self.nodes
            .iter()
            .find(|node| node.project == project)
            .map(|node| node.required_by.as_slice())
    }
}
