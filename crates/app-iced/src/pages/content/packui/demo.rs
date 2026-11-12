//! 出图夹具（D30）：`ULL_CONTENT_PACK` 选「同一页的哪一态」。
//!
//! ## 为什么是环境变量
//!
//! `pages_demo` 的 `--screen=` 只够选到**哪一页**，选不到「同一页的哪一态」；而
//! `pages/harness.rs` 与 `cli.rs` 不在本 mission 的 scope。所以沿用本仓既有的取证开关范式
//! （`ULL_START_ROUTE` / `ULL_CONTENT_KIND` / `ULL_CONTENT_STATE`）：
//!
//! | 变量 | 取值 | 作用 |
//! |---|---|---|
//! | `ULL_CONTENT_PACK` | `importing` / `pick` / `partial` / `conflict` / `failed` / `export` | 选导入导出区的哪一态（不设 = 只有入口） |
//!
//! 夹具**不直接造视图模型**，而是造 presenter 侧的快照（`ImportState` / `ExportState`），
//! 再走产品路径同一个 [`Pack::of`]。这样截图里看到的每一行字都是产品路径会画的那一行，
//! 而不是「为了出图另写一套」。

use std::path::PathBuf;

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    ConflictChoiceView, ConflictFileView, ConflictReasonView, ExportClassView, ExportFileView,
    ExportPreviewView, ExportReasonView, ExportState, FileActionView, ImportCancelView,
    ImportErrorView, ImportFailureStageView, ImportProgressView, ImportReportView, ImportStageView,
    ImportState, ImportStatusView, InstanceId, LoaderKind, MissingFileView, MissingReasonView,
    PackFormatView,
};

use crate::pages::content::Target;

use super::model::{Pack, PackPageState};

/// 取证开关名。
const ENV_PACK: &str = "ULL_CONTENT_PACK";

/// 两个实例（与内容库页的演示环境同一批，避免同一张图上出现两套实例名）。
fn targets() -> Vec<Target> {
    vec![
        Target {
            id: "survival".to_string(),
            name: "Survival".to_string(),
            mc_version: "1.20.1".to_string(),
            loader: LoaderKind::Fabric,
        },
        Target {
            id: "modded".to_string(),
            name: "Modded".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: LoaderKind::Forge,
        },
    ]
}

/// 按环境变量造一态（不设 = 只有入口，没有进度也没有报告）。
pub(super) fn pack(lang: Lang) -> (Pack, PackPageState) {
    let state = std::env::var(ENV_PACK).unwrap_or_default();
    pack_in(&state, lang)
}

/// 按名字造一态（单测直接调它，不碰环境变量：并行测试里改 env 是竞态）。
pub(super) fn pack_in(state: &str, lang: Lang) -> (Pack, PackPageState) {
    let catalog = lang.catalog();
    let instance: Option<InstanceId> = Some("survival".to_string());
    let mut page = PackPageState {
        instance,
        ..PackPageState::default()
    };
    let mut import = ImportState::default();
    let mut export = ExportState::default();

    match state {
        "importing" => import.progress = Some(progress()),
        // M160：导入前的冲突清单（分页 + 三个入口 + 计数摘要）
        "pick" => {
            import.scan = Some(scan(312));
            // 停在第 2 页，且已经逐项点过两条「覆盖」——图上能同时看见分页与逐项状态
            page.pick_page = 1;
            // 两条落到**第 2 页**的路径上：图上能同时看见「保留」与「覆盖」两种行状态
            page.pick_choices.insert(
                "config/generated/module-041.toml".to_string(),
                unlucky_presenter::dto::ConflictChoiceView::Overwrite,
            );
            page.pick_choices.insert(
                "config/generated/module-044.toml".to_string(),
                unlucky_presenter::dto::ConflictChoiceView::Overwrite,
            );
        }
        "partial" => import.report = Some(report(ImportStatusView::Partial, false)),
        "conflict" => {
            import.report = Some(report(ImportStatusView::Partial, true));
            page.expanded = Some("config/sodium-options.json".to_string());
        }
        // 致命态（M161 分成三档出图：预检没过 / 已回滚 / 回滚没做完）。
        // `failed` 保留旧名字（= 已回滚），另外两个是新档位。
        "failed" | "failed_rolled_back" => {
            let mut fatal = report(ImportStatusView::Failed, false);
            fatal.error = Some(ImportErrorView {
                code: "import_integrity".to_string(),
                label: "Integrity check".to_string(),
                message: "mods/sodium.jar: sha1 mismatch after write".to_string(),
                hint: Some("Re-download the pack from its source".to_string()),
            });
            fatal.rolled_back = true;
            fatal.failure_stage = Some(ImportFailureStageView::RolledBack);
            import.report = Some(fatal);
        }
        // 预检没过：**一个字节都没写**（与上一条只差 `failure_stage`，图上要能一眼分辨）
        "failed_preflight" => {
            let mut fatal = report(ImportStatusView::Failed, false);
            fatal.error = Some(ImportErrorView {
                code: "pack_traversal".to_string(),
                label: "Path traversal".to_string(),
                message: "overrides/../../etc/passwd".to_string(),
                hint: Some("Re-download the pack from its source".to_string()),
            });
            fatal.files.clear();
            fatal.bytes_written = 0;
            fatal.rolled_back = true;
            fatal.failure_stage = Some(ImportFailureStageView::NotStarted);
            import.report = Some(fatal);
        }
        // 回滚没做完：目标目录可能与导入前不同
        "failed_incomplete" => {
            let mut fatal = report(ImportStatusView::Failed, false);
            fatal.error = Some(ImportErrorView {
                code: "import_integrity".to_string(),
                label: "Integrity check".to_string(),
                message: "mods/sodium.jar: sha1 mismatch after write".to_string(),
                hint: None,
            });
            fatal.rolled_back = false;
            fatal.failure_stage = Some(ImportFailureStageView::RollbackIncomplete);
            import.report = Some(fatal);
        }
        // 取消态：面板收回了，但「后台还在跑」这句话要留在屏上
        "cancelled" => import.cancelled = true,
        "export" => {
            page.export_open = true;
            page.export_name = "Survival 1.20.1".to_string();
            export.preview = Some(preview());
        }
        _ => {}
    }

    let model = Pack::of(&import, &export, &page, &targets(), catalog);
    (model, page)
}

/// 导入中：第 2 阶段（解包覆盖内容），计数已知。
fn progress() -> ImportProgressView {
    ImportProgressView {
        stage: ImportStageView::Extract,
        done: 418,
        total: 640,
        current: Some("overrides/config/sodium-options.json".to_string()),
        pack: "Better MC 1.2.0".to_string(),
    }
}

/// 一条落地文件。
fn file(
    path: &str,
    action: FileActionView,
    bytes: u64,
) -> unlucky_presenter::dto::ImportedFileView {
    unlucky_presenter::dto::ImportedFileView {
        path: path.to_string(),
        action,
        bytes,
    }
}

/// 报告（`conflicts` 决定造不造冲突那一块）。
fn report(status: ImportStatusView, conflicts: bool) -> ImportReportView {
    let mut report = ImportReportView {
        status,
        format: Some(PackFormatView::Curseforge),
        pack_name: Some("Better MC".to_string()),
        pack_version: Some("1.2.0".to_string()),
        game_version: Some("1.20.1".to_string()),
        loader: Some("fabric-loader 0.15.11".to_string()),
        version_id: Some("1.20.1-fabric0.15.11".to_string()),
        target: PathBuf::from("C:/ULL/instances/survival/.minecraft"),
        files: vec![
            file("mods/sodium.jar", FileActionView::Installed, 812_000),
            file("mods/lithium.jar", FileActionView::Installed, 640_000),
            file("config/xaeroplus.cfg", FileActionView::KeptUser, 4_200),
        ],
        conflicts: Vec::new(),
        missing: vec![
            MissingFileView {
                path: Some("mods/create-1.2.3.jar".to_string()),
                display: "mods/create-1.2.3.jar".to_string(),
                reason: MissingReasonView::FetchFailed,
                detail: "HTTP 404".to_string(),
            },
            MissingFileView {
                path: None,
                display: "123456:789012".to_string(),
                reason: MissingReasonView::NoSource,
                detail: "CurseForge 文件端点未接通".to_string(),
            },
            MissingFileView {
                path: Some("mods/jei-1.20.1.jar".to_string()),
                display: "mods/jei-1.20.1.jar".to_string(),
                reason: MissingReasonView::Offline,
                detail: String::new(),
            },
        ],
        rejected: vec!["org.lwjgl3:3.3.1".to_string()],
        warnings: vec!["`server-overrides/` was not unpacked (client import)".to_string()],
        error: None,
        registered: Some("Survival".to_string()),
        bytes_written: 3_400_000,
        rolled_back: true,
        failure_stage: None,
        cancel: ImportCancelView::NotRequested,
    };
    if conflicts {
        report.conflicts = vec![
            conflict(
                "config/sodium-options.json",
                ConflictReasonView::UserModified,
            ),
            conflict("mods/xaeroplus.jar", ConflictReasonView::UserDeleted),
            conflict("options.txt", ConflictReasonView::PreExisting),
        ];
    }
    report
}

fn conflict(path: &str, reason: ConflictReasonView) -> ConflictFileView {
    ConflictFileView {
        path: path.to_string(),
        reason,
        // 默认档永远是「保留我的」——夹具也一样，截图不许示范成默认覆盖
        choice: ConflictChoiceView::KeepUser,
        pack_sha1: "9f2c41ab77de3350aa1b8c0d5e6f708192a3b4c5".to_string(),
        base_sha1: Some("3a7d90ff11bc22445566ef7788990011aabbccdd".to_string()),
        disk_sha1: Some("c1d2e3f4050617283940a5b6c7d8e9f0a1b2c3d4".to_string()),
        pack_size: 4_200,
    }
}

/// 一条待决冲突（M160 预览清单的一行）。
fn pending(index: usize) -> unlucky_presenter::dto::ImportConflictView {
    // 前几条用真实形态的路径，其余按序号生成：截图里既要有可读的真实感，也要有"很多条"的证据。
    let path = match index {
        0 => "config/sodium-options.json".to_string(),
        1 => "mods/create-1.2.3.jar".to_string(),
        2 => "config/jei/jei-client.ini".to_string(),
        3 => "options.txt".to_string(),
        _ => format!("config/generated/module-{index:03}.toml"),
    };
    unlucky_presenter::dto::ImportConflictView {
        path,
        source: if index.is_multiple_of(3) {
            unlucky_presenter::dto::ConflictSourceView::ClientOverrides
        } else {
            unlucky_presenter::dto::ConflictSourceView::Overrides
        },
        reason: if index.is_multiple_of(2) {
            unlucky_presenter::dto::ConflictReasonView::PreExisting
        } else {
            unlucky_presenter::dto::ConflictReasonView::UserModified
        },
        disabled: false,
        pack_sha1: "9f2c41ab77de3350aa1b8c0d5e6f708192a3b4c5".to_string(),
        pack_size: 3_800 + (index as u64 * 37) % 900,
        disk_sha1: "c1d2e3f4050617283940a5b6c7d8e9f0a1b2c3d4".to_string(),
        disk_size: 4_100 + (index as u64 * 53) % 1_200,
        base_sha1: (!index.is_multiple_of(2))
            .then(|| "3a7d90ff11bc22445566ef7788990011aabbccdd".to_string()),
    }
}

/// 预检结果（`count` 条冲突；图上靠分页只画当前这一页）。
fn scan(count: usize) -> unlucky_presenter::dto::ImportScanView {
    let conflicts: Vec<unlucky_presenter::dto::ImportConflictView> =
        (0..count).map(pending).collect();
    let lost_bytes = conflicts.iter().map(|each| each.disk_size).sum();
    let incoming_bytes = conflicts.iter().map(|each| each.pack_size).sum();
    unlucky_presenter::dto::ImportScanView {
        target: PathBuf::from("C:/ULL/instances/survival/.minecraft"),
        format: Some(PackFormatView::Modrinth),
        pack_name: Some("Better MC".to_string()),
        pack_version: Some("1.2.0".to_string()),
        game_version: Some("1.20.1".to_string()),
        conflicts,
        unknown_remote: 4,
        lost_bytes,
        incoming_bytes,
        notes: Vec::new(),
        error: None,
    }
}

/// 导出预检：12 内联 / 1 跳过（包不完整）。
fn preview() -> ExportPreviewView {
    let mut files = vec![ExportFileView {
        path: "mods/create-1.2.3.jar".to_string(),
        class: ExportClassView::Inline,
        reason: ExportReasonView::NoRemoteMatch,
        bytes: 2_100_000,
        disabled: false,
    }];
    for index in 0..11 {
        files.push(ExportFileView {
            path: format!("mods/generated-{index:02}.jar"),
            class: ExportClassView::Inline,
            reason: ExportReasonView::NoRemoteMatch,
            bytes: 512_000,
            disabled: false,
        });
    }
    // 13 条「没进包」是真实形态：预检那块必须自己做有界滚动，否则把「确认写盘」顶出屏幕
    for index in 0..12 {
        files.push(ExportFileView {
            path: format!("mods/.cache/backup-{index:02}.jar"),
            class: ExportClassView::Skipped,
            reason: ExportReasonView::SymbolicLink,
            bytes: 0,
            disabled: false,
        });
    }
    files.push(ExportFileView {
        path: "mods/bad:name.jar".to_string(),
        class: ExportClassView::Skipped,
        reason: ExportReasonView::UnsafePath,
        bytes: 0,
        disabled: false,
    });

    ExportPreviewView {
        format: PackFormatView::Modrinth,
        instance: "survival".to_string(),
        name: "Survival 1.20.1".to_string(),
        version: "1.0.0".to_string(),
        target: PathBuf::from("C:/ULL/downloads/Survival-1.0.0.mrpack"),
        files,
        notes: vec![
            "CF 表达不了禁用位：`.disabled` 文件已内联".to_string(),
            "离线目录：所有文件都证明不了可被远程替代，全部内联".to_string(),
        ],
    }
}
