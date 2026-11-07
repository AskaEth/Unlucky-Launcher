//! 导入/导出区的视图模型：**presenter 快照 + 页面选择 → 要画的东西**。
//!
//! ## 分层（与内容库页、下载页同一条）
//!
//! | 数据源 | 谁转 | 用途 |
//! |---|---|---|
//! | `AppState::import` / `AppState::export` | [`Pack::of`] | 产品运行时的真页面 |
//! | [`super::demo`] 的固定夹具 | 同上 | `pages_demo` 出图与单测 |
//!
//! 页面自己**不留**业务状态：进度、报告、预检都在 presenter（`ImportState` / `ExportState`）。
//! [`PackPageState`] 里只有「用户在这一屏上点了什么」——路径草稿、目标实例、格式、包名、
//! 展开的那条冲突。
//!
//! ## 文件分工（守 400 行）
//!
//! | 文件 | 职责 |
//! |---|---|
//! | 本文件 | 整块 [`Pack`] + 页内选择 [`PackPageState`] + 探测诊断 [`Probe`] |
//! | [`progress`] | 阶段 / 进度 → 下载页面板的入参（**唯一**的进度形状来源） |
//! | [`report`] | 导入结果：三态 + 容忍域逐条 + 致命块 |
//! | [`export`] | 导出：格式选择 + 预检（不写盘） |
//!
//! ## 容忍 / 致命在模型层就分开
//!
//! [`report::Report::missing`] 是「搞不到某个文件」（实例可用），[`report::Report::fatal`]
//! 是「已整包回滚」。两者字段类型不同、画的时候颜色与图标也不同，不可能混成一个列表。

mod export;
mod progress;
mod report;

pub use export::{format_label, request_of as request_export, Export, Preview};
pub use progress::Running;
pub use report::{ConflictRow, Report, Row};

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{ExportState, ImportState, InstanceId, PackFormatView, ProbeView};

use crate::pages::content::Target;

/// 页面自留的选择（所有者在 `App::pack`，重建快照不该把它们冲掉）。
#[derive(Debug, Clone, PartialEq)]
pub struct PackPageState {
    /// 路径草稿（拖入或手输；提交后才进请求）
    pub draft: String,
    /// 有文件悬在窗口上方（拖拽高亮）
    pub hover: bool,
    /// 目标实例（`None` = 按包名新建）
    pub instance: Option<InstanceId>,
    /// 新实例名
    pub instance_name: String,
    /// 导出格式
    pub format: PackFormatView,
    /// 导出包名
    pub export_name: String,
    /// 导出包版本
    pub export_version: String,
    /// 落盘路径草稿（空 = 让 host 给缺省）
    pub output: String,
    /// 导出面板开着吗
    pub export_open: bool,
    /// 展开的那条冲突路径
    pub expanded: Option<String>,
}

impl Default for PackPageState {
    fn default() -> Self {
        Self {
            draft: String::new(),
            hover: false,
            instance: None,
            instance_name: String::new(),
            // 导出的首选是 Modrinth：CF 的 `files[]` 只能写能证明 `projectID`/`fileID` 的项，
            // 而离线目录证明不了任何一条 ⇒ 全内联 + 一堆「表达不了」的注记。mrpack 才是主路。
            format: PackFormatView::Modrinth,
            export_name: String::new(),
            export_version: "1.0.0".to_string(),
            output: String::new(),
            export_open: false,
            expanded: None,
        }
    }
}

/// 一个实例候选（导入目标与导出源共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackTarget {
    /// 实例 id
    pub id: InstanceId,
    /// 一行显示名（`Survival · 1.20.1 · Fabric`）
    pub label: String,
}

/// 探测失败时的具名诊断块。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    /// 最像哪个格式（`None` = 三个都不像）
    pub looks_like: Option<PackFormatView>,
    /// 「看起来像 X，但缺 Y」（或「没有任何已知格式认领」）
    pub lead: String,
    /// 包里实际看到的标记（host 已截断，只给代表性几条）
    pub saw: Vec<String>,
    /// 兜底说明（core 原文：试过哪些格式）
    pub detail: String,
}

/// 整块导入/导出区要画的东西。
#[derive(Debug, Clone, PartialEq)]
pub struct Pack {
    /// 拖拽高亮
    pub hover: bool,
    /// 路径草稿
    pub draft: String,
    /// 入口说明（拖什么）
    pub drop_hint: &'static str,
    /// 没有系统文件框的原因（如实说，不许装作有）
    pub no_dialog_note: &'static str,
    /// 目标实例候选
    pub targets: Vec<PackTarget>,
    /// 选中的目标实例
    pub instance: Option<InstanceId>,
    /// 新实例名
    pub instance_name: String,
    /// 正在导入（有它就画那块面板）
    pub running: Option<Running>,
    /// 探测失败诊断
    pub probe: Option<Probe>,
    /// 用户取消过这次导入：面板收回但**后台还在跑**（core 没有取消令牌），
    /// 所以要把这句话留在界面上，别让用户以为「没建成」（复审 r1 P1-2）。
    pub cancelled: bool,
    /// 取消提示的正文
    pub cancelled_note: &'static str,
    /// 导入结果
    pub report: Option<Report>,
    /// 导出块
    pub export: Export,
}

impl Pack {
    /// presenter 快照 + 页面选择 → 视图模型（**唯一的真数据入口**）。
    pub fn of(
        import: &ImportState,
        export: &ExportState,
        page: &PackPageState,
        targets: &[Target],
        catalog: Catalog,
    ) -> Self {
        let candidates: Vec<PackTarget> = targets
            .iter()
            .map(|target| PackTarget {
                id: target.id.clone(),
                label: format!(
                    "{} · {} · {}",
                    target.name,
                    target.mc_version,
                    crate::pages::content::loader_name(target.loader)
                ),
            })
            .collect();
        let instance = page
            .instance
            .clone()
            .or_else(|| candidates.first().map(|target| target.id.clone()));

        Self {
            hover: page.hover,
            draft: page.draft.clone(),
            drop_hint: catalog.text(Key::PackDropHint),
            no_dialog_note: catalog.text(Key::PackNoDialog),
            targets: candidates.clone(),
            instance: instance.clone(),
            instance_name: page.instance_name.clone(),
            running: import
                .progress
                .as_ref()
                .map(|progress| progress::running_of(progress, catalog)),
            probe: import.probe.as_ref().map(|probe| probe_of(probe, catalog)),
            cancelled: import.cancelled,
            cancelled_note: catalog.text(Key::PackCancelled),
            report: import
                .report
                .as_ref()
                .map(|report| report::report_of(report, page, catalog)),
            export: export::export_of(export, page, candidates, catalog),
        }
    }

    /// 有没有东西可画（没有就整块收起来，不占版面）。
    pub fn is_idle(&self) -> bool {
        self.running.is_none()
            && self.probe.is_none()
            && self.report.is_none()
            && !self.cancelled
            && !self.export.open
            && self.draft.is_empty()
    }
}

/// 探测诊断：把「缺哪个标记」摊开。
///
/// `saw` 为空时补一句「没有可识别的东西」——空列表直接画出来是一行空白，用户会以为是渲染坏了。
fn probe_of(probe: &ProbeView, catalog: Catalog) -> Probe {
    let lead = match probe.looks_like {
        Some(format) => catalog.render(
            Key::PackProbeLooksLike,
            &[
                ("format", export::format_label(format, catalog)),
                ("missing", &probe.missing.join(" · ")),
            ],
        ),
        None => catalog.text(Key::PackProbeUnknown).to_owned(),
    };
    let saw = if probe.saw.is_empty() {
        vec![catalog.text(Key::PackProbeNothing).to_owned()]
    } else {
        probe.saw.clone()
    };
    Probe {
        looks_like: probe.looks_like,
        lead,
        saw,
        detail: probe.detail.clone(),
    }
}

#[cfg(test)]
mod tests;
