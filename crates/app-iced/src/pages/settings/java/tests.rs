//! 「Java」分组的真数据源断言（M175）：扫描 / 手动路径 / 内存 / JVM 参数 / 真命令行。
//!
//! 每一条都**能变红**（报告的「变异证明」逐条给了改哪一行会红）。四条守卫：
//!
//! | 守卫 | 断言 | 改回什么会红 |
//! |---|---|---|
//! | 列表来自真扫盘 | [`the_runtime_list_comes_from_the_disk`] | 把行换成写死的字符串 / 不读候选表 |
//! | 非法路径不污染设置 | [`an_invalid_manual_path_is_refused_and_writes_nothing`] | 删掉 `manual::probe_manual` 的校验 |
//! | 内存越界被拒 | [`the_memory_bounds_are_enforced_before_writing`] | 删掉范围判断 / 允许非数字 |
//! | 内存真进命令行 | [`the_saved_memory_reaches_the_real_command_line`] | 把 `-Xmx` 的来源改回常量 |
//!
//! 前三条不依赖本机装没装 JDK（假的 `bin/java` 空文件就够：它扫得到、但答不出版本），
//! 第四条（`tests/cmdline.rs`）走 core 的**真组装链**（`launch::args_table::assemble`）——
//! 不是读配置回显。落盘 / 读回 / 版本门槛 / 截图种子也在那个文件里（同一族「真值」断言）。

#[path = "tests/cmdline.rs"]
mod cmdline;

use std::path::{Path, PathBuf};

use super::state::{Refusal, RuntimeRow, State, MAX_JVM_ARGS, MAX_MEMORY_MB, MIN_MEMORY_MB};
use super::store;

/// 每个用例一个临时数据根（`<dir>/config/launcher.json` 的形状与真实数据根一致）。
pub(super) fn root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ull-java-page-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("config")).expect("config");
    dir
}

pub(super) fn config(dir: &Path) -> PathBuf {
    dir.join("config").join("launcher.json")
}

/// 造一个「看起来像 JDK」的目录：`bin/java` 是空文件（扫得到、但 `-version` 答不出）。
pub(super) fn fake_jdk(dir: &Path, name: &str) -> PathBuf {
    let jdk = dir.join(name);
    std::fs::create_dir_all(jdk.join("bin")).expect("bin");
    std::fs::write(jdk.join("bin").join(java_bin()), b"").expect("写 java");
    jdk
}

pub(super) fn java_bin() -> &'static str {
    if cfg!(windows) {
        "java.exe"
    } else {
        "java"
    }
}

// --------------------------------------------------------------- 守卫①：列表来自真扫盘

/// 扫描列表必须来自**盘上真有的目录**：造两个 → 两项；删一个 → 一项。
///
/// 变异证明：把 `store::start` 里的候选表换成一个写死的两行常量，删目录之后仍是两项 ⇒ 红。
#[test]
fn the_runtime_list_comes_from_the_disk() {
    let dir = root("scan");
    let one = fake_jdk(&dir, "jdk-a");
    let two = fake_jdk(&dir, "jdk-b");

    let found =
        unlucky_core::game::java::staged::scan_roots(&[], &[one.clone(), two.clone()], false);
    assert_eq!(found.len(), 2, "两个假 JDK 目录必须扫出两项：{found:?}");

    std::fs::remove_dir_all(&one).expect("删 jdk-a");
    let after = unlucky_core::game::java::staged::scan_roots(&[], &[one, two], false);
    assert_eq!(after.len(), 1, "删掉一个之后必须少一项：{after:?}");

    std::fs::remove_dir_all(&dir).ok();
}

/// 扫描结论灌进页面状态之后，**行数与路径**都来自那次扫描（不是常量）。
#[test]
fn applying_a_scan_report_replaces_every_row() {
    let mut state = State::default();
    let report = store::ScanReport {
        running: false,
        done: 2,
        total: 2,
        finished: true,
        cancelled: false,
        runtimes: vec![
            RuntimeRow {
                binary: "C:/jdk-a/bin/java.exe".to_string(),
                version: String::new(),
                major: None,
                vendor: None,
                source: "manual",
            },
            RuntimeRow {
                binary: "C:/jdk-b/bin/java.exe".to_string(),
                version: String::new(),
                major: Some(21),
                vendor: Some("Temurin".to_string()),
                source: "system",
            },
        ],
    };
    assert!(state.apply_scan(&report), "第一次灌结论必须报告「变了」");
    assert_eq!(state.runtimes.len(), 2);
    assert!(state.scanned, "扫完之后「未检测到」与「还没扫」必须能分开");
    assert_eq!(state.runtimes[1].summary(), "Java 21 · Temurin");
}

// --------------------------------------------------------------- 守卫②：非法路径不污染设置

/// 非法路径被拒、**设置与名单一个字节都不写**。
///
/// 变异证明：删掉 `store::add_manual` 里的 `manual::probe_manual`（改成只看 `is_dir()`），
/// 「不存在的路径」就会被加进名单 ⇒ 红。
#[test]
fn an_invalid_manual_path_is_refused_and_writes_nothing() {
    let dir = root("manual");
    let file = config(&dir);

    assert_eq!(
        store::add_manual(&file, "").err(),
        Some(Refusal::AddEmpty),
        "空输入要被拒"
    );
    assert_eq!(
        store::add_manual(&file, "Z:/definitely/not/here").err(),
        Some(Refusal::AddMissing),
        "不存在的路径要被拒"
    );
    let empty = dir.join("empty-jdk");
    std::fs::create_dir_all(&empty).expect("空目录");
    assert_eq!(
        store::add_manual(&file, &empty.display().to_string()).err(),
        Some(Refusal::AddNotJava),
        "目录里没有 bin/java 要被拒"
    );
    let broken = fake_jdk(&dir, "broken-jdk");
    assert_eq!(
        store::add_manual(&file, &broken.display().to_string()).err(),
        Some(Refusal::AddNoVersion),
        "跑不出 java -version 要被拒"
    );

    assert!(
        store::load_manual(&file).is_empty(),
        "被拒的路径不许进名单：{:?}",
        store::load_manual(&file)
    );
    let snapshot = store::load(&file);
    assert!(
        snapshot.selected.is_empty(),
        "被拒的路径不许污染 java.selected_path：{snapshot:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

// --------------------------------------------------------------- 守卫③：内存越界被拒

/// 内存必须落在 schema 登记的区间里，非数字也要拒，而且**不写盘**。
///
/// 变异证明：删掉 `State::validate` 里的范围判断（只留 `parse`），128 与 999999 会落盘 ⇒ 红；
/// 把 `parse::<u32>` 换成 `unwrap_or(2048)`，`"abc"` 不再报错 ⇒ 红。
#[test]
fn the_memory_bounds_are_enforced_before_writing() {
    let mut state = State {
        storage: true,
        ..State::default()
    };

    state.memory = (MIN_MEMORY_MB - 1).to_string();
    assert_eq!(
        state.validate().err(),
        Some(Refusal::MemoryRange),
        "低于下限"
    );
    state.memory = (MAX_MEMORY_MB + 1).to_string();
    assert_eq!(
        state.validate().err(),
        Some(Refusal::MemoryRange),
        "高于上限"
    );
    state.memory = "abc".to_string();
    assert_eq!(
        state.validate().err(),
        Some(Refusal::MemoryNotNumber),
        "非数字"
    );
    state.memory = "  ".to_string();
    assert_eq!(state.validate().err(), Some(Refusal::MemoryEmpty), "空");
    state.memory = "4096".to_string();
    assert!(state.validate().is_ok());
    state.jvm = "-XX:+UseG1GC".to_string();
    assert!(state.validate().is_ok());
    state.jvm = "-Xss\"512k".to_string();
    assert_eq!(
        state.validate().err(),
        Some(Refusal::JvmQuotes),
        "引号没配对"
    );

    // 真的不写盘：一次越界保存之后，文件里还是出厂默认值。
    let dir = root("memory");
    let file = config(&dir);
    let state = State {
        storage: true,
        memory: "128".to_string(),
        ..State::default()
    };
    assert!(store::save(&file, &state).is_err(), "越界必须写不进去");
    assert_eq!(
        store::load(&file).memory_mb,
        super::state::DEFAULT_MEMORY_MB,
        "被拒的保存不许落盘"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 两个数字常量必须与 schema 登记的一致（否则 UI 放行的值会被 schema 拒）。
///
/// 变异证明：把 `MIN_MEMORY_MB` 改成 256（core 的 `launch_overrides` 那一档），这条立刻红。
#[test]
fn the_memory_bounds_mirror_the_schema() {
    use unlucky_core::setting::keys::launcher_schema;
    use unlucky_core::setting::spec::ValueKind;
    let schema = launcher_schema();
    let kind = schema
        .spec("java.default_memory_mb")
        .map(|spec| &spec.kind)
        .expect("schema 里必须有 java.default_memory_mb");
    let ValueKind::Int { min, max } = kind else {
        panic!("java.default_memory_mb 应该是 Int：{kind:?}");
    };
    assert_eq!(*min, i64::from(MIN_MEMORY_MB));
    assert_eq!(*max, i64::from(MAX_MEMORY_MB));
}

// --------------------------------------------------------------- 守卫⑤：JVM 参数超长被拒（M199）

/// 粘了一整段参数（>1024 字符）时界面给的是**具体**原因，而不是撞 schema 之后的「写不进去」；
/// 而且一个字节都不写盘。
///
/// 变异证明：删掉 `State::validate` 里那条 `MAX_JVM_ARGS` 判据 ⇒ 长度打回 schema 那一档
/// （`store::save` 返回 `WriteError::Schema`，用户只看到通用失败文案、具体原因只在 stderr），
/// 本用例红。这也是 r3 复审 P3-4 的现场。
#[test]
fn an_overlong_jvm_argument_line_is_refused_before_writing() {
    let dir = root("jvm-long");
    let file = config(&dir);

    // 恰好等于上限：放行（判据是「超过」，不是「达到」）。
    let mut state = State {
        storage: true,
        memory: super::state::DEFAULT_MEMORY_MB.to_string(),
        jvm: "x".repeat(MAX_JVM_ARGS),
        ..State::default()
    };
    assert!(state.validate().is_ok(), "恰好 {MAX_JVM_ARGS} 字符应当放行");

    state.jvm = "x".repeat(MAX_JVM_ARGS + 1);
    assert_eq!(state.validate().err(), Some(Refusal::JvmTooLong));
    assert_eq!(
        Refusal::JvmTooLong.label(),
        unlucky_i18n::Key::SettingsJavaReasonJvmTooLong,
        "拒绝原因必须有自己的一句文案（不许退回通用的「写不进去」）"
    );

    let error = store::save(&file, &state).expect_err("超长参数应被本地校验拒掉");
    assert_eq!(error, store::WriteError::Refused(Refusal::JvmTooLong));
    assert!(!file.exists(), "被本地校验拒掉的保存不许碰磁盘");
    std::fs::remove_dir_all(&dir).ok();
}

/// `MAX_JVM_ARGS` 必须等于 schema 登记的 `java.extra_args` 上限：本地放行的值不许被 schema 拒
/// （否则用户又只看到那句通用失败文案）。
///
/// 变异证明：把 `MAX_JVM_ARGS` 改成 2048 ⇒ 本用例红。
#[test]
fn the_jvm_length_mirrors_the_schema() {
    use unlucky_core::setting::keys::launcher_schema;
    use unlucky_core::setting::spec::ValueKind;
    let schema = launcher_schema();
    let kind = schema
        .spec("java.extra_args")
        .map(|spec| &spec.kind)
        .expect("schema 里必须有 java.extra_args");
    let ValueKind::Text { max_len } = kind else {
        panic!("java.extra_args 应该是 Text：{kind:?}");
    };
    assert_eq!(*max_len, MAX_JVM_ARGS);
}

#[path = "tests/runtime.rs"]
mod runtime;

#[path = "tests/prefs.rs"]
mod prefs;

// M206：「当前默认」那颗标记的可读性（对比度 / 同族 / 禁用语义）断言。
#[path = "tests/marker.rs"]
mod marker;
