//! 实例页的**展示转换**（M120 从 `app/view.rs` 原样搬出：那边给 i18n 让出行数）。
//!
//! | 函数 | 产物 | 谁读 |
//! |---|---|---|
//! | [`supporting_line`] | 列表行 / 详情副标题 `1.20.4 · Fabric 0.16.5` | [`crate::pages::Row`] / [`crate::pages::Detail`] |
//! | [`runtime_status`] | 运行状态行（列表 overline + 详情状态行共用一份） | 同上 |
//! | [`detail_of`] | 详情列的全部字段 | [`crate::pages::Detail`] |
//! | [`last_played`] | `2026-10-29 19:22 UTC` / `Never` | 元信息卡 `LAST PLAYED` |
//!
//! 这里只做「快照字段 → 要画什么」，不碰控件树。**文案的语言由调用方的 [`Catalog`] 决定**
//! （`view::model` 把 `App::lang` 的目录传进来），不再是 M90 时期的英文基线字面量。
//!
//! ## 两条接法上的取舍
//!
//! 1. **原版徽标走词表**：`LoaderInfo::badge()` 对原版返回**中文**「原版」（那是 presenter 的
//!    面向中文日志的 `label()`），英文界面里直接画它就是「中文漏进英文界面」。所以原版这一支
//!    改走 [`Key::LoaderVanilla`]，装加载器时仍是 `Fabric 0.16.5`（品牌 + 版本号，不翻译）；
//! 2. **运行状态只翻标签本体**：`Running · pid 1234` → `运行中 · pid 1234`，`pid` 与数字、
//!    `·` 分隔符一个字都不动（[`Key::InstancesRunning`] 的占位符只有 `{pid}`）。

use unlucky_i18n::{Catalog, Key, Lang};
use unlucky_presenter::dto::{
    InstanceRuntime, InstanceSummary, LaunchFailureCause, LaunchFailureStage, LaunchFinished,
    LaunchStage, LoaderKind, LogOutcome,
};
use unlucky_presenter::state::AppState;

use crate::pages::{Detail, LaunchFacts, Meta, Mods, Saves};

/// 列表行的副文案：`1.20.4 · Fabric 0.16.5`（原版只写「原版 / Vanilla」，见 [`loader_badge`]）。
pub(super) fn supporting_line(instance: &InstanceSummary, catalog: Catalog) -> String {
    format!(
        "{} · {}",
        instance.mc_version,
        loader_badge(instance, catalog)
    )
}

/// 加载器徽标：原版走词表，其余交给 `LoaderInfo::badge()`（品牌 + 版本号，原样）。
///
/// **判据只能是「加载器类型本身是原版」**。曾经写成「原版 **或** 版本号为空」⇒
/// `1.20.1-forge` / `1.20.1-neoforge` / `1.20.1-OptiFine` 这类**取不到版本段**的 id（`badge()`
/// 对它们给的就是品牌名 `Forge` / `NeoForge` / `OptiFine`）会被显示成「原版 / Vanilla」——
/// 那是 M120 r1 的 p2-1 回归，`the_loader_badge_keeps_the_brand_when_the_version_is_missing` 守着。
fn loader_badge(instance: &InstanceSummary, catalog: Catalog) -> String {
    match instance.loader.kind {
        // 唯一需要过词表的一支：`LoaderKind::label()` 对原版返回的是**中文**「原版」
        LoaderKind::Vanilla => catalog.text(Key::LoaderVanilla).to_string(),
        // 其余品牌名不翻译；版本号为空时 `badge()` 自己就只给品牌名
        _ => instance.loader.badge(),
    }
}

/// 隔离档位：`isolated` / `shared` 是两个稳定 id（`core::game::IsolationMode::as_str`），
/// 直接上屏就是英文界面漏中文、中文界面漏英文。只认这两档，**其余原样**（`inherit` / `custom` /
/// 未来新增的值不许在这里被猜成一个意思）。
fn isolation(raw: &str, catalog: Catalog) -> String {
    match raw {
        "isolated" => catalog.text(Key::InstancesIsolationIsolated).to_string(),
        "shared" => catalog.text(Key::InstancesIsolationShared).to_string(),
        other => other.to_string(),
    }
}

/// 运行状态（列表行的 overline 与详情页的状态行共用一份文案）。
pub(crate) fn runtime_status(runtime: &InstanceRuntime, catalog: Catalog) -> Option<String> {
    match runtime {
        InstanceRuntime::Idle => None,
        // 「阶段 · 百分比」：百分比是数字，不过词表；只有阶段名换语言
        InstanceRuntime::Busy { stage, progress } => Some(format!(
            "{} · {:.0}%",
            stage_label(*stage, catalog),
            progress.ratio() * 100.0
        )),
        InstanceRuntime::Running { pid } => {
            Some(catalog.render(Key::InstancesRunning, &[("pid", &pid.to_string())]))
        }
        // 失败有**两种形状**，各说各的话（复审 P1-1）：
        // - `Stage`：进程**没起来** → 「哪一步 + 那一步的原文」（原文不翻译，是取证信息）；
        // - `Crashed` / `SilentFailure`：**起来过** → 说结局（崩溃 / 疑似静默失败 + 退出码），
        //   绝不说成「启动游戏进程失败」。
        //
        // 这一份是**详情页**用的全文（详情列会自动换行）；列表行那份是 [`runtime_status_row`]
        // （单行、不许折行，见 D-M79-5）。
        InstanceRuntime::Failed { cause } => Some(failure_line(cause, catalog, true)),
    }
}

/// 一条失败状态行。
///
/// `with_detail = false` 是**列表行**那一档：`Stage` 只留「哪一步」，不带原文
/// （原文可能是一长串路径，单行 overline 会把它裁成半截字）；结局类的两种形状本来就没有原文。
fn failure_line(cause: &LaunchFailureCause, catalog: Catalog, with_detail: bool) -> String {
    match cause {
        LaunchFailureCause::Stage { stage, detail } if with_detail => catalog.render(
            Key::LaunchStageFailed,
            &[
                ("stage", failure_stage_label(*stage, catalog)),
                ("detail", detail.as_str()),
            ],
        ),
        LaunchFailureCause::Stage { stage, .. } => catalog.render(
            Key::InstancesFailed,
            &[("reason", failure_stage_label(*stage, catalog))],
        ),
        LaunchFailureCause::Crashed { exit_code } => {
            let reason = match exit_code {
                Some(code) => catalog.render(Key::InstancesCrashed, &[("code", &code.to_string())]),
                // 平台没给退出码：只说「崩溃」，不编一个 0 出来。
                None => catalog.text(Key::LaunchOutcomeCrash).to_string(),
            };
            catalog.render(Key::InstancesFailed, &[("reason", reason.as_str())])
        }
        LaunchFailureCause::SilentFailure { .. } => {
            // 静默失败的**关键信息是「exit 0 却没起来」**，不是退出码本身（复审 P1-1 第 ③ 条）。
            let reason = catalog.text(Key::InstancesSilentFailed).to_string();
            catalog.render(Key::InstancesFailed, &[("reason", reason.as_str())])
        }
    }
}

/// 列表行用的运行状态：**单行**（`ListItem` 的 overline 不折行，D-M79-5）。
///
/// 与 [`runtime_status`] 只差一处：失败时不带 `detail`——那一段是原始错误文本（可能是一长串
/// 路径），放进 60/76 高的单行 overline 会被裁成半截字。全文在**详情页**（[`runtime_status`]）。
pub(crate) fn runtime_status_row(runtime: &InstanceRuntime, catalog: Catalog) -> Option<String> {
    match runtime {
        InstanceRuntime::Failed { cause } => Some(failure_line(cause, catalog, false)),
        other => runtime_status(other, catalog),
    }
}

/// 失败阶段文案（presenter 给 slug，界面在这里选词）。
///
/// 与 [`stage_label`] 同一套做法：**不**复用 presenter 侧的任何中文常量。
pub(crate) fn failure_stage_label(stage: LaunchFailureStage, catalog: Catalog) -> &'static str {
    catalog.text(match stage {
        LaunchFailureStage::VersionManifest => Key::LaunchStageVersionManifest,
        LaunchFailureStage::Inheritance => Key::LaunchStageInheritance,
        LaunchFailureStage::InstanceConfig => Key::LaunchStageInstanceConfig,
        LaunchFailureStage::Isolation => Key::LaunchStageIsolation,
        LaunchFailureStage::Identity => Key::LaunchStageIdentity,
        LaunchFailureStage::Spawn => Key::LaunchStageSpawn,
    })
}

/// 一次启动收尾的 Toast 文案（**唯一的产地**：presenter 只发结构，不拼句子）。
///
/// 三种结局各有各的说法：正常结束报时长，崩溃/静默失败报「结局 + 阶段 + 原文」，
/// 组装阶段就失败（`failure` 有值）报「哪一步 + 原文」。
#[must_use]
pub(crate) fn launch_toast(finished: &LaunchFinished, lang: Lang) -> String {
    let catalog = lang.catalog();
    if let Some(failure) = &finished.failure {
        return catalog.render(
            Key::LaunchStageFailed,
            &[
                ("stage", failure_stage_label(failure.stage, catalog)),
                ("detail", failure.detail.as_str()),
            ],
        );
    }
    let outcome = catalog.text(match finished.outcome {
        LogOutcome::Ok => Key::LaunchOutcomeNormal,
        LogOutcome::Crashed => Key::LaunchOutcomeCrash,
        LogOutcome::SilentFail => Key::LaunchOutcomeSilent,
    });
    let seconds = format!("{:.1}", finished.uptime_ms as f64 / 1000.0);
    catalog.render(
        Key::LaunchToastEnded,
        &[("outcome", outcome), ("seconds", seconds.as_str())],
    )
}

/// 阶段文案：**不**复用 `LaunchStage::label()`（那是核心层的中文诊断串，英文界面会漏中文）。
fn stage_label(stage: LaunchStage, catalog: Catalog) -> &'static str {
    catalog.text(match stage {
        LaunchStage::Download => Key::InstancesStageDownload,
        LaunchStage::Verify => Key::InstancesStageVerify,
        LaunchStage::Launch => Key::InstancesStageLaunch,
        LaunchStage::Window => Key::InstancesStageWindow,
    })
}

/// 详情列的内容（字段全部来自 [`InstanceSummary`]）。
pub(super) fn detail_of(instance: &InstanceSummary, catalog: Catalog) -> Detail {
    Detail {
        id: instance.id.clone(),
        name: instance.name.clone(),
        subtitle: supporting_line(instance, catalog),
        meta: vec![
            meta(
                catalog.text(Key::InstancesMetaVersion),
                &instance.mc_version,
            ),
            meta(
                catalog.text(Key::InstancesMetaLoader),
                &loader_badge(instance, catalog),
            ),
            meta(
                catalog.text(Key::InstancesMetaIsolation),
                &isolation(&instance.isolation, catalog),
            ),
            meta(
                catalog.text(Key::InstancesMetaVersionId),
                &instance.version_id,
            ),
            meta(
                catalog.text(Key::InstancesMetaFiles),
                catalog.text(if instance.version_present {
                    Key::InstancesPresentOnDisk
                } else {
                    Key::InstancesMissing
                }),
            ),
            meta(
                catalog.text(Key::InstancesMetaLastPlayed),
                &last_played(instance.last_played.as_deref(), catalog),
            ),
        ],
        // 真快照没有「正文描述」这种字段（偏离 #9）：演示数据才有正文段
        note: None,
        status: runtime_status(&instance.runtime, catalog),
        can_launch: matches!(
            instance.runtime,
            InstanceRuntime::Idle | InstanceRuntime::Failed { .. }
        ),
        can_stop: matches!(
            instance.runtime,
            InstanceRuntime::Busy { .. } | InstanceRuntime::Running { .. }
        ),
        // M157：启动参数区的**只读事实**（门槛 / 生效值 / 三态词表全部来自 presenter 快照，
        // 页面不复制任何一条判据；表单值在 `App::launch_form`）。
        launch: LaunchFacts {
            allowed: instance.launch.graphics_backend_allowed,
            mc_version: instance.launch.mc_version.clone(),
            min_version: instance.launch.graphics_backend_min_version.clone(),
            memory_effective_mb: instance.launch.memory_effective_mb,
            java_effective: instance.launch.java_effective.clone(),
            backend_effective: instance.launch.graphics_backend_effective.clone(),
            options: instance.launch.graphics_backend_options.clone(),
        },
    }
}

fn meta(label: &str, value: &str) -> Meta {
    Meta {
        label: label.to_string(),
        value: value.to_string(),
    }
}

/// ISO-8601（`2026-10-29T19:22:00Z`）→ `2026-10-29 19:22 UTC`；`None` = 从未启动。
///
/// 只做切片不做时区换算：core 给的是 UTC，`Z` 就是它的时区；本地时区换算等时间格式 mission。
/// 非 ASCII 或长度不足时原样返回（不 panic、不猜）。
pub(super) fn last_played(raw: Option<&str>, catalog: Catalog) -> String {
    let Some(text) = raw else {
        return catalog.text(Key::InstancesNeverPlayed).to_string();
    };
    match (text.get(0..10), text.get(11..16)) {
        (Some(date), Some(time)) if date.starts_with(|c: char| c.is_ascii_digit()) => {
            format!("{date} {time} UTC")
        }
        _ => text.to_string(),
    }
}

/// 本地 Mod 的展示模型（M224）：presenter 的视图态 → 页面要画的东西。
///
/// 转换本体在 [`crate::pages::instances::mods`] 的 `from_state`（文案要用 `Texts`，那是页面层的
/// 取词入口）——这里只把"当前语言"给过去，与 [`detail_of`] 同一条接法。
pub(super) fn mods_of(state: &AppState, catalog: Catalog) -> Mods {
    crate::pages::mods_from_state(&state.mods, catalog)
}

/// 存档 / 截图的展示模型（M235）：presenter 的视图态 → 页面要画的东西。
///
/// 与 [`mods_of`] 同一条接法：转换本体在 `pages::instances::saves::from_state`（文案要用
/// `Texts`），这里只把"当前语言"给过去。
pub(super) fn saves_of(state: &AppState, catalog: Catalog) -> Saves {
    crate::pages::saves_from_state(&state.saves, catalog)
}
