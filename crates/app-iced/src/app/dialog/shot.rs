//! 出图取证用的「启动前框」夹具（`ULL_JAVA_GUARD_SHOT` / `ULL_LEGACY_ADVISORY_SHOT`；
//! 只在 `--screenshot` 下生效）。
//!
//! # 它解决什么
//!
//! M331 的交付图要拍「托管里只装了 17、用户点开 1.12.2（只能用 Java 8）」那个状态；
//! M349 要拍「老版本原版 + 更高版本 Java」那条**只是建议**的框（两档各一张）。
//! 真机复现要装 JDK、建实例、点启动——截图通道要的是**确定性**。做法与 `toast_shot` /
//! `mods_shot` / `saves_shot` 一致：**注入那个事实**，然后走**产品函数**
//! [`App::open_java_guard_dialog`]，于是弹框、i18n 文案、三枚按钮全部是产品代码画的。
//!
//! # 边界（不要把它读成产品能力）
//!
//! - 只在环境变量存在时生效（对应变量非空且不为 `0`）**且**本次是取证启动（`--screenshot`）；
//! - 注入的只是 `LaunchBlockedView` 那几个字段（与 `CoreEvent::LaunchBlocked` 同一个形状；
//!   M334 加了 `install_major`、M349 让它成了 `Option`（钉过实例 Java 时没有装机出口），现在九个）；
//!   判断（`check` / `decide` / `legacy_advisory_facts`）在 core，这里一个字都没重写；
//! - 数字取真实组合：M331 那档是「1.12.2 + 只装了 17」（要求 `min = max = 8` ⇒ `TooNew`）；
//!   M349 两档是「1.7.10 + Java 21」（没实测过）与「1.12.2 + Java 17」（实测过），
//!   要求都是 `min = 8`、**没有上限**（M348），所以方向是 `Advisory(_)`。

use unlucky_core::game::java::{MEASURED_LEGACY_JAVA, MEASURED_LEGACY_VERSION};
use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AdvisoryEvidence, AdvisoryTier, JavaGuardDirection, LaunchBlockedView,
};

use crate::app::state::App;

/// 夹具开关是否打开（变量非空且不为 `0`）。
fn on(name: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = value.trim().to_string();
    if value.is_empty() || value == "0" {
        return None;
    }
    Some(value)
}

/// 注入一次启动前的事实（走产品函数弹框）。
///
/// 两个夹具互斥且都有「先到先得」的优先级：M349 的开关在前（它是本模块后来的用途，
/// 同时设两个变量时以它为准，免得拍出来的图与预期不符）。
pub(crate) fn seed(app: &mut App) {
    if !app.has_screenshot() {
        return;
    }
    let lang = app.lang();
    if let Some(view) = advisory_fixture(lang) {
        app.open_java_guard_dialog(&view);
        return;
    }
    if on("ULL_JAVA_GUARD_SHOT").is_none() {
        return;
    }
    let blocked = LaunchBlockedView {
        id: "1.12.2".to_string(),
        name: "生存服 1.12.2".to_string(),
        version_id: "1.12.2".to_string(),
        direction: JavaGuardDirection::TooNew,
        required_min: 8,
        required_max: Some(8),
        actual: 17,
        install_major: Some(8),
        evidence: None,
    };
    app.open_java_guard_dialog(&blocked);
}

/// M349 的两档建议框（`ULL_LEGACY_ADVISORY_SHOT=measured|unmeasured`）。
///
/// 值不认识时按 `unmeasured` 出图（那是更该被看着验收的那一档：更旧的版本、更肯定的建议，
/// 正文最长）；同时把认不出的值打到 stderr，出图的人不会拿错图当证据。
fn advisory_fixture(lang: Lang) -> Option<LaunchBlockedView> {
    let raw = on("ULL_LEGACY_ADVISORY_SHOT")?;
    // 实例名是**数据**（不是界面文案），但出图要能证明「英文档里没有中文残留」，
    // 所以英文图给一个英文实例名（复审 P3-4）。
    let (legacy_name, measured_name) = match lang {
        Lang::En => ("Nostalgia 1.7.10", "Survival 1.12.2"),
        Lang::ZhCn => ("怀旧服 1.7.10", "生存服 1.12.2"),
    };
    let (version_id, name, actual, tier, pinned) = match raw.as_str() {
        "measured" => ("1.12.2", measured_name, 17, AdvisoryTier::Measured, false),
        "pinned" => ("1.7.10", legacy_name, 21, AdvisoryTier::Unmeasured, true),
        other => {
            if other != "unmeasured" {
                eprintln!(
                    "[shot] ULL_LEGACY_ADVISORY_SHOT={other} 不认识，按 unmeasured 出图（认得的：measured / unmeasured / pinned）"
                );
            }
            ("1.7.10", legacy_name, 21, AdvisoryTier::Unmeasured, false)
        }
    };
    Some(LaunchBlockedView {
        id: version_id.to_string(),
        name: name.to_string(),
        version_id: version_id.to_string(),
        direction: JavaGuardDirection::Advisory(tier),
        // 老版本原版的要求：下限 8、**没有上限**（M348）。
        required_min: 8,
        required_max: None,
        actual,
        // 「下载 Java 8」：该版本按设计用的那个；`pinned` 那一档**没有**这枚按钮（复审 r2 P2-1）。
        install_major: (!pinned).then_some(8),
        // 证据与产品**同源**：真实事件由 presenter 从 core 的常量带过来，夹具读同一对常量
        // （不写字面——写死的话改 `MEASURED_LEGACY_VERSION` 时只有产品变、图不变，图就在撒谎）。
        evidence: Some(AdvisoryEvidence {
            version_id: MEASURED_LEGACY_VERSION.to_string(),
            java: MEASURED_LEGACY_JAVA,
        }),
    })
}
