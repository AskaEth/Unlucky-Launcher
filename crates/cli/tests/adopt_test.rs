//! M221 CLI 端到端：`ull instances scan` / `ull instances adopt` 的真实入口（解析 → 执行 → 两条输出）。
//!
//! 用的都是**真磁盘**数据根（`support::TempDir`）+ 一份「别的启动器铺好的」`.minecraft`。
//! 每条用例都断言外部文件哈希/mtime 不变（`fixture::assert_unchanged`），
//! 因为「只登记不搬文件」是本 mission 的核心承诺。

#[path = "adopt_test/cli_surface.rs"]
mod cli_surface;
#[path = "adopt_test/collision_cases.rs"]
mod collision_cases;
#[path = "adopt_test/fixture.rs"]
mod fixture;
#[path = "support/mod.rs"]
mod support;

use std::path::Path;

use support::{parse_json, run_cli, TempDir};

use crate::fixture::{assert_unchanged, foreign_data_root, snapshot, snapshot_opt, Node};

/// 缺省登记：落 ULL 数据区的独立目录，隔离档 `custom`，**不沿用全局 `isolated`**。
///
/// 抓得住：
/// - 缺省沿用全局 `isolated`（运行目录会变成 `versions/1.20.1`，第二条断言会红）——
///   这是本 mission 点名的最容易漏的一条；
/// - 命令只登记不输出（`--json` 缺 `instance`）；
/// - 登记过程写了外部 `.minecraft`（哈希断言）。
///
/// 抓不住：GUI 是否也能触发同一条路（GUI 接线是后续 mission）。
#[test]
fn adopt_without_isolation_uses_a_separate_run_dir_and_keeps_global_isolated_out_of_it() {
    let dir = TempDir::new("adopt-default");
    let root = dir.path();
    foreign_data_root(root);
    // 全局档显式写成 isolated（D20 的缺省）：登记**不许**继承它去踩别人的版本目录。
    set_global_isolated(&dir);
    let minecraft = root.join(".minecraft");
    let before = snapshot(&minecraft);

    let (code, stdout, stderr) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let after = snapshot(&minecraft);
    assert_unchanged(&before, &after, "adopt 之后");

    let value = parse_json(&stdout);
    let instance = &value["data"]["instance"];
    assert_eq!(instance["isolation"], "custom");
    assert_eq!(instance["run_dir_source"], "instance_dir");
    let run_dir = instance["run_dir"].as_str().expect("run_dir");
    assert!(
        run_dir.contains("instances"),
        "缺省运行目录应落 ULL 数据区：{run_dir}"
    );
    assert!(
        !Path::new(run_dir).starts_with(&minecraft),
        "缺省运行目录不许落进别人的 .minecraft：{run_dir}"
    );
    assert!(
        !run_dir.ends_with("versions/1.20.1") && !run_dir.ends_with("versions\\1.20.1"),
        "不许沿用全局 isolated 的 versions/<id>：{run_dir}"
    );
    assert_eq!(instance["created"], true);

    // 再走一次 `instances list`：档案真的进去了（不是只写了实例文件）。
    let (code, stdout, _) = run_cli(&["instances", "list", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0);
    let listed = parse_json(&stdout);
    assert_eq!(listed["data"]["instances"][0]["version_id"], "1.20.1");
    assert_eq!(listed["data"]["instances"][0]["version_present"], true);
}

/// 登记之后 `launch --dry-run` 的 `--gameDir` 就是选定的运行目录（本 mission 的功能落点）。
///
/// 抓得住：登记把隔离档写错（`inherit`）⇒ 启动派生坚持全局 `isolated`，`game_dir` 变成
/// `versions/1.20.1`（第二条断言会红）；`gameDir` 没进启动计划。
/// 抓不住：真进程是否真的用这个 CWD（要真跑游戏，本 mission 不跑）。
#[test]
fn the_launch_plan_points_game_dir_at_the_chosen_run_dir() {
    let dir = TempDir::new("adopt-launch");
    let root = dir.path();
    foreign_data_root(root);
    set_global_isolated(&dir);

    let (code, stdout, stderr) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let adopted = parse_json(&stdout);
    let run_dir = adopted["data"]["instance"]["run_dir"]
        .as_str()
        .expect("run_dir")
        .to_string();
    let instance_id = adopted["data"]["instance"]["id"]
        .as_str()
        .expect("instance id")
        .to_string();

    let dir_arg = dir.arg();
    let args = [
        "launch",
        instance_id.as_str(),
        "--dry-run",
        "--json",
        "--data-dir",
        dir_arg.as_str(),
    ];
    let (code, stdout, stderr) = run_cli(&args);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let plan = parse_json(&stdout);
    assert_eq!(plan["data"]["game_dir"], run_dir.as_str());
    assert_eq!(plan["data"]["isolation"], "custom");
    assert_eq!(plan["data"]["isolation_source"], "instance");
    let command = plan["data"]["argv"]
        .as_array()
        .expect("启动计划里应有 argv");
    let game_dir_flag = command
        .windows(2)
        .find(|pair| pair[0].as_str() == Some("--gameDir"))
        .map(|pair| pair[1].as_str().unwrap_or_default().to_string())
        .expect("命令行里应有 --gameDir");
    // `argv` 是**脱敏后**的（主目录 → `<home>`，见 `core::launch::redact`），所以比尾段而不是全等；
    // 全等的那条断言在 `data.game_dir` 上（那是未脱敏的权威值）。
    let tail = format!(
        "instances/{}",
        Path::new(&run_dir)
            .file_name()
            .expect("运行目录名")
            .to_string_lossy()
    );
    assert!(
        game_dir_flag.ends_with(&tail) || game_dir_flag.ends_with(&tail.replace('/', "\\")),
        "--gameDir 必须指向选定的运行目录：{game_dir_flag} 应以 {tail} 结尾"
    );
}

/// `--isolation shared`：明确选共享 ⇒ 运行目录 = 别人的游戏根，**人读输出必须说出后果**。
///
/// 抓得住：shared 档没有告知（stdout 里没有「共用」）、shared 被写成 custom + 自己的目录、
/// 登记往游戏根写文件（哈希断言）。
/// 抓不住：用户是否真的看见这句（UI 验收另说）。
#[test]
fn shared_isolation_states_the_consequence_in_human_output() {
    let dir = TempDir::new("adopt-shared");
    let root = dir.path();
    foreign_data_root(root);
    let minecraft = root.join(".minecraft");
    let before = snapshot(&minecraft);

    let (code, stdout, stderr) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--isolation",
        "shared",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let after = snapshot(&minecraft);
    assert_unchanged(&before, &after, "shared 登记之后");
    assert!(
        stdout.contains("共用"),
        "必须说出「共用别人的 .minecraft」：{stdout}"
    );
    assert!(stdout.contains("一个字节都没动"), "{stdout}");
}

/// `--dry-run`：只检查不写盘（连配置目录都不建）。
///
/// 抓得住：`--dry-run` 真的写了档案或建了运行目录（快照会红）。
/// 抓不住：`--dry-run` 有没有顺带读了不该读的东西。
#[test]
fn dry_run_writes_nothing() {
    let dir = TempDir::new("adopt-dry");
    let root = dir.path();
    foreign_data_root(root);
    let before = snapshot(root);
    // 判据强度自检（R5）：快照必须含**目录**节点，否则「凭空建个空目录」看不出来。
    assert!(
        matches!(before.get(".minecraft"), Some(Node::Dir { .. })),
        "快照缺少目录节点：{:?}",
        before.get(".minecraft")
    );
    assert!(
        before.contains_key(".minecraft/versions"),
        "快照缺少 versions 目录节点"
    );

    let (code, stdout, stderr) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--dry-run",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let after = snapshot(root);
    assert_unchanged(&before, &after, "--dry-run 之后");
    // `--dry-run` 连 `.ULL` 这个**空目录**都不许建（存在性判据用 `snapshot_opt`）。
    assert!(
        snapshot_opt(&root.join(".ULL")).is_none(),
        "--dry-run 不该建配置目录"
    );
    assert!(stdout.contains("检查通过"), "{stdout}");
    assert!(stdout.contains("没有写任何文件"), "{stdout}");
}

/// 坏的版本（缺父）⇒ 退出码 1，`--json` 里带**逐条**问题码。
///
/// 抓得住：不合格版本被登记成功（退出码变 0）、问题码被折叠成一句散文（`detail.problems` 缺项）。
/// 抓不住：用户在原来的启动器里修好之后重试（自然覆盖）。
#[test]
fn not_adoptable_version_exits_one_with_structured_problems() {
    let dir = TempDir::new("adopt-refuse");
    let root = dir.path();
    foreign_data_root(root);
    let before = snapshot(root);

    let (code, stdout, _) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1-orphan",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1, "{stdout}");
    let value = parse_json(&stdout);
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "adopt_not_adoptable");
    assert_eq!(
        value["error"]["detail"]["problems"][0]["code"],
        "missing_parent"
    );
    let after = snapshot(root);
    assert_unchanged(&before, &after, "失败之后");
}

/// 重复登记 ⇒ 退出码 1 + 无副作用（档案逐字节不变）。
///
/// 抓得住：重复登记当成功（退出码 0）或失败时改了档案。
/// 抓不住：并发两次登记（没有锁；这不是并发入口）。
#[test]
fn repeating_the_same_adopt_exits_one_without_side_effects() {
    let dir = TempDir::new("adopt-repeat");
    let root = dir.path();
    foreign_data_root(root);
    let (code, _, stderr) = run_cli(&["instances", "adopt", "1.20.1", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "{stderr}");
    let config = root.join(".ULL/config");
    let before = snapshot(&config);

    let (code, stdout, _) = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--json",
        "--data-dir",
        &dir.arg(),
    ]);
    assert_eq!(code, 1, "{stdout}");
    assert_eq!(
        parse_json(&stdout)["error"]["code"],
        "adopt_already_registered"
    );
    let after = snapshot(&config);
    assert_unchanged(&before, &after, "重复登记失败之后");
}

/// `instances scan`：只读列出候选，坏版本逐条给原因。
///
/// 抓得住：扫描变成空列表（那是本 mission 点名不许的行为）、坏版本被静默跳过、
/// 扫描写了文件。
/// 抓不住：真实 `.minecraft` 里那些平台特有的怪目录（夹具只覆盖已知形状）。
#[test]
fn scan_lists_candidates_and_reasons_without_writing() {
    let dir = TempDir::new("adopt-scan");
    let root = dir.path();
    foreign_data_root(root);
    let before = snapshot(root);

    let (code, stdout, stderr) =
        run_cli(&["instances", "scan", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "stdout={stdout}\nstderr={stderr}");
    let after = snapshot(root);
    assert_unchanged(&before, &after, "scan 之后");

    let value = parse_json(&stdout);
    assert_eq!(value["data"]["counts"]["total"], 2);
    assert_eq!(value["data"]["counts"]["adoptable"], 1);
    let candidates = value["data"]["candidates"].as_array().expect("candidates");
    let ok = candidates
        .iter()
        .find(|item| item["version_id"] == "1.20.1")
        .expect("有 1.20.1");
    assert_eq!(ok["adoptable"], true);
    assert_eq!(ok["jar_present"], true);
    let orphan = candidates
        .iter()
        .find(|item| item["version_id"] == "1.20.1-orphan")
        .expect("有 1.20.1-orphan");
    assert_eq!(orphan["adoptable"], false);
    assert_eq!(orphan["problems"][0]["code"], "missing_parent");
}

/// 把全局隔离档设成 `isolated`（走 schema 校验的正式入口，不手写 launcher.json）。
fn set_global_isolated(dir: &TempDir) {
    let dir_arg = dir.arg();
    let (code, _, stderr) = run_cli(&[
        "config",
        "set",
        "instances.mode",
        "isolated",
        "--data-dir",
        dir_arg.as_str(),
    ]);
    assert_eq!(code, 0, "设置全局隔离档失败：{stderr}");
}
