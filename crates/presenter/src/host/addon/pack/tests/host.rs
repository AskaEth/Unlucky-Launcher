//! M336：宿主侧失败**只给码与细节，不给散文**（台账 P31 finding②）。
//!
//! # 这一族以前长什么样
//!
//! `import::run` / `import::spawn_scan` 在「目标实例不可用」时把
//! `"import_host"` + `"Target instance is not usable"` 一起塞进 `ImportErrorView`，界面画成
//! `Target instance is not usable · import_host`——中文界面里混硬编码英文，尾巴上挂着内部码。
//!
//! # 现在钉住什么
//!
//! | 用例 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | [`every_host_failure_is_a_structured_problem`] | 四条成因各自的稳定码、`label == code`、`message` 只是细节 | 界面有没有按码取词（`packui/model/tests/host.rs` 那条管） |
//! | [`no_host_failure_carries_english_prose`] | 谁把 `"Target instance is not usable"` / `"directory already exists and is not empty: {…}"` / `"create dir failed: {…}"` 那种**英文散文**塞回 `label`/`message`/`hint`，本用例当场红 | 别的失败路径（core 的 `pack_*` / `import_*`）仍是中文散文——那不在本 mission 的 scope |
//! | [`an_occupied_directory_and_a_blocked_one_are_told_apart`] | 两条真路径（`target_of` 撞非空目录 / `prepare_target` 建不出目录）真的落到**不同**的码上，而不是塌成同一句 | 系统 IO 的原文（`{detail}` 那一格本来就是 OS 给的，不是本仓的英文） |
//! | [`an_unresolvable_instance_keeps_its_upstream_text_out_of_the_label`] | 上游（`pack.rs::instance_root`，不在 scope）那句英文**只**落在细节格里，没被当成大类名 | 上游将来把它改成中文 / 结构化输（那要动 `pack.rs`） |
//!
//! 变异：把 `import.rs` 里任一处 `convert::host_failed_report(..)` 换回
//! `"Target instance is not usable"` 字面量 ⇒ [`no_host_failure_carries_english_prose`] 红。

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use crate::dto::{ConflictDecisionsView, ImportRequestView};

use super::harness::{temp_dir, write};

/// 一处「目标实例不可用」的成因（四条都造一遍，判据要覆盖到每一条出口）。
fn all_problems() -> Vec<super::super::import::TargetProblem> {
    use super::super::import::TargetProblem;
    vec![
        TargetProblem::Occupied {
            path: std::path::PathBuf::from("/planes/instances/Better MC"),
        },
        TargetProblem::Unreadable {
            detail: "Access is denied. (os error 5)".to_string(),
        },
        TargetProblem::CreateFailed {
            detail: "The system cannot find the path specified. (os error 3)".to_string(),
        },
        TargetProblem::InstanceGone {
            detail: "instance not found: survival".to_string(),
        },
    ]
}

/// 四条成因各自的稳定码、`label == code`、`message` 只是细节。
#[test]
fn every_host_failure_is_a_structured_problem() {
    use super::super::import::TargetProblem;

    let expected = vec![
        (
            TargetProblem::Occupied {
                path: std::path::PathBuf::new(),
            },
            "import_host_occupied",
        ),
        (
            TargetProblem::Unreadable {
                detail: String::new(),
            },
            "import_host_unreadable",
        ),
        (
            TargetProblem::CreateFailed {
                detail: String::new(),
            },
            "import_host_create_failed",
        ),
        (
            TargetProblem::InstanceGone {
                detail: String::new(),
            },
            "import_host_instance_gone",
        ),
    ];
    // 码不许撞：撞了界面就只能画同一句话，四条成因又塌回一条。
    let codes: std::collections::HashSet<&str> = expected.iter().map(|(_, code)| *code).collect();
    assert_eq!(codes.len(), expected.len(), "四条成因的码撞了：{codes:?}");

    for (problem, code) in &expected {
        assert_eq!(
            problem.code(),
            *code,
            "稳定码漂了（`--json` 消费者会跟着断）"
        );
        let view = super::super::convert::host_failed_report(
            std::env::temp_dir(),
            Path::new("C:/packs/Better MC.mrpack"),
            problem,
        );
        let error = view.error.expect("这一族必须有致命块");
        assert_eq!(error.code, *code);
        assert_eq!(
            error.label, *code,
            "宿主侧不给人读名：这一格只能是稳定码（人读名在目录里）"
        );
        assert_eq!(error.message, problem.detail(), "`message` 只放细节");
        assert!(error.hint.is_none(), "这一族没有补救建议可给");
        // 与 `failed_report_with` 同一条形状：没进管线 ⇒ 没格式、没处理器链、一个字节没写。
        assert!(view.format.is_none() && view.processors.is_none());
        assert_eq!(view.bytes_written, 0);
    }
}

/// 谁把英文散文塞回来，这里就红。
#[test]
fn no_host_failure_carries_english_prose() {
    // 这几句正是以前被硬编码上屏的（`import_host` 那一族）。它们现在只许出现在目录的英文列里，
    // 不许出现在宿主产的报告里——宿主一旦又开始拼句，中文界面就又开始混英文。
    const BANNED: [&str; 5] = [
        "Target instance is not usable",
        "directory already exists and is not empty",
        "read instance dir failed",
        "create dir failed",
        "not usable",
    ];
    for problem in all_problems() {
        let view = super::super::convert::host_failed_report(
            std::env::temp_dir(),
            Path::new("C:/packs/Better MC.mrpack"),
            &problem,
        );
        let error = view.error.expect("这一族必须有致命块");
        let drawn = [
            ("label", error.label.as_str()),
            ("message", error.message.as_str()),
            ("hint", error.hint.as_deref().unwrap_or("")),
        ];
        for (field, text) in drawn {
            for banned in BANNED {
                assert!(
                    !text.contains(banned),
                    "{field} 里又出现了英文散文 {banned:?}：{text}"
                );
            }
        }
    }
}

/// 宿主侧失败**必须落一行日志**（M336 r1 复审 p2-a）。
///
/// # 为什么这条用例值得存在
///
/// 复审实测：这条链上一个日志调用都没有，而四处代码文档写着「进日志」。于是四个稳定码
/// **哪里都不落地**——上屏被 M336 掐了（对），`--json` 没有产品消费者，日志也没有。用户报
/// 「导入失败」时，支持侧连是哪一档（目录非空 / 读不出 / 建不出 / 实例没了）都分不出来。
///
/// # 抓得住 / 抓不住
///
/// **抓得住**：真跑 `import::run`（不是「函数被调用过」），日志文件里出现
/// `[pack] import_host_occupied: <目录>`——码与细节都在，且没把英文散文写回去。
/// **抓不住**：用户机器上 `logs/` 不可写时那行会静默丢（`launcher_log` 是 best-effort，这是它
/// 的设计，不是本用例能改的）。
///
/// 变异证明：把 `import.rs::report_host_failure` 里那行 `launcher_log::record(..)` 删掉 ⇒ 本用例红。
///
/// 为什么要重试：`launcher_log` 的目录是**进程级全局**（`set_dir` 以最后一次为准），presenter
/// 的用例并行跑、七十多处 `HostEnv::at_root` 都会重新登记。每轮换一个新目录，抢到就能在一个
/// 全新文件里看到我们的行；抢不到（文件不存在 / 只有别人的行）就再来一轮，64 轮全不中就当失败
/// （不跳过——跳过等于这条判据不存在）。
#[test]
fn a_host_failure_leaves_its_code_and_detail_in_the_launcher_log() {
    let mut last = String::from("<日志文件一次都没建出来>");
    for attempt in 0..64 {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ull-m336-host-log-{}-{attempt}-{stamp}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.join("data");
        std::fs::create_dir_all(&data).expect("data root");
        // `at_root` 顺带把这棵树的 `logs/` 登记给 `launcher_log`（`host/env.rs:95`）。
        let env = crate::host::HostEnv::at_root(&data);

        // 一个 `diagnose` 认得的最小 MultiMC 包——否则 `run` 在探测那一关就早退，
        // 根本走不到 `target_of`（那样这条用例就变成在测另一件事了）。
        let source = dir.join("Better MC");
        write(
            &source.join("mmc-pack.json"),
            br#"{"formatVersion": 1, "components": [
                {"uid": "net.minecraft", "version": "1.20.1"}]}"#,
        );
        write(&source.join(".minecraft/mods/sodium.jar"), &[0x11; 64]);

        let request = ImportRequestView {
            pack: source,
            instance: None,
            instance_name: Some("Taken".to_string()),
            decisions: ConflictDecisionsView::default(),
        };
        // 目标目录**已存在且不空** ⇒ `target_of` 报 Occupied（真路径，不手搓 `TargetProblem`）。
        let (target, _) = super::super::import::target_of(&env, &request).expect("首次算得出");
        std::fs::create_dir_all(&target).expect("建目录");
        std::fs::write(target.join("marker.txt"), b"x").expect("占位文件");

        let (tx, _rx) = crate::host::job::channel();
        // 显式登记日志目录（`set_dir` 是进程级全局、以最后一次为准）：只要在 `run` 之前抢到，
        // 我们的行就落在这个全新目录里。
        unlucky_core::launch::launcher_log::set_dir(env.layout.logs());
        super::super::import::run(&env, &tx, &request, 7, Arc::new(AtomicBool::new(false)));
        drop(tx);

        let file = env
            .layout
            .logs()
            .join(unlucky_core::launch::launcher_log::FILE_NAME);
        if let Ok(text) = std::fs::read_to_string(&file) {
            if let Some(line) = text
                .lines()
                .find(|line| line.contains("import_host_occupied"))
            {
                assert!(line.contains("[pack]"), "日志行要带 scope：{line}");
                // 细节 = 那个目录。用「目录名 + 中间那一层」判，不比整条路径：
                // Windows 上 `Path::display()` 的 `/` 与 `\` 混着出现，逐字比是给自己找麻烦。
                let detail = line
                    .split_once("import_host_occupied: ")
                    .map(|(_, rest)| rest)
                    .unwrap_or_default();
                assert!(
                    detail.contains("instances") && detail.contains("Taken"),
                    "细节（哪个目录）也要进日志，否则支持侧只看到「目标实例不可用」：{line}"
                );
                assert!(
                    !line.contains("Target instance is not usable"),
                    "日志里写码与细节，不写英文散文：{line}"
                );
                let _ = std::fs::remove_dir_all(&dir);
                return;
            }
            last = text;
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
    panic!("64 轮都没在 launcher.log 里看到 `import_host_occupied`（进程级全局日志目录被并行用例抢走了？）最后一次文件内容：{last}");
}

/// 两条真路径落到**不同**的码上。
#[test]
fn an_occupied_directory_and_a_blocked_one_are_told_apart() {
    use super::super::import::TargetProblem;

    let dir = temp_dir("m336-occupied");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);
    let request = ImportRequestView {
        pack: dir.join("Better MC.mrpack"),
        instance: None,
        instance_name: Some("Better MC".to_string()),
        decisions: ConflictDecisionsView::default(),
    };

    // ① 空目录 ⇒ 可复用（这不算失败）。
    let (target, register) = super::super::import::target_of(&env, &request).expect("首次算得出");
    assert!(register);
    std::fs::create_dir_all(&target).expect("建目录");
    // ② 目录里已经有东西 ⇒ 拒绝，且**说清楚是哪一个目录**。
    std::fs::write(target.join("marker.txt"), b"x").expect("write");
    match super::super::import::target_of(&env, &request) {
        Err(TargetProblem::Occupied { path }) => assert_eq!(path, target),
        other => panic!("非空目录必须报 Occupied：{other:?}"),
    }

    // ③ 建不出目录（父路径是个文件）⇒ 另一条码，不是上面那条。
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"x").expect("write blocker");
    match super::super::import::prepare_target(&blocker.join("child")) {
        Err(problem) => {
            assert_eq!(problem.code(), "import_host_create_failed");
            assert!(
                !problem.detail().contains("create dir failed"),
                "细节里不许再包一层自己拼的英文壳：{}",
                problem.detail()
            );
        }
        Ok(()) => panic!("父路径是文件时建目录必须失败"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}

/// 上游那句英文只落在细节格里。
#[test]
fn an_unresolvable_instance_keeps_its_upstream_text_out_of_the_label() {
    use super::super::import::TargetProblem;

    let dir = temp_dir("m336-ghost");
    let data = dir.join("data");
    std::fs::create_dir_all(&data).expect("data root");
    let env = crate::host::HostEnv::at_root(&data);
    let request = ImportRequestView {
        pack: dir.join("Better MC.mrpack"),
        // 档案里没有这个实例 ⇒ `pack.rs::instance_root` 回 `instance not found: ghost`
        instance: Some("ghost".to_string()),
        instance_name: None,
        decisions: ConflictDecisionsView::default(),
    };
    match super::super::import::target_of(&env, &request) {
        Err(TargetProblem::InstanceGone { detail }) => {
            assert!(!detail.is_empty(), "上游原因要留着进 `--json`");
            let view = super::super::convert::host_failed_report(
                std::env::temp_dir(),
                &request.pack,
                &TargetProblem::InstanceGone {
                    detail: detail.clone(),
                },
            );
            let error = view.error.expect("致命块");
            assert_eq!(error.label, "import_host_instance_gone");
            assert_eq!(error.message, detail);
        }
        other => panic!("不存在的实例必须报 InstanceGone：{other:?}"),
    }

    let _ = std::fs::remove_dir_all(&dir);
}
