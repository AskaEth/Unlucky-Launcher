//! 运行目录**碰撞**：slug 同名 / 大小写不敏感 / 「空目录已被实例占着」/ 「认领目录还不存在」。
//!
//! 复审 r1 的 R1：旧实现只比展示名，第二次登记看到「目录是空的」就复用了 ⇒ 两个从未选择
//! shared/custom 的实例共用一个运行目录（`mods`/`saves` 互相污染）。
//! 判据必须是「有没有实例占着这个**真实**目录」，而不是「目录空不空」。

use unlucky_core::game::adopt::{self, AdoptTarget};
use unlucky_core::game::install::instance::read_instance_config;
use unlucky_core::game::install::pack_dir;

use crate::fixture::{foreign_minecraft, request, TempRoot};

/// **slug 碰撞**：`Better MC` 与 `Better-MC` 派生到同一个目录名 —— 两个实例必须落在不同目录。
///
/// 复审 r1 的 R1：旧实现只比展示名，第二次登记看到「目录是空的」就复用了，于是两个从未选择
/// shared/custom 的实例共用一个运行目录（`mods`/`saves` 互相污染）。
///
/// 抓得住：
/// - 判据只看目录空不空、不看已有实例占没占（两条 `run_dir` 不同的断言会红）；
/// - 顺延目录没建出来 / 提示里没点出撞的是谁。
///
/// 抓不住：`pack_dir::instance_dir_name` 本身把两个名字折成同一个目录（那是既有行为，
/// 本 mission 不改它，只保证**登记侧**不共用）。
#[test]
fn slug_colliding_names_do_not_share_a_run_dir() {
    let root = TempRoot::new("slug-collide");
    foreign_minecraft(&root.game_root());

    let mut first = request("1.20.1", AdoptTarget::InstanceDir);
    first.name = Some("Better MC".to_string());
    let a = adopt::adopt(&root.game(), &root.launcher(), &first).expect("第一次登记");
    let mut second = request("1.20.1", AdoptTarget::InstanceDir);
    second.name = Some("Better-MC".to_string());
    let b = adopt::adopt(&root.game(), &root.launcher(), &second).expect("第二次登记");

    assert_ne!(a.run_dir, b.run_dir, "两个实例不许共用同一个运行目录");
    assert!(
        a.run_dir.is_dir() && b.run_dir.is_dir(),
        "两个目录都要真实存在"
    );
    let bumped = b
        .run_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned());
    assert!(
        bumped.as_deref().is_some_and(|name| name.ends_with("-2")),
        "撞车后应顺延成 -2：{:?}",
        b.run_dir
    );
    assert!(
        b.notes.iter().any(|note| note.contains("同一个物理目录")),
        "必须把「撞了谁」说出来：{:?}",
        b.notes
    );

    // 两份实例设置各自指向自己的目录（不是都指第一个）。
    let config_a = read_instance_config(&root.launcher(), &a.instance_id)
        .expect("读设置")
        .expect("有设置文件");
    let config_b = read_instance_config(&root.launcher(), &b.instance_id)
        .expect("读设置")
        .expect("有设置文件");
    assert_eq!(config_a.game_dir.as_deref(), Some(a.run_dir.as_path()));
    assert_eq!(config_b.game_dir.as_deref(), Some(b.run_dir.as_path()));
}

/// **大小写碰撞**（Windows/macOS）：`Foo` 与 `foo` 是同一个物理目录 ⇒ 同样必须顺延。
///
/// 抓得住：`dir_key` 不做大小写归一（Windows 上第二次登记会复用同一个目录）。
/// 抓不住：Linux（大小写敏感，两条目录本来就不同——那里这条不适用，见 `cfg`）。
#[cfg(windows)]
#[test]
fn names_differing_only_in_case_do_not_share_a_run_dir() {
    let root = TempRoot::new("case-collide");
    foreign_minecraft(&root.game_root());

    let mut first = request("1.20.1", AdoptTarget::InstanceDir);
    first.name = Some("Foo".to_string());
    let a = adopt::adopt(&root.game(), &root.launcher(), &first).expect("第一次登记");
    let mut second = request("1.20.1", AdoptTarget::InstanceDir);
    second.name = Some("foo".to_string());
    let b = adopt::adopt(&root.game(), &root.launcher(), &second).expect("第二次登记");

    assert_ne!(a.run_dir, b.run_dir, "大小写不同不是「不同目录」的理由");
    let a_real = a.run_dir.canonicalize().expect("真实路径 A");
    let b_real = b.run_dir.canonicalize().expect("真实路径 B");
    assert_ne!(a_real, b_real, "两个运行目录的真实路径必须不同");
}

/// 空目录**已被别的实例占着** ⇒ 也不能当「失败残留」复用，要顺延。
///
/// 这是 R1 的另一半：判据必须是「有没有实例占着」，而不是「目录空不空」。
///
/// 抓得住：只按 `directory_occupied` 判（第一次登记建出的空目录会被第二次白白复用）。
/// 抓不住：目录非空且无人认领的情形（那条保持 `TargetNotEmpty`，见下一条用例）。
#[test]
fn an_empty_dir_owned_by_another_instance_is_not_reused() {
    let root = TempRoot::new("owned-empty");
    foreign_minecraft(&root.game_root());
    let mut first = request("1.20.1", AdoptTarget::InstanceDir);
    first.name = Some("Better MC".to_string());
    let a = adopt::adopt(&root.game(), &root.launcher(), &first).expect("第一次登记");
    // 目录此刻是空的（新建、还没启动过游戏）——旧判据会认为「可复用」。
    let entries_before = std::fs::read_dir(&a.run_dir).expect("读目录").count();
    assert_eq!(entries_before, 0, "夹具前提：这个目录此刻是空的");

    let mut second = request("1.20.1", AdoptTarget::InstanceDir);
    second.name = Some("Better-MC".to_string());
    let b = adopt::adopt(&root.game(), &root.launcher(), &second).expect("第二次登记");
    assert_ne!(b.run_dir, a.run_dir);
}

/// **认领的目录还不存在**时也必须能比对出碰撞。
///
/// 场景：第一个实例用 `--isolation custom --game-dir <某路径>` 登记（custom 档**不创建**目录），
/// 那条路径恰好等于第二个实例缺省派生出来的目录名。第二个实例看到的目录「不存在、非空判据为假」，
/// 但它已经被第一个实例认领了。
///
/// 抓得住：`claims`/`dir_key` 只在「路径存在」时才比较（那样第二条 `run_dir` 相同的断言会红）；
/// 抓不住：跨卷/大小写之外的别名（例如 8.3 短名），本判据不覆盖。
#[test]
fn a_claim_on_a_not_yet_existing_dir_still_collides() {
    let root = TempRoot::new("claim-missing");
    foreign_minecraft(&root.game_root());
    let default_dir = pack_dir::instance_dir(&root.launcher(), "Better MC");
    assert!(!default_dir.exists(), "夹具前提：这个目录还不存在");

    let mut first = request("1.20.1", AdoptTarget::Custom(default_dir.clone()));
    first.name = Some("Better MC".to_string());
    let a = adopt::adopt(&root.game(), &root.launcher(), &first).expect("custom 登记成功");
    assert_eq!(a.run_dir, default_dir);
    assert!(!default_dir.exists(), "custom 目标不该被创建");

    let mut second = request("1.20.1", AdoptTarget::InstanceDir);
    second.name = Some("Better-MC".to_string());
    let b = adopt::adopt(&root.game(), &root.launcher(), &second).expect("第二次登记成功");

    assert_ne!(b.run_dir, default_dir, "认领侧不存在也要能比对出碰撞");
    assert!(
        b.notes.iter().any(|note| note.contains("同一个物理目录")),
        "碰撞要写进 notes：{:?}",
        b.notes
    );
}

/// **Win32 尾点/尾空格别名**：`custom` 认领 `Foo.`（目录还没建）与缺省派生 `Foo` 是**同一个物理目录**。
///
/// 复审 r2 的 R2.1：普通路径的尾点/尾空格会被 Win32 折叠，旧实现把尚不存在的尾段原样拼进比较键，
/// 于是第二次默认登记仍拿到 `Foo`（两次都 exit0、两个 UUID、同一物理目录）。
///
/// 抓得住：`dir_key` 只做分隔符/大小写归一（`Foo.` 与 `Foo` 键不等，第二条断言红）。
///
/// 抓不住：verbatim 路径（`\\?\…`）——那种写法**不**折叠，是另一个物理目录，见下一条用例。
#[cfg(windows)]
#[test]
fn a_trailing_dot_claim_still_collides_with_the_derived_dir() {
    for suffix in [".", " "] {
        let root = TempRoot::new("win32-alias");
        foreign_minecraft(&root.game_root());
        let claimed = pack_dir::instance_dir(&root.launcher(), &format!("Foo{suffix}"))
            .with_file_name(format!("Foo{suffix}"));
        assert!(!claimed.exists(), "夹具前提：认领目录还不存在");

        let mut first = request("1.20.1", AdoptTarget::Custom(claimed.clone()));
        first.name = Some("First".to_string());
        let a = adopt::adopt(&root.game(), &root.launcher(), &first).expect("custom 登记成功");

        let mut second = request("1.20.1", AdoptTarget::InstanceDir);
        second.name = Some("Foo".to_string());
        let b = adopt::adopt(&root.game(), &root.launcher(), &second)
            .expect("第二次默认登记应成功（顺延到 -2，而不是复用同一目录）");

        assert_ne!(
            b.run_dir, a.run_dir,
            "尾点/尾空格别名不许让两个实例共用一个运行目录（suffix={suffix:?}）"
        );
        assert!(
            b.run_dir
                .file_name()
                .map(|name| name.to_string_lossy().ends_with("-2"))
                .unwrap_or(false),
            "应顺延成 -2：{}",
            b.run_dir.display()
        );
        assert!(
            !a.run_dir.exists(),
            "custom 认领的目录不该被创建：{}",
            a.run_dir.display()
        );
    }
}

/// 正向：**verbatim** 路径（`\\?\…`）不折叠 —— 它是另一个物理目录，必须允许并存。
///
/// 抓得住：归一化时盲目 trim（verbatim 的 `Foo.` 会被折叠成 `Foo`，这条第二次登记会顺延，
/// `-2` 断言红）。
///
/// 抓不住：别的重解析点形态；也不验 8.3 短名。
#[cfg(windows)]
#[test]
fn a_verbatim_trailing_dot_path_is_not_folded() {
    let root = TempRoot::new("verbatim");
    foreign_minecraft(&root.game_root());
    let base = pack_dir::instance_dir(&root.launcher(), "Bar");
    let verbatim =
        std::path::PathBuf::from(format!(r"\\?\{}", base.with_file_name("Bar.").display()));
    let mut first = request("1.20.1", AdoptTarget::Custom(verbatim.clone()));
    first.name = Some("First".to_string());
    adopt::adopt(&root.game(), &root.launcher(), &first).expect("verbatim custom 登记成功");

    let mut second = request("1.20.1", AdoptTarget::InstanceDir);
    second.name = Some("Bar".to_string());
    let b = adopt::adopt(&root.game(), &root.launcher(), &second).expect("默认登记成功");
    assert_eq!(
        b.run_dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned()),
        Some("Bar".to_string()),
        "verbatim 的 Bar. 与普通 Bar 是两个目录，不该顺延"
    );
}
