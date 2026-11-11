//! 运行目录**碰撞**的 CLI 面：slug 同名 / 大小写不敏感不许让两个实例共用一个目录（复审 r1 的 R1）。
//!
//! 从 `tests/adopt_test.rs` 拆出来只为守 400 行硬线；用例体未改。

use std::path::Path;

use crate::support::{parse_json, run_cli, TempDir};

use crate::fixture::foreign_data_root;

/// **slug 碰撞的两个展示名不能绑定同一个物理运行目录**（R1 的 CLI 面）。
///
/// 复审 r1 的实跑：`Better MC` 与 `Better-MC` 都成功、两个 UUID、却指向同一个目录，
/// 随后 `launch --dry-run` 的 `--gameDir` 也是同一个。这条把它钉住。
///
/// 抓得住：
/// - 登记侧不查已有实例占用的目录（两个 `run_dir` 相同，第一条断言红）；
/// - `launch` 的 `game_dir` 与登记的目录不一致（第三条断言红）。
///
/// 抓不住：`pack_dir` 的 slug 规则本身（既有行为，本 mission 不改）。
#[test]
fn colliding_slug_names_get_separate_run_dirs() {
    let dir = TempDir::new("adopt-slug");
    let root = dir.path();
    foreign_data_root(root);
    let dir_arg = dir.arg();

    let first = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--name",
        "Better MC",
        "--json",
        "--data-dir",
        dir_arg.as_str(),
    ]);
    assert_eq!(
        first.0, 0,
        "第一次登记应成功：stdout={} stderr={}",
        first.1, first.2
    );
    let first = parse_json(&first.1);
    let second = run_cli(&[
        "instances",
        "adopt",
        "1.20.1",
        "--name",
        "Better-MC",
        "--json",
        "--data-dir",
        dir_arg.as_str(),
    ]);
    assert_eq!(
        second.0, 0,
        "第二次登记应成功：stdout={} stderr={}",
        second.1, second.2
    );
    let second = parse_json(&second.1);

    let first_dir = first["data"]["instance"]["run_dir"]
        .as_str()
        .expect("run_dir A")
        .to_string();
    let second_dir = second["data"]["instance"]["run_dir"]
        .as_str()
        .expect("run_dir B")
        .to_string();
    assert_ne!(first_dir, second_dir, "两个实例不许共用运行目录");
    assert!(
        Path::new(&first_dir).is_dir() && Path::new(&second_dir).is_dir(),
        "两个运行目录都要真实存在"
    );

    // 真启动计划（dry-run）也要分开：两条实例各自的 game_dir 不同。
    for (value, expect) in [(&first, &first_dir), (&second, &second_dir)] {
        let id = value["data"]["instance"]["id"]
            .as_str()
            .expect("instance id");
        let args = [
            "launch",
            id,
            "--dry-run",
            "--json",
            "--data-dir",
            dir_arg.as_str(),
        ];
        let (code, stdout, stderr) = run_cli(&args);
        assert_eq!(
            code, 0,
            "stdout={stdout}
stderr={stderr}"
        );
        assert_eq!(
            parse_json(&stdout)["data"]["game_dir"]
                .as_str()
                .expect("game_dir"),
            expect.as_str()
        );
    }
}

/// **Win32 尾点/尾空格**（r2 R2.1 的 CLI 面）：两次登记 + 两条 `launch --dry-run` 不许落到同一物理目录。
///
/// 抓得住：认领目录的尾点/尾空格不归一（第二次默认登记仍拿到 `Foo`，`run_dir` 相同断言红）。
///
/// 抓不住：`\?\` verbatim 写法（那本来就不折叠，是另一个目录）。
#[cfg(windows)]
#[test]
fn win32_trailing_dot_or_space_claims_do_not_share_a_run_dir() {
    for suffix in [".", " "] {
        let dir = TempDir::new("adopt-win32-alias");
        let root = dir.path();
        foreign_data_root(root);
        let dir_arg = dir.arg();
        let claimed = root
            .join(".ULL/data/instances")
            .join(format!("Foo{suffix}"));
        let claimed_arg = claimed.display().to_string();

        let first = run_cli(&[
            "instances",
            "adopt",
            "1.20.1",
            "--name",
            "First",
            "--isolation",
            "custom",
            "--game-dir",
            claimed_arg.as_str(),
            "--json",
            "--data-dir",
            dir_arg.as_str(),
        ]);
        assert_eq!(
            first.0, 0,
            "custom 登记应成功：stdout={} stderr={}",
            first.1, first.2
        );
        let first = parse_json(&first.1);

        let second = run_cli(&[
            "instances",
            "adopt",
            "1.20.1",
            "--name",
            "Foo",
            "--json",
            "--data-dir",
            dir_arg.as_str(),
        ]);
        assert_eq!(
            second.0, 0,
            "默认登记应成功：stdout={} stderr={}",
            second.1, second.2
        );
        let second = parse_json(&second.1);

        let first_dir = first["data"]["instance"]["run_dir"]
            .as_str()
            .expect("run_dir A")
            .to_string();
        let second_dir = second["data"]["instance"]["run_dir"]
            .as_str()
            .expect("run_dir B")
            .to_string();
        assert_ne!(
            first_dir, second_dir,
            "尾点/尾空格别名不许共用运行目录（suffix={suffix:?}）"
        );

        // 两条启动计划的 --gameDir 也必须分开（argv 已脱敏，比尾段）。
        for (value, expect) in [(&first, &first_dir), (&second, &second_dir)] {
            let id = value["data"]["instance"]["id"]
                .as_str()
                .expect("instance id");
            let args = [
                "launch",
                id,
                "--dry-run",
                "--json",
                "--data-dir",
                dir_arg.as_str(),
            ];
            let (code, stdout, stderr) = run_cli(&args);
            assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
            let plan = parse_json(&stdout);
            assert_eq!(
                plan["data"]["game_dir"].as_str().expect("game_dir"),
                expect.as_str(),
                "启动计划必须落在自己的运行目录（suffix={suffix:?}）"
            );
        }
    }
}
