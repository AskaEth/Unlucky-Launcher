//! 安装面板的**展示模型**（M305 从 `model.rs` 拆出；行数门禁 400）。
//!
//! 这一份只做「快照 → 要画的字段」的转换：文案走目录（[`Catalog`]），进度取
//! [`InstallProgressView::ratio`]（离散读数，插值在 `motion::progress`）。
//! 三个终态判据（[`InstallView::failed`] / [`InstallView::terminal`] / [`InstallView::finished`]）
//! 都在这里，页面只按它们分支——不许页面自己 match `InstallStage`（那会出现「面板说装完、
//! 列表说还在装」两套口径）。

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{human_bytes, InstallProgressView, InstallStage};

/// 一次安装的展示态（结构化字段 + 已求好的文案）。
#[derive(Debug, Clone, PartialEq)]
pub struct InstallView {
    /// 目标版本（= 选中的那一行）
    pub version: String,
    /// 阶段
    pub stage: InstallStage,
    /// 阶段词（`下载文件`）
    pub stage_label: String,
    /// 计数行（`128/512 文件`；未知时是「等第一个文件…」）
    pub detail: String,
    /// 速率 + 剩余时间（`1.2 MB/s · 剩 1m15s`；未知时 `None`）
    pub rate: Option<String>,
    /// 整体进度 0.0~1.0
    pub ratio: f32,
    /// 进度是否已知（`false` = 进度条画不确定态）
    pub known: bool,
}

impl InstallView {
    /// 快照 → 展示态。
    pub(crate) fn of(progress: &InstallProgressView, catalog: Catalog) -> Self {
        let known = progress.is_known();
        let detail = if known {
            catalog.render(
                Key::DownloadProgress,
                &[
                    ("done", &progress.files_done.to_string()),
                    ("total", &progress.files_total.to_string()),
                ],
            )
        } else {
            catalog.text(Key::DownloadProgressUnknown).to_owned()
        };
        let rate = if progress.speed_bps == 0 || progress.eta_secs == 0 {
            None
        } else {
            let eta = if progress.eta_secs >= 60 {
                catalog.render(
                    Key::DownloadEtaMins,
                    &[
                        ("mins", &(progress.eta_secs / 60).to_string()),
                        ("secs", &format!("{:02}", progress.eta_secs % 60)),
                    ],
                )
            } else {
                catalog.render(
                    Key::DownloadEtaSecs,
                    &[("secs", &progress.eta_secs.to_string())],
                )
            };
            Some(catalog.render(
                Key::DownloadRate,
                &[
                    ("speed", &format!("{}/s", human_bytes(progress.speed_bps))),
                    ("eta", eta.as_str()),
                ],
            ))
        };

        Self {
            version: progress.version.clone(),
            stage: progress.stage,
            stage_label: catalog.text(stage_key(progress.stage)).to_owned(),
            detail,
            rate,
            ratio: progress.ratio(),
            known,
        }
    }

    /// 失败了（错误态要出可读文案 + 出口，不许静默）。
    pub const fn failed(&self) -> bool {
        matches!(self.stage, InstallStage::Failed)
    }

    /// 收尾了（`Done` / `Failed` / `Cancelled`）。
    pub fn terminal(&self) -> bool {
        self.stage.is_terminal()
    }

    /// **结束且没什么可说**的收尾：成功（`Done`）与取消（`Cancelled`）。
    ///
    /// 这两种终态页面没有动作可给，留着只会让安装面板停在一个点不动的状态上（复审 p2 的根因）。
    /// 失败**不在其中**：它还要给原因出口。
    pub fn finished(&self) -> bool {
        matches!(self.stage, InstallStage::Done | InstallStage::Cancelled)
    }

    /// 还在跑（「取消安装」只在这时给）。
    pub fn running(&self) -> bool {
        !self.stage.is_terminal()
    }
}

/// 阶段 → 文案 key（**不**用 `InstallStage::label()`：那是中文业务串）。
const fn stage_key(stage: InstallStage) -> Key {
    match stage {
        InstallStage::Planning => Key::DownloadStagePlanning,
        InstallStage::Downloading => Key::DownloadStageDownloading,
        InstallStage::Installing => Key::DownloadStageInstalling,
        InstallStage::Registering => Key::DownloadStageRegistering,
        InstallStage::Done => Key::DownloadStageDone,
        InstallStage::Failed => Key::DownloadStageFailed,
        InstallStage::Cancelled => Key::DownloadStageCancelled,
    }
}
