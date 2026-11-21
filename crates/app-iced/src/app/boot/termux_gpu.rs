//! Termux 上自动启用随附的 Vulkan ICD（glibc 侧 turnip，`ull_freedreno.json`）。
//!
//! 背景：Termux 软件源里的 glibc Mesa（24.2.6）不认识 Adreno 840，硬件 GPU 需要用
//! `packaging/docs/termux-glibc-build.md` §5 里自编的 Mesa 26（KGSL）。驱动装在
//! `$PREFIX/glibc/share/vulkan/icd.d/ull_freedreno.json` 时，这里在探测之前把它设进
//! `VK_ICD_FILENAMES` / `VK_DRIVER_FILES`，让 `auto` 也能选到 GPU。
//!
//! 只在 **glibc 构建**（`target_os = "linux"`）上生效：Termux 原生（bionic）构建的
//! `target_os = "android"`，glibc 驱动对它没有意义。用户显式设了这两个变量时不覆盖。

use std::path::{Path, PathBuf};

use unlucky_core::platform::paths::ProcessEnv;
use unlucky_core::platform::termux;

/// 随附 ICD 的相对路径（相对 `$PREFIX`）。
const ICD_REL: &str = "glibc/share/vulkan/icd.d/ull_freedreno.json";

/// 在 GPU 探测之前调用：装了就启用，没装就什么都不做。
pub(super) fn enable() {
    if !cfg!(target_os = "linux") {
        return;
    }
    if std::env::var_os("VK_ICD_FILENAMES").is_some()
        || std::env::var_os("VK_DRIVER_FILES").is_some()
    {
        return;
    }
    let Some(prefix) = termux::prefix(&ProcessEnv) else {
        return;
    };
    let icd = icd_path(&prefix);
    if icd.is_file() {
        std::env::set_var("VK_ICD_FILENAMES", &icd);
        std::env::set_var("VK_DRIVER_FILES", &icd);
    }
}

/// `$PREFIX` 下随附 ICD 的路径（纯函数，测试用）。
fn icd_path(prefix: &Path) -> PathBuf {
    prefix.join(ICD_REL)
}

#[cfg(test)]
mod tests {
    use super::icd_path;

    #[test]
    fn icd_path_is_under_the_glibc_prefix() {
        let path = icd_path(std::path::Path::new("/data/data/com.termux/files/usr"));
        assert_eq!(
            path,
            std::path::PathBuf::from(
                "/data/data/com.termux/files/usr/glibc/share/vulkan/icd.d/ull_freedreno.json"
            )
        );
    }
}
