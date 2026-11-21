//! `refresh.rs` 的用例（从 `refresh.rs` 拆出守 400 行硬线；M313 r1）。
//!
//! 只放两件事：①退场行数据池的**函数级**判据 ②它被 `refresh_view` **真的调了**的 App 级判据
//! （P2-1：第一版的接线守卫是重言式，删掉调用点仍全绿）。
//!
//! *注：`pages/**` 的「不许碰时长」守卫把页面测试文件也当产品代码，所以页面侧不能推进时间轴；
//! 这里用 `motion::rows::drawn` 直接看泵那一侧的状态，不推进时间（退场腿走完的行为由
//! `components/motion/tests/rows.rs` 的算术用例守）。*

use super::carry_leaving_rows;
use crate::app::components::motion;
use crate::app::state::App;
use crate::cli::Options;
use crate::pages;
use crate::theme::Families;
use unlucky_design::color::Role;
use unlucky_design::motion::MotionScale;
use unlucky_presenter::dto::{
    InstanceRuntime, InstanceSummary, LaunchArgsView, LaunchFailureCause, LoaderInfo, LoaderKind,
};
use unlucky_presenter::mock::MockBackend;
use unlucky_presenter::router::Route;
use unlucky_presenter::settings_bridge::SettingsBridge;

const FAMILIES: Families = Families {
    text: "Segoe UI",
    display: "Segoe UI",
    mono: "Consolas",
};

fn row(id: &str) -> pages::model::Row {
    pages::model::Row {
        id: id.to_string(),
        name: format!("实例 {id}"),
        supporting: "1.20.4".to_string(),
        status: None,
    }
}

fn model(ids: &[&str]) -> pages::Model {
    let mut model = pages::Model::new(pages::Screen::Instances, 1180.0, 760.0, None);
    model.instances.rows = ids.iter().map(|id| row(id)).collect();
    model
}

/// presenter 快照里的一个实例（只填这一条链路读得到的字段）。
fn instance(id: &str) -> InstanceSummary {
    InstanceSummary {
        id: id.to_string(),
        name: format!("实例 {id}"),
        mc_version: "1.20.4".to_string(),
        loader: LoaderInfo {
            kind: LoaderKind::Fabric,
            version: "0.16.5".to_string(),
        },
        last_played: None,
        runtime: InstanceRuntime::Idle,
        icon: None,
        isolation: "isolated".to_string(),
        version_id: "1.20.4".to_string(),
        version_present: true,
        launch: LaunchArgsView::default(),
    }
}

/// 跑真 `refresh_view` 的 App（mock 后端 + 内存设置桥：不碰盘、不碰网）。
fn app() -> App {
    App::with_presenter(
        Options::default(),
        FAMILIES,
        unlucky_design::color::SchemeFlavor::Light,
        Box::new(MockBackend::new()),
        SettingsBridge::memory_only(),
    )
}

/// **删掉的实例在淡出期间仍然有数据可画**（函数级：`carry_leaving_rows` 本身算得对）。
///
/// 抓得住：函数体被掏空 / 只看快照 —— `leaving` 会是空的，那一行只能画空壳；或者它把
/// **在册行**也搬进退场池（`known` 判据写错，同一行画两遍）。
/// 抓不住：`refresh_view` 有没有**调**它（那是下面那条 App 级用例的事）；
/// 也抓不住淡出的观感与帧数（`components/motion` 的算术 + 出图）。
#[test]
fn a_deleted_instance_keeps_its_row_data_alive_while_it_fades() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);

    // 上一帧：a / b / c 都在册（`app/view.rs::model` 在那一拍观察过这份清单）。
    let before = model(&["a", "b", "c"]);
    assert!(!motion::rows::observe(
        motion::rows::List::Instances,
        &["a", "b", "c"]
    ));

    // 这一拍：b 被删。presenter 的快照里已经没有它，`view::model` 只能给出 a / c。
    assert!(motion::rows::observe(
        motion::rows::List::Instances,
        &["a", "c"]
    ));
    let mut next = model(&["a", "c"]);
    assert!(
        next.instances.leaving.is_empty(),
        "前提：光靠快照重建不出退场行（这正是要搬的那一列）"
    );

    carry_leaving_rows(&before, &mut next);
    assert_eq!(next.instances.leaving.len(), 1, "退场行必须有数据可画");
    assert_eq!(next.instances.leaving[0].id, "b");
    assert_eq!(
        next.instances.leaving[0].name, "实例 b",
        "画的是同一行的旧数据"
    );

    // 在册行不许被搬进退场池（否则同一行画两遍）。
    assert!(!next.instances.leaving.iter().any(|row| row.id == "a"));
    motion::reset();
}

/// **`refresh_view` 真的调了 `carry_leaving_rows`**（App 级行为判据，M313 r1 P2-1 的主修）。
///
/// 为什么必须走真 App：第一版这条是用 `include_str!` + `contains` 写的，而 needle 字面量与
/// `include_str!` **在同一个文件里** ⇒ `contains` 恒真（复审实测：把调用点整行删掉，整个
/// lib 套件仍 1124 passed / 0 failed）。那是断言自己那一行，不是接线。
///
/// 抓得住：①调用点被删 / 被注释（`leaving` 恒空 ⇒ 删掉的实例淡出时只剩空壳）；
/// ②`previous` / `next` 弄反（从**新**模型里找那条已经不存在的行 ⇒ 一样搬不出东西）；
/// ③函数体被掏空。
/// 抓不住：调用位置（必须在 `self.view` 被替换之前——那由代码顺序读出来，没有用例守）；
/// 也抓不住淡出的观感与帧数（`components/motion` 的算术 + 出图）。
#[test]
fn deleting_an_instance_keeps_its_row_alive_through_refresh_view() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);

    let mut app = app();
    app.state.instances = vec![instance("keep"), instance("gone")];
    app.view_dirty = true;
    app.refresh_view();
    assert_eq!(
        app.view.instances.rows.len(),
        2,
        "前提：两条实例都在册（`tiles()` 没被搜索词过滤）"
    );
    assert!(
        app.view.instances.leaving.is_empty(),
        "前提：还没有任何退场行"
    );

    // 删掉 gone：presenter 的快照里再没有它 —— 这正是 `carry_leaving_rows` 要搬的那种行。
    app.state.instances.retain(|entry| entry.id != "gone");
    app.view_dirty = true;
    app.refresh_view();

    assert_eq!(app.view.instances.rows.len(), 1, "前提：在册只剩 keep");
    assert!(
        app.view
            .instances
            .leaving
            .iter()
            .any(|row| row.id == "gone"),
        "退场行的数据池里必须有 gone：不然那条退场行只剩空壳（P2-1 的原始缺陷）"
    );
    assert!(
        motion::rows::drawn(motion::rows::List::Instances)
            .iter()
            .any(|id| id == "gone"),
        "泵还在画 gone（退场腿没走完，行必须钉在原位）"
    );
    assert_eq!(
        app.view.instances.leaving[0].name, "实例 gone",
        "搬的是**上一帧**那一行的数据（`previous` / `next` 弄反就取不到它）"
    );
    motion::reset();
}

/// **接线（源码网，第二道）**：`refresh.rs` 里除定义之外还有一处**非注释**的调用。
///
/// 与上面那条 App 级用例的分工：这条只防「调用点被整段删掉 / 单行注释掉」，看得见代码形状，
/// 看不见行为；needle 字面量住在**本文件**（不在被扫描的 `refresh.rs` 里），所以不再自指
/// （P2-1 的原始缺陷正是自指）。
///
/// 抓得住：调用点被删 / 被 `//` 注释。
/// 抓不住：实参传错（类型上换不了位置）、调用位置、多行注释里的伪装。
#[test]
fn the_carry_is_wired_into_refresh_view() {
    let source = include_str!("../refresh.rs");
    let calls: usize = source
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .map(|line| line.matches("carry_leaving_rows(").count())
        .sum();
    assert!(
        calls >= 2,
        "`refresh.rs` 里只找到 {calls} 处 `carry_leaving_rows(`（期望 定义 + 调用点）：\
         实例删除时退场行没有数据可画"
    );
}

/// **A6 的接线（M314）**：`refresh_view` 真的把「状态行这一档」报给了帧泵。
///
/// 抓得住：`refresh_view` 里那行 `sync_launch_tone()` 被删 / 被注释（泵永远没账 ⇒
/// [`motion::launch::frame`] 恒 `None`，失败那一句永远不吃交叉淡入，页面只能退回正文色）。
/// 抓不住：淡入的观感与帧数（那是 `components/motion/tests/launch.rs` 的算术 + 出图）；
/// 也抓不住角色表选得对不对（那是 `pages::instances::launch_tone` 的单测）。
#[test]
fn the_launch_tone_reaches_the_pump_through_refresh_view() {
    motion::reset();
    motion::configure_for_test(MotionScale::STANDARD, None);

    let mut app = app();
    app.state.instances = vec![instance("alpha")];
    app.state.navigate(Route::Detail("alpha".to_string()));
    app.view_dirty = true;
    app.refresh_view();

    // 第一拍：空闲（没有状态行），落档、不起轨道。
    assert!(
        motion::launch::frame("alpha").is_none(),
        "空闲那一档没有状态行"
    );
    assert!(!motion::launch::running(), "落档不许起轨道");

    // 第二拍：运行中 → 状态行长出来了（版面变化，仍不起轨道），泵记的账认了这个实例。
    app.state.instances[0].runtime = InstanceRuntime::Running { pid: 4321 };
    app.view_dirty = true;
    app.refresh_view();
    let running =
        motion::launch::frame("alpha").expect("泵必须认下这个实例（接线断了这里就是 None）");
    assert_eq!(
        (running.from, running.to),
        (Role::OnSurface, Role::OnSurface),
        "运行中 = 正文色，且直接落档"
    );
    assert_eq!(running.progress, 1.0);

    // 第三拍：失败（红句）→ **起跑**交叉淡入。
    app.state.instances[0].runtime = InstanceRuntime::Failed {
        cause: LaunchFailureCause::Crashed { exit_code: Some(1) },
    };
    app.view_dirty = true;
    app.refresh_view();
    let failing = motion::launch::frame("alpha").expect("有账");
    assert_eq!(
        (failing.from, failing.to),
        (Role::OnSurface, Role::Error),
        "两端必须是「上一档 → 失败红」"
    );
    assert_eq!(failing.progress, 0.0, "起手那一帧还是上一档的颜色");
    assert!(motion::launch::running(), "失败那一拍必须起跑淡入");
    motion::reset();
}
///
/// 为什么单独立一条：上面那两条都只判「调用存在」、「搬得对」，**判不了顺序**。把调用点挪到
/// 替换之后（`previous` 拿到的已经是新模型）时，退场行池恒空，而前两条仍然全绿——这正是
/// P28 记账的那种「看起来有守卫」的洞。
///
/// needle（两个 `find` 的字面量）住在**本文件**、被扫的是 `../refresh.rs`（P28-2 的规矩：
/// 自指重言式就是这么来的，needle 与断言不许同处一文件）。两份源码都先**去掉注释行**：
/// 注释掉的调用在 `find` 眼里仍然存在，那会让「把调用注释掉」这种破坏漏过去。
///
/// 抓得住：①调用点被挪到替换之后（下标反了 ⇒ 第一句红）；②调用点被整段删掉 / **注释**
/// 掉（`expect` 先炸，`find` 返回 `None`）。
/// 抓不住：多行注释（`/* */`）里的伪装；也抓不住「搬的是正确的两份模型」（那是上面那条
/// App 级用例的活）。
#[test]
fn the_carry_runs_before_the_view_is_replaced() {
    let source = include_str!("../refresh.rs")
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let call = source
        .find("carry_leaving_rows(&self.view")
        .expect("refresh.rs 里找不到 carry_leaving_rows(&self.view..) 的调用点");
    let replace = source
        .find("self.view = next")
        .expect("refresh.rs 里找不到 self.view = next 的替换点");
    assert!(
        call < replace,
        "carry_leaving_rows 必须在 self.view 被替换**之前**调用（实测调用点下标 {call} \
         ≥ 替换点下标 {replace}）：排在之后时 previous 拿到的已经是新模型，退场行池恒空"
    );
}
