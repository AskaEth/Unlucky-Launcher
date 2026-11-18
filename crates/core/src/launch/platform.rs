//! 平台上下文：命令行组装与进程管理需要知道的操作系统事实。
//!
//! 依据 `analyse/02` §4.1（Linux/macOS `nice`）、§4.4（原生编码）、§4.6（原生库路径）、
//! §4.10（环境变量，`APPDATA` 重定向只在 Windows 有意义）。
//!
//! `os_version` 不再是空占位：`current()` 从 [`crate::platform::os_info`] 的运行期探测取值，
//! 清单里带 `rules.os.version` 的参数项因此能真正命中（D19 · `PlanDoc/11-os-detection.md` §2）。

use std::path::PathBuf;

pub use crate::platform::os_info::OsFamily;

/// 平台上下文。
#[derive(Debug, Clone)]
pub struct PlatformCtx {
    /// 操作系统家族。
    pub os: OsFamily,
    /// `rules.os.arch` 用的架构名。
    pub arch: String,
    /// 系统原生编码（`-Dfile.encoding` 的默认值来源）。
    pub native_charset: String,
    /// 主目录（脱敏与 `-Duser.home` 用）。
    pub home_dir: Option<PathBuf>,
    /// 系统版本串（`rules.os.version` 用，例如 `10.0.19045`）。
    pub os_version: String,
}

impl PlatformCtx {
    /// 按当前编译目标推断，版本串取运行期真实探测值。
    pub fn current() -> Self {
        let info = crate::platform::os_info::current();
        Self {
            os: info.family,
            arch: info.arch.clone(),
            native_charset: native_charset().to_string(),
            home_dir: home_dir(),
            os_version: info.os_version().to_string(),
        }
    }

    /// 测试用：显式指定系统与架构，版本串留空（规则求值走保守分支）。
    pub fn new(os: OsFamily, arch: &str) -> Self {
        Self {
            os,
            arch: arch.to_string(),
            native_charset: native_charset().to_string(),
            home_dir: None,
            os_version: String::new(),
        }
    }

    /// 显式指定版本串（测试与适配层注入用）。
    pub fn with_os_version(mut self, version: impl Into<String>) -> Self {
        self.os_version = version.into();
        self
    }

    /// classpath 分隔符（Windows `;`，POSIX `:`）。
    pub fn path_separator(&self) -> char {
        if self.os == OsFamily::Windows {
            ';'
        } else {
            ':'
        }
    }

    /// 文件名分隔符的字符串形式（供 `${file_separator}` 占位符）。
    pub fn file_separator(&self) -> &'static str {
        if self.os == OsFamily::Windows {
            "\\"
        } else {
            "/"
        }
    }
}

impl Default for PlatformCtx {
    fn default() -> Self {
        Self::current()
    }
}

/// 系统原生编码名。
///
/// 中文 Windows 上系统编码是 GBK，但 Java 18+ 默认 UTF-8；这里统一给 `UTF-8`，
/// 需要跟随系统时由用户在 `-Dfile.encoding=COMPAT` 语义下覆盖（`analyse/02` §4.4）。
fn native_charset() -> &'static str {
    "UTF-8"
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}
