//! 整合包**导出**（M112）：实例目录 → `.mrpack`（优先）/ CurseForge `manifest.json`（次之）。
//!
//! 设计依据：`DevDocs/PlanDoc/08-mod-and-recommend.md` §2（D10）、
//! `DevDocs/SubAgentsTalk/addon-整合包导入导出对接要点.md` §4（导出八条），格式模型与安全闸
//! 是 M110 的 [`crate::addon::pack`]，本模块**只消费，不重写**。
//!
//! # 一句话口径：不认识的 mod 不能丢、不能编
//!
//! 用户原话（M112 任务书）：「要么能证明它可被远程替代，要么就必须内联进 `overrides/` 并在
//! 结果里逐条报告」。落到代码就是三分类，**没有第四类**：
//!
//! | 分类 | 判据 | 产物 |
//! |------|------|------|
//! | 远程引用（[`FileClass::Remote`]） | 目标格式**能表达**它 + 某个目录（[`RemoteCatalog`]）**能证明**它的身份 | 清单 `files[]` 一条 |
//! | 内联（[`FileClass::Inline`]） | 证明不了，或证明了但目标格式表达不了 | 进 `overrides/`，原字节照搬 |
//! | 跳过（[`FileClass::Skipped`]） | 读不了 / 是符号链接 / 路径不安全 | 不写进包，**逐条**进报告 |
//!
//! 「能证明」用两条独立查询（契约 §4.4 的顺序）：**Modrinth SHA-1** →
//! [`RemoteCatalog::modrinth_by_sha1`]；**CurseForge 指纹**（[`crate::addon::pack::curseforge::fingerprint`]）
//! → [`RemoteCatalog::curseforge_by_fingerprint`]。两条都命中时两条直链都进 `downloads[]`
//! （HMCL 同款，兼容面更大）。**没有网络时**用 [`OfflineCatalog`]：所有文件都内联，包照样完整可导入
//! ——这不是降级失败，是"证明不了就内联"的正面行为。
//!
//! # 每条格式决策落在哪（tower 定案，别按任务书原话改回去）
//!
//! 1. **`.disabled` 走路径后缀**（`mods/x.jar.disabled`），不是 `env.client=optional`：后者只是
//!    "安装时可取消勾选"，装回来默认仍然启用 ⇒ 跨启动器不保真。官方 Modrinth 导出也是后缀。
//! 2. **覆盖目录一律 `overrides/`**（不用 HMCL 那套 `client-overrides/`）：Modrinth 官方与
//!    Prism 都用它，兼容面最大。分侧语义在本轮导出里不产生（实例目录本身不分侧）。
//! 3. **`game` + `dependencies` 从实例快照取**，绝不只写 `minecraft` 漏加载器：快照来自
//!    `versions/<id>/<id>.json` 的继承链 + 库坐标（[`snapshot`]），**不解析**
//!    `<mc>-<loader>-<ver>` 这类实例 id 字符串（社区实例名不统一，那是猜）。
//! 4. **CF `files[]` 只写能确定 `projectID`/`fileID` 的项**：CF 没有"内联到 files[]"的概念，
//!    证明不了的（以及**禁用文件**：CF 根本没有禁用表达）一律进 `overrides/` 并进报告。
//! 5. **尺寸护栏**：`fileSize` 是 u32（超了报 `pack_limits`，不静默截断）、整包撞 ZIP64 阈值由
//!    `format::write` 拒绝（`pack_limits`，不留半个包）。
//!
//! # 刻意不做的事（分工边界，别以为是漏做）
//!
//! - **MultiMC / Prism 导出**：不做（P1 只做导入）。它的包是"整实例快照"，要先把所有远程文件
//!   下载下来再打包，成本极高；导入侧 M111 已覆盖。**这是有意省略。**
//! - **不装加载器 / 不补全清单**：加载器元数据解析与库下载是 `addon::loader` + `game::install`；
//!   CF 的 `projectID/fileID → 文件名/URL` 补全需要 CF 客户端（D10 §5 规划中），本轮由调用方注入
//!   [`RemoteCatalog`]，本模块不发网络请求。
//! - **不碰 UI 类型**：core 零 UI 依赖（`core-no-ui` 门禁），报告只出现 `String` / `&'static str`。
//! - **不改实例**：导出只读实例目录。
//!
//! # 没验的部分（报告里同样登记）
//!
//! 真实 Modrinth / CurseForge API 查询（要网络与 key，`RemoteCatalog` 目前只有离线实现）、
//! 超大实例（GB 级、上千文件——指纹要对每个文件多读两遍）、Windows 之外的平台、
//! 以及「M111 合并后的 `import_pack → export → import_pack` 真端到端」。

pub mod catalog;
pub(crate) mod classify;
pub mod pipeline;
pub mod report;
pub mod scan;
pub mod snapshot;

pub use catalog::{OfflineCatalog, RemoteCatalog, RemoteHit, RemoteSource};
pub use pipeline::{export_instance, ExportRequest};
pub use report::{ExportFile, ExportReport, FileClass, FileReason};
pub use scan::{scan_instance, LocalFile, SkippedLocal, DEFAULT_CONTENT_ROOTS, OVERRIDES_DIR};
pub use snapshot::{read_instance_snapshot, InstanceSnapshot, SnapshotLoader};
