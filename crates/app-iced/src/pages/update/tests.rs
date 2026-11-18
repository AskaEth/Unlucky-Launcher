//! 自更新视图模型的单测（从 `pages/update.rs` 拆出，那边贴着 400 行硬线）。

use super::*;

use iced::advanced::widget::tree::Tag;
use iced::advanced::widget::Tree;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_i18n::Lang;

use crate::theme::Families;

/// 提示条的正文**同时**给体积与说明摘要（mission：提示条含版本号、体积、说明摘要）。
#[test]
fn the_available_banner_carries_both_size_and_notes() {
    for lang in [Lang::En, Lang::ZhCn] {
        let body = available_body(lang.catalog(), 21_500_000, "修掉启动崩溃");
        assert!(body.contains("20.5 MB"), "{lang:?} 缺体积：{body}");
        assert!(body.contains("修掉启动崩溃"), "{lang:?} 缺说明：{body}");
    }
    // 发布没写说明时只留体积，不留一个空尾巴。
    let bare = available_body(Lang::En.catalog(), 1024, "  ");
    assert!(bare.contains("1.0 KB"), "{bare}");
    assert!(!bare.contains('·'), "没有说明时不该留分隔符：{bare}");
}

/// 视图模型的提示条判据与 presenter 同序（就位 > 进行中 > 失败 > 新版本）。
#[test]
fn the_banner_priority_matches_the_presenter() {
    let mut update = Update {
        status: UpdateStatus::Available {
            version: "0.2.3".to_string(),
            size: 1,
            notes: String::new(),
            forced: false,
        },
        ..Update::default()
    };
    assert!(matches!(update.banner(), Some(Banner::Available { .. })));

    update.skipped = Some("0.2.3".to_string());
    assert!(update.banner().is_none(), "忽略之后不再打扰");

    // 忽略的是**那一个版本**：同一次运行里冒出 0.2.4 仍然提示（与 presenter 同一语义）。
    update.status = UpdateStatus::Available {
        version: "0.2.4".to_string(),
        size: 1,
        notes: String::new(),
        forced: false,
    };
    assert!(
        matches!(update.banner(), Some(Banner::Available { version, .. }) if version == "0.2.4"),
        "0.2.3 被忽略不该压住 0.2.4"
    );

    update.skipped = None;
    update.applying = true;
    assert!(matches!(update.banner(), Some(Banner::Applying)));

    update.ready = Some("0.2.3".to_string());
    assert!(matches!(update.banner(), Some(Banner::Ready { .. })));
}

// ---------------------------------------------------------------- M308：重试按 retryable 分流
//
// 判据分两条腿（真控件树读不到字符串：iced 0.14 的 `text` 是 stateless，`tag()` 恒为
// `Tag::stateless()`）：
//
// | 要断的事 | 用什么 | 抓不住什么 |
// |---|---|---|
// | 不可重试时**没有**「重试」按钮 / 可重试时**有** | 真控件树里按钮节点的条数（探针取 tag，与 `pages/instances/tests/cold_boot.rs` 同一手法） | 是哪一枚（按条数判） |
// | 那句「已拒绝、重试没用」 | `failed_body`（**唯一**取词入口，中英各一条） | 它被 layout 丢掉（那是像素的事） |
// | 提示条真的用了那个入口 | 源码守卫（断言 `banner()` 里那行调用还在） | 参数传对没传对（前一条管文案，这一条只管接线） |
//
// 变异：把 `failed_actions` 改成无条件返回那枚按钮 ⇒ 第一条用例红；
// 把 `banner()` 的正文改回 `reason.to_string()` ⇒ 第三条用例红（前两条抓不住这一种，写在明处）。

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

/// 一次**不可重试/可重试**的失败结论（`silent = false`：用户主动查过，提示条要出来）。
fn failed(retryable: bool) -> Update {
    Update {
        status: UpdateStatus::Failed {
            reason: "清单签名对不上清单字节".to_string(),
            retryable,
        },
        ..Update::default()
    }
}

/// 探针：任意一个 `buttons::*` 元素在树里的标签（`interaction::tracked` 那层包裹是同一个类型，
/// 所以「可选按钮」与「禁用的按钮」都算同一个 tag）。
fn button_tag(ui: &Ui) -> Tag {
    let probe = buttons::filled(
        ui,
        Button::new("probe", Message::Update(UpdateAction::CheckNow)),
    );
    Tree::new(probe.as_widget()).tag
}

fn count_tag(tree: &Tree, tag: Tag) -> usize {
    usize::from(tree.tag == tag)
        + tree
            .children
            .iter()
            .map(|child| count_tag(child, tag))
            .sum::<usize>()
}

/// 这条结论画出来的提示条里有几枚按钮（**真控件树**）。
fn button_count(ui: &Ui, update: &Update, catalog: Catalog) -> usize {
    let element = banner(ui, update, catalog).expect("这条用例的状态总要画一条提示条");
    count_tag(&Tree::new(element.as_widget()), button_tag(ui))
}

/// 验签类失败（`retryable = false`）：**那枚「重试」按钮不在树里**，正文换成「已拒绝、重试没用」。
///
/// 这是 M307 抓到的第二处不一致：`error.rs` 早把签名五兄弟标成不可重试，界面照旧画一颗重试。
#[test]
fn a_refused_update_draws_no_retry_button_and_says_why() {
    let ui = ui();
    let catalog = Catalog::of(Lang::ZhCn);
    let update = failed(false);

    assert_eq!(
        button_count(&ui, &update, catalog),
        0,
        "不可重试的失败里不许出现「重试」那枚按钮（重放一千次还是同一个坏包）"
    );

    let zh = failed_body(catalog, "清单签名对不上清单字节", false);
    assert!(
        zh.contains("这次更新已被拒绝"),
        "正文要说清「已被拒绝」：{zh}"
    );
    assert!(zh.contains("重试没用"), "正文还要说清「重试没用」：{zh}");
    assert!(
        zh.contains("清单签名对不上清单字节"),
        "原因要原样带出来（用户要知道是哪一步拒的）：{zh}"
    );
    let en = failed_body(Lang::En.catalog(), "the signature does not match", false);
    assert!(
        en.contains("retrying will not help"),
        "英文同一条判据：{en}"
    );
}

/// 可重试的失败（网络 / 摘要不符 / IO）：「重试」按钮**在**树里，且正文就是原因（不换句子）。
#[test]
fn a_retryable_failure_keeps_the_retry_button() {
    let ui = ui();
    let catalog = Catalog::of(Lang::ZhCn);
    let update = failed(true);

    assert_eq!(
        button_count(&ui, &update, catalog),
        1,
        "可重试的失败必须留一条重试的路（M161：失败不许把按钮永久钉死）"
    );
    assert_eq!(
        failed_body(catalog, "取更新清单失败：连接被重置", true),
        "取更新清单失败：连接被重置",
        "可重试的正文就是原因本身，别套「已拒绝」那句"
    );
}

/// 提示条的正文**走** `failed_body`（源码守卫）。
///
/// 为什么要这一条：`text` 控件没有状态，真控件树里读不到那句话；`failed_body` 自己的用例只能
/// 证明「函数返回值对」，证明不了「提示条用了它」。变异（把 `banner()` 的正文改回
/// `reason.to_string()`）只会让这一条红——这就是它的全部价值，说在明处。
#[test]
fn the_banner_routes_its_body_through_failed_body() {
    let source = include_str!("../update.rs");
    assert!(
        source.contains("failed_body(catalog, reason, retryable)"),
        "提示条的失败正文必须经取词入口 failed_body(...)：这句话不见了就等于「重试没用」又被吞掉"
    );
}

/// **M357 / P1-1**：条高与「它从页面高度里吃掉多少」的公式（数字钉死，改公式就红）。
///
/// 64 = 上下内边距 12×2 + `LabelLarge` 行高 20 + `Space::X1` 4 + `BodySmall` 行高 16；
/// 44 = 24 + 20（`Applying` 只有标题，连按钮都没有）。76 / 56 = 条高 + 与页面之间的 `Space::X3`。
///
/// **抓得住**：公式漏掉间距 / 把 `Applying` 也按「有正文」算 / 没有提示条时照样返回 76；
/// **抓不住**：条在屏幕上的像素高（那要图：`DevDocs/artifacts/narrowfix/base/settings-banner-*`，
/// 修前实测 64 / 44 与公式逐像素一致）。
#[test]
fn the_banner_height_is_the_page_budget_it_eats() {
    // 缺省演示态 = 有新版本（标题 + 正文 + 两个按钮）
    let available = demo();
    assert_eq!(available.banner().expect("有新版本").height(), 64.0);
    assert_eq!(available.banner_reserve(), 76.0, "64 + Space::X3 12");

    // 进行中：只有标题（`Applying` 不画正文、也没有按钮）
    let mut applying = demo();
    applying.applying = true;
    assert_eq!(applying.banner().expect("进行中").height(), 44.0);
    assert_eq!(applying.banner_reserve(), 56.0, "44 + 12");

    // 已是最新：没有条 ⇒ 一点都不许扣
    let mut up_to_date = demo();
    up_to_date.status = UpdateStatus::UpToDate {
        latest: Some("0.2.2".to_string()),
    };
    assert!(up_to_date.banner().is_none());
    assert_eq!(up_to_date.banner_reserve(), 0.0);
}
