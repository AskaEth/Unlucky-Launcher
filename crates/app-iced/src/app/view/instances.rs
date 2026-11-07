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

use unlucky_i18n::{Catalog, Key};
use unlucky_presenter::dto::{InstanceRuntime, InstanceSummary, LaunchStage, LoaderKind};

use crate::pages::{Detail, Meta};

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
pub(super) fn runtime_status(runtime: &InstanceRuntime, catalog: Catalog) -> Option<String> {
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
        // `reason` 是 core 给的诊断原文（**数据**，不翻译）；只有前半句是界面文案
        InstanceRuntime::Failed { reason } => {
            Some(catalog.render(Key::InstancesFailed, &[("reason", reason)]))
        }
    }
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
