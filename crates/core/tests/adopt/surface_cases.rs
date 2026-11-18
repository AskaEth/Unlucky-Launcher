//! 写盘面与入口可用性：**配置目录被链出去**必须拒绝（R2）、**versions 入口断链**必须具名报错（R3）。
//!
//! 两类都属于「本 mission 承诺的东西被链接绕过」：前者是「只写自己的配置」被
//! `config/instances` 的目录联接打穿（会替别人的启动器在 `.minecraft/config` 里造 UUID 文件），
//! 后者是「坏东西不能当空列表」被 `read_dir` 的 NotFound 打穿（断链 junction 与「真没有」同态）。
//!
//! 两条都用**真链接夹具**；建不出链接时走 `fixture::link_guard`（强制模式下必红，见 R4）。

use unlucky_core::game::adopt::{self, AdoptTarget};

use crate::fixture::{
    assert_unchanged, foreign_minecraft, link_guard, request, snapshot, try_link_dir, TempRoot,
};

/// `config/instances` 被目录联接指到游戏根的 `config/`：adopt 必须先拒绝，且游戏根零写入。
///
/// 抓得住：
/// - 只核运行目录、不核配置写面（那样这次登记会 exit0，并在 `.minecraft/config/<uuid>.json`
///   多出一个文件——`before/after` 全节点快照与「外部 config 目录为空」两条都会红）；
/// - `plan`（dry-run）也放行（第四条断言红：预检说「可以」，真写才失败）；
/// - 拒绝时先写了目录/配置（快照与目录列举都会红）。
///
/// 抓不住：检查与写入之间的 TOCTOU（本模块不是并发/对抗入口）。
#[cfg(windows)]
#[test]
fn a_junction_on_the_instances_dir_must_not_write_into_the_game_root() {
    use crate::fixture::try_junction;

    let root = TempRoot::new("cfg-instances-junction");
    foreign_minecraft(&root.game_root());
    let game_config = root.game_root().join("config");
    std::fs::create_dir_all(&game_config).expect("建别人的 config 目录");
    let instances_dir = root.path().join(".ULL").join("config").join("instances");
    std::fs::create_dir_all(instances_dir.parent().expect("config 目录")).expect("建 .ULL/config");
    if !link_guard(
        try_junction(&game_config, &instances_dir),
        "config/instances 目录联接",
    ) {
        return;
    }
    let before = snapshot(&root.game_root());
    // 别人的 `config/` 里本来就有文件（`example.toml` 是玩家配置）：判据是**没有新增条目**。
    let config_before = listing(&game_config);

    let plan_error = adopt::plan(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("配置写面被链走时，dry-run 也必须报错");
    assert_eq!(plan_error.code(), "adopt_write_surface_in_game_tree");

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("配置写面被链走时必须拒绝登记");
    assert_eq!(error.code(), "adopt_write_surface_in_game_tree");

    let after = snapshot(&root.game_root());
    assert_unchanged(&before, &after, "拒绝之后（游戏根零写入）");
    assert_eq!(
        config_before,
        listing(&game_config),
        "绝不许在别人的 .minecraft/config 里新增文件（尤其 UUID 配置）"
    );
    assert!(
        !listing(&game_config)
            .iter()
            .any(|name| name.ends_with(".json") && name.len() == 41),
        "不该出现 UUID 形状的 json：{:?}",
        listing(&game_config)
    );
}

/// `config/` 整层被链到游戏根：同样拒绝（判定粒度到**每一个**写盘目标，不只看 `instances`）。
///
/// 抓得住：只检查 `config/instances` 而不检查 `config/` 与 `profiles.json`（那样这条会 exit0）。
/// 抓不住：跨卷/跨盘的整体迁移（`<.ULL>` 自己是联接）——那是允许的，见 `run_dir` 模块文档。
#[test]
fn a_linked_config_dir_must_not_write_into_the_game_root() {
    let root = TempRoot::new("cfg-dir-link");
    foreign_minecraft(&root.game_root());
    let game_config = root.game_root().join("config");
    std::fs::create_dir_all(&game_config).expect("建别人的 config 目录");
    let ull = root.path().join(".ULL");
    std::fs::create_dir_all(&ull).expect("建 .ULL");
    let config_dir = ull.join("config");
    if !link_guard(try_link_dir(&game_config, &config_dir), "config 目录链接") {
        return;
    }
    let before = snapshot(&root.game_root());

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("config/ 被链走时必须拒绝登记");
    assert_eq!(error.code(), "adopt_write_surface_in_game_tree");

    let after = snapshot(&root.game_root());
    assert_unchanged(&before, &after, "拒绝之后（游戏根零写入）");
}

/// `versions/` 入口**存在但断链**（目标被移走）⇒ 具名报错，而不是「这个数据根还没有版本」。
///
/// 抓得住：`read_dir` 的 `NotFound` 一律当空列表（旧实现；Windows 断链 junction 正是这个形状）
/// ——第一条断言会红成「成功空列表」。
/// 抓不住：`versions/` 是普通文件的情形（走 `GameError::io`，错误码不同，靠人读）。
#[test]
fn a_dangling_versions_entry_is_an_error_not_an_empty_scan() {
    let root = TempRoot::new("dangling-versions");
    std::fs::create_dir_all(root.game_root()).expect("建游戏根");
    let moved = root.path().join("moved-versions");
    std::fs::create_dir_all(moved.join("1.20.1")).expect("建将被移走的 versions");
    let versions = root.game_root().join("versions");
    if !link_guard(try_link_dir(&moved, &versions), "versions 入口链接") {
        return;
    }
    // 目标被移走 ⇒ 入口成为断链；lstat 仍然看得见它。
    std::fs::remove_dir_all(&moved).expect("移走目标");
    assert!(
        std::fs::symlink_metadata(&versions).is_ok(),
        "夹具有问题：断链入口应当仍能被 lstat 看到"
    );

    let error = adopt::scan(&root.game(), &root.launcher())
        .expect_err("断链的 versions 入口必须报错，不能当空列表");
    assert_eq!(error.code(), "adopt_versions_unreadable", "{error}");
}

/// `versions` 真不存在 ⇒ 仍然是空结果（别把「新数据根」误报成故障）。
///
/// 抓得住：为了修 R3 把所有 NotFound 都当错误（这条会红）。
/// 抓不住：真正不可读（EACCES）那一支——Windows 上造不可读目录不可靠。
#[test]
fn a_truly_missing_versions_dir_stays_empty() {
    let root = TempRoot::new("no-versions");
    std::fs::create_dir_all(root.game_root()).expect("建空游戏根");
    let scan = adopt::scan(&root.game(), &root.launcher()).expect("真不存在 ⇒ 空结果");
    assert!(scan.candidates.is_empty());
}

/// 目录里直接子项的名字（排序后，用于「有没有新增」的判据）。
///
/// cfg 必须与唯一的调用点（`a_junction_on_the_instances_dir_must_not_write_into_the_game_root`）
/// 对齐：那条用例是 `#[cfg(windows)]`（目录联接夹具只在 Windows 上建得出来），Linux 上调用点
/// 整块编掉，未 gate 的 helper 就成了 `dead_code`（CI 的 `-D warnings` 会让 clippy 退出 101）。
#[cfg(windows)]
fn listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("读目录")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

/// `.ULL/data` 被预先联接到**游戏树**上 ⇒ 默认登记必须拒绝（r2 R2.2），而不是写完再说「隔离」。
///
/// 复审 r2 的实跑：`data -> .minecraft` 时 dry-run 与 adopt 都 exit0，外部多出
/// `instances/<slug>` 且游戏根 mtime 变化，输出仍称「与别人的 .minecraft 隔离」。
///
/// 抓得住：写面重叠不核（那样这次登记会成功，游戏树快照与「外部零新增」断言都会红）；
/// 只核配置不核 data（`data` 面上的缺省运行目录仍会落进游戏树）。
///
/// 抓不住：检查与写入之间的 TOCTOU；也不把「后续游戏自己写盘」算作本判据的证据。
#[cfg(windows)]
#[test]
fn data_linked_into_the_game_tree_is_refused_before_any_write() {
    use crate::fixture::try_junction;

    let root = TempRoot::new("data-to-game");
    foreign_minecraft(&root.game_root());
    let data = root.path().join(".ULL").join("data");
    std::fs::create_dir_all(data.parent().expect(".ULL")).expect("建 .ULL");
    if !link_guard(try_junction(&root.game_root(), &data), "data -> 游戏树联接") {
        return;
    }
    let before = snapshot(&root.game_root());

    let plan_error = adopt::plan(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("dry-run 也必须拒绝与游戏树重叠的写盘面");
    assert_eq!(plan_error.code(), "adopt_write_surface_in_game_tree");

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("默认登记必须拒绝与游戏树重叠的 data 写面");
    assert_eq!(error.code(), "adopt_write_surface_in_game_tree");

    assert_unchanged(
        &before,
        &snapshot(&root.game_root()),
        "拒绝之后（游戏树零写入）",
    );
    assert!(
        !root.game_root().join("instances").exists(),
        "绝不许在别人的 .minecraft 里建 instances 目录"
    );
}

/// `.ULL` 整棵树被预先联接到游戏树 ⇒ 同样拒绝（配置 + data 一起落进游戏树）。
///
/// 抓得住：只核 `data` 或只核 `config/instances`（那样 `profiles.json` 与 `data/` 仍会写穿）。
///
/// 抓不住：`<.ULL>` 自己是链接但指向**游戏树之外**的情形——那是合法迁移，见下一条正向对照。
#[cfg(windows)]
#[test]
fn home_linked_into_the_game_tree_is_refused_before_any_write() {
    use crate::fixture::try_junction;

    let root = TempRoot::new("home-to-game");
    foreign_minecraft(&root.game_root());
    let home = root.path().join(".ULL");
    if !link_guard(try_junction(&root.game_root(), &home), ".ULL -> 游戏树联接") {
        return;
    }
    let before = snapshot(&root.game_root());

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("默认登记必须拒绝与游戏树重叠的 .ULL 写面");
    assert_eq!(error.code(), "adopt_write_surface_in_game_tree");

    assert_unchanged(
        &before,
        &snapshot(&root.game_root()),
        "拒绝之后（游戏树零写入）",
    );
}

/// **正向对照**：把 `data` 整体迁到游戏树**之外**仍然允许（迁移是合法的，重叠才拒）。
///
/// 抓得住：把「与游戏树重叠」写成「任何链接都拒」（这条正向用例会红）。
/// 抓不住：迁移后的数据目录内容（只断言登记成功且游戏树不变）。
#[test]
fn data_migrated_outside_the_game_tree_still_registers() {
    let root = TempRoot::new("data-outside");
    foreign_minecraft(&root.game_root());
    let external = root.path().join("external-data");
    std::fs::create_dir_all(&external).expect("建外部数据目录");
    let data = root.path().join(".ULL").join("data");
    std::fs::create_dir_all(data.parent().expect(".ULL")).expect("建 .ULL");
    if !link_guard(try_link_dir(&external, &data), "data -> 游戏树之外的联接") {
        return;
    }
    let before = snapshot(&root.game_root());

    let outcome = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("迁到游戏树之外的 data 应允许");
    // 对外返回的是**逻辑**路径（`<.ULL>/data/instances/<slug>`），真实位置才是迁移后的目录。
    let real = outcome.run_dir.canonicalize().expect("运行目录真实路径");
    let external_real = external.canonicalize().expect("外部数据目录真实路径");
    assert!(
        real.starts_with(&external_real),
        "运行目录的真实位置应落在迁移后的数据目录里：{}",
        real.display()
    );
    assert_unchanged(
        &before,
        &snapshot(&root.game_root()),
        "登记之后（游戏树零写入）",
    );
}

/// **更高祖先断链**（r2 R2.3）：完整版本树挂在迁移联接下面，联接目标被搬走后 scan 不许报空。
///
/// 复审 r2 的对照：`--data-dir link/nested` 断开前 total=1；把 `link` 的目标改名后，
/// `versions` 与游戏根本身都 NotFound，旧实现返回 `ok:true / total:0`。
///
/// 抓得住：只在 `versions` 与游戏根两层 lstat（更高层断链会漏，第二条断言红）；
/// 或反过来把「真正的新数据根」也判成错误（断开**之前**那条 total==1 的断言与
/// `a_truly_missing_versions_dir_stays_empty` 会红）。
#[test]
fn a_dangling_ancestor_above_the_game_root_is_not_an_empty_scan() {
    let root = TempRoot::new("ancestor-dangling");
    let real = root.path().join("target");
    let game = real.join("nested").join(".minecraft");
    std::fs::create_dir_all(game.join("versions").join("1.20.1")).expect("建完整版本树");
    std::fs::write(
        game.join("versions/1.20.1/1.20.1.json"),
        br#"{"id":"1.20.1","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写清单");
    let link = root.path().join("game-parent-link");
    if !link_guard(try_link_dir(&real, &link), "游戏根上层联接") {
        return;
    }
    let data_root = root.path().join("data");
    std::fs::create_dir_all(&data_root).expect("建数据根");
    let minecraft = link.join("nested").join(".minecraft");
    let game_layout = unlucky_core::game::GameLayout::new(&minecraft);
    let launcher = unlucky_core::platform::layout::LauncherLayout::new(data_root.join(".ULL"));

    // 断开之前：完整版本树看得见。
    let scan = adopt::scan(&game_layout, &launcher).expect("联接完好的树应可扫");
    assert_eq!(scan.candidates.len(), 1, "完整版本树应扫到 1 个候选");

    // 断开：把联接目标改名，入口还在但解析不出来（versions 与游戏根本身都 NotFound）。
    std::fs::rename(&real, root.path().join("moved-target")).expect("移走联接目标");
    assert!(
        std::fs::symlink_metadata(&link).is_ok(),
        "夹具有问题：断链入口应仍能被 lstat 看到"
    );

    let error =
        adopt::scan(&game_layout, &launcher).expect_err("祖先断链必须具名报错，不能当空结果");
    assert_eq!(error.code(), "adopt_versions_unreadable", "{error}");
}
