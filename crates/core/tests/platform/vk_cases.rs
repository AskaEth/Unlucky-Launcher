//! Vulkan 能力探测（`platform::vk_probe`）的三平台注入样本。
//!
//! 全部用 [`VkEvidence`] 注入，不读真机、不起子进程：
//! 三平台解析函数是纯函数且跨平台编译，任意宿主都能跑这几个用例。
//!
//! 关键语义（`research/05-mc-vulkan.md` §4.3 的真机证据）：
//! **Windows 注册表 `HKLM\SOFTWARE\Khronos\Vulkan\Drivers` 缺失不等于没有 Vulkan**
//! （实测机 Vulkan 1.3.217 可用但该键不存在），所以只能降级到 `LoaderOnly`，不能判 `Missing`。

use std::path::PathBuf;

use crate::prelude::*;

#[test]
fn windows_loader_without_registry_key_is_only_loader_only() {
    // 真机证据的复刻：vulkan-1.dll 在，注册表键缺失 → 只能说「loader 在、驱动未确认」。
    let probe = classify(VkEvidence::new(OsFamily::Windows, true, false));
    assert_eq!(probe, VkProbe::LoaderOnly);
    assert!(probe.has_loader());
    assert!(!probe.is_missing(), "注册表缺失被误判成没有 Vulkan");
    assert!(!probe.is_available(), "没有驱动信号不该承诺可用");
}

#[test]
fn windows_loader_plus_registry_entry_is_available() {
    assert_eq!(
        classify(VkEvidence::new(OsFamily::Windows, true, true)),
        VkProbe::Available
    );
}

#[test]
fn windows_without_loader_is_missing() {
    assert_eq!(
        classify(VkEvidence::new(OsFamily::Windows, false, true)),
        VkProbe::Missing
    );
}

#[test]
fn linux_loader_with_icd_manifests_is_available() {
    assert_eq!(
        classify(VkEvidence::new(OsFamily::Linux, true, true)),
        VkProbe::Available
    );
    // 只有 loader、没有 ICD 目录/环境变量：同样不否定（驱动可能由 loader 自寻）。
    assert_eq!(
        classify(VkEvidence::new(OsFamily::Linux, true, false)),
        VkProbe::LoaderOnly
    );
    assert_eq!(
        classify(VkEvidence::new(OsFamily::Linux, false, false)),
        VkProbe::Missing
    );
}

#[test]
fn macos_molten_vk_needs_no_separate_icd_signal() {
    // macOS 没有系统 Vulkan：MoltenVK 既是 loader 又是 ICD，没有 LoaderOnly 这一档。
    assert_eq!(
        classify(VkEvidence::new(OsFamily::MacOs, true, false)),
        VkProbe::Available
    );
    assert_eq!(
        classify(VkEvidence::new(OsFamily::MacOs, false, true)),
        VkProbe::Missing
    );
}

#[test]
fn probe_names_are_stable_and_lower_snake() {
    assert_eq!(VkProbe::Unknown.as_str(), "unknown");
    assert_eq!(VkProbe::LoaderOnly.as_str(), "loader_only");
    assert_eq!(VkProbe::Available.as_str(), "available");
    assert_eq!(VkProbe::Missing.as_str(), "missing");
    assert_eq!(VkProbe::default(), VkProbe::Unknown);
    assert!(!VkProbe::Unknown.has_loader());
}

#[test]
fn loader_candidates_cover_each_platform_signature() {
    let windows = loader_candidates(OsFamily::Windows);
    assert!(
        windows
            .iter()
            .any(|path| path.ends_with("System32/vulkan-1.dll")
                || path.ends_with(r"System32\vulkan-1.dll")),
        "Windows 判据应是 System32\\vulkan-1.dll：{windows:?}"
    );

    let linux = loader_candidates(OsFamily::Linux);
    assert!(
        linux
            .iter()
            .any(|path| path.to_string_lossy().contains("libvulkan.so.1")),
        "Linux 判据应含 libvulkan.so.1：{linux:?}"
    );

    let macos = loader_candidates(OsFamily::MacOs);
    let macos_names = macos
        .iter()
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(
        macos_names.iter().any(|name| name == "libvulkan.1.dylib"),
        "macOS 判据应含 libvulkan.1.dylib：{macos_names:?}"
    );
    assert!(
        macos_names.iter().any(|name| name == "libMoltenVK.dylib"),
        "macOS 判据应含 MoltenVK：{macos_names:?}"
    );
}

#[test]
fn registry_query_output_is_parsed_for_driver_entries() {
    // 键不存在：只有本地化错误行（中英各一例，真机输出）。
    let missing_en =
        "ERROR: The system was unable to find the specified registry key or value.\r\n";
    let missing_zh = "\r\n错误: 系统找不到指定的注册表项或值。\r\n";
    assert!(!registry_has_driver_entry(missing_en));
    assert!(!registry_has_driver_entry(missing_zh));
    assert!(!registry_has_driver_entry(""));
}

#[test]
fn registry_query_real_format_is_recognised() {
    // 真实 `reg query KEY`（不带 /s）格式，本机实测：
    //   空行 → 键头行（顶格、完整 hive 名、无尾反斜杠）→ 缩进的值行
    //   `    <值名>    REG_DWORD    0x0`
    // Vulkan 的 ICD 注册是「值名 = manifest 绝对路径」，名字里可能有空格，所以列宽不定。
    let present = "\r\n\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n    \
C:\\Windows\\System32\\DriverStore\\FileRepository\\amdvlk64.inf_amd64_1b2c\\amdvlk64.json    REG_DWORD    0x0\r\n    \
C:\\vendor a\\vk_vendor_a.json    REG_DWORD    0x0\r\n";
    assert!(registry_has_driver_entry(present));

    // 值可能是 REG_SZ（第三方 loader/层注册的写法）。
    let reg_sz = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n    \
C:\\vendor\\vk.json    REG_SZ    \r\n";
    assert!(registry_has_driver_entry(reg_sz));
}

#[test]
fn registry_key_without_values_is_not_a_driver_signal() {
    // 键存在但没有任何值项：只有键头行。
    let header_only = "\r\nHKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n";
    assert!(!registry_has_driver_entry(header_only));

    // 键存在但只有子键（无 /s 时子键也缩进打印，且不带 REG_ 类型标记）。
    let subkeys_only = "\r\n\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\\Drivers\r\n    \
LayerOne\r\n    LayerTwo\r\n";
    assert!(!registry_has_driver_entry(subkeys_only));

    // 键头是**别的**键时，其值行不能算数（防止把上层/邻居键的值当成 ICD 注册）。
    let other_key = "\r\n\
HKEY_LOCAL_MACHINE\\SOFTWARE\\Khronos\\Vulkan\r\n    \
ImplicitLayers    REG_DWORD    0x0\r\n";
    assert!(!registry_has_driver_entry(other_key));
}

#[test]
fn ldconfig_output_is_parsed_for_the_loader() {
    let with_loader =
        "\tlibvulkan.so.1 (libc6,x86-64) => /usr/lib/x86_64-linux-gnu/libvulkan.so.1\n\
\tlibvulkan.so (libc6,x86-64) => /usr/lib/x86_64-linux-gnu/libvulkan.so\n";
    assert!(ldconfig_has_libvulkan(with_loader));
    assert!(!ldconfig_has_libvulkan(
        "\tlibGL.so.1 (libc6,x86-64) => /usr/lib/libGL.so.1\n"
    ));
}

#[test]
fn icd_env_prefers_the_legacy_variable_and_skips_blanks() {
    let vars = |pairs: &'static [(&'static str, &'static str)]| {
        env_icd_path(move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_string())
        })
    };
    assert_eq!(
        vars(&[("VK_ICD_FILENAMES", "/a/nvidia_icd.json")]).as_deref(),
        Some("/a/nvidia_icd.json")
    );
    assert_eq!(
        vars(&[("VK_DRIVER_FILES", "/b/intel_icd.json")]).as_deref(),
        Some("/b/intel_icd.json")
    );
    // 旧名优先（两者都设时 Khronos loader 也是旧名优先）。
    assert_eq!(
        vars(&[
            ("VK_ICD_FILENAMES", "/a/one.json"),
            ("VK_DRIVER_FILES", "/b/two.json"),
        ])
        .as_deref(),
        Some("/a/one.json")
    );
    // 空白值当作没设。
    assert_eq!(vars(&[("VK_ICD_FILENAMES", "   ")]), None);
    assert_eq!(vars(&[]), None);
    assert_eq!(ICD_ENV_VARS.len(), 2);
    assert_eq!(ICD_DIRS.len(), 3);
    assert!(VULKAN_DRIVERS_KEY.contains("Khronos"));
}

#[test]
fn icd_manifest_count_ignores_non_json_and_missing_dirs() {
    let dir = temp_dir("vk-icd");
    assert_eq!(count_icd_manifests(&dir), 0);
    std::fs::write(dir.join("nvidia_icd.json"), "{}").expect("写 json");
    std::fs::write(dir.join("intel_icd.json"), "{}").expect("写 json");
    std::fs::write(dir.join("README"), "x").expect("写非 json");
    assert_eq!(count_icd_manifests(&dir), 2);
    assert_eq!(count_icd_manifests(&dir.join("nope")), 0);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn probe_vk_on_this_host_is_not_unknown() {
    // 真探一次：只断言「给了确定结论」（本机有没有 Vulkan 都合法，不许恒 Unknown）。
    assert_ne!(probe_vk(), VkProbe::Unknown);
}

/// 唯一临时目录（不引 tempfile 依赖）。
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let index = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("ull-vk-{tag}-{}-{index}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录");
    dir
}
