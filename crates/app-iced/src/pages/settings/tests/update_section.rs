//! 设置页「更新」分组（M159）的断言：注册表位置、四条状态各有各的句子。
//!
//! 为什么只测「词表 + 判据」而不测控件：iced 的 `Element` 没有内省接口（`tests/mod.rs` 顶部那条
//! 写过理由），上屏文本只能从**取词的那一个入口**取。这里守的是「四种结论的句子不是同一句」
//! ——正是 mission 里「检查失败不等于已是最新」那条硬要求的最小可测形态。

use unlucky_i18n::{Catalog, Key, Lang};

use super::super::update::{GROUP as UPDATE_GROUP_META, UPDATE_GROUP};
use super::super::{GROUPS, SELECTED};
use crate::pages::update::UpdateStatus;
use crate::pages::{Screen, Update};

/// 更新组**下标固定为 4**：加组只追加不重排，插到它前面会让 `APPEARANCE_GROUP` / `GENERAL_GROUP`
/// 两个下标一起失效，也会把更新组自己的 `UPDATE_GROUP` 指到别人身上。
///
/// M174 之后它**不再是最后一组**（助手组追加在它后面）：这条断言因此从「len == 下标 + 1」
/// 改成「下标 4 上就是更新组，且它是**倒数第二**组」——追加语义仍然成立，而「更新组被挤走」
/// 照样会红。
#[test]
fn the_update_group_sits_at_the_end_of_the_registry() {
    assert_eq!(UPDATE_GROUP, 4, "更新组的下标不许变（加组只追加）");
    assert_eq!(
        GROUPS.len(),
        UPDATE_GROUP + 2,
        "更新组之后只该有 M174 的助手组"
    );
    assert_eq!(GROUPS[UPDATE_GROUP].name, UPDATE_GROUP_META.name);
    assert_eq!(GROUPS[UPDATE_GROUP].name, "Updates");
    assert!(SELECTED < GROUPS.len());
}

/// 四种结论的句子**互不相同**，且「失败」那一句必须把原因带出来。
///
/// 抓得住：把「检查失败」写成「已是最新」（那是 mission 明令禁止的撒谎：断网时会说「你是最新」）。
/// 抓不住：句子翻得顺不顺、颜色对不对（那是人读与截图的事）。
#[test]
fn the_four_outcomes_read_differently() {
    let catalog = Lang::ZhCn.catalog();
    let up_to_date = catalog.render(Key::SettingsUpdateUpToDate, &[("version", "0.2.2")]);
    let available = catalog.render(
        Key::SettingsUpdateAvailable,
        &[("version", "0.2.3"), ("size", "21.0 MB")],
    );
    let missing = catalog.render(Key::SettingsUpdateMissingArtifact, &[("version", "0.2.3")]);
    let failed = catalog.render(
        Key::SettingsUpdateFailed,
        &[("reason", "取更新清单失败：连接被重置")],
    );

    let all = [&up_to_date, &available, &missing, &failed];
    for (index, left) in all.iter().enumerate() {
        for right in all.iter().skip(index + 1) {
            assert_ne!(left, right, "两条结论说的是同一句话：{left} / {right}");
        }
    }
    assert!(
        failed.contains("连接被重置"),
        "失败那一句必须带出原因，实际：{failed}"
    );
    assert!(
        !failed.contains("0.2.2"),
        "失败那一句不该冒出当前版本号（读起来像「已是最新」）：{failed}"
    );
}

/// 页面视图模型与 presenter 的判据同序：就位 > 进行中 > 失败（仅主动查） > 有新版本。
///
/// 两处各算一次（presenter 判据 + 页面视图），顺序必须一致，否则同一份状态在提示条与设置页上
/// 会各说一套。这里只钉页面这一侧的顺序；presenter 那一侧在 `presenter` 的 `dto::update` 单测里。
#[test]
fn the_banner_priority_is_ready_then_applying_then_failure_then_available() {
    let mut update = Update {
        current_version: "0.2.2".to_string(),
        channel: "stable".to_string(),
        auto_check: true,
        status: UpdateStatus::Failed {
            reason: "连不上".to_string(),
            retryable: true,
        },
        checked_at: String::new(),
        applying: true,
        ready: None,
        skipped: None,
        silent: false,
    };
    assert!(matches!(
        update.banner(),
        Some(crate::pages::Banner::Applying)
    ));

    update.applying = false;
    assert!(matches!(
        update.banner(),
        Some(crate::pages::Banner::Failed { .. })
    ));

    update.status = UpdateStatus::Available {
        version: "0.2.3".to_string(),
        size: 1024,
        notes: String::new(),
        forced: false,
    };
    assert!(matches!(
        update.banner(),
        Some(crate::pages::Banner::Available { .. })
    ));

    update.ready = Some("0.2.3".to_string());
    assert!(matches!(
        update.banner(),
        Some(crate::pages::Banner::Ready { .. })
    ));
}

/// 演示模型里更新组能画（`pages_demo` 的 harness 路径）：`Model.update` 有默认值，不是 `None` 才画。
#[test]
fn the_demo_model_carries_an_update_snapshot() {
    let model = crate::pages::demo_model(Screen::Settings, 1180.0, 760.0);
    assert_eq!(model.update, crate::pages::update::demo());
    assert!(
        model.update.available().is_some(),
        "演示快照要带一个可装的版本"
    );
    // 其它页不掺演示更新状态（实例页/下载页的基线图不该多出一条提示条）。
    assert_eq!(
        crate::pages::demo_model(Screen::Instances, 1180.0, 760.0).update,
        Update::default()
    );
    let catalog: Catalog = Lang::En.catalog();
    assert!(!catalog.text(Key::SettingsGroupUpdate).is_empty());
}

/// **忙碌时按钮禁用，且禁用态与状态行同源**（M161 收的债：以前点了没反应）。
///
/// 三条判据：
/// ① `Checking` / `applying` 时 `can_check()` 与 `can_apply()` 都是 false（按钮画成置灰）；
/// ② 这两态下设置页的状态行**必须**是「正在检查…」/「正在安装…」——否则用户看到一颗灰按钮
///    却没有任何解释；
/// ③ 结论回来（失败也算）之后恢回可点：超时/网络失败不许把按钮永久钉死。
#[test]
fn the_buttons_are_disabled_exactly_while_something_is_running() {
    use super::super::update::status_line;

    let catalog = Catalog::of(Lang::ZhCn);
    let available = UpdateStatus::Available {
        version: "0.2.3".to_string(),
        size: 1024,
        notes: String::new(),
        forced: false,
    };

    // 静息：有候选 ⇒ 两颗按钮都能点。
    let idle = Update {
        status: available.clone(),
        ..Update::default()
    };
    assert!(!idle.busy());
    assert!(idle.can_check(), "静息时「立即检查」可点");
    assert!(idle.can_apply(), "有候选且不忙 ⇒ 「下载并安装」可点");

    // 正在查：两颗都置灰，状态行说「正在检查」。
    let checking = Update {
        status: UpdateStatus::Checking,
        ..idle.clone()
    };
    assert!(checking.busy(), "Checking 就是忙碌");
    assert!(!checking.can_check(), "查的时候不许再点「立即检查」");
    assert!(!checking.can_apply(), "查的时候不许装（presenter 也拒）");
    assert_eq!(
        status_line(catalog, &checking).0,
        catalog.text(Key::SettingsUpdateChecking),
        "灰按钮旁边必须有「正在检查…」这句话"
    );

    // 正在装：`applying` 也算忙碌（此时 status 可能仍是 Available）。
    let applying = Update {
        applying: true,
        ..idle.clone()
    };
    assert!(applying.busy());
    assert!(!applying.can_apply(), "装的时候安装键置灰，不是消失");
    assert!(!applying.can_check(), "装的时候也不许再来一条检查");
    // 状态行在这一档也必须说「正在安装…」（复审 P2 附注：此前只有截图兜着）。
    assert_eq!(
        super::super::update::status_text(catalog, &applying),
        (catalog.text(Key::SettingsUpdateApplying).to_string(), false),
        "applying 档的状态行必须是「正在安装…」且不是错误色"
    );
    // 同一档的两句不许撞（「正在安装」≠「正在检查」）。
    assert_ne!(
        catalog.text(Key::SettingsUpdateApplying),
        catalog.text(Key::SettingsUpdateChecking)
    );

    // 结论回来（这里是最坏的结论：失败）⇒ 恢回可点。
    let failed = Update {
        status: UpdateStatus::Failed {
            reason: "连不上".to_string(),
            retryable: true,
        },
        ..Update::default()
    };
    assert!(!failed.busy(), "失败 = 这一轮结束了");
    assert!(failed.can_check(), "失败之后必须能重试");
    assert!(!failed.can_apply(), "没有候选时安装键不可点");
}
