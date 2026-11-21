//! M221 core 侧集成测试入口：从**别人的** `.minecraft` 只读登记版本（adopt）。
//!
//! 分工：
//! - `fixture`：临时真实磁盘目录 + 一份「别的启动器铺好的」`.minecraft`；
//! - `snapshot`：**全节点**快照（目录/文件/链接 + mtime），core 与 CLI 测试共用这一份；
//! - `scan_cases`：`scan` / `inspect` 的判据（正常 / 坏 JSON / 缺父 / 成环 / 缺清单 / 越界符号链接）；
//! - `plan_cases`：还没登记时的**只读**判据与拒绝理由（dry-run 零写盘 / 缺父 / 不存在 / 非法目标）；
//! - `collision_cases`：缺省运行目录**不许被两个实例共用**（slug / 大小写 / 已被占 / 认领但未创建）；
//! - `register_cases`：`adopt` 的落点与写盘面（缺省独立目录 / shared / isolated / custom /
//!   重名 / 重复 / 非空目录 / 数据区联接 / **slug 与大小写碰撞不许共用运行目录** / 外部树全节点不变）；
//! - `surface_cases`：配置写面被链接走出去必须拒绝（R2）、`versions` 入口断链必须报错（R3）；
//! - `fixture_cases`：快照工具自身的回归（它得抓得住「凭空多一个空目录」，R5）。
//!
//! 这些用例**不启动真实 Minecraft**：验的是「登记这一步做了什么、没做什么」，启动链路由
//! `tests/game/*` 与 CLI 的 `launch --dry-run` 各自覆盖。

mod collision_cases;
mod fixture;
mod fixture_cases;
mod plan_cases;
mod register_cases;
mod scan_cases;
mod snapshot;
mod surface_cases;
