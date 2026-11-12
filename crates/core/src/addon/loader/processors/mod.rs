//! Installer Processors 执行器（M35）：把 install profile 的 `processors[]` 跑起来。
//!
//! 设计输入：`DevDocs/analyse/05-modloader-modpack.md` §1.2（Forge 新式 installer 的
//! processor 流水线）与 HMCL `ForgeNewInstallTask` / `ProcessorTask` 的实现。
//!
//! # 覆盖范围
//!
//! | 版本段 | 有无客户端 processor | 本模块 |
//! |--------|----------------------|--------|
//! | Forge 1.13 ~ 1.20.x（spec 1） | 有（拆包/映射/打补丁） | ✅ 真跑 |
//! | NeoForge 1.20.5+（26.x 只剩 1 个 `PROCESS_MINECRAFT_JAR`） | 有 | ✅ 真跑 |
//! | Forge spec 0（1.12.2 重建版）/ Fabric / Quilt | 无 | 不需要（走 M32 的 json 路线） |
//!
//! # 流水线
//!
//! ```text
//! plan    install profile 的 data 表 → 变量表（{MINECRAFT_JAR}/{SIDE}/[坐标]/'字面量'）
//!         → 每步 java -cp 参数 + 产物/摘要 + processor 依赖库待下清单
//! download 依赖库（走 M27 引擎：镜像/健康度/退避/校验）
//! exec    串行外调 java（主类取自 jar 的 MANIFEST）→ 产物存在性（硬）+ 摘要（软）
//! ```
//!
//! # 三条边界
//!
//! 1. **不写版本 JSON**：那是 M27 `execute` 的事，且必须等 processors 全部成功
//!    （版本清单存在 = 这一版真的装完了，半成品不许留）；
//! 2. **不改共享层**：原版 client jar 先复制到临时目录再交给 processor，
//!    processor 只会改临时副本；
//! 3. **不猜**：`data` 表里缺变量、processor 缺 `jar`、产物没落盘，一律明确报错，
//!    不拿半截参数去试（那种失败的 stderr 更难查）。
//!
//! # 幂等
//!
//! 两级：①每步按 `outputs` 的 SHA-1 命中整步跳过（重跑零成本、不碰文件系统）；
//! ②调用方以「版本 JSON 已落盘」为粗粒度判据整段跳过。

pub mod download;
pub mod error;
pub mod exec;
pub mod jar;
pub mod plan;
pub mod prepare;
pub mod spec;
pub mod text;
pub mod vars;

#[cfg(test)]
mod tests;

pub use download::download_libraries;
pub use error::{ProcessorError, ProcessorErrorKind};
pub use exec::{run, select_java, ProcessorRun, StepOutcome, StepSink, StepStatus};
pub use plan::{ProcessorPlan, ProcessorPlanInput, ProcessorStep, StepOutput};
pub use spec::{Datum, Processor};
pub use text::{host_redactor, tail};
pub use vars::{EmbeddedFile, Variables, VariablesInput, MINECRAFT_JAR_NAME};
