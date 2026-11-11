//! Linux arm64 的 natives 改写与下载兜底。
//!
//! 两件事实拼出来的特例：
//! 1. Mojang 清单里 LWJGL3 的 natives 只写 `natives-linux`（x86_64 的 `.so`）；
//!    直接用它，在 aarch64 JVM 里加载必失败。
//! 2. 同一个 Maven 坐标确实有 `natives-linux-arm64` 分类器，但 `libraries.minecraft.net`
//!    对它一律 404（实测 3.3.2 的 lwjgl/glfw/opengl/stb/tinyfd/openal），Maven Central 与
//!    阿里云镜像有。
//!
//! 所以 Linux arm64 上：分类器改写（`Library::native_classifier` /
//! `Library::linux_arm64_native_name`）+ 地址兜底到镜像。**只在 linux + arm64 生效**，
//! x86_64 / Windows / macOS 的地址一个字节不变。用例仍在 `libraries.rs` 的 tests 里
//! （它们从 `plan()` 的出口断言，不直接调这里的私有步骤）。
//!
//! 从 `libraries.rs` 拆出来是为了守住 400 行红线（`guards.yml` 的 `file-size`）。

use std::path::{Path, PathBuf};

use crate::game::arguments::Environment;
use crate::game::library::Library;

/// Linux arm64 的 natives 兜底仓库：Mojang 的 `libraries.minecraft.net` 对
/// `natives-linux-arm64` 一律 404（实测 3.3.2 的 lwjgl/glfw/opengl/stb/tinyfd/openal），
/// 同一坐标在 Maven Central / 阿里云镜像有。
pub const LINUX_ARM64_NATIVES_BASE: &str = "https://maven.aliyun.com/repository/public/";

/// Linux arm64 的 natives 分类器（见 `Library::native_classifier`）。
pub const LINUX_ARM64_CLASSIFIER: &str = "natives-linux-arm64";

/// 现代清单里 LWJGL3 natives 条目的改写结果。
pub(super) struct Arm64Native {
    pub(super) name: String,
    pub(super) relative: PathBuf,
    pub(super) url: String,
}

/// Linux arm64：把 `org.lwjgl:<module>:<version>:natives-linux` 改写成 arm64。
pub(super) fn linux_arm64_native_entry(
    library: &Library,
    env: &Environment,
) -> Option<Arm64Native> {
    let name = library.linux_arm64_native_name(env)?;
    let base = name.strip_suffix(":natives-linux-arm64")?;
    let mut parts = base.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let relative = PathBuf::from(format!(
        "{}/{artifact}/{version}/{artifact}-{version}-natives-linux-arm64.jar",
        group.replace('.', "/")
    ));
    let url = format!(
        "{LINUX_ARM64_NATIVES_BASE}{}",
        relative.to_string_lossy().replace('\\', "/")
    );
    Some(Arm64Native {
        name,
        relative,
        url,
    })
}

/// arm64 natives 的兜底 URL（按相对路径拼镜像基址）。
pub(super) fn arm64_native_url(relative: &Path) -> String {
    format!(
        "{LINUX_ARM64_NATIVES_BASE}{}",
        relative.to_string_lossy().replace('\\', "/")
    )
}
