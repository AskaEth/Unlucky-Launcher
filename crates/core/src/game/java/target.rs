//! 宿主平台 → Adoptium 请求参数，以及归档形态（Win = zip，Linux/mac = tar.gz）。
//!
//! Adoptium 的 `os` / `arch` 取值与 Rust 的 `target_os` / `target_arch` 不同形
//! （`windows`/`mac`/`linux` × `x64`/`aarch64`/`x86-32`……），这里做唯一一次翻译。
//! 平台差异仍然按项目纪律收敛在 `core::platform` 的语义里，本文件只做**命名映射**，
//! 不写任何能力探测。

use super::error::{JavaError, JavaErrorKind};

/// 一次 Adoptium 请求的目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdoptiumTarget {
    /// Adoptium 的 `os` 段（`windows` / `linux` / `mac`）。
    pub os: &'static str,
    /// Adoptium 的 `arch` 段（`x64` / `aarch64` / `x86-32` …）。
    pub arch: &'static str,
}

impl AdoptiumTarget {
    /// 宿主平台（不认识的架构直接拒绝，不猜一个错的产物下来）。
    ///
    /// # Errors
    ///
    /// 当前 OS / 架构没有 Temurin 产物时返回 [`JavaErrorKind::Unsupported`]。
    pub fn host() -> Result<Self, JavaError> {
        let os = if cfg!(windows) {
            "windows"
        } else if cfg!(target_os = "macos") {
            "mac"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else {
            return Err(JavaError::new(
                JavaErrorKind::Unsupported,
                format!("当前系统（{}）没有 Temurin 产物", std::env::consts::OS),
            )
            .with_hint("手动指定 JAVA_HOME，或用 `ull launch --java <path>` 挑已有 JDK"));
        };
        let arch = arch_name(std::env::consts::ARCH).ok_or_else(|| {
            JavaError::new(
                JavaErrorKind::Unsupported,
                format!("当前架构（{}）没有 Temurin 产物", std::env::consts::ARCH),
            )
            .with_hint("手动指定 JAVA_HOME，或用 `ull launch --java <path>` 挑已有 JDK")
        })?;
        Ok(Self { os, arch })
    }

    /// 构造（测试注入用）。
    #[must_use]
    pub const fn new(os: &'static str, arch: &'static str) -> Self {
        Self { os, arch }
    }

    /// 该平台的产物是不是 zip（Windows 是；Linux/mac 的 Temurin 只发 `.tar.gz`）。
    #[must_use]
    pub fn is_zip(self) -> bool {
        self.os == "windows"
    }

    /// 归档扩展名（拼「期望文件名」与给出「不支持的形态」提示时用）。
    #[must_use]
    pub fn archive_ext(self) -> &'static str {
        if self.is_zip() {
            "zip"
        } else {
            "tar.gz"
        }
    }
}

/// Rust 架构名 → Adoptium 架构名（不认识返回 `None`）。
#[must_use]
pub fn arch_name(arch: &str) -> Option<&'static str> {
    match arch {
        "x86_64" => Some("x64"),
        "aarch64" => Some("aarch64"),
        "x86" | "i686" => Some("x86-32"),
        "arm" => Some("arm"),
        "ppc64le" => Some("ppc64le"),
        "s390x" => Some("s390x"),
        "riscv64" => Some("riscv64"),
        _ => None,
    }
}
