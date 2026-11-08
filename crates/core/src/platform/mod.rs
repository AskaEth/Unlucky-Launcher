//! OS 抽象层：路径规则、平台目录差异、进程与文件锁、软渲染/显示能力探测辅助。
//!
//! 设计定稿：`DevDocs/PlanDoc/04-data-and-config.md` §1–§2（三模式路径与 `.ULL` 四分区）。
//! Win7 单独适配（决策 D11）在此收敛平台差异判断，避免散落各模块打补丁。
//!
//! 公共面（M12 已落地）：
//! - [`paths`]：宿主平台族与标准目录探测（环境变量可注入），可搬运路径字符串；
//! - [`layout`]：`.ULL/` 四分区（config/data/cache/logs）与各已知路径的唯一入口；
//! - [`data_dir`]：三模式数据目录解析（Portable 默认 / Standard / Custom + 高级覆盖）；
//! - [`redirect`]：迁移后留在旧目录的 `redirect.json` 指针（防便携盘旧目录被再次初始化）；
//! - [`receipt`]：迁移收据（源指纹防重放）；
//! - [`dir_migration`]：数据目录迁移（分区逐个搬 + 失败逆序回滚）；
//! - [`atomic`]：原子写（tmp → fsync → rename）；[`hash`]：迁移收据用的 SHA-256。
//!
//! 运行期系统探测（M20 / D19 · `11-os-detection.md`）：
//! - [`os_info`]：`OsInfo` 快照 + 最低支持判定（One / Unsupported，基线 **Windows 10+ /
//!   macOS 12+ / Linux 达标**）+ 启动自检；
//! - [`windows_version`]：真实 `major.minor.build`（注册表 `CurrentBuildNumber` 快路径 →
//!   整键 `CurrentVersion` 家族 → `cmd /c ver` 兜底）；
//! - [`macos_version`]：`sw_vers` 产品版本 + `sysctl hw.optional.arm64` 运行架构；
//! - [`linux_caps`]：发行版（仅显示）+ 内核 release（喂 `os.version`）+ 会话/GL 能力探测；
//! - [`vk_probe`]：Vulkan 能力的安全层探测（loader 文件 / ICD 注册 / 环境变量，无 `unsafe`）；
//! - [`theme_probe`]：系统主题（浅色/深色）探测，供主题三态的 `auto` 初始化对齐与兜底跟随。
//! - [`wallpaper`]：桌面壁纸**路径探测**（D38 配色三级优先里第②级的输入）。本模块只做路径
//!   探测——不解码图片、不取色、不生成调色板，莫奈取色与三级降级在 `crates/design` 的
//!   后续 mission。
//!
//! 尚未落地（后续 mission）：文件锁、进程工具、真实 GL/Vulkan 能力探测（钩子见
//! [`linux_caps::probe_gl`] 与 [`vk_probe::probe_vk`]，FFI 受限，实际探测归 app 层渲染后端选择）。

pub mod atomic;
pub mod data_dir;
pub mod dir_migration;
pub mod error;
pub mod hash;
pub mod layout;
pub mod linux_caps;
pub mod macos_version;
pub mod os_info;
pub mod paths;
pub mod receipt;
pub mod redirect;
pub mod termux;
pub mod theme_probe;
pub mod vk_probe;
pub mod wallpaper;
pub mod windows_version;
