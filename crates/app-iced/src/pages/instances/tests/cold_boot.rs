//! M302 · 冷启动首帧的**三态**（复审 r1 P2-1 的判据）。
//!
//! ## 背景（为什么这组不是「措辞」而是缺陷）
//!
//! 扫盘搬进后台线程之后，`App::dispatch(RefreshAll)` 返回时快照还没到（`pump` 只抽队列、
//! 不 `tick`），于是**首帧** `AppState::instances` 必然为空。改前那一帧画出的是：
//! 列头 `全部` + 计数 **0**、右栏「**还没有实例**」+「导入一个已经装在盘上的版本」**实心 CTA**
//! ——一句假话，而且那枚 CTA 是能点的。改后必须分成三态：**首个快照未到 / 确实零实例 / 有实例**。
//!
//! ## 判据怎么落在「真控件树」上（iced 0.14 的 `Element` 没有文本内省）
//!
//! 真控件树能数的是**节点**，不是文字内容，所以这组按两条腿走：
//!
//! | 要断的事 | 用什么 | 抓不住什么 |
//! |---|---|---|
//! | 未就绪时**没有**空态 CTA | 真树里 `Button` 节点的条数：未就绪 = 有实例那一屏，只有「确实零实例」多一枚 | 哪一枚（按条数判，不按文案判） |
//! | 超时那支**有**重试按钮 | 同上：比未就绪多一枚 | 点下去派了什么（`app/update/instances.rs` 的用例管） |
//! | 列表项 = N | 每多一行多 2 条文字节点（名字 + 版本），用两条测量做差分 | 行里写的是什么（模型断言 / 出图） |
//! | 占位在、且**不是**「还没有实例」 | 源码断言：`Scanning` 那一支调 `scan_placeholder(.., texts.scanning(), ..)`，不碰 `empty_none_*` / `adopt_cta` | 屏幕上的那句字（同上） |
//!
//! 基线数字（实测，`ScanPhase` 三态 × 1180×760）：未就绪 12 条文字 / 7 枚按钮；超时 13 / 8；
//! 已就绪零实例 14 / 8；已就绪三行 19 / 7。按钮 7 里有 5 枚来自 `scrollable` 自带的滚动条，
//! 所以断言一律写成**态与态之间的差分**，不钉绝对值——加一条页头动作不该让这组红。

use iced::advanced::widget::tree::Tag;
use iced::advanced::widget::Tree;
use iced::widget::text;
use iced::Element;

use unlucky_design::color::SchemeFlavor;
use unlucky_design::typography::Platform;
use unlucky_presenter::dto::LaunchArgsView;
use unlucky_presenter::router::NavSection;

use crate::app::components::buttons::{self, Button};
use crate::pages::instances::LaunchForm;
use crate::pages::layout::source;
use crate::pages::model::{Instances, Row, ScanPhase};
use crate::pages::{app, Action, Message, Model, Screen};
use crate::theme::{Families, Ui};

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

/// 一行实例在真控件树里贡献的**文字节点数**（headline 名字 + supporting 版本；`status: None` ⇒ 2）。
///
/// 为什么用「行的文字节点」数行数：`navigation::list_item` 的根是**通用容器**，它的 tree tag
/// 与页面上每个卡片相同（实测空列表也能数出 8 个），所以不能按 tag 数行。
/// 这条不变式本身也是判据：谁给一行加/减一段文字，下面的差分断言立刻红，逼着一起改这里。
const TEXTS_PER_ROW: usize = 2;

fn ui() -> Ui {
    Ui::with_platform(SchemeFlavor::Light, Platform::Windows, FAMILIES)
}

fn form() -> LaunchForm {
    LaunchForm::from_view(None, &LaunchArgsView::default())
}

/// 一屏实例页模型（宽窗 = list-detail，右侧没有选中项）。
fn model(scan: ScanPhase, rows: Vec<Row>) -> Model {
    let mut model = Model::new(
        Screen::Instances,
        1180.0,
        760.0,
        Some(NavSection::Instances),
    );
    model.instances = Instances {
        rows,
        detail: None,
        scan,
        // 用例直接构造模型 = 没走 `motion::rows::observe`，所以没有退场池
        leaving: Vec::new(),
    };
    model
}

fn rows(count: usize) -> Vec<Row> {
    (0..count)
        .map(|index| Row {
            id: format!("i{index}"),
            name: format!("demo-{index}"),
            supporting: "1.20.4".to_string(),
            status: None,
        })
        .collect()
}

// ------------------------------------------------------------------ 真控件树取样

fn tree_of(el: &Element<'_, Message>) -> Tree {
    Tree::new(el.as_widget())
}

/// 文字节点的标签（`text` 与包着它的 `container` 同 tag，见 iced 的 `container::tag` 委托）。
fn text_tag() -> Tag {
    let probe: Element<'static, Message> = text("probe").into();
    tree_of(&probe).tag
}

/// 按钮节点的标签（页头两枚 `buttons::text`、空态那枚 `buttons::filled`、滚动条自带的按钮
/// 都算：判据要的正是「这一屏一共有几枚可点的按钮」）。
fn button_tag(ui: &Ui) -> Tag {
    let probe = buttons::filled(ui, Button::new("probe", Message::Action(Action::Back)));
    tree_of(&probe).tag
}

fn count_tag(tree: &Tree, tag: Tag) -> usize {
    usize::from(tree.tag == tag)
        + tree
            .children
            .iter()
            .map(|child| count_tag(child, tag))
            .sum::<usize>()
}

/// 一屏的 `(文字节点数, 按钮数)`。
fn counts(ui: &Ui, form: &LaunchForm, model: &Model) -> (usize, usize) {
    let el = app(ui, model, form);
    let tree = tree_of(&el);
    (
        count_tag(&tree, text_tag()),
        count_tag(&tree, button_tag(ui)),
    )
}

// ------------------------------------------------------------------ ① 首帧（无快照）

/// **首帧画的是占位，不是假空态**：没有空态 CTA、没有列表行、文案走 `scanning()`。
///
/// 抓得住：① `ScanPhase::Scanning` 那一支被删掉 / 落回 `detail_empty(.., false)`（复审判定的
/// 那屏假空态：「0 个实例」+「还没有实例」+ 实心 CTA）；② 未就绪时被塞了行（每条实例多 2 条
/// 文字节点）；③ 未就绪时计数位写了数字（多一条文字节点）。
/// 抓不住：屏幕上那句字长什么样（`Element` 无内省 ⇒ 文案 key 由 [`the_scanning_branch_never_touches_the_empty_state_copy`] 钉）。
#[test]
fn the_first_frame_is_a_placeholder_not_the_empty_state() {
    let ui = ui();
    let form = form();

    let (scanning_texts, scanning_buttons) =
        counts(&ui, &form, &model(ScanPhase::Scanning, Vec::new()));
    let (empty_texts, empty_buttons) = counts(&ui, &form, &model(ScanPhase::Ready, Vec::new()));
    let (rows_texts, rows_buttons) = counts(&ui, &form, &model(ScanPhase::Ready, rows(3)));

    assert_eq!(
        scanning_buttons, rows_buttons,
        "未就绪那一屏必须与「有实例」同构：**没有**空态那枚 CTA（按钮数相同 = 少的那枚不存在）"
    );
    assert_eq!(
        empty_buttons,
        scanning_buttons + 1,
        "「确实零实例」比未就绪恰好多一枚按钮——那枚就是「导入已有版本」的 CTA"
    );
    assert_eq!(
        empty_texts,
        scanning_texts + 2,
        "已就绪空态比未就绪多两条文字（计数位 + 空态副文案），同时少了占位那两条里的… —— \
         实际组合见文件头；差得不是 2 说明有一屏多/少了文字节点"
    );
    assert!(
        scanning_texts < rows_texts,
        "未就绪那屏比「有三条实例」少（每行两条文字：{} vs {}）",
        scanning_texts,
        rows_texts
    );
}

// ------------------------------------------------------------------ ② 快照到达

/// **快照到达后占位消失、列表项 = N**。
///
/// 抓得住：`Ready` 分支没把行画出来；行被画成别的节点（`list_item` 一条 2 条文字的不变式被破坏）。
/// 抓不住：行的文字内容与排序（模型断言 / 出图）。
#[test]
fn a_landed_snapshot_replaces_the_placeholder_with_the_rows() {
    let ui = ui();
    let form = form();

    let (one, _) = counts(&ui, &form, &model(ScanPhase::Ready, rows(1)));
    let (three, _) = counts(&ui, &form, &model(ScanPhase::Ready, rows(3)));

    assert_eq!(
        three - one,
        2 * TEXTS_PER_ROW,
        "三行比一行多 {} 条文字节点 ⇒ 每行 {TEXTS_PER_ROW} 条（名字 + 版本）。\
         少了说明行没画出来，多了说明行里塞了别的东西",
        2 * TEXTS_PER_ROW
    );
}

// ------------------------------------------------------------------ ③ 确实零实例

/// **真·空数据根（扫完、零实例）画的是空态引导，不是「扫描中」**。
///
/// 与 ① 的区别只在 `scan` 一位：`Ready + 零行` 才是「还没有实例」那句话的合法场合。
/// 抓得住：把 `Ready` 也画成占位（用户永远看不到「去导入一个」的出路）；把占位的文案接到空态 key 上
/// （① 的文字差分与本条的按钮差分一起红）。
/// 抓不住：`scan` 这一位在真后端里什么时候变成 `Ready`——那由 `app/update/instances.rs` 的
/// app 级用例（快照/超时/空快照三种事件）与 presenter 的 `refresh_tests` 用真空盘钉。
#[test]
fn a_finished_scan_with_zero_instances_shows_the_empty_guide() {
    let ui = ui();
    let form = form();

    let (_, scanning_buttons) = counts(&ui, &form, &model(ScanPhase::Scanning, Vec::new()));
    let (_, empty_buttons) = counts(&ui, &form, &model(ScanPhase::Ready, Vec::new()));
    let (_, rows_buttons) = counts(&ui, &form, &model(ScanPhase::Ready, rows(2)));

    assert_eq!(
        empty_buttons,
        rows_buttons + 1,
        "零实例 + 已就绪 ⇒ 比「有实例」多一枚按钮：那枚「导入已有版本」的 CTA 是此刻唯一的出路"
    );
    assert_eq!(
        empty_buttons,
        scanning_buttons + 1,
        "同一枚 CTA 在未就绪时**不**存在（那是复审 P2-1 的假话：还没扫完就劝你去导入）"
    );
}

// ------------------------------------------------------------------ ④ 超时那支（有界兜底）

/// **超时那屏有一枚可点的「重新扫描」**（有界兜底：线程 panic / 卡在慢盘时用户还有出路）。
///
/// 抓得住：`Stalled` 那一支退化成普通占位（没有重试 ⇒ 用户只能重启启动器）。
/// 抓不住：点下去派了什么（`app/update/instances.rs` 的 `retry_scan_dispatches_a_scan_only_refresh`）。
#[test]
fn the_stalled_screen_offers_a_retry() {
    let ui = ui();
    let form = form();

    let (scanning_texts, scanning_buttons) =
        counts(&ui, &form, &model(ScanPhase::Scanning, Vec::new()));
    let (stalled_texts, stalled_buttons) =
        counts(&ui, &form, &model(ScanPhase::Stalled, Vec::new()));

    assert_eq!(
        stalled_buttons,
        scanning_buttons + 1,
        "超时那一屏必须多一枚按钮（「重新扫描」）"
    );
    assert_eq!(
        stalled_texts,
        scanning_texts + 1,
        "那枚按钮自带一条文字（「重新扫描」），其余与占位一致"
    );
}

// ------------------------------------------------------------------ ⑤ 源码结构（文案 key）

/// **扫描中那一支不许碰空态的文案与动作**（`Element` 无内省 ⇒ 文案身份只能读源码）。
///
/// 抓得住：`scan_placeholder` 里混进 `empty_none_*` / `adopt_cta` / `adopt_action`（把假话搬回来）；
/// `detail_side` 把 `Ready` 以外的东西交给 `detail_empty`（未就绪又走回假空态）；
/// 取词换到别的 key（占位会显示成空行或错文案）。
/// 抓不住：真的画出来的像素（出图那张）。
#[test]
fn the_scanning_branch_never_touches_the_empty_state_copy() {
    // 分派与占位在 `detail/scan.rs`（从 `detail.rs` 拆出去守 400 行硬线）
    let scan = include_str!("../detail/scan.rs");

    let dispatch = source::code_text(source::function_body(scan, "fn detail_side"));
    let scanning_at = dispatch.find("ScanPhase::Scanning").expect("扫描中那一支");
    let stalled_at = dispatch.find("ScanPhase::Stalled").expect("超时那一支");
    let ready_at = dispatch.find("ScanPhase::Ready").expect("就绪那一支");
    assert!(
        scanning_at < stalled_at && stalled_at < ready_at,
        "三态必须按 `scan` 分派（顺序 = 判据：未就绪先于「有没有行」）：\n{dispatch}"
    );
    assert!(
        !dispatch[..ready_at].contains("detail_empty"),
        "未就绪的两支**不许**落到 `detail_empty`（那会把「还没扫完」画成「还没有实例」）：\n{dispatch}"
    );
    assert!(
        dispatch[ready_at..].contains("detail_empty"),
        "`Ready` 那一支必须落到 `detail_empty`（零实例的引导在那里）：\n{dispatch}"
    );
    assert!(
        dispatch[..ready_at].contains("InstanceMessage::RetryScan"),
        "超时那一支必须接上「重新扫描」（否则那枚按钮点了没反应）：\n{dispatch}"
    );

    let placeholder = source::code_text(source::function_body(scan, "fn scan_placeholder"));
    for forbidden in [
        "empty_none_title",
        "empty_none_body",
        "adopt_cta",
        "adopt_action",
    ] {
        assert!(
            !placeholder.contains(forbidden),
            "占位里混进了 `{forbidden}`（那属于「确实零实例」那一支）：\n{placeholder}"
        );
    }

    // 取词入口：扫描中/超时读的是 M302 新加的三个 key，不是空态那两个
    let texts = include_str!("../text.rs");
    for (method, key) in [
        ("fn scanning", "Key::InstancesScanning"),
        ("fn scan_stalled", "Key::InstancesScanStalled"),
        ("fn scan_retry", "Key::InstancesScanRetry"),
    ] {
        let body = source::code_text(source::function_body(texts, method));
        assert!(
            body.contains(key),
            "`{method}` 没读 `{key}`（占位会显示成空行或错文案）：\n{body}"
        );
    }
}
