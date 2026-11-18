//! 整合包的**格式无关模型**与**格式扩展点**（M110）。导入/导出管线（M111/M112）建在这上面。
//!
//! 设计定稿：`DevDocs/PlanDoc/08-mod-and-recommend.md` §2（D10），格式细节来自
//! `DevDocs/analyse/05-modloader-modpack-p2.md` §2.1–§2.3（HMCL 源码级拆解，A 级来源）。
//!
//! # 本模块只做三件事
//!
//! | 层 | 模块 | 职责 |
//! |----|------|------|
//! | 安全读取 | [`archive`] | 目录与 zip 统一成"规整过的条目表"：路径穿越双检、符号链接拒绝、条数/体积上限 |
//! | 格式 | [`format`] | [`format::PackFormat`] trait + [`format::PackRegistry`]：探测、读、渲染、写，**优先级写死** |
//! | 模型 | [`manifest`] | [`manifest::PackManifest`]：包名/版本/作者/游戏版本/加载器/文件条目/覆盖目录，未知字段原样保留 |
//!
//! 三个内置格式：[`curseforge`]（`manifest.json`，可读可写）、[`modrinth`]（`.mrpack`，可读可写）、
//! [`multimc`]（`mmc-pack.json`，**只读**）。
//!
//! # 导出面的边界（r1 之后写死）
//!
//! 默认 [`format::PackFormat::write`] **只保证同格式往返**：覆盖目录按模型原样搬，
//! 不做目录映射 / `side` 分派 / 加载器 id 规范化 / `extra` 分桶（都属于 M112）。
//! 表达不了的时候由 [`format::PackFormat::check_overrides`] 直接报 `pack_unsupported`，
//! 而不是产出一个"成功但内容为空"的包。
//!
//! # 刻意不做的事（分工边界）
//!
//! - **不发网络请求**：CF 的 `projectID/fileID` → 文件名/URL 补全属于 M111（要用 CF API 与
//!   MurmurHash2 指纹），本模块只把"缺什么"如实记成 `None`；
//! - **不落盘解压**：`archive` 只读，不写实例目录。overrides 的三方合并（base/theirs/ours）
//!   是 M111 的解压器职责，本模块提供它要的输入：每条覆盖文件的实例内相对路径 + SHA-1；
//! - **不碰 UI 类型**：core 零 UI 依赖（`core-no-ui` 门禁），这里连 `String` 之外的表现层
//!   类型都不出现；
//! - **不猜格式**：探测顺序写死在 [`format::PackRegistry::builtin`]，同一份包命中多格式时
//!   用固定的注册顺序选，并把其余命中记进 [`format::PackMatch::also_matched`]。
//!
//! # 安全（每条都有恶意样本测试，见 `tests/safety.rs`）
//!
//! 路径穿越判据是**两条独立检查**：①本模块的 [`path::normalize_file`]（七条写死的规则：
//! 空名/`\0`/`\`/`:`/前导 `/`/任何 `..` 段/规整后为空）；②`zip` crate 的
//! `enclosed_name()`（按 Windows 规则解析盘符、NTFS 数据流、保留设备名）。
//! 为什么不能只留一条：`enclosed_name()` 允许"深度 0 的根目录"，也就是 `"/etc/passwd"`
//! 它会放行；反过来，本模块的规则不认 Windows 保留设备名。**两条都过才算安全**。
//!
//! # 测试夹具的来源（不许把"看起来像"当真实）
//!
//! `tests/fixtures.rs` 里的 CF / mrpack / MultiMC 三个包**全部是手造的**：字段取自
//! analyse/05 记录的 HMCL 解析器形状（A 级来源），内容是为了覆盖边界而编的（含一个刻意的
//! 损坏文件：截断的 `config/broken.toml`、半截 `mods/half.jar`、零字节 `mods/broken.jar`）。
//! **没有一个来自真实导出**——本任务环境离线，没有真实整合包样本可对照。真实样本的字段
//! 偏移风险因此只能由 M111/M115（格式字段盘点）在拿到真包后回收，报告里已登记这条欠账。

pub mod archive;
pub mod curseforge;
pub mod error;
pub mod format;
pub mod manifest;
pub mod modrinth;
pub mod multimc;
pub mod path;

#[cfg(test)]
mod tests;

pub use archive::{
    PackArchive, PackEntryInfo, PackLimits, ReadBudget, PACK_MAX_ENTRIES, PACK_MAX_ENTRY_BYTES,
    PACK_MAX_UNCOMPRESSED_BYTES, PACK_ZIP64_BYTES_THR,
};
pub use curseforge::CurseForge;
pub use error::{PackError, PackErrorKind, PackResult};
pub use format::{PackFormat, PackMatch, PackPayload, PackRegistry};
pub use manifest::{
    EnvSupport, OverrideFile, PackFile, PackLoader, PackManifest, PackOverrides, PackSide,
};
pub use modrinth::Modrinth;
pub use multimc::MultiMc;
