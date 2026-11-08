//! Termux（Android 上的用户空间）宿主识别。
//!
//! Termux 的软件前缀不在 `/usr`，而是 `$PREFIX`（典型值
//! `/data/data/com.termux/files/usr`）；JDK 由 `pkg install openjdk-17` 落到
//! `$PREFIX/lib/jvm/java-17-openjdk`。这里只做**识别与路径推导**，不碰包管理，
//! 也不把 `$PREFIX` 当成编译期常量写死。

use std::path::PathBuf;

use super::paths::EnvSource;

/// Termux 的默认前缀（`PREFIX` 缺失时的兜底）。
pub const DEFAULT_PREFIX: &str = "/data/data/com.termux/files/usr";

/// 当前环境是否运行在 Termux 里。
///
/// 判据取 Termux 自己导出的两个变量：`TERMUX_VERSION`（版本）或 `PREFIX`
/// 指向默认前缀。两者都没有时按普通 Linux 处理——不猜。
#[must_use]
pub fn is_termux(env: &dyn EnvSource) -> bool {
    env.var("TERMUX_VERSION").is_some()
        || env
            .var("PREFIX")
            .is_some_and(|prefix| prefix.trim_end_matches('/') == DEFAULT_PREFIX)
}

/// Termux 的软件前缀（仅当识别为 Termux 时）。
#[must_use]
pub fn prefix(env: &dyn EnvSource) -> Option<PathBuf> {
    if !is_termux(env) {
        return None;
    }
    Some(
        env.var("PREFIX")
            .map_or_else(|| PathBuf::from(DEFAULT_PREFIX), PathBuf::from),
    )
}

/// Termux 的 JDK 安装根（`$PREFIX/lib/jvm`）。
#[must_use]
pub fn jvm_root(env: &dyn EnvSource) -> Option<PathBuf> {
    prefix(env).map(|prefix| prefix.join("lib").join("jvm"))
}

#[cfg(test)]
mod tests {
    use super::{is_termux, jvm_root, prefix, DEFAULT_PREFIX};
    use crate::platform::paths::MapEnv;

    #[test]
    fn plain_linux_is_not_termux() {
        let env = MapEnv::new().with("HOME", "/home/u");
        assert!(!is_termux(&env));
        assert_eq!(prefix(&env), None);
        assert_eq!(jvm_root(&env), None);
    }

    #[test]
    fn default_prefix_is_recognised() {
        let env = MapEnv::new().with("PREFIX", DEFAULT_PREFIX);
        assert!(is_termux(&env));
        assert_eq!(
            jvm_root(&env).unwrap(),
            std::path::PathBuf::from(DEFAULT_PREFIX).join("lib/jvm")
        );
    }

    #[test]
    fn version_variable_alone_is_enough() {
        let env = MapEnv::new()
            .with("TERMUX_VERSION", "0.118.3")
            .with("PREFIX", "/data/data/com.termux/files/usr");
        assert!(is_termux(&env));
    }

    #[test]
    fn other_prefix_is_not_termux() {
        // `PREFIX` 被搬到别的目录不在识别范围内——只有版本变量或默认前缀算数。
        let env = MapEnv::new().with("PREFIX", "/opt/termux");
        assert!(!is_termux(&env));
    }
}
