//! `core::game::java::discover`：标准安装位置扫描与来源优先级。
//!
//! 真机上扫到什么取决于本机装了什么，所以断言只钉**不变量**：
//! 扫描结果形状、来源优先级（env 胜过扫盘）、去重保留高优先级、缺目录不报错。

use unlucky_core::game::java::discover;
use unlucky_core::game::java::mapping::{JavaRequirement, RequirementSource};
use unlucky_core::game::java::{pick, JavaOrigin, JavaRuntime};

fn runtime(path: &str, major: u32, origin: JavaOrigin) -> JavaRuntime {
    JavaRuntime {
        binary: std::path::PathBuf::from(path),
        origin,
        version: Some(format!("openjdk version \"{major}.0.1\"")),
        major: Some(major),
        bits: 64,
        vendor: None,
    }
}

fn at_least(minimum: u32) -> JavaRequirement {
    JavaRequirement::new(minimum, None, RequirementSource::Table)
}

#[test]
fn scanned_paths_always_point_at_a_java_binary() {
    // 本机可能一个都没有（裸机/容器）——那也算通过，只是列表为空。
    for binary in discover::scan_java_binaries() {
        assert!(
            binary.is_file(),
            "扫描结果必须是真实存在的文件：{}",
            binary.display()
        );
        assert_eq!(
            binary
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            Some(unlucky_core::game::java::java_binary_name().to_string()),
            "扫描结果必须是 bin/java[.exe]：{}",
            binary.display()
        );
    }
}

#[test]
fn standard_roots_are_absolute_and_unique() {
    let roots = discover::standard_roots();
    for root in &roots {
        assert!(
            root.is_absolute(),
            "候选根必须是绝对路径：{}",
            root.display()
        );
    }
    let mut keys: Vec<String> = roots.iter().map(|root| discover::path_key(root)).collect();
    let total = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), total, "候选根不该重复");
}

#[test]
fn environment_java_beats_scanned_java_when_both_satisfy() {
    // D13 的「优先级链不变」：环境变量是用户意图，扫盘只是候选。
    let candidates = [
        runtime(
            "C:/Program Files/Eclipse Adoptium/jdk-21/bin/java.exe",
            21,
            JavaOrigin::System,
        ),
        runtime(
            "C:/Program Files/Microsoft/jdk-21/bin/java.exe",
            21,
            JavaOrigin::JavaHome,
        ),
    ];
    let picked = pick(&candidates, &at_least(17)).expect("应有候选");
    assert_eq!(picked.origin, JavaOrigin::JavaHome);
}

#[test]
fn scanned_java_is_still_used_when_nothing_else_matches() {
    // MSI 装了 Temurin 21、没设环境变量、PATH 上是 Java 8：1.20.5 只认扫盘那个。
    let candidates = [
        runtime("java", 8, JavaOrigin::Path),
        runtime(
            "C:/Program Files/Java/jdk-21/bin/java.exe",
            21,
            JavaOrigin::System,
        ),
    ];
    let picked = pick(&candidates, &at_least(21)).expect("应有候选");
    assert_eq!(picked.origin, JavaOrigin::System);
}

#[test]
fn dedupe_collapses_one_install_found_twice() {
    // `JAVA_HOME` 指向 Program Files 里的 JDK → 同一个 java 不应列两次。
    let candidates = discover::dedupe(vec![
        runtime(
            "C:/Program Files/Java/jdk-21/bin/java.exe",
            21,
            JavaOrigin::JavaHome,
        ),
        runtime(
            "c:/program files/java/jdk-21/bin/java.exe",
            21,
            JavaOrigin::System,
        ),
    ]);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].origin, JavaOrigin::JavaHome);
}

#[test]
fn registry_absence_is_tolerated() {
    // 非 Windows 必然为空；Windows 上没装老 Oracle JDK 也是空。
    let homes = discover::registry_homes();
    for home in &homes {
        assert!(
            home.is_absolute(),
            "注册表里的 JavaHome 应是绝对路径：{}",
            home.display()
        );
    }
}
