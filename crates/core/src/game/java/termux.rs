//! Termux 系统 JDK（`openjdk-<major>` 包）的安装与定位。
//!
//! Termux 的 openjdk 是 `.deb`（bionic），由 apt/dpkg 装进 `$PREFIX`，与 Temurin 的
//! zip/tar.gz 归档不是同一形态，所以**不走** `source::fetch` 的归档链。这里委托包管理器：
//!
//! 1. 用内置镜像生成一份**隔离**的 sources.list；
//! 2. `apt-get update && install` 的 lists/cache 都指向临时目录，不污染用户的 apt 状态；
//! 3. 装完在 `$PREFIX/lib/jvm` 下找 `java-<major>-openjdk`，用 `bin/java` 是否存在做判据。
//!
//! 镜像顺序是实测可用的国内源；全部失败才报错，错误里带每一跳的原因。

use std::path::{Path, PathBuf};

use crate::platform::child::quiet;
use crate::platform::paths::ProcessEnv;
use crate::platform::termux as host;

use super::error::{JavaError, JavaErrorKind};

/// 内置镜像（termux-main 仓库根）。顺序即尝试顺序。
pub const MIRRORS: &[&str] = &[
    "https://mirrors.sau.edu.cn/termux/apt/termux-main",
    "https://mirrors.tuna.tsinghua.edu.cn/termux/apt/termux-main",
    "https://mirrors.ustc.edu.cn/termux/apt/termux-main",
    "https://packages.termux.dev/apt/termux-main",
];

/// 当前宿主是不是 Termux。
#[must_use]
pub fn available() -> bool {
    host::is_termux(&ProcessEnv)
}

/// 大版本 → Termux 包名（主源只有 17 / 21 / 25）。
#[must_use]
pub fn package_for(major: u32) -> Option<String> {
    matches!(major, 17 | 21 | 25).then(|| format!("openjdk-{major}"))
}

/// `$PREFIX/lib/jvm` 下与 `major` 对应的 JDK 根（`bin/java` 必须存在）。
#[must_use]
pub fn installed_home(prefix: &Path, major: u32) -> Option<PathBuf> {
    let root = prefix.join("lib").join("jvm");
    let entries = std::fs::read_dir(root).ok()?;
    let dashed = format!("java-{major}-openjdk");
    let plain = format!("openjdk-{major}");
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join("bin").join("java").is_file())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == dashed || name == plain)
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// 一条 `deb` 行（测试与安装共用同一份拼法）。
#[must_use]
pub fn sources_line(mirror: &str) -> String {
    format!("deb {} stable main\n", mirror.trim_end_matches('/'))
}

/// 装一个 Termux JDK，返回它的根目录。
///
/// # Errors
///
/// 当前不在 Termux、主源没有该大版本、所有镜像都失败、装完仍找不到 `bin/java` 时返回错误。
///
/// 返回 `(JDK 根, 是否复用了已装)`：包管理器刚装完与「本来就有」对调用方是两种结果
/// （一个要报安装，一个要报「未走网络」）。
pub async fn install(major: u32) -> Result<(PathBuf, bool), JavaError> {
    let Some(package) = package_for(major) else {
        return Err(JavaError::new(
            JavaErrorKind::Unsupported,
            format!("Termux 主源没有 Java {major}（只有 17 / 21 / 25）"),
        )
        .with_hint("`pkg search openjdk` 看可用包；其它大版本请用归档发行版"));
    };
    let Some(prefix) = host::prefix(&ProcessEnv) else {
        return Err(
            JavaError::new(JavaErrorKind::Unsupported, "termux 源只在 Termux 里可用")
                .with_hint("普通 Linux/Windows 用 Temurin / Oracle / MS / Zulu"),
        );
    };

    // 幂等：已经装了同一大版本就不碰网络。
    if let Some(home) = installed_home(&prefix, major) {
        return Ok((home, true));
    }

    let workspace = workspace_dir(&prefix, major);
    std::fs::create_dir_all(&workspace).map_err(|error| {
        JavaError::new(
            JavaErrorKind::Io,
            format!(
                "建 Termux 安装临时目录失败（{}）：{error}",
                workspace.display()
            ),
        )
    })?;

    let package_for_task = package.clone();
    tokio::task::spawn_blocking(move || run_install(&prefix, &workspace, &package_for_task, major))
        .await
        .map_err(|error| {
            JavaError::new(
                JavaErrorKind::Io,
                format!("Termux 安装任务没能完成：{error}"),
            )
        })?
        .map(|home| (home, false))
}

/// 逐镜像尝试 `apt-get`（阻塞，跑在 `spawn_blocking` 里）。
fn run_install(
    prefix: &Path,
    workspace: &Path,
    package: &str,
    major: u32,
) -> Result<PathBuf, JavaError> {
    let mut failures: Vec<String> = Vec::new();
    for mirror in MIRRORS {
        match apt_install(prefix, workspace, mirror, package) {
            Ok(()) => {
                if let Some(home) = installed_home(prefix, major) {
                    // 装完把隔离的 lists/cache/archives 清掉（几十到上百 MB，别留在 $PREFIX/tmp）。
                    let _ = std::fs::remove_dir_all(workspace);
                    return Ok(home);
                }
                failures.push(format!("{mirror}：装完找不到 bin/java"));
            }
            Err(error) => failures.push(format!("{mirror}：{error}")),
        }
    }
    Err(JavaError::new(
        JavaErrorKind::Network,
        format!(
            "Termux 包安装失败（试过 {} 个镜像）：{}",
            MIRRORS.len(),
            failures.join("；")
        ),
    )
    .with_hint("确认 apt 可用、磁盘空间足够，或手动 `pkg install openjdk-<major>`"))
}

/// 对单个镜像跑 `apt-get update` + `install`（lists/cache 隔离在 workspace 内）。
fn apt_install(prefix: &Path, workspace: &Path, mirror: &str, package: &str) -> Result<(), String> {
    let sources = workspace.join("sources.list");
    std::fs::write(&sources, sources_line(mirror))
        .map_err(|error| format!("写 sources.list 失败：{error}"))?;

    let lists = workspace.join("lists");
    let cache = workspace.join("cache");
    let archives = workspace.join("archives");
    for dir in [&lists, &cache, &archives] {
        std::fs::create_dir_all(dir).map_err(|error| format!("建临时目录失败：{error}"))?;
    }

    let options = [
        format!("Dir::Etc::sourcelist={}", sources.display()),
        "Dir::Etc::sourceparts=-".to_string(),
        format!("Dir::State::lists={}", lists.display()),
        format!("Dir::Cache={}", cache.display()),
        format!("Dir::Cache::archives={}", archives.display()),
        "Acquire::ForceIPv4=true".to_string(),
        "Acquire::Retries=2".to_string(),
        "Acquire::http::Timeout=30".to_string(),
        "Acquire::https::Timeout=30".to_string(),
    ];

    run_apt(prefix, &options, &["update"])?;
    run_apt(
        prefix,
        &options,
        &["install", "-y", "--no-install-suggests", package],
    )
}

/// 执行一次 `apt-get`（`-o` 选项在前，子命令在后）。
fn run_apt(prefix: &Path, options: &[String], args: &[&str]) -> Result<(), String> {
    let apt = prefix.join("bin").join("apt-get");
    let mut command = quiet(&apt);
    command.env("DEBIAN_FRONTEND", "noninteractive");
    for option in options {
        command.arg("-o").arg(option);
    }
    command.args(args);
    let output = command
        .output()
        .map_err(|error| format!("执行 {} 失败：{error}", apt.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let last = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("（无 stderr 输出）");
    Err(format!(
        "apt-get {} 退出 {}：{last}",
        args.first().copied().unwrap_or("?"),
        output.status.code().unwrap_or(-1)
    ))
}

/// 临时工作区（按大版本区分，避免两次安装互相踩 lists）。
fn workspace_dir(prefix: &Path, major: u32) -> PathBuf {
    prefix
        .join("tmp")
        .join("ull-java-termux")
        .join(format!("jdk-{major}"))
}

#[cfg(test)]
mod tests {
    use super::{installed_home, package_for, sources_line, MIRRORS};

    #[test]
    fn only_the_majors_that_termux_ships_are_installable() {
        assert_eq!(package_for(17).as_deref(), Some("openjdk-17"));
        assert_eq!(package_for(21).as_deref(), Some("openjdk-21"));
        assert_eq!(package_for(25).as_deref(), Some("openjdk-25"));
        assert_eq!(package_for(8), None);
        assert_eq!(package_for(11), None);
    }

    #[test]
    fn sources_line_has_one_deb_and_keeps_the_mirror_root() {
        let line = sources_line("https://mirrors.tuna.tsinghua.edu.cn/termux/apt/termux-main/");
        assert_eq!(
            line,
            "deb https://mirrors.tuna.tsinghua.edu.cn/termux/apt/termux-main stable main\n"
        );
        assert!(MIRRORS.iter().all(|mirror| mirror.starts_with("https://")));
    }

    #[test]
    fn installed_home_requires_bin_java_and_matching_major() {
        let base = std::env::temp_dir().join(format!("ull-termux-home-{}", std::process::id()));
        let good = base.join("lib").join("jvm").join("java-21-openjdk");
        let empty = base.join("lib").join("jvm").join("java-17-openjdk");
        std::fs::create_dir_all(good.join("bin")).unwrap();
        std::fs::write(good.join("bin").join("java"), b"").unwrap();
        std::fs::create_dir_all(&empty).unwrap();

        assert_eq!(installed_home(&base, 21), Some(good));
        assert_eq!(installed_home(&base, 17), None, "没有 bin/java 的目录不算");
        assert_eq!(installed_home(&base, 25), None);

        std::fs::remove_dir_all(&base).ok();
    }
}
