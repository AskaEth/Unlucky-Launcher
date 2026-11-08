//! `update` 的单元测试。
//!
//! 覆盖面（与 mission 的验收条目一一对应）：
//!
//! * 版本比较边界：`0.9` vs `0.10`、预发布 vs 正式、alpha/beta/rc、构建元数据不排序；
//! * 清单校验：重复版本、零长度、缺摘要/摘要格式错、未知平台与格式；
//! * 渠道准入：阶梯比较 + 预发布阶段表；
//! * 校验失败：摘要不符、包体截断（短了/多了）、失败后暂存文件不残留；
//! * 替换与回滚：成功后备份仍在、搬一半失败回滚、目标目录不可写、Windows 占用；
//! * 一条龙（`job`）：只有便携单文件 exe 能原地自替换（msi/zip 在取包之前就被拒）、
//!   「没有可装更新」不碰目标、`ULL_UPDATE_DIR` 只在调试构建里生效；
//! * 更新源：缺清单不是错误、渠道不符报错、远端地址拒绝；
//! * GitHub 源：固定清单地址、产物 URL 解析、非 stable 无更新、与 `DirSource` 对拍。
//!
//! 全程离线：所有字节都在临时目录或 127.0.0.1 的假 HTTP 服务上造，没有一个用例发公网请求。

mod channel_cases;
mod checker_cases;
mod github_cases;
mod install_cases;
mod job_cases;
mod manifest_cases;
mod source_cases;
mod support;
mod target_cases;
mod verify_cases;
mod version_cases;
