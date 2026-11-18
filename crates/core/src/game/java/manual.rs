//! 用户手动指定的 JDK 路径（M175）：校验、厂商识别、名单持久化。
//!
//! 场景：MSI 装的 JDK 落在 `%ProgramFiles%` 之外的盘符上、或者用户自己解压的一份 JDK ——
//! [`super::discover`] 的标准位置扫描看不见它们，用户只能在设置页里**手动指一个路径**。
//!
//! # 三条口径
//!
//! 1. **收目录也收可执行文件**：两样都是用户会粘的东西，统一折成 `bin/java[.exe]`；
//! 2. **校验就是真跑一次 `java -version`**：目录存在、里面有 `bin/java` 都不算数——
//!    一份放错的 `java`（空文件、别的平台的可执行文件）必须在这里被拒，而不是等启动时崩；
//! 3. **这份名单是 UI 的持久化，不是启动策略**：真正「用哪个 Java 启动」仍由
//!    `java.selected_path`（core 的 schema 键）决定，名单只是「我手动加过哪些」的记忆。
//!
//! 名单落一个纯文本文件（一行一个路径）：它是 UI 侧的登记表，不参与 schema 校验，
//! 也不该借道 `launcher.json`（那条路要新键 ⇒ 动 schema 版本与迁移，代价远大于收益）。

use std::path::{Path, PathBuf};

use super::extract::java_binary_name;
use super::probe::{self, JavaOrigin, JavaRuntime};

/// 用户可粘路径的长度上限（防手滑粘进来一整段日志）。
pub const MAX_PATH_CHARS: usize = 512;

/// 手动路径被拒的原因（稳定 id 给 UI 选文案；不做本地化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualFailure {
    /// 空串 / 全空白
    Empty,
    /// 超过 [`MAX_PATH_CHARS`]
    TooLong,
    /// 盘上没这个文件或目录
    Missing,
    /// 有这个东西，但里面没有 `bin/java[.exe]`（或给的不是一个 java 可执行文件）
    NotJava,
    /// 有 `java`，但 `java -version` 跑不起来 / 读不出版本号
    NoVersion,
}

impl ManualFailure {
    /// 稳定 id（UI 文案 key 的后缀，见 `pages/settings/java`）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong => "too-long",
            Self::Missing => "missing",
            Self::NotJava => "not-java",
            Self::NoVersion => "no-version",
        }
    }
}

/// 用户给的路径 → `java` 可执行文件（不探测，只看文件系统）。
///
/// # Errors
/// 空 / 太长 / 不存在 / 不是 java 可执行文件时返回原因。
pub fn resolve_binary(input: &str) -> Result<PathBuf, ManualFailure> {
    let text = input.trim();
    if text.is_empty() {
        return Err(ManualFailure::Empty);
    }
    if text.chars().count() > MAX_PATH_CHARS {
        return Err(ManualFailure::TooLong);
    }
    let path = Path::new(text);
    if path.is_file() {
        return looks_like_java(path)
            .then(|| path.to_path_buf())
            .ok_or(ManualFailure::NotJava);
    }
    if path.is_dir() {
        let binary = path.join("bin").join(java_binary_name());
        return binary
            .is_file()
            .then_some(binary)
            .ok_or(ManualFailure::NotJava);
    }
    Err(ManualFailure::Missing)
}

/// 文件名是不是 `java` / `javaw`（Windows 带 `.exe`；POSIX 不区分大小写属于宽容）。
fn looks_like_java(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.eq_ignore_ascii_case("java") || stem.eq_ignore_ascii_case("javaw"))
}

/// 跑一次 `java -version`，同时把**原始输出**交回来（厂商要从整段输出里认，不能只看首行）。
///
/// 为什么需要原始输出：`-version` 的第一行只有 `openjdk version "21.0.11"`，发行方印在**第二行**
/// （`OpenJDK Runtime Environment Microsoft-11230047` / `… Temurin-21.0.4+7`）。只拿首行去认厂商，
/// Microsoft / Temurin 的 JDK 会一律显示成 `OpenJDK`——M175 出图时实测到了这个错。
#[must_use]
pub fn probe_with_output(binary: &Path, origin: JavaOrigin) -> (JavaRuntime, String) {
    probe_with_timeout(binary, origin, super::proc::PROBE_TIMEOUT, &|| false)
}

/// 同 [`probe_with_output`]，但带**超时与取消**（扫描线程用这一条）。
#[must_use]
pub fn probe_with_timeout(
    binary: &Path,
    origin: JavaOrigin,
    timeout: std::time::Duration,
    cancelled: &dyn Fn() -> bool,
) -> (JavaRuntime, String) {
    let mut runtime = JavaRuntime::unknown(binary.to_path_buf(), origin);
    let mut command = crate::platform::child::quiet(binary);
    command.arg("-version");
    let Some(output) = super::proc::run_with_timeout(command, timeout, cancelled) else {
        return (runtime, String::new());
    };
    let mut text = String::from_utf8_lossy(&output.stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stdout));
    if let Some((line, major)) = probe::parse_version_output(&text) {
        runtime.major = Some(major);
        runtime.version = Some(line);
        runtime.bits = probe::bits_of(&text);
    }
    runtime.vendor = vendor_of(&text);
    (runtime, text)
}

/// 严格探测：必须真跑得起来 `-version`。
///
/// 与 [`probe::probe`] 的差别只有一个：后者「探测不出也返回一个 runtime」，这里探测不出就是**拒绝**。
/// 手动添加这条路上，一个答不出话的 `java` 就是无效输入，不能进名单。
///
/// # Errors
/// 校验不过（[`resolve_binary`]）或 `-version` 读不出版本号时返回原因。
pub fn probe_manual(input: &str) -> Result<JavaRuntime, ManualFailure> {
    let binary = resolve_binary(input)?;
    let (runtime, _output) = probe_with_output(&binary, JavaOrigin::System);
    if runtime.major.is_none() {
        return Err(ManualFailure::NoVersion);
    }
    Ok(runtime)
}

/// `java -version` 输出里的发行方（认不出返回 `None`，不猜）。
///
/// 为什么从输出里认而不是从安装目录名认：用户手动指的路径跟目录命名毫无关系，
/// 而 `java -version` 的首行/次行本来就把发行方印出来了
/// （`OpenJDK Runtime Environment Temurin-21.0.4+7`、`Java(TM) SE Runtime Environment`）。
#[must_use]
pub fn vendor_of(text: &str) -> Option<String> {
    let lowered = text.to_ascii_lowercase();
    // 顺序即判据：「Temurin 构建的 OpenJDK」也能匹配到 openjdk，所以具体品牌必须排在前面。
    const MARKERS: &[(&str, &str)] = &[
        ("temurin", "Temurin"),
        ("adoptium", "Temurin"),
        ("microsoft", "Microsoft"),
        ("zulu", "Zulu"),
        ("azul", "Zulu"),
        ("corretto", "Corretto"),
        ("semeru", "Semeru"),
        ("graalvm", "GraalVM"),
        ("sapmachine", "SapMachine"),
        ("oracle", "Oracle"),
        // 老 Oracle / HotSpot 的 `-version` 不提 "Oracle"（Java 8 就是 `Java(TM) SE Runtime
        // Environment` + `Java HotSpot(TM) 64-Bit Server VM`），这两条是那一代的唯一指纹。
        ("se runtime", "Oracle"),
        ("hotspot", "Oracle"),
        ("openjdk", "OpenJDK"),
    ];
    MARKERS
        .iter()
        .find(|(needle, _)| lowered.contains(needle))
        .map(|(_, display)| (*display).to_string())
}

/// 读名单（文件不存在 / 读不动 ⇒ 空名单，不报错）。
///
/// 去掉空行、去重（按 [`probe::discover::path_key`](super::discover::path_key) 的同一套键），
/// 顺序保持文件里的顺序（用户添加的先后）。
#[must_use]
pub fn load_paths(file: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let path = PathBuf::from(line);
        if !paths
            .iter()
            .any(|known| super::discover::path_key(known) == super::discover::path_key(&path))
        {
            paths.push(path);
        }
    }
    paths
}

/// 写名单（一行一个，末尾换行；父目录不存在就建）。
///
/// # Errors
/// 目录建不出来或写盘失败时返回 IO 错误。
pub fn save_paths(file: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    for path in paths {
        text.push_str(&path.display().to_string());
        text.push('\n');
    }
    std::fs::write(file, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ull-java-manual-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("临时目录");
        dir
    }

    #[test]
    fn recognises_vendors_from_real_version_lines() {
        assert_eq!(
            vendor_of("OpenJDK Runtime Environment Temurin-21.0.4+7 (build 21.0.4+7-LTS)"),
            Some("Temurin".to_string())
        );
        assert_eq!(
            vendor_of("OpenJDK 64-Bit Server VM Microsoft-8321 (build 17.0.13+11)"),
            Some("Microsoft".to_string())
        );
        assert_eq!(
            vendor_of("openjdk version \"17.0.9\" 2023-10-17\nOpenJDK Runtime Environment Zulu17.46+19-CA"),
            Some("Zulu".to_string())
        );
        assert_eq!(
            vendor_of(
                "java version \"1.8.0_402\"\nJava(TM) SE Runtime Environment (build 1.8.0_402-b06)"
            ),
            Some("Oracle".to_string())
        );
        assert_eq!(vendor_of("nothing useful here"), None);
    }

    #[test]
    fn accepts_a_directory_and_a_binary_but_rejects_anything_else() {
        let dir = temp("resolve");
        let jdk = dir.join("jdk-21");
        std::fs::create_dir_all(jdk.join("bin")).expect("bin");
        let binary = jdk.join("bin").join(java_binary_name());
        std::fs::write(&binary, b"").expect("写 java");

        assert_eq!(
            resolve_binary(&jdk.display().to_string()),
            Ok(binary.clone())
        );
        assert_eq!(resolve_binary(&binary.display().to_string()), Ok(binary));
        assert_eq!(resolve_binary("   "), Err(ManualFailure::Empty));
        assert_eq!(
            resolve_binary(&dir.join("nope").display().to_string()),
            Err(ManualFailure::Missing)
        );
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).expect("empty");
        assert_eq!(
            resolve_binary(&empty.display().to_string()),
            Err(ManualFailure::NotJava)
        );
        // 目录里有一个**不叫 java 的**可执行文件：也不算。
        let text = dir.join("notes.txt");
        std::fs::write(&text, b"hi").expect("写文本");
        assert_eq!(
            resolve_binary(&text.display().to_string()),
            Err(ManualFailure::NotJava)
        );
        let long = "a".repeat(MAX_PATH_CHARS + 1);
        assert_eq!(resolve_binary(&long), Err(ManualFailure::TooLong));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_binary_that_cannot_answer_version_is_refused() {
        let dir = temp("noversion");
        let jdk = dir.join("jdk");
        std::fs::create_dir_all(jdk.join("bin")).expect("bin");
        // 空文件：解析不出版本号 ⇒ 拒绝（这条正是「文件存在 ≠ 能用」的判据）。
        std::fs::write(jdk.join("bin").join(java_binary_name()), b"").expect("写 java");
        assert_eq!(
            probe_manual(&jdk.display().to_string()).err(),
            Some(ManualFailure::NoVersion)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn the_list_round_trips_and_dedupes() {
        let dir = temp("list");
        let file = dir.join("config").join("java-extra-paths.txt");
        let paths = vec![
            PathBuf::from("C:/jdk/17"),
            PathBuf::from("C:/jdk/21"),
            PathBuf::from("C:\\jdk\\17"),
        ];
        save_paths(&file, &paths).expect("写名单");
        // 第三条是第一条的另一种写法：读回来只留一条（与扫描去重同一套键）。
        assert_eq!(load_paths(&file).len(), 2);
        assert_eq!(load_paths(&file)[0], PathBuf::from("C:/jdk/17"));

        let missing = dir.join("nope.txt");
        assert!(load_paths(&missing).is_empty(), "缺文件是空名单，不是错误");
        std::fs::remove_dir_all(&dir).ok();
    }
}
