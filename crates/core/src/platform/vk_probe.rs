//! Vulkan 能力探测的**安全层**（无 `unsafe`，无 FFI）：只报可观测事实，不下「能不能玩」的结论。
//!
//! 依据 `DevDocs/PlanDoc/research/05-mc-vulkan.md`（M26）§2.3 与 §4.3：
//! MC 的 Vulkan 后端在 Windows/Linux 上**不打包** `vulkan-1.dll` / `libvulkan.so.1`，
//! 而是运行期去加载系统的 loader（natives 只给 macOS 发 MoltenVK）。所以「版本支持 Vulkan」
//! 与「这台机器能跑 Vulkan」是两件事，前置探测不可省。
//!
//! 分工照抄 [`super::linux_caps::probe_gl`]：core 只做「文件存在性 / 注册表 / 环境变量 /
//! 标准目录」这一层，真探测（建 instance、枚举物理设备、查 `VK 1.2` +
//! `VK_KHR_dynamic_rendering` + `VK_KHR_push_descriptor` 三项硬门槛）归 app 层。
//!
//! # ⚠️ 注册表键缺失 ≠ 没有 Vulkan（真机证据）
//!
//! 实测机（Windows 11 + AMD 独显，Vulkan 1.3.217 可用）上
//! `HKLM\SOFTWARE\Khronos\Vulkan\Drivers` **压根不存在**——ICD 由 loader 自己按
//! `%SystemRoot%\System32` 与驱动安装目录自寻。因此该键只作**加分项**（[`VkProbe::Available`]），
//! 缺失时降级为 [`VkProbe::LoaderOnly`]、**绝不**判 [`VkProbe::Missing`]。
//!
//! 解析函数是纯函数且跨平台编译，单测用注入样本覆盖三平台（见 `tests/platform/vk_cases.rs`）。

use std::path::{Path, PathBuf};

use crate::platform::os_info::OsFamily;

/// Windows 上的 Vulkan loader 文件名（系统目录内）。
pub const WINDOWS_LOADER: &str = "vulkan-1.dll";

/// `HKLM\SOFTWARE\Khronos\Vulkan\Drivers`：显式 ICD 注册键（**只作加分**，见模块文档）。
///
/// ICD 注册为这个键下的**值**（值名 = manifest 绝对路径，数据是 `dword:00000000`），
/// 不是子键（Vulkan-Loader `LoaderDriverInterface.md` 的 Windows 驱动发现一节）。
pub const VULKAN_DRIVERS_KEY: &str = r"HKLM\SOFTWARE\Khronos\Vulkan\Drivers";

/// 同一个键在 `reg query` 输出里的**键头行**形态。
///
/// `reg query` 即使传入缩写 hive 名（`HKLM`），输出也用完整 hive 名
/// （`HKEY_LOCAL_MACHINE`）且不带尾反斜杠——解析时必须按这个形态比对。
pub const VULKAN_DRIVERS_KEY_HEADER: &str = r"HKEY_LOCAL_MACHINE\SOFTWARE\Khronos\Vulkan\Drivers";

/// Khronos loader 认的 ICD 路径环境变量（旧名在前，新名在后）。
pub const ICD_ENV_VARS: &[&str] = &["VK_ICD_FILENAMES", "VK_DRIVER_FILES"];

/// Linux 的 ICD 清单目录（`*.json`，loader 启动时按此枚举驱动）。
pub const ICD_DIRS: &[&str] = &[
    "/usr/share/vulkan/icd.d",
    "/usr/local/share/vulkan/icd.d",
    "/etc/vulkan/icd.d",
];

/// Vulkan 能力（四档；`Unknown` 是派生默认值，「没探过」不等于「没有」）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VkProbe {
    /// 未探测（非探测路径构造的快照）。
    #[default]
    Unknown,
    /// loader 在，但没有看到任何 ICD 注册信号。
    ///
    /// **不是**「没有 Vulkan」：Windows 上注册表键缺失是常态（模块文档的真机证据），
    /// 设备是否存在要靠 app 层的真探测确认。
    LoaderOnly,
    /// loader + 驱动信号都在（或 macOS 上 MoltenVK 在场）——大概率可用。
    Available,
    /// 连 loader 都找不到：Vulkan 后端一定起不来。
    Missing,
}

impl VkProbe {
    /// 简短名（日志/摘要行用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::LoaderOnly => "loader_only",
            Self::Available => "available",
            Self::Missing => "missing",
        }
    }

    /// loader 是否在场（`Available` 与 `LoaderOnly` 都为真）。
    #[must_use]
    pub const fn has_loader(self) -> bool {
        matches!(self, Self::Available | Self::LoaderOnly)
    }

    /// 是否已能确定「这台机器跑不了 Vulkan」（只有 [`Self::Missing`]）。
    #[must_use]
    pub const fn is_missing(self) -> bool {
        matches!(self, Self::Missing)
    }

    /// 是否足够确定「可以用 Vulkan」（只有 [`Self::Available`]）。
    ///
    /// UI/建议路径用它可以保守一点：`LoaderOnly` 应当提示「先试一次再说」而不是承诺可用。
    #[must_use]
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

/// 平台无关的探测证据（单测直接注入，不读真机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VkEvidence {
    /// 系统家族（决定判定口径）。
    pub family: OsFamily,
    /// loader 文件是否找到（`vulkan-1.dll` / `libvulkan.so.1` / `libvulkan.1.dylib` / MoltenVK）。
    pub loader_present: bool,
    /// 是否看到驱动信号：Windows 注册表键、Linux 的 ICD 目录/环境变量、macOS 无此项。
    pub icd_signal: bool,
}

impl VkEvidence {
    /// 构造。
    #[must_use]
    pub const fn new(family: OsFamily, loader_present: bool, icd_signal: bool) -> Self {
        Self {
            family,
            loader_present,
            icd_signal,
        }
    }
}

/// 证据 → 结论（纯函数，三平台共用；macOS 单独口径见函数体注释）。
#[must_use]
pub fn classify(evidence: VkEvidence) -> VkProbe {
    if evidence.family == OsFamily::MacOs {
        // macOS 没有系统 Vulkan：LWJGL 自带的 MoltenVK 既是 loader 又是 ICD（Vulkan→Metal 翻译），
        // 所以「MoltenVK/libvulkan.1.dylib 在」本身就是可用证据，没有 LoaderOnly 这一档。
        return if evidence.loader_present {
            VkProbe::Available
        } else {
            VkProbe::Missing
        };
    }
    if !evidence.loader_present {
        return VkProbe::Missing;
    }
    if evidence.icd_signal {
        VkProbe::Available
    } else {
        // 注册表键缺失是常态，只降级不否定（模块文档的真机证据）。
        VkProbe::LoaderOnly
    }
}

/// 探测当前机器的 Vulkan 能力（一次性，结果由 [`crate::platform::os_info::current`] 缓存）。
#[must_use]
pub fn probe_vk() -> VkProbe {
    classify(probe_evidence())
}

/// 采集当前机器的探测证据（真读盘/注册表；单测请直接用 [`classify`]）。
#[must_use]
pub fn probe_evidence() -> VkEvidence {
    let family = OsFamily::current();
    VkEvidence::new(family, loader_present(family), icd_signal(family))
}

/// loader 候选绝对路径（按平台；`SystemRoot` 缺失时回落到 `C:\Windows`）。
#[must_use]
pub fn loader_candidates(family: OsFamily) -> Vec<PathBuf> {
    match family {
        OsFamily::Windows => {
            let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_owned());
            vec![Path::new(&root).join("System32").join(WINDOWS_LOADER)]
        }
        OsFamily::MacOs => [
            "/usr/local/lib/libvulkan.1.dylib",
            "/opt/homebrew/lib/libvulkan.1.dylib",
            "/usr/local/lib/libvulkan.dylib",
            "/opt/homebrew/lib/libvulkan.dylib",
            // MC 的 lwjgl-vulkan natives 里带的就是这个。
            "/usr/local/lib/libMoltenVK.dylib",
            "/opt/homebrew/lib/libMoltenVK.dylib",
        ]
        .iter()
        .map(PathBuf::from)
        .collect(),
        OsFamily::Linux => [
            "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
            "/usr/lib/aarch64-linux-gnu/libvulkan.so.1",
            "/usr/lib64/libvulkan.so.1",
            "/usr/lib/libvulkan.so.1",
            "/usr/local/lib/libvulkan.so.1",
            // 开发包只给不带版本号的名字（`libvulkan.so`）。
            "/usr/lib/x86_64-linux-gnu/libvulkan.so",
            "/usr/lib64/libvulkan.so",
            "/usr/local/lib/libvulkan.so",
        ]
        .iter()
        .map(PathBuf::from)
        .collect(),
    }
}

fn loader_present(family: OsFamily) -> bool {
    if loader_candidates(family).iter().any(|path| path.is_file()) {
        return true;
    }
    // 标准路径没命中时，Linux 用 `ldconfig -p` 兜底（发行版把 loader 放在别处时更权威）。
    #[cfg(target_os = "linux")]
    {
        ldconfig_has_libvulkan(&run("ldconfig", &["-p"]).unwrap_or_default())
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

fn icd_signal(family: OsFamily) -> bool {
    match family {
        OsFamily::Windows => {
            let text = run("reg", &["query", VULKAN_DRIVERS_KEY]).unwrap_or_default();
            registry_has_driver_entry(&text)
        }
        OsFamily::Linux => {
            env_icd_path(|key| std::env::var(key).ok()).is_some()
                || ICD_DIRS
                    .iter()
                    .any(|dir| count_icd_manifests(Path::new(dir)) > 0)
        }
        // macOS 的 ICD 就是 MoltenVK 本身，已在 loader 判据里。
        OsFamily::MacOs => false,
    }
}

/// 从环境变量取 ICD 清单路径（`VK_ICD_FILENAMES` 优先，其次 `VK_DRIVER_FILES`）。
#[must_use]
pub fn env_icd_path(get: impl Fn(&str) -> Option<String>) -> Option<String> {
    ICD_ENV_VARS.iter().find_map(|key| {
        get(key)
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    })
}

/// 目录下 `*.json` ICD 清单的条数（目录不存在返回 0）。
#[must_use]
pub fn count_icd_manifests(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
        })
        .count()
}

/// `reg query <键>`（**不带** `/s`）的输出里，该键是否存在**值项**。
///
/// 真实格式（本机实测，Win11 22H2）：一个空行 → 键头行（顶格、用完整 hive 名、无尾反斜杠）
/// → 缩进的值行 `    <值名>    REG_DWORD    0x0`。Vulkan 的 ICD 注册正是「**值名 = manifest
/// 绝对路径**」（`"C:\vendor\vk.json"=dword:00000000`，见 Vulkan-Loader
/// `LoaderDriverInterface.md` 的 Windows 驱动发现一节），不是子键。
///
/// 所以判据是「键头行 + 其后至少一条带 `REG_` 类型标记的缩进行」：键存在但没有任何值时
/// 只有键头行；子键在没有 `/s` 时也缩进打印，但**不带** `REG_` 标记，据此区分。键不存在时
/// 输出只有本地化错误行（`ERROR: The system was unable to find…` /
/// `错误: 系统找不到指定的注册表项或值。`），既无键头也无数值行。
///
/// 注：值名可能含空格（manifest 绝对路径），所以不能按列切分，只能按 token 找类型标记。
#[must_use]
pub fn registry_has_driver_entry(text: &str) -> bool {
    let mut in_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !line.starts_with([' ', '\t']) {
            // 顶格行 = 键头行（或错误行）；遇到别的键就重置状态。
            in_key = trimmed
                .trim_end_matches('\\')
                .eq_ignore_ascii_case(VULKAN_DRIVERS_KEY_HEADER);
            continue;
        }
        if in_key && trimmed.split_whitespace().any(|t| t.starts_with("REG_")) {
            return true;
        }
    }
    false
}

/// `ldconfig -p` 输出里是否列了 `libvulkan.so.1`。
#[must_use]
pub fn ldconfig_has_libvulkan(text: &str) -> bool {
    text.lines()
        .any(|line| line.contains("libvulkan.so.1") || line.contains("libvulkan.so "))
}

/// 跑一个只读命令取 stdout（`reg query` / `ldconfig -p`；失败返回 `None`）。
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = super::child::quiet(program).args(args).output().ok()?;
    if output.stdout.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}
