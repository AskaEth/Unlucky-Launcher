//! 出图用的向导状态夹具（D30 视觉验收的输入）。
//!
//! ## 为什么要有它
//!
//! 向导是**四段有状态**的界面，而截图通道只跑一次 `view()`（第 3 帧落盘），没有真人点按钮。
//! 组件库的做法是 `--screen=gallery` + `ForcedState`，页面层的做法是 `pages/instances/demo.rs`
//! 的 `DEMO`——同一类要求，同一类做法：**给渲染器一份确定的状态**，而不是让截图去赌异步时序。
//!
//! ## 哪些是真、哪些是夹具（复审请照这条核对）
//!
//! | 内容 | 来源 |
//! |---|---|
//! | 候选 / 问题 / 失败原因 | **夹具**（合成的 id 与问题文本，只为把版面撑到真实形态） |
//! | 运行目录 / 隔离档 | 夹具（固定路径，便于两张图逐像素对照） |
//! | **后果原文** | **真**：`unlucky_core::game::adopt::target_notes()` 现算，一个字不抄 |
//! | 文案（标题/标签/按钮） | **真**：`unlucky_i18n` 目录 |
//! | 渲染 | **真**：`--software-renderer`（tiny-skia）1180×760，与产品路径同一棵控件树 |
//!
//! 也就是说：这些图证明**版面与文案**，不证明 core 的判据（判据的证据在 `host/adopt/tests.rs`
//! 与 `core/tests/adopt/`）。
//!
//! ## 怎么用
//!
//! ```text
//! ULL_ADOPT_STATE=pick|pick-selected|empty|preview|shared|failed|done \
//!   unlucky-launcher-iced.exe --software-renderer --screenshot <path> [--screenshot-theme dark]
//! ```
//!
//! 只在 `--screenshot` 已经给了的情况下生效（交互模式读这个环境变量没有意义，直接忽略）。

use std::path::PathBuf;

use unlucky_core::game::adopt::{target_notes, AdoptTarget};
use unlucky_presenter::dto::{
    AdoptCandidateView, AdoptDoneView, AdoptEvent, AdoptFailureView, AdoptIsolationView,
    AdoptPlanView, AdoptProblemView, AdoptScanView, AdoptState, AdoptTargetView,
};

use super::{Busy, Form};

/// 夹具里的版本目录（只是**画**出来的路径，不碰盘）。
const VERSIONS_DIR: &str = "D:/minecraft/.minecraft/versions";

/// 夹具里的数据区运行目录。
const INSTANCE_RUN_DIR: &str = "D:/ULL/.ULL/data/instances/Better-MC";

/// 夹具里的游戏根。
const GAME_ROOT: &str = "D:/minecraft/.minecraft";

/// 环境变量名（见模块文档）。
pub(crate) const ENV: &str = "ULL_ADOPT_STATE";

/// 按名字造一份「打开着的向导」；不认识的名字返回 `None`（不影响正常启动）。
pub(crate) fn seed(name: &str) -> Option<(Form, AdoptState)> {
    let name = name.trim().to_ascii_lowercase();
    let mut form = Form {
        open: true,
        ..Form::default()
    };
    let mut state = AdoptState::default();

    match name.as_str() {
        // 扫描完成、还没选版本（列表里三态齐全：可导入 / 已登记 / 有问题）
        "pick" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            form.busy = Busy::Idle;
        }
        // 扫描完成、**已选中一个可导入的版本**（选中行底色 + 预检按钮启用态）
        "pick-selected" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            form.selected = Some("1.20.1".to_string());
            form.busy = Busy::Idle;
        }
        // 游戏根下真没有版本（空态）
        "empty" => {
            state.apply(AdoptEvent::Scan(Box::new(AdoptScanView {
                versions_dir: PathBuf::from(VERSIONS_DIR),
                candidates: Vec::new(),
            })));
            form.busy = Busy::Idle;
        }
        // 选中缺省档（InstanceDir）并点过预检
        "preview" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            state.apply(AdoptEvent::Planned(Box::new(plan_view(
                AdoptTargetView::InstanceDir,
                AdoptIsolationView::Custom,
                INSTANCE_RUN_DIR,
            ))));
            form.selected = Some("1.20.1".to_string());
            form.checked = true;
            form.busy = Busy::Idle;
        }
        // 选中共享档（Shared）：后果原文由 core `target_notes()` 现算
        "shared" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            state.apply(AdoptEvent::Planned(Box::new(plan_view(
                AdoptTargetView::Shared,
                AdoptIsolationView::Shared,
                GAME_ROOT,
            ))));
            form.selected = Some("1.20.1".to_string());
            form.target = AdoptTargetView::Shared;
            form.checked = true;
            form.busy = Busy::Idle;
        }
        // 预检失败：两条问题逐条列出（不许折叠成一句）
        "failed" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            state.apply(AdoptEvent::Failed(Box::new(AdoptFailureView {
                code: "adopt_not_adoptable".to_string(),
                message: "版本 1.19.4 不能登记：清单不是合法 JSON".to_string(),
                problems: vec![
                    AdoptProblemView {
                        code: "manifest_invalid_json".to_string(),
                        message: format!(
                            "清单不是合法 JSON {VERSIONS_DIR}/1.19.4/1.19.4.json：expected `,` or `}}` at line 12 column 3"
                        ),
                    },
                    AdoptProblemView {
                        code: "missing_parent".to_string(),
                        message: "版本 1.19.4 继承的父版本 1.19.3 不在盘上".to_string(),
                    },
                ],
            })));
            form.selected = Some("1.19.4".to_string());
            form.checked = true;
            form.busy = Busy::Idle;
        }
        // 登记完成
        "done" => {
            state.apply(AdoptEvent::Scan(Box::new(scan_view())));
            state.apply(AdoptEvent::Done(Box::new(AdoptDoneView {
                instance_id: "2f1c0a5e-7f4b-4c1d-9a2e-6b8d5c3a1f70".to_string(),
                name: "Better MC".to_string(),
                version_id: "1.20.1-fabric-0.15.0".to_string(),
                isolation: AdoptIsolationView::Custom,
                run_dir: PathBuf::from(INSTANCE_RUN_DIR),
                dir_created: true,
                notes: target_notes(&AdoptTarget::InstanceDir, &PathBuf::from(INSTANCE_RUN_DIR)),
            })));
            form.selected = Some("1.20.1-fabric-0.15.0".to_string());
            form.checked = true;
            form.busy = Busy::Idle;
        }
        _ => return None,
    }
    Some((form, state))
}

/// 一份扫描结果（三态齐全）。
fn scan_view() -> AdoptScanView {
    AdoptScanView {
        versions_dir: PathBuf::from(VERSIONS_DIR),
        candidates: vec![
            AdoptCandidateView {
                version_id: "1.20.1".to_string(),
                jar_present: true,
                inherits: Vec::new(),
                registered: Vec::new(),
                problems: Vec::new(),
            },
            AdoptCandidateView {
                version_id: "1.20.1-fabric-0.15.0".to_string(),
                jar_present: false,
                inherits: vec!["1.20.1-fabric-0.15.0".to_string(), "1.20.1".to_string()],
                registered: vec!["Better MC".to_string()],
                problems: Vec::new(),
            },
            AdoptCandidateView {
                version_id: "1.19.4".to_string(),
                jar_present: true,
                inherits: Vec::new(),
                registered: Vec::new(),
                problems: vec![AdoptProblemView {
                    code: "manifest_invalid_json".to_string(),
                    message: format!(
                        "清单不是合法 JSON {VERSIONS_DIR}/1.19.4/1.19.4.json：expected `,` or `}}` at line 12 column 3"
                    ),
                }],
            },
        ],
    }
}

/// 一份预检结果。**`notes` 由 core 现算**（共享档/版本目录档的后果原文只有一处）。
fn plan_view(
    target: AdoptTargetView,
    isolation: AdoptIsolationView,
    run_dir: &str,
) -> AdoptPlanView {
    let core_target = match target {
        AdoptTargetView::InstanceDir => AdoptTarget::InstanceDir,
        AdoptTargetView::Shared => AdoptTarget::Shared,
        AdoptTargetView::VersionDir => AdoptTarget::VersionDir,
    };
    AdoptPlanView {
        version_id: "1.20.1".to_string(),
        name: "1.20.1".to_string(),
        target,
        isolation,
        run_dir: PathBuf::from(run_dir),
        notes: target_notes(&core_target, &PathBuf::from(run_dir)),
    }
}
