//! 阶段与进度：**把导入进度映射成下载页那块面板的入参**（不另画一套进度 UI）。
//!
//! ## 任务书第 5 条怎么落
//!
//! 「沿用既有安装面板组件与状态机，禁止再写第二套进度 UI」——
//!
//! 1. 导入的 `ImportStageView` / 计数先映射成面板的入参形状（[`panel_progress`] →
//!    `InstallProgressView` → [`InstallView`]）；
//! 2. 画法交给 `pages::download::install_panel`（同一个函数、同一套几何，进度条 / 计数 /
//!    取消按钮都归它）；
//! 3. 任务书要的「解包 / overrides / 补文件」三档由**一行步骤说明**（[`stage_line`]）承载。
//!
//! **已知重叠（如实写，别再说成修好了）**：面板自带的阶段词是**安装**语义
//! （`Resolving version` / `Downloading files` / `Building the version` / `Registering instance`），
//! 而 `install_panel_model` 永远会填它（M109 合流 M105 后 `InstallView` 不再对外导出，
//! 没法从外面塞一个自定义标签或清空）。所以屏幕上同时出现「Step 2/5 · Unpacking overrides」
//! 与「Building the version」：**粗/细两层，不是两句矛盾的话**，但 `Extract`/`Commit` 都映射到
//! `Installing`，那一档的用词偏「安装」。
//!
//! 要彻底解决得让面板接受调用方给的阶段词（已报 finding：把安装面板提成共享组件）。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{
    ImportProgressView, ImportStageView, InstallProgressView, InstallStage,
};

use crate::pages::download::Download;

/// 抛进面板的进度行。
#[derive(Debug, Clone, PartialEq)]
pub struct Running {
    /// 包名（面板标题那一行）
    pub pack: String,
    /// 细档位说明（`步骤 2/5 · 解包覆盖内容`）
    pub stage_line: String,
    /// 交给下载页那块面板的进度（**唯一**的进度形状来源；`None` = 面板无话可说）
    pub panel: Option<Download>,
}

/// 细档位说明（`步骤 2/5 · 解包覆盖内容`）。
#[must_use]
pub fn stage_line(stage: ImportStageView, catalog: Catalog) -> String {
    let total = ImportStageView::ALL.len();
    let index = ImportStageView::ALL
        .iter()
        .position(|item| *item == stage)
        .map_or(1, |at| at + 1);
    catalog.render(
        Key::PackStageLine,
        &[
            ("index", &index.to_string()),
            ("total", &total.to_string()),
            ("stage", catalog.text(stage_key(stage))),
        ],
    )
}

/// 阶段 → 文案 key。
#[must_use]
pub const fn stage_key(stage: ImportStageView) -> Key {
    match stage {
        ImportStageView::Probe => Key::PackStageProbe,
        ImportStageView::Extract => Key::PackStageExtract,
        ImportStageView::Fetch => Key::PackStageFetch,
        ImportStageView::Commit => Key::PackStageCommit,
        ImportStageView::Register => Key::PackStageRegister,
    }
}

/// 导入阶段 → 安装面板的粗阶段（**唯一映射处**）。
///
/// `Extract` / `Commit` 都落到 `Installing`：对那块面板而言它们都是「在写盘」，区别由
/// [`stage_line`] 说给用户。
#[must_use]
pub const fn panel_stage(stage: ImportStageView) -> InstallStage {
    match stage {
        ImportStageView::Probe => InstallStage::Planning,
        ImportStageView::Extract | ImportStageView::Commit => InstallStage::Installing,
        ImportStageView::Fetch => InstallStage::Downloading,
        ImportStageView::Register => InstallStage::Registering,
    }
}

/// 导入进度 → 下载页面板的入参。
///
/// `version` / `instance` 都填包名：面板第一行显示的就是「这次在装什么」，
/// 而导入的对象是包，不是某个版本 id。
#[must_use]
pub fn panel_progress(progress: &ImportProgressView) -> InstallProgressView {
    InstallProgressView {
        version: progress.pack.clone(),
        instance: progress.pack.clone(),
        loader: String::new(),
        stage: panel_stage(progress.stage),
        files_done: u64::from(progress.done),
        files_total: u64::from(progress.total),
        bytes_done: 0,
        bytes_total: 0,
        speed_bps: 0,
        eta_secs: 0,
        // 整合包导入的失败不产生内容库分类（它有自己的报告视图）。
        failure_kind: None,
        // 导入没有「只重试这些」这条路径（重试整个导入由导入区自己管）。
        retry: Vec::new(),
        message: progress.current.clone().unwrap_or_default(),
    }
}

/// 进度事件 → 面板要的视图模型。
///
/// 面板入参走 `pages::download::install_panel_model`（M109 合流 M105 后提供的**唯一**入口）：
/// 它把 `InstallProgressView` 转成面板要的 `Download`，终态（完成/取消）返回 `None`。
/// 导入的五个阶段都不是终态，所以正常路径下一定是 `Some`；`None` 只做防御（面板没了就只留步骤行）。
///
/// 面板自带那个粗阶段词是**安装**语义（`Resolving version` / `Downloading files` /
/// `Building the version` / `Registering instance`），所以细档位由 [`stage_line`] 说，
/// 二者是「粗/细」关系而不是两套说法——[`panel_stage`] 只挑语义最近的那一档。
pub(super) fn running_of(progress: &ImportProgressView, catalog: Catalog) -> Running {
    let panel = panel_progress(progress);
    Running {
        pack: progress.pack.clone(),
        stage_line: stage_line(progress.stage, catalog),
        panel: crate::pages::download::install_panel_model(&panel, catalog),
    }
}
