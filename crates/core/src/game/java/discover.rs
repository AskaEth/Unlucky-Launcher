//! 标准安装位置扫描 + 全部发现源的合并去重（`java list` 的「系统发现」半边）。
//!
//! 只认 `JAVA_HOME`/`PATH` 会漏掉最常见的一类用户：**用 MSI 装的 Temurin / Microsoft OpenJDK
//! 不设环境变量**（安装器只登记开始菜单）。HMCL 的 `JavaManager.searchAllJavaInRepository`
//! 与 PCL 的 `JavaUtils.SearchFolders` 都扫这些位置，本模块照同一思路：
//!
//! | 平台 | 扫描位置 |
//! | --- | --- |
//! | Windows | `%ProgramFiles%\{Java, Eclipse Adoptium, Microsoft, Zulu, Amazon Corretto, BellSoft, Semeru}\*`（含 `ProgramFiles(x86)` / `ProgramW6432` 变体，WOW64 才看得到 32 位安装）、`%APPDATA%\.minecraft\runtime\*`（**官方启动器装的运行时**，跨产品兼容）、`HKLM\SOFTWARE\JavaSoft\JDK`（老 Oracle 写的，新版发行版通常没有→容错跳过） |
//! | Linux | `/usr/lib/jvm/*`、`/usr/java/*` |
//! | Termux | `$PREFIX/lib/jvm/*`（`java-17-openjdk` 这类；`$PREFIX` 由 [`crate::platform::termux`] 识别） |
//! | macOS | `/Library/Java/JavaVirtualMachines/*.jdk/Contents/Home` |
//!
//! 三个纪律：
//!
//! 1. **目录不存在是常态**（本机实测 `%APPDATA%\.minecraft\runtime` 与 `JavaSoft\JDK` 都没有），
//!    一律静默跳过，绝不报错；
//! 2. **同一路径多来源命中只列一次**，保留优先级高者（`JAVA_HOME` 指向 Program Files 里的
//!    JDK 时，用户看到的是 `JAVA_HOME` 而不是 `system`）；
//! 3. **扫描只在需要时才做**：`java list` 全扫；启动路径上先看托管+环境，不满足才扫盘
//!    （别为了一次启动白跑 N 次 `java -version`）。

use std::path::{Path, PathBuf};

use crate::platform::child::quiet;
use crate::platform::paths::{EnvSource, ProcessEnv};
use crate::platform::termux;

use super::extract::java_binary_name;
use super::probe::{self, discover_managed_roots, system_candidates, JavaOrigin, JavaRuntime};

/// 全部发现源（托管 → `JAVA_HOME` → `PATH` → 标准安装位置），已按路径去重。
#[must_use]
pub fn candidates(managed_root: &Path) -> Vec<JavaRuntime> {
    candidates_in_roots(std::slice::from_ref(&managed_root.to_path_buf()))
}

/// 同上，但托管区可以是**多个安装根**（Portable 数据根 + Standard/Custom 落点，D23）。
#[must_use]
pub fn candidates_in_roots(managed_roots: &[PathBuf]) -> Vec<JavaRuntime> {
    let mut all = discover_managed_roots(managed_roots);
    all.extend(system_candidates());
    all.extend(standard_candidates());
    dedupe(all)
}

/// 标准安装位置扫出来的运行时（逐个探测）。
#[must_use]
pub fn standard_candidates() -> Vec<JavaRuntime> {
    scan_java_binaries()
        .into_iter()
        .map(|binary| probe::probe(&binary, JavaOrigin::System))
        .collect()
}

/// 扫描标准安装位置，返回 `bin/java[.exe]` 路径表（稳定排序，未探测）。
#[must_use]
pub fn scan_java_binaries() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in standard_roots() {
        let binary = root.join("bin").join(java_binary_name());
        if binary.is_file() {
            found.push(binary);
        }
    }
    for home in registry_homes() {
        let binary = home.join("bin").join(java_binary_name());
        if binary.is_file() {
            found.push(binary);
        }
    }
    found.sort();
    found.dedup();
    found
}

/// 候选 JDK 根目录（`bin/java` 的父目录的父目录）。
#[must_use]
pub fn standard_roots() -> Vec<PathBuf> {
    standard_roots_in(&ProcessEnv)
}

/// 同 [`standard_roots`]，但环境变量可注入（测试 / 便携化封装用）。
#[must_use]
pub fn standard_roots_in(env: &dyn EnvSource) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if cfg!(windows) {
        for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
            let Some(base) = std::env::var_os(variable) else {
                continue;
            };
            let base = PathBuf::from(base);
            for vendor in WINDOWS_VENDOR_DIRS {
                roots.extend(child_dirs(&base.join(vendor)));
            }
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            roots.extend(child_dirs(
                &PathBuf::from(appdata).join(".minecraft").join("runtime"),
            ));
        }
    } else if cfg!(target_os = "macos") {
        for entry in child_dirs(Path::new("/Library/Java/JavaVirtualMachines")) {
            // `*.jdk/Contents/Home`（`java_home` 语义）。
            let home = entry.join("Contents").join("Home");
            roots.push(if home.is_dir() { home } else { entry });
        }
    } else {
        for base in ["/usr/lib/jvm", "/usr/java"] {
            roots.extend(child_dirs(Path::new(base)));
        }
        // Termux 的 JDK 落在 `$PREFIX/lib/jvm`（如 `java-17-openjdk`），不在 `/usr` 下。
        if let Some(root) = termux::jvm_root(env) {
            roots.extend(child_dirs(&root));
        }
    }
    roots.sort();
    roots.dedup();
    roots
}

/// Windows 各发行版的安装父目录（每种下面按子目录名当版本）。
const WINDOWS_VENDOR_DIRS: &[&str] = &[
    "Java",
    "Eclipse Adoptium",
    "Microsoft",
    "Zulu",
    "Amazon Corretto",
    "BellSoft",
    "Semeru",
];

/// 列一个目录的直接子目录（不存在/无权限一律空）。
fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect()
}

/// 注册表里登记的 `JavaHome`（只查老 Oracle 的 `JavaSoft\JDK`；查不到返回空）。
///
/// 不走额外依赖：`reg.exe` 在 Win10+ 恒定存在，输出格式也稳定
/// （`    JavaHome    REG_SZ    C:\Program Files\Java\jdk-17`）。
#[must_use]
pub fn registry_homes() -> Vec<PathBuf> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut homes = Vec::new();
    for view in ["64", "32"] {
        let Ok(output) = quiet("reg")
            .args(["query", "HKLM\\SOFTWARE\\JavaSoft\\JDK", "/s", "/reg:"])
            .arg(view)
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        homes.extend(parse_java_home(&String::from_utf8_lossy(&output.stdout)));
    }
    homes.sort();
    homes.dedup();
    homes
}

/// 从 `reg query /s` 的输出里抠出 `JavaHome` 的值（纯函数，便于用真实输出做单测）。
#[must_use]
pub fn parse_java_home(output: &str) -> Vec<PathBuf> {
    output
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("JavaHome")?;
            let (_, value) = rest.split_once("REG_SZ")?;
            let home = value.trim();
            (!home.is_empty()).then(|| PathBuf::from(home))
        })
        .collect()
}

/// 按路径去重，保留**先出现**的那个（调用方按优先级排好序）。
#[must_use]
pub fn dedupe(runtimes: Vec<JavaRuntime>) -> Vec<JavaRuntime> {
    let mut seen: Vec<String> = Vec::with_capacity(runtimes.len());
    let mut kept = Vec::with_capacity(runtimes.len());
    for runtime in runtimes {
        let key = path_key(&runtime.binary);
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        kept.push(runtime);
    }
    kept
}

/// 去重键：剥掉 Windows 的 `\\?\` 前缀、分隔符一律折成 `/`、大小写只在 Windows 折叠。
///
/// 前两步与平台无关：同一条 `bin/java[.exe]` 用 `/` 还是 `\` 拼出来都是同一个文件，
/// 键必须相同——`JAVA_HOME` 的写法与扫盘拼出来的路径常在这点上不一致，键不齐就重复列。
/// 大小写折叠**只能**跟着平台走：NTFS 不区分大小写，而 POSIX 上 `Java` 与 `java` 是两个
/// 目录，折了会把两份真实安装误并成一个。
///
/// 代价（已知且有意的取舍）：POSIX 上文件名里的字面反斜杠也会被当分隔符。JDK 安装路径带
/// 反斜杠属于病态情况，换来的是「同一条 java 的两种拼法永远同键」这条平台无关的不变量。
#[must_use]
pub fn path_key(path: &Path) -> String {
    let text = path.display().to_string();
    let text = text
        .strip_prefix(r"\\?\")
        .unwrap_or(&text)
        .replace('\\', "/");
    if cfg!(windows) {
        text.to_ascii_lowercase()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{dedupe, parse_java_home, path_key, standard_roots};
    use crate::game::java::probe::{JavaOrigin, JavaRuntime};
    // 这两个只有 Termux 扫描的两个测试用，而那两个测试是 `cfg(unix)`——导入必须同样 gate，
    // 否则 Windows 上不编译测试、导入就成了未使用（`-D warnings` 直接判错）。
    #[cfg(unix)]
    use super::standard_roots_in;
    #[cfg(unix)]
    use crate::platform::paths::MapEnv;

    fn runtime(path: &str, origin: JavaOrigin) -> JavaRuntime {
        JavaRuntime {
            binary: std::path::PathBuf::from(path),
            origin,
            version: None,
            major: Some(21),
            bits: 64,
            vendor: None,
        }
    }

    #[test]
    fn dedupe_keeps_the_higher_priority_source() {
        let list = vec![
            runtime(
                "C:/Program Files/Java/jdk-21/bin/java.exe",
                JavaOrigin::JavaHome,
            ),
            runtime(
                "C:\\Program Files\\Java\\jdk-21\\bin\\java.exe",
                JavaOrigin::System,
            ),
            runtime(
                "C:/Program Files/Zulu/zulu-17/bin/java.exe",
                JavaOrigin::System,
            ),
        ];
        let kept = dedupe(list);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].origin, JavaOrigin::JavaHome, "重复路径保留高优先级");
    }

    #[test]
    fn path_key_normalises_separators_on_every_platform() {
        // 平台无关的不变量：同一条 `bin/java` 用 `/` 还是 `\` 拼出来，键必须相同。
        // （M49：原来这里只在 Windows 上成立，Linux 上 `\` 原样留着 → 断言挂。）
        let forward = path_key(std::path::Path::new("C:/a/b/java.exe"));
        let backward = path_key(std::path::Path::new("C:\\a\\b\\java.exe"));
        assert_eq!(forward, backward);
        assert!(
            !forward.contains('\\'),
            "键里不该残留反斜杠（与平台无关）：{forward}"
        );
        assert!(
            !forward.starts_with(r"\\"),
            "`\\\\?\\` 前缀不进键：{forward}"
        );
    }

    #[test]
    fn one_install_normalises_to_one_key_whatever_the_spelling() {
        // 回归钉子：`JAVA_HOME` 与扫盘常对同一个 java 给出不同写法，四种拼法必须同键。
        let spellings = [
            "C:/Program Files/Java/jdk-21/bin/java.exe",
            r"C:\Program Files\Java\jdk-21\bin\java.exe",
            r"\\?\C:\Program Files\Java\jdk-21\bin\java.exe",
            r"C:\Program Files\Java\jdk-21/bin/java.exe",
        ];
        let key = path_key(std::path::Path::new(spellings[0]));
        for spelling in spellings {
            assert_eq!(
                path_key(std::path::Path::new(spelling)),
                key,
                "同一安装的两种写法必须同键：{spelling}"
            );
        }
    }

    #[test]
    fn path_key_folds_case_only_where_the_filesystem_does() {
        // 大小写折叠跟平台走，两侧都钉住：NTFS 不区分大小写 → 折；
        // POSIX 上 `Java` 与 `java` 是两个目录 → 不折（折了会把两份真实安装误并）。
        let upper = path_key(std::path::Path::new(
            "C:/Program Files/Java/jdk-21/bin/java.exe",
        ));
        let lower = path_key(std::path::Path::new(
            "c:/program files/java/jdk-21/bin/java.exe",
        ));
        if cfg!(windows) {
            assert_eq!(upper, lower, "Windows：大小写不同的同一路径应同键");
        } else {
            assert_ne!(upper, lower, "POSIX：大小写是有效区别，不应同键");
        }
    }

    #[test]
    fn parses_real_reg_query_output() {
        // 实测形态（`reg query HKLM\SOFTWARE\JavaSoft\JDK /s`）。
        let output = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\r\n    CurrentVersion    REG_SZ    1.8\r\n\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\JavaSoft\\JDK\\1.8\r\n    JavaHome    REG_SZ    C:\\Program Files\\Java\\jdk1.8.0_402\r\n";
        let homes = parse_java_home(output);
        assert_eq!(
            homes,
            vec![std::path::PathBuf::from(
                r"C:\Program Files\Java\jdk1.8.0_402"
            )]
        );
    }

    #[test]
    fn missing_registry_key_is_not_an_error() {
        assert!(parse_java_home("错误: 系统找不到指定的注册表项或值。\r\n").is_empty());
    }

    #[test]
    fn standard_roots_never_panic_on_missing_directories() {
        // 本机大概率缺若干目录；这里只要求「能跑、不 panic」。
        let roots = standard_roots();
        assert!(roots.iter().all(|root| !root.as_os_str().is_empty()));
    }

    #[cfg(unix)]
    #[test]
    fn termux_jvm_root_is_scanned() {
        let base = std::env::temp_dir().join(format!("ull-termux-jvm-{}", std::process::id()));
        let jdk = base.join("lib").join("jvm").join("java-17-openjdk");
        std::fs::create_dir_all(jdk.join("bin")).unwrap();
        std::fs::write(jdk.join("bin").join("java"), b"").unwrap();

        let env = MapEnv::new()
            .with("TERMUX_VERSION", "0.118.3")
            .with("PREFIX", base.to_str().unwrap());
        let roots = standard_roots_in(&env);
        assert!(
            roots.contains(&jdk),
            "Termux 的 $PREFIX/lib/jvm 必须进扫描表：{roots:?}"
        );

        std::fs::remove_dir_all(&base).ok();
    }

    #[cfg(unix)]
    #[test]
    fn non_termux_env_does_not_scan_prefix() {
        let base = std::env::temp_dir().join(format!("ull-not-termux-{}", std::process::id()));
        let jdk = base.join("lib").join("jvm").join("java-17-openjdk");
        std::fs::create_dir_all(&jdk).unwrap();

        let env = MapEnv::new().with("PREFIX", base.to_str().unwrap());
        let roots = standard_roots_in(&env);
        assert!(
            !roots.contains(&jdk),
            "非 Termux 不该扫 $PREFIX/lib/jvm：{roots:?}"
        );

        std::fs::remove_dir_all(&base).ok();
    }
}
