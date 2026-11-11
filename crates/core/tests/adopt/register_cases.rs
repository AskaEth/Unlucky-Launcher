//! `plan` / `adopt` 的落点与写盘面：登记只写自己的配置，外部 `.minecraft` 一个字节都不动。
//!
//! 每条用例都带「抓得住 / 抓不住」，并且**外部快照断言**是每条的核心：`versions/`
//! `libraries/` `assets/` `mods/` `saves/` `config/` `launcher_profiles.json` 的路径集合、
//! 字节数、内容哈希、mtime 全都不许变（`fixture::assert_unchanged`）。

use unlucky_core::game::adopt::{self, AdoptTarget};
use unlucky_core::game::install::instance::{read_instance_config, read_profiles, Isolation};
use unlucky_core::game::isolation::{derive_game_dir, IsolationMode};

use crate::fixture::{assert_unchanged, foreign_minecraft, request, snapshot, TempRoot};

/// 缺省（不给 `--isolation`）：运行目录落 ULL 数据区的独立目录，隔离档写 `custom`。
///
/// 抓得住：缺省沿用全局 `isolated`（那样 `run_dir` 会变成 `versions/1.20.1`）、
/// `gameDir` 没落盘、外部文件被写（哈希/mtime 断言）、profiles 没登记。
/// 抓不住：真实启动是否真的把 `--gameDir` 用上（core 侧只验派生结果，CLI 侧用 `launch --dry-run` 验）。
#[test]
fn default_target_lands_in_the_launcher_data_area() {
    let root = TempRoot::new("default");
    foreign_minecraft(&root.game_root());
    let before = snapshot(&root.game_root());

    let outcome = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("登记应成功");
    let after = snapshot(&root.game_root());
    assert_unchanged(&before, &after, "缺省登记之后");

    assert_eq!(outcome.isolation, Isolation::Custom);
    assert!(outcome.created);
    assert!(outcome.dir_created, "缺省目录应被新建");
    let instances_dir = root.path().join(".ULL/data/instances");
    assert!(
        outcome.run_dir.starts_with(&instances_dir),
        "运行目录必须在 ULL 数据区：{}",
        outcome.run_dir.display()
    );
    assert!(outcome.run_dir.is_dir());
    assert!(
        !outcome.run_dir.starts_with(root.game_root()),
        "缺省运行目录不许落在别人的 .minecraft 里"
    );

    // 落盘的三样东西：profiles.json、实例设置、新建的空运行目录。
    let profiles = read_profiles(&root.launcher()).expect("读档案");
    assert_eq!(profiles.profiles.len(), 1);
    assert_eq!(profiles.profiles[0].name, "1.20.1");
    assert_eq!(profiles.profiles[0].version_id, "1.20.1");
    let config = read_instance_config(&root.launcher(), &outcome.instance_id)
        .expect("读设置")
        .expect("设置文件存在");
    assert_eq!(config.isolation, Some(Isolation::Custom));
    assert_eq!(config.game_dir.as_deref(), Some(outcome.run_dir.as_path()));

    // 启动时 `--gameDir` 真的会取这个目录（D20 覆盖链的权威求值）。
    let derivation = derive_game_dir(
        &root.game(),
        "1.20.1",
        IsolationMode::Isolated,
        Some(&config),
        None,
    )
    .expect("派生运行目录");
    assert_eq!(derivation.game_dir, outcome.run_dir);
    assert_eq!(derivation.isolation, Isolation::Custom);
}

/// `--isolation shared`：运行目录 = 别人的游戏根，且**人读提示必须说出后果**。
///
/// 抓得住：shared 档被当成 custom 写 `game_dir`、提示里没提「共用/别人的 .minecraft」、
/// 登记过程往游戏根写了东西（哈希断言）。
/// 抓不住：用户是否真的读到了这句提示（那属于 UI 验收）。
#[test]
fn shared_target_points_at_the_game_root_and_says_what_it_means() {
    let root = TempRoot::new("shared");
    foreign_minecraft(&root.game_root());
    let before = snapshot(&root.game_root());

    let outcome = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::Shared),
    )
    .expect("登记应成功");
    let after = snapshot(&root.game_root());
    assert_unchanged(&before, &after, "shared 登记之后");

    assert_eq!(outcome.isolation, Isolation::Shared);
    assert_eq!(outcome.run_dir, root.game_root());
    assert!(
        outcome.notes.iter().any(|note| note.contains("共用")),
        "必须告知「共用别人的 .minecraft」：{:?}",
        outcome.notes
    );
    let config = read_instance_config(&root.launcher(), &outcome.instance_id)
        .expect("读设置")
        .expect("设置文件存在");
    assert_eq!(config.isolation, Some(Isolation::Shared));
    assert_eq!(config.game_dir, None, "shared 档不该固化成 custom");
}

/// `--isolation isolated`：运行目录 = `versions/<id>`，**登记这一刻仍不往那里写任何东西**。
///
/// 抓得住：`VersionDir` 被漏做（`isolation` 不是 `Isolated`）、提示里没提「会新增玩家数据」、
/// 登记当场就在版本目录里建了 `mods/`（下面的目录内容断言会红）。
/// 抓不住：首次启动之后游戏自己往里写（那是启动链路的事，本 mission 不跑真游戏）。
#[test]
fn version_dir_target_is_explicit_and_writes_nothing_yet() {
    let root = TempRoot::new("isolated");
    foreign_minecraft(&root.game_root());
    let before = snapshot(&root.game_root());

    let outcome = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::VersionDir),
    )
    .expect("登记应成功");
    let after = snapshot(&root.game_root());
    assert_unchanged(&before, &after, "isolated 登记之后");

    assert_eq!(outcome.isolation, Isolation::Isolated);
    assert_eq!(outcome.run_dir, root.game_root().join("versions/1.20.1"));
    assert!(
        outcome.notes.iter().any(|note| note.contains("新增")),
        "必须告知「第一次启动会在这里新增玩家数据」：{:?}",
        outcome.notes
    );
    assert!(!outcome.dir_created, "版本目录已存在，不该新建");
    let mut entries: Vec<String> = std::fs::read_dir(&outcome.run_dir)
        .expect("读版本目录")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec!["1.20.1.jar".to_string(), "1.20.1.json".to_string()],
        "版本目录里不该多出任何东西"
    );
}

/// 展示名已被别的实例占用 ⇒ 显式报错，不改任何档案（不静默复用别的版本）。
///
/// 抓得住：`register` 的幂等复用路径被当成「导入成功」（那样第二个版本会指向第一个实例的档案）。
/// 抓不住：CLI 的 hint 文案（那条在 cli 侧用例里）。
#[test]
fn a_taken_name_is_an_explicit_error() {
    let root = TempRoot::new("dupname");
    foreign_minecraft(&root.game_root());
    adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("第一个登记成功");
    let profiles_before =
        std::fs::read(root.path().join(".ULL/config/profiles.json")).expect("读档案");

    let mut second = request("1.20.1-fabric-0.15.0", AdoptTarget::InstanceDir);
    second.name = Some("1.20.1".to_string());
    let error = adopt::adopt(&root.game(), &root.launcher(), &second).expect_err("重名应被拒");
    assert_eq!(error.code(), "adopt_duplicate_name");
    let profiles_after =
        std::fs::read(root.path().join(".ULL/config/profiles.json")).expect("读档案");
    assert_eq!(profiles_before, profiles_after, "失败不许改档案");
}

/// 同一个版本重复登记 ⇒ 无副作用失败（`adopt_already_registered`），档案逐字节不变。
///
/// 抓得住：重复登记变成「静默成功」（第二条断言会红），或失败时仍改写了档案。
/// 抓不住：并发两次 adopt 的竞态（没有锁；本命令不是并发入口，如实记在这里）。
#[test]
fn repeating_the_same_adopt_fails_without_side_effects() {
    let root = TempRoot::new("repeat");
    foreign_minecraft(&root.game_root());
    adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("第一次登记成功");
    let config_dir = root.path().join(".ULL/config");
    let before = snapshot(&config_dir);

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("重复登记应报错");
    assert_eq!(error.code(), "adopt_already_registered");
    let after = snapshot(&config_dir);
    assert_unchanged(&before, &after, "重复登记失败之后");
}

/// 缺省运行目录已存在且非空 ⇒ 拒绝（不猜「合并进已有实例」，不覆盖用户内容）。
///
/// 抓得住：非空目录被复用（内容被游戏写进去只是时间问题）、拒绝时仍写了档案。
/// 抓不住：空目录复用那条正向路径（见下一条用例）。
#[test]
fn a_non_empty_default_run_dir_is_refused() {
    let root = TempRoot::new("occupied");
    foreign_minecraft(&root.game_root());
    let plan = adopt::plan(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("先算一次落点");
    std::fs::create_dir_all(&plan.run_dir).expect("建目录");
    std::fs::write(plan.run_dir.join("someone-elses-file.txt"), b"keep me").expect("写文件");

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("非空目录应被拒");
    assert_eq!(error.code(), "adopt_target_not_empty");
    assert_eq!(
        std::fs::read(plan.run_dir.join("someone-elses-file.txt")).expect("读文件"),
        b"keep me"
    );
    assert!(
        !root.path().join(".ULL/config/profiles.json").exists(),
        "拒绝时不许留档案"
    );
}

/// 缺省运行目录存在但**为空** ⇒ 复用（上次失败的残留不该让用户永远重试不了）。
///
/// 抓得住：空目录也被拒（那就把「先建目录再登记」这条路堵死了）。
/// 抓不住：目录里的隐藏文件（`read_dir` 能看见，但平台差异在这里没覆盖）。
#[test]
fn an_empty_default_run_dir_is_reused() {
    let root = TempRoot::new("empty-dir");
    foreign_minecraft(&root.game_root());
    let plan = adopt::plan(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("先算一次落点");
    std::fs::create_dir_all(&plan.run_dir).expect("建空目录");

    let outcome = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect("空目录可复用");
    assert!(!outcome.dir_created, "已存在的空目录不算新建");
    assert_eq!(outcome.run_dir, plan.run_dir);
}

/// 数据区里预先放了指向外部的目录联接 ⇒ 缺省运行目录拒绝沿它写盘。
///
/// 抓得住：`create_dir_all` 直接沿联接在数据根之外建目录（外部会多出条目，最后一条断言红），
/// 或错误码不是 `adopt_target_invalid`。
/// 抓不住：数据根**自己**是链接的情形——那是用户的数据目录迁移手法，允许（判据是落点仍在
/// 解析后的数据根内）；也抓不住检查与建目录之间的 TOCTOU。
#[cfg(windows)]
#[test]
fn a_junction_inside_the_data_area_does_not_get_written_through() {
    let root = TempRoot::new("datajunction");
    foreign_minecraft(&root.game_root());
    let outside = root.path().join("outside-instances");
    std::fs::create_dir_all(&outside).expect("建外部目录");
    // 逐段 join：`mklink` 把正斜杠当开关，拼成一串带 `/` 的路径会被判「无效参数」。
    let data_instances = root.path().join(".ULL").join("data").join("instances");
    std::fs::create_dir_all(data_instances.parent().expect("父目录")).expect("建 data 目录");
    if !crate::fixture::link_guard(
        crate::fixture::try_junction(&outside, &data_instances),
        "数据区目录联接",
    ) {
        return;
    }

    let error = adopt::adopt(
        &root.game(),
        &root.launcher(),
        &request("1.20.1", AdoptTarget::InstanceDir),
    )
    .expect_err("数据区里的联接必须被拒");
    assert_eq!(error.code(), "adopt_target_invalid", "{error}");
    let entries: Vec<String> = std::fs::read_dir(&outside)
        .expect("读外部目录")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(
        entries.is_empty(),
        "不许沿联接在数据根之外建目录：{entries:?}"
    );
}
