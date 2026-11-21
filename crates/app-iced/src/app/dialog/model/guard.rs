//! 「Java 不符 / 老版本建议」那个**启动前**框的文案（M331 + M349 r2；从 `model` 拆出：
//! 那边贴着 400 行硬线，而这一段自成一体的「挑句子 + 填数字」）。
//!
//! 三条纪律：
//!
//! 1. **界面不判判据**：方向、档位、五个数字都由 presenter 给（`LaunchBlockedView`），这里只挑 key。
//! 2. **证据数字不写死**（M349 r2 / 复审 P2-2）：`{measured_version}` / `{measured_java}` 由 DTO 的
//!    `evidence` 填，它来自 core 的 `MEASURED_LEGACY_VERSION` / `MEASURED_LEGACY_JAVA`——命令行
//!    那句读的是同一批 key，所以改常量两边一起变。
//! 3. **主按钮 = 推荐动作**：硬拦推荐「换 Java」，建议推荐「装 Java 8」（见 `model::current`）。

use unlucky_i18n::catalog::{Catalog, Key};

use unlucky_presenter::dto::{AdvisoryTier, JavaGuardDirection, LaunchBlockedView};

use super::super::{GuardInstall, GuardInstallFailure, JavaGuardCard};

/// 第三个出口那枚按钮的文案，以及要追加到正文里的那一句（`None` = 还没点，正文不用多说）。
///
/// 「装好了」那一档按钮换成「现在启动」：装完却说不出下一步，用户只会把框关掉再去列表里找
/// 那一条实例——中间隔着一个他刚被拦下的记忆。
pub(super) fn install_state_text(
    catalog: &Catalog,
    guard: &JavaGuardCard,
    install_major: Option<u32>,
) -> (String, Option<String>) {
    // 调用方只在 `Some` 时才画这枚按钮（钉过实例 Java 时它落地不了，见 DTO 的字段文档）；
    // 这里的兜底只为不让一条提示变成 panic。
    let major = install_major.unwrap_or_default().to_string();
    let args: &[(&str, &str)] = &[("major", &major)];
    match &guard.install {
        GuardInstall::Idle => (catalog.render(Key::JavaGuardDownload, args), None),
        // 点了还没回音 / 正在下载：按钮写「正在下载…」，正文**另说一句**（不复读同一行，
        // 复审 r1 nit：同屏两行一模一样的话等于没说）。点了也不重复派（见 `update/dialog.rs`）。
        GuardInstall::Requested | GuardInstall::Running => (
            catalog.render(Key::JavaGuardDownloading, args),
            Some(catalog.text(Key::JavaGuardDownloadingHint).to_string()),
        ),
        GuardInstall::Done => (
            catalog.text(Key::JavaGuardRetryLaunch).to_string(),
            Some(catalog.render(Key::JavaGuardDownloadDone, args)),
        ),
        GuardInstall::Failed { failure } => {
            let line = match failure {
                GuardInstallFailure::Vendor { vendor, detail } => {
                    // 厂商名走目录：`temurin` 这种内部 id 不上屏（与「命令行不印内部 key」同一条纪律）。
                    let reason = format!("{}: {detail}", vendor_label(catalog, vendor));
                    catalog.render(
                        Key::JavaGuardDownloadFailed,
                        &[("major", &major), ("reason", &reason)],
                    )
                }
                GuardInstallFailure::NoVendor => {
                    catalog.render(Key::JavaGuardDownloadNoVendor, args)
                }
                GuardInstallFailure::Local { detail } => catalog.render(
                    Key::JavaGuardDownloadFailedLocal,
                    &[("reason", detail.as_str())],
                ),
            };
            (catalog.render(Key::JavaGuardDownload, args), Some(line))
        }
    }
}

/// 发行版 id → 目录里的名字（`core::game::java::VENDOR_IDS` 那五家 + 兜底）。
fn vendor_label(catalog: &Catalog, vendor: &str) -> &'static str {
    let key = match vendor {
        "temurin" => Key::JavaVendorTemurin,
        "oracle" => Key::JavaVendorOracle,
        "microsoft" => Key::JavaVendorMicrosoft,
        "zulu" => Key::JavaVendorZulu,
        "termux" => Key::JavaVendorTermux,
        _ => Key::JavaVendorUnknown,
    };
    catalog.text(key)
}

/// 「Java 不符」框的**标题**（`None` 那一档见 [`super::Modal`] 的普通弹窗）。
///
/// M349：`Advisory` 那一支不是「不符」，标题必须是另一句——沿用「这个版本要用另一个 Java」
/// 会让用户以为点下去就起不来（那正是这条建议要避免的误读）。
#[must_use]
pub(crate) fn guard_title<'a>(catalog: &'a Catalog, view: &LaunchBlockedView) -> &'a str {
    match view.direction {
        JavaGuardDirection::TooOld | JavaGuardDirection::TooNew => {
            catalog.text(Key::JavaGuardTitle)
        }
        JavaGuardDirection::Advisory(_) => catalog.text(Key::JavaAdvisoryTitle),
    }
}

/// 「Java 不符」框的正文（每行一段，全部按当前语言现求）。
///
/// 方向由 presenter 给（`JavaGuardDirection`，与 core 的 `JavaMismatch` 一对一），
/// 这里只挑句子 + 填数字，**不重算**「到底哪个方向不符」。
///
/// M349：`Advisory` 两支共用一行「实例 · 版本」的抬头，正文只多一句建议句——那句的字面来自
/// 目录（`JavaAdvisoryMeasured` / `JavaAdvisoryUnmeasured`），**判据与数字来自 core 的同一个
/// 函数**（`legacy_advisory_facts`），界面不再判一次「这是不是老版本」。
#[must_use]
pub(crate) fn guard_body(catalog: &Catalog, view: &LaunchBlockedView) -> Vec<String> {
    let name = view.name.as_str();
    let version = view.version_id.as_str();
    let mut body = vec![catalog.render(
        Key::JavaGuardInstance,
        &[("name", name), ("version", version)],
    )];
    // 钉过实例 Java（`install_major == None`）：先说清**为什么这里不给一键装**，否则用户会
    // 以为是启动器坏了（复审 r2 P2-1 实测的那个「点了还是同一个框」）。
    if view.install_major.is_none() {
        body.push(catalog.render(
            Key::JavaAdvisoryPinned,
            &[("java", &view.actual.to_string())],
        ));
    }
    let required = match view.direction {
        JavaGuardDirection::TooOld => catalog.render(
            Key::JavaGuardTooOld,
            &[("min", &view.required_min.to_string())],
        ),
        JavaGuardDirection::TooNew => catalog.render(
            Key::JavaGuardTooNew,
            // `TooNew` 一定带着上限（core 的 `JavaMismatch::TooNew { maximum, .. }`）；
            // 兜底用下限只是不让 `unwrap` 把一条提示变成一次 panic。
            &[(
                "max",
                &view.required_max.unwrap_or(view.required_min).to_string(),
            )],
        ),
        JavaGuardDirection::Advisory(tier) => {
            // 老版本原版**没有上限**，所以这里只说「按设计要多少 / 现在是多少」。
            let key = match tier {
                AdvisoryTier::Measured => Key::JavaAdvisoryMeasured,
                AdvisoryTier::Unmeasured => Key::JavaAdvisoryUnmeasured,
            };
            // 证据（哪一组实测过）由 presenter 从 core 的常量带过来，**目录里只有占位符**：
            // 这里绝不写「1.12.2 + Java 17」这类字面（复审 P2-2）。
            if let Some(evidence) = &view.evidence {
                let min = view.required_min.to_string();
                let actual = view.actual.to_string();
                let measured_java = evidence.java.to_string();
                body.push(catalog.render(
                    key,
                    &[
                        ("version", &view.version_id),
                        ("min", &min),
                        ("actual", &actual),
                        ("measured_version", &evidence.version_id),
                        ("measured_java", &measured_java),
                    ],
                ));
            }
            // `evidence` 缺了就**降级成数字行**（复审 r3 P3-3）：正文只剩「实例 · 版本」等于
            // 什么都没说，用户连「该用多少 / 现在多少」都看不到。数字仍然全部来自 DTO，不编。
            if view.evidence.is_none() {
                body.push(catalog.render(
                    Key::JavaGuardTooOld,
                    &[("min", &view.required_min.to_string())],
                ));
                body.push(catalog.render(
                    Key::JavaGuardActual,
                    &[("actual", &view.actual.to_string())],
                ));
            }
            return body;
        }
    };
    body.push(required);
    body.push(catalog.render(
        Key::JavaGuardActual,
        &[("actual", &view.actual.to_string())],
    ));
    body.push(catalog.text(Key::JavaGuardConsequence).to_string());
    body
}
