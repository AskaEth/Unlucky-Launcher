//! 诊断包的断言：清单齐全、脱敏生效、缺件写明白、超大文件截断。
//!
//! 为什么这些值得单开文件：守卫的三条（内容有清单 / 脱敏生效 / 没 crash-report 也能导出）
//! 都要**真读回 zip** 才作数——只断言 `write_bundle` 返回 `Ok` 抓不住任何一种坏法。

use std::io::Read;
use std::path::{Path, PathBuf};

use super::{
    default_target_dir, write_bundle, BundleEntry, BundleSpec, HEAD_BYTES, MAX_ENTRY_BYTES,
};

fn temp_dir(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ull-m187-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("建夹具目录");
    root
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("建父目录");
    }
    std::fs::write(path, text).expect("写夹具");
}

/// 打开 zip 读某个条目的全文（不存在返回 `None`）。
fn entry_text(path: &Path, name: &str) -> Option<String> {
    let file = std::fs::File::open(path).expect("打开 zip");
    let mut archive = zip::ZipArchive::new(file).expect("读 zip");
    let mut entry = archive.by_name(name).ok()?;
    let mut text = String::new();
    entry.read_to_string(&mut text).expect("读条目");
    Some(text)
}

fn names(path: &Path) -> Vec<String> {
    let file = std::fs::File::open(path).expect("打开 zip");
    let mut archive = zip::ZipArchive::new(file).expect("读 zip");
    (0..archive.len())
        .filter_map(|index| {
            let item = archive.by_index(index).ok()?;
            item.name().ok().map(|name| name.to_string())
        })
        .collect()
}

/// 接近真实日志的填充物。
///
/// 为什么不用 `"x".repeat(n)`：连续 4 MiB 的同字符会被脱敏器的「长不透明串」规则
/// （长度 ≥64 的 base64/hex 形状）整段替换成占位符，长度就不再超限 —— 那样测的是脱敏，
/// 不是截断。
fn filler(bytes: usize) -> String {
    let line = "2026-11-08T15:30:12.441Z [Render thread/INFO]: loading chunk with some words
";
    let mut text = String::with_capacity(bytes + line.len());
    while text.len() < bytes {
        text.push_str(line);
    }
    text
}

fn spec(root: &Path, secrets: Vec<String>) -> BundleSpec {
    BundleSpec {
        instance: "1.20.4 · Fabric".into(),
        version: "1.20.4-fabric".into(),
        session: "0f8fad5b-d9cb-469f-a165-70867728950e".into(),
        exit_code: Some(1),
        exit_type: "application_error".into(),
        uptime_ms: 12_345,
        command: "javac -Xmx4096m -Djava.library.path=<home>".into(),
        java: "C:/Java/bin/javaw.exe (major 21)".into(),
        system: "os=windows version=10.0 arch=x86_64".into(),
        notes: vec!["本次没有生成 crash-report".into()],
        missing: vec!["crash-reports/ 不存在".into()],
        entries: vec![
            BundleEntry::new("launcher", root.join("launcher.log")),
            BundleEntry::new("session", root.join("game").join("s1.jsonl")),
        ],
        secrets,
        home: Some(root.join("home")),
    }
}

#[test]
fn bundle_has_a_manifest_and_the_collected_files() {
    let root = temp_dir("manifest");
    write(&root.join("launcher.log"), "[launch] spawn argv=...\n");
    write(&root.join("game").join("s1.jsonl"), "{\"kind\":\"exit\"}\n");

    let target = default_target_dir(&root).join("m187.zip");
    let bundle = write_bundle(&spec(&root, Vec::new()), &target).expect("导出");

    assert!(bundle.bytes > 0, "落盘字节数不该是 0");
    assert_eq!(bundle.files, 2, "两个现场文件都该进包");
    assert_eq!(bundle.truncated, 0);

    let listed = names(&target);
    assert!(listed.contains(&"manifest.txt".to_string()), "{listed:?}");
    assert!(
        listed.contains(&"launcher/launcher.log".to_string()),
        "{listed:?}"
    );
    assert!(
        listed.contains(&"session/s1.jsonl".to_string()),
        "{listed:?}"
    );

    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    // 清单要能把「发给别人看」需要的三件事说清：哪一场、什么退出、文件在哪。
    assert!(manifest.contains("instance: 1.20.4 · Fabric"), "{manifest}");
    assert!(manifest.contains("exit_code: 1"), "{manifest}");
    assert!(
        manifest.contains("exit_type: application_error"),
        "{manifest}"
    );
    assert!(manifest.contains("launcher/launcher.log  <-"), "{manifest}");
}

/// 守卫②：**脱敏真的生效**（主目录 / token / 会话 id 三条各自替换）。
#[test]
fn secrets_home_and_session_ids_are_scrubbed_in_every_entry() {
    let root = temp_dir("scrub");
    let home = root.join("home");
    let token = "eyJhbGciOiJIUzI1NiJ9.eyJ4IjoxMjN9.c2lnbmF0dXJlLXZhbHVl";
    let session = "0f8fad5b-d9cb-469f-a165-70867728950e";
    let dirty = format!(
        "argv=--accessToken {token}\npath={}\nsession={session}\n",
        home.join("AppData").display()
    );
    write(&root.join("launcher.log"), &dirty);
    write(&root.join("game").join("s1.jsonl"), &dirty);

    let target = default_target_dir(&root).join("m187.zip");
    let mut spec = spec(&root, vec![session.to_string()]);
    spec.command = format!("java -accessToken {token}");
    let bundle = write_bundle(&spec, &target).expect("导出");
    assert_eq!(bundle.files, 2);

    for name in ["manifest.txt", "launcher/launcher.log", "session/s1.jsonl"] {
        let text = entry_text(&target, name).unwrap_or_else(|| panic!("缺条目 {name}"));
        assert!(!text.contains(token), "{name} 漏了 token：{text}");
        assert!(!text.contains(session), "{name} 漏了会话 id：{text}");
    }
    let log = entry_text(&target, "launcher/launcher.log").expect("日志");
    assert!(log.contains("<home>"), "主目录没被替换：{log}");
    assert!(
        !log.contains(&home.display().to_string()),
        "家目录原文还在：{log}"
    );
}

/// 守卫③：**没有 crash-report 也能导出**，而且「没有」这件事写进清单（不许静默漏掉）。
#[test]
fn missing_crash_report_is_stated_in_the_manifest() {
    let root = temp_dir("missing");
    write(
        &root.join("game").join("s1.jsonl"),
        "{\"kind\":\"stderr\"}\n",
    );

    let target = default_target_dir(&root).join("m187.zip");
    let bundle = write_bundle(&spec(&root, Vec::new()), &target).expect("导出");
    assert_eq!(bundle.files, 1, "只有会话 JSONL 时也要出包");

    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    assert!(manifest.contains("missing"), "{manifest}");
    assert!(manifest.contains("crash-reports/ 不存在"), "{manifest}");
    assert!(manifest.contains("本次没有生成 crash-report"), "{manifest}");
    // 清单里也要能看见那个「不存在的文件名」，否则用户以为我们忘收了。
    assert!(manifest.contains("session/s1.jsonl"), "{manifest}");
}

/// 有上限：单个超限文件只收头部，并在清单里记一行「已截断」。
#[test]
fn oversized_entries_are_truncated_and_recorded() {
    let root = temp_dir("truncate");
    let big = root.join("game").join("latest.log");
    let body = filler(usize::try_from(MAX_ENTRY_BYTES).expect("常数") + 32);
    write(&big, &body);

    let target = default_target_dir(&root).join("m187.zip");
    let mut spec = spec(&root, Vec::new());
    spec.entries = vec![BundleEntry::new("game", &big)];
    let bundle = write_bundle(&spec, &target).expect("导出");

    assert_eq!(bundle.truncated, 1, "超限文件必须计入截断数");
    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    assert!(manifest.contains("已截断"), "{manifest}");
    assert!(
        bundle.bytes < MAX_ENTRY_BYTES + 8 * 1024,
        "包不该把整个超限文件塞进去（{} 字节）",
        bundle.bytes
    );
}

#[test]
fn missing_source_files_are_skipped_not_fatal() {
    let root = temp_dir("gone");
    let mut spec = spec(&root, Vec::new());
    spec.entries = vec![BundleEntry::new("game", root.join("nope/absent.log"))];

    let target = default_target_dir(&root).join("m187.zip");
    let bundle = write_bundle(&spec, &target).expect("缺文件也要出包");
    assert_eq!(bundle.files, 0);
    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    assert!(manifest.contains("读不出来"), "{manifest}");
}

/// P2-2：**包内条目名与清单里的名字必须一字不差**，且名字也要过脱敏。
///
/// 抓得住：r1 复审抓到的原 bug —— 条目名用原始文件名（会话 id 原样泄漏），而 manifest 过了
/// redactor（写 `<redacted:token>`）⇒ 清单指向的条目在包里不存在。
#[test]
fn entry_names_are_scrubbed_and_match_the_manifest_exactly() {
    let root = temp_dir("names");
    let session = "0f8fad5b-d9cb-469f-a165-70867728950e";
    write(
        &root.join("game").join(format!("{session}.jsonl")),
        "{}
",
    );
    write(
        &root.join("launcher.log"),
        "boot
",
    );

    // 第三条**不钉名**：它的源文件名里带会话 id，靠 `entry_names` 的脱敏兜住。
    write(
        &root.join("game").join(format!("hs-{session}.log")),
        "x
",
    );
    let mut spec = spec(&root, vec![session.to_string()]);
    spec.entries = vec![
        BundleEntry::new("launcher", root.join("launcher.log")),
        // 源文件名就是会话 id：钉成 session.jsonl（调用方 app-iced 的做法）
        BundleEntry::named(
            "session",
            root.join("game").join(format!("{session}.jsonl")),
            "session.jsonl",
        ),
        BundleEntry::new("game", root.join("game").join(format!("hs-{session}.log"))),
    ];
    let target = default_target_dir(&root).join("names.zip");
    write_bundle(&spec, &target).expect("导出");

    let listed = names(&target);
    assert!(
        listed.iter().any(|name| name == "session/session.jsonl"),
        "钉死的条目名没生效：{listed:?}"
    );
    assert!(
        !listed.iter().any(|name| name.contains(session)),
        "条目名里还有原始会话 id：{listed:?}"
    );

    // 清单的 files 段必须与真实条目一一对应（顺序也一致）。
    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    let manifest_names: Vec<String> = manifest
        .lines()
        .filter(|line| line.starts_with("- ") && line.contains("  <- "))
        .map(|line| {
            line.trim_start_matches("- ")
                .split("  <- ")
                .next()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    let zip_names: Vec<String> = listed
        .iter()
        .filter(|name| name.as_str() != "manifest.txt")
        .cloned()
        .collect();
    assert_eq!(manifest_names, zip_names, "清单与包内条目对不上");
}

/// P2-3：截断必须**保留尾部**——崩溃证据（`exit` 事件 / 栈尾）在尾部。
///
/// 抓得住：只留头部的截断（r1 复审 P2-3 的原实现）会让包里没有 `exit` 行。
#[test]
fn truncation_keeps_the_tail_where_the_crash_evidence_lives() {
    let root = temp_dir("tail");
    let session = root.join("game").join("session.jsonl");
    let head_marker = "HEAD-MARKER-first-line";
    let tail_marker = "{\"kind\":\"exit\",\"payload\":{\"code\":1}}";
    let body = format!(
        "{head_marker}
{}{tail_marker}
",
        filler(usize::try_from(MAX_ENTRY_BYTES).expect("常数") + 64)
    );
    write(&session, &body);

    let mut spec = spec(&root, Vec::new());
    spec.entries = vec![BundleEntry::named("session", &session, "session.jsonl")];
    let target = default_target_dir(&root).join("tail.zip");
    let bundle = write_bundle(&spec, &target).expect("导出");

    assert_eq!(bundle.truncated, 1, "超限文件必须计入截断数");
    let kept = entry_text(&target, "session/session.jsonl").expect("条目");
    assert!(kept.contains(head_marker), "头部标记丢了");
    assert!(
        kept.contains(tail_marker),
        "尾部标记丢了 —— 包里将没有 exit 事件（证据是残的）"
    );
    assert!(
        kept.contains("<truncated "),
        "中间该有一行截断说明：{kept:.80}"
    );
    let manifest = entry_text(&target, "manifest.txt").expect("清单");
    assert!(manifest.contains("已截断"), "{manifest}");
}

/// P2-1（r2 复审新增，按 p1 对待）：**截断过的文件也必须脱敏**。
///
/// 抓得住的原 bug：上一版在字节上切两刀，切在多字节字符中间时 `from_utf8` 失败 ⇒
/// 整份 entry 走「原样字节」分支，家目录 / 用户名 / 令牌原文按字节进包（隐私旁路）。
///
/// 构造要点：让一个 3 字节的 `中` **正好跨在 `HEAD_BYTES` 边界上**，文件里放注册过的
/// 会话 id 与家目录路径，尾部再放 exit 事件（证明截断仍然保尾）。
#[test]
fn truncated_entries_are_still_redacted() {
    let root = temp_dir("trunc-redact");
    let home = root.join("home");
    let session = "0f8fad5b-d9cb-469f-a165-70867728950e";
    let head_budget = usize::try_from(HEAD_BYTES).expect("常数");

    // 前段：真实的日志行 + 家目录 + 会话 id；填充到「离边界只差 2 字节」，
    // 再放一个 3 字节的 CJK 字 ⇒ 它必然跨在 HEAD_BYTES 上。
    let mut text = String::new();
    text.push_str(&format!(
        "[boot] home={}
",
        home.display()
    ));
    text.push_str(&format!(
        "[launch] session={session}
"
    ));
    let line = "2026-11-08T15:30:12.441Z [Render thread/INFO]: loading chunk
";
    while text.len() + line.len() < head_budget - 2 {
        text.push_str(line);
    }
    while text.len() < head_budget - 2 {
        text.push('a');
    }
    text.push('中');
    while text.len() < usize::try_from(MAX_ENTRY_BYTES).expect("常数") + 64 {
        text.push_str(line);
    }
    text.push_str(
        "{\"kind\":\"exit\",\"payload\":{\"code\":1}}
",
    );

    let log = root.join("game").join("latest.log");
    write(&log, &text);
    assert!(
        !text.is_char_boundary(head_budget),
        "构造失败：切点应落在字中间"
    );

    let mut spec = spec(&root, vec![session.to_string()]);
    spec.entries = vec![BundleEntry::named("game", &log, "latest.log")];
    let target = default_target_dir(&root).join("trunc-redact.zip");
    let bundle = write_bundle(&spec, &target).expect("导出");
    assert_eq!(
        bundle.truncated, 1,
        "这份文件必须真的被截断（否则测不到旁路）"
    );

    let kept = entry_text(&target, "game/latest.log").expect("条目");
    // ① 包里找不到原文（家目录 / 会话 id）
    assert!(
        !kept.contains(&home.display().to_string()),
        "截断后的文件仍带着家目录原文 —— 脱敏被跳过了"
    );
    assert!(!kept.contains(session), "截断后的文件仍带着会话 id 原文");
    // ② 占位符在（说明脱敏真的跑过）
    assert!(kept.contains("<home>"), "没有 <home> 占位符");
    // ③ 尾部证据仍在（截断仍保尾）
    assert!(kept.contains("\"kind\":\"exit\""), "尾部 exit 事件丢了");
}
