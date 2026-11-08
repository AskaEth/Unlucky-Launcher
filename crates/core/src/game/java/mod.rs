//! Java 运行时管理（D13 v1 → D23 v2）：托管 JDK 的查询 / 下载 / 解压 / 探测，以及 MC↔Java 映射。
//!
//! ```text
//! target    宿主平台 → Adoptium 的 os/arch（Win=zip、Linux/mac=tar.gz）
//! vendor    四家发行版与用户可调的优先级链（国内 Temurin→Oracle→MS / 国外 Temurin→MS→Oracle）
//! api       Temurin：api.adoptium.net 查询最新 GA → (github 直链, sha256, size)
//! oracle    Oracle OpenJDK：镜像目录页解析（hash 段不可推导，禁止拼 URL）+ .sha256
//! microsoft MS OpenJDK：aka.ms 模板 + 内置四段版本快照 + .sha256sum.txt
//! zulu      Azul Zulu：两跳 JSON API（列表 → 详情）+ CDN 直链（第四备选）
//! source    发行版分派 + 回退链记录（命中与降级原因进事件流）
//! location  落点三模式（Portable / Standard / Custom）+ 孤儿目录清理
//! release   一次版本查询的统一产物（vendor 原生版本串 + 归档 + sha256 + 大小）
//! net       版本查询用的极小 GET 文本工具（下载不走这里）
//! install   编排：回退链 → 下载引擎（镜像/健康度/退避）→ 解压 → 探测验证
//! extract   zip / tar.gz 解压 + 顶层目录剥离 + 幂等戳记 + zip-slip 防护
//! probe     java -version 探测、托管目录扫描（两种布局都认）
//! discover  标准安装位置扫描（Program Files 系 / 官方启动器 runtime / 注册表 / /usr/lib/jvm）+ 去重
//! select    启动选 Java 的**纯**优先链（满足要求优先，再按来源）
//! mapping   MC↔Java 静态表（HMCL `GameJavaVersion`）+ 版本 JSON 优先 + 不符检查
//! ```
//!
//! # 三条边界
//!
//! 1. **不碰网络细节**：包下载一律交 `core::download` 的引擎，本模块不写重试循环
//!    （`MirrorTable::expand` 负责 Temurin 的镜像展开；Oracle 的目录解析在 `oracle`）；
//! 2. **不弹窗**：不符检查只产出数据（[`JavaMismatch`]），提示交给 CLI 的告警行与 GUI；
//! 3. **不改启动策略**：「用哪个 Java 启动」是启动期的派生结果，不落盘；落盘的是**装到哪**
//!    （[`JavaInstallRoot`]，由 `java.installRoot` + 数据目录模式决定）。
//!
//! # 托管布局（D23 后两种并存）
//!
//! ```text
//! Portable：data/java-managed/
//! ├── .cache/OpenJDK21U-jdk_x64_windows_hotspot_21.0.13_9.zip   归档缓存（可删）
//! └── jdk-21-temurin/                                          一个 major+vendor 一份（永不改）
//!     ├── .ull-jdk.json                                        戳记（幂等判据）
//!     └── bin/java[.exe]
//!
//! Standard / Custom：%APPDATA%\Java\<vendor>\<version>\     （mac/Linux 见 location 模块）
//! ```

pub mod api;
pub mod discover;
pub mod error;
pub mod extract;
pub mod install;
pub mod location;
pub mod mapping;
pub mod microsoft;
pub mod net;
pub mod oracle;
pub mod probe;
pub mod release;
pub mod select;
pub mod source;
pub mod target;
pub mod termux;
pub mod vendor;
pub mod zulu;

pub use self::api::{
    latest_url, parse_latest, JdkArchive, JdkRelease, ADOPTIUM_API_BASE, JDK_VENDOR,
};
pub use self::discover::{
    candidates, candidates_in_roots, scan_java_binaries, standard_candidates, standard_roots,
};
pub use self::error::{JavaError, JavaErrorKind};
pub use self::extract::{
    extract_archive, is_ready, java_binary, java_binary_name, read_stamp, ExtractReport, JdkStamp,
    STAMP_FILE,
};
pub use self::install::{
    archive_path, archive_request, chain_order, install, managed_root_dir, plan, InstallRequest,
    InstalledJdk,
};
pub use self::location::{
    cleanup_orphans, sanitize_version, scan_roots, CleanupReport, JavaInstallRoot, LocationMode,
};
pub use self::mapping::{
    check, recommended_major, requirement_for, requirement_for_version, JavaMismatch,
    JavaRequirement, RequirementSource,
};
pub use self::probe::{
    discover_managed, discover_managed_roots, managed_dir_name, parse_managed_dir, probe,
    resolve_on_path, system_candidates, JavaOrigin, JavaRuntime, DEFAULT_MAJOR,
};
pub use self::release::parse_sha256_text;
pub use self::select::{pick, resolve, JavaResolution};
pub use self::source::{ChainLog, VendorAttempt};
pub use self::target::AdoptiumTarget;
pub use self::vendor::{order as vendor_order, JdkVendor, VENDOR_IDS};
