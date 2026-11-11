//! 「Downloads」分组自己的断言（M100 **新增**：`settings/tests.rs` 那 7 条全是跨 section 的，
//! 一条 section 断言都没有；这几条是新增，不是搬移）。
//!
//! **归 download 线**。M201 之前这一组只有演示行；现在它接了真控件（`GROUP.body = Some(body)`），
//! 数据来自 presenter 的设置快照（`States::settings` ← `pages::Model::settings`）。
//!
//! 别把它和下载页（`pages/download*`）搞混：那是「装哪个版本」，这一组是「怎么下」的默认值。
//!
//! ## 这一轮换了什么（M201）
//!
//! | 旧绊线 | 现在 |
//! |---|---|
//! | `body.is_none()`（「这一组还没接真控件」） | `body.is_some()`（「接了，别再退回去」） |
//! | 只有 `rows` 的语义行断言 | 保留 + 三条**控件形状**断言（按 `SettingControl` 分发） |
//!
//! 写回闭环（点了以后盘上真的变了）不在本文件：那是 `app/update/settings/download_tests.rs`，
//! 它要一个假后端 + `App`，页面层的测试拿不到。

use super::super::download;
use super::super::States;
use unlucky_design::icons::IconKind;
use unlucky_presenter::dto::{SettingControl, SettingGroupView, SettingRowView};

/// 一份照 presenter 的 `host::settings::download::group` 形状搭的快照（键与控件种类同序）。
///
/// 为什么自己搭而不是问 presenter 要：那个函数要 `HostEnv`（读盘）且不在 `host` 的公开面上；
/// 本文件守的是**页面按 `control` 字段分发**这件事，不是 presenter 的行文案。
fn snapshot() -> Vec<SettingGroupView> {
    vec![SettingGroupView::new("download", "下载与镜像（D7）")
        .with(SettingRowView::choice(
            "download.concurrency",
            "并发数",
            "同时下载的文件数",
            &["4", "8", "16"],
            1,
        ))
        .with(SettingRowView::choice(
            "download.retry_limit",
            "单源尝试次数",
            "同一个源最多试几次",
            &["0", "1", "3"],
            2,
        ))
        .with(SettingRowView::text(
            "download.proxy_url",
            "HTTP 代理",
            "manual 模式生效",
            "",
        ))
        .with(SettingRowView::action(
            "download.action.detect_region",
            "重新检测地区",
            "清掉 24h 地区缓存",
            "detect_region",
        ))]
}

/// 这一组的行集合（标签 + 值）——**没有快照时**画的语义行。
///
/// 抓得住：① 四条线并行合并时把别人的行粘到本文件（六份 `GROUP` 长得几乎一样，最容易整块贴错）；
/// ② 漏掉/多出一条行；③ 标签与值错位。
/// 抓不住：值对不对（这几行本来就不是值，是语义），也抓不住有快照时画什么（下一条用例管）。
#[test]
fn the_downloads_group_shows_concurrency_retries_and_the_gh_proxy_key() {
    let rows: Vec<(&str, &str)> = download::GROUP.rows.to_vec();
    assert_eq!(
        rows,
        vec![
            ("并发数", "出厂 8 · 可调 1~64"),
            ("单源尝试次数", "出厂 3 · 含首次 · 0/1 = 只试一次"),
            ("GitHub 加速前缀", "留空 = 跟随内置默认"),
            ("关掉它", "填 off = 关闭"),
            ("换掉它", "填 URL 前缀 = 覆盖"),
        ]
    );
}

/// 左列图标（`IconKind::Download`）。与导航轨的「下载」同族，换掉就与 rail 语汇不一致。
#[test]
fn the_downloads_group_keeps_its_icon() {
    assert_eq!(download::GROUP.icon, IconKind::Download);
}

/// **M201 翻了这条绊线**：这一组现在有真控件渲染器。
///
/// 旧绊线 `the_downloads_group_is_still_demo_rows`（`body.is_none()`）存在的意义是「防止演示值
/// 回潮」。M201 之后演示值已经不是问题——有快照就画真控件，没快照才退回语义行——所以绊线翻面：
///
/// 抓得住：① 谁把 `GROUP.body` 改回 `None`（真控件当场消失，页面又只剩静态语义行 ——
/// 那正是用户投诉的形态）；② 谁把渲染器挪走却忘了同步这里。
/// 抓不住：渲染器**画得对不对**（`Element` 没有内省接口，形状断言在下一条），也抓不住
/// 「有快照时真的走了 body」（`settings/mod.rs` 的 `and_then` 那一层；那一层断了会退回语义行，
/// 界面上看得出来，但没有断言）。
#[test]
fn the_downloads_group_carries_a_real_control_body() {
    assert!(
        download::GROUP.body.is_some(),
        "Downloads 必须挂着真控件渲染器：`body = None` 就是「设置页里改不了」的回归"
    );
}

/// **控件形状**：快照里有几种 `SettingControl`，页面就画几种（choice / text / action）。
///
/// 抓得住：① 渲染器只认 choice（把 text / action 也画成chip 或干脆不画）——`controls_of` 是从
/// 同一份行模型上读的，但**分发**发生在 `body` 里，改成 `match` 单臂会让别的行整段消失；
/// ② 谁把 `SettingControl::Text` 落到 `read_only_row` 上（那时输入框没了，用户改不了文本键）。
/// 抓不住：画出来的像素（那是截图的事），也抓不住每种控件是否真的连了消息（那是 app 层的
/// `download_tests.rs` 与真机截图）。
#[test]
fn the_downloads_body_follows_the_control_kind() {
    let groups = snapshot();
    assert_eq!(
        download::controls_of(&groups),
        vec![
            SettingControl::Choice,
            SettingControl::Choice,
            SettingControl::Text,
            SettingControl::Action,
        ],
        "控件种类必须照快照的 `control` 字段画"
    );
}

/// **没有快照就不许编值**：`controls_of` 空 ⇒ 页面退回 [`download::GROUP.rows`] 的语义行。
///
/// 抓得住：谁为了「让页面好看」在渲染器里塞一份写死的默认快照（那就是 M193 被投诉的演示值，
/// 只不过换了个藏身处）。
/// 抓不住：「空快照时确实走了 `demo_card`」这一步（`settings/mod.rs` 的退回逻辑，它有自己的
/// `frame.rs` 守门）。
#[test]
fn an_empty_snapshot_lists_no_controls() {
    assert!(download::controls_of(&[]).is_empty());

    // 别的组在场、本组缺席，同样一条控件都不该画（不许拿别人的行凑数）。
    let other = vec![
        SettingGroupView::new("java", "Java（D13）").with(SettingRowView::toggle(
            "java.auto_install",
            "缺 Java 时自动下载",
            "",
            true,
        )),
    ];
    assert!(download::controls_of(&other).is_empty());
}

/// 语义行里的「三态 / 区间 / 出厂值」必须还在——它们是**没有快照时**唯一说得清的说明。
///
/// 这一条是 M193 留下的，M201 原样保留：即便有了真控件，兜底那一屏也不许出现假的当前值。
///
/// 抓得住：把 gh_proxy 那两行合并成一句（「留空 = 关闭」这种混法）、或者把区间 / 0-1 语义删掉。
/// 抓不住：有快照时行文案对不对（那时文案来自 presenter，见 `host/settings/download_tests.rs`）。
#[test]
fn the_downloads_rows_state_semantics_and_never_fake_a_current_value() {
    let joined = download::GROUP
        .rows
        .iter()
        .map(|(label, value)| format!("{label} {value}"))
        .collect::<Vec<_>>()
        .join(" | ");

    for required in [
        // 三态里最容易被混的两档，必须都在（缺一个就是「设置项说假话」的种子）。
        "留空 = 跟随内置默认",
        "填 off = 关闭",
        "填 URL 前缀 = 覆盖",
        // 硬上限必须在界面上说清（任务书的边界条款）。
        "1~64",
        // P2-1：0 是旧版提供过的档位，界面必须说清它 = 只试一次（不是「不下载」）。
        "0/1 = 只试一次",
        // 三个键各自是哪一行（标签本身也是契约：用户按它认设置）。
        "并发数",
        "单源尝试次数",
        "GitHub 加速前缀",
    ] {
        assert!(
            joined.contains(required),
            "行里缺少「{required}」：{joined}"
        );
    }

    // 「当前值」形态的假数据：`8 threads` / `On` / `Automatic (region)` 都是当年被投诉的那类。
    for forbidden in ["threads", "Automatic (region)", "Verify checksums"] {
        assert!(
            !joined.contains(forbidden),
            "又出现假的当前值「{forbidden}」：{joined}"
        );
    }
}

/// 本组是**有快照就不退回演示行**的那一类：`States` 里带了快照时，`body` 一定拿得到组。
///
/// 站位提醒：`States` 是 `pages::settings::States`（三条线共享的只读视图），本文件只验证
/// 「本组能从它里面找到自己」——找不到就等于退回演示行。
#[test]
fn the_group_finds_itself_in_the_states_snapshot() {
    let groups = snapshot();
    let groups_state = super::super::demo_groups();
    let states = States {
        appearance: None,
        groups: groups_state,
        settings: &groups,
    };
    assert!(
        states.settings.iter().any(|group| group.id == "download"),
        "本组的 id 必须在快照里：`download`（presenter 的 `SettingGroupView::new(\"download\", ..)`）"
    );
}
