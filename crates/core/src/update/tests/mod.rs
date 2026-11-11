//! `update` 的单元测试。
//!
//! 覆盖面（与 mission 的验收条目一一对应）：
//!
//! * 版本比较边界：`0.9` vs `0.10`、预发布 vs 正式、alpha/beta/rc、构建元数据不排序；
//! * 清单校验：重复版本、零长度、缺摘要/摘要格式错、未知平台与格式；
//! * 渠道准入：阶梯比较 + 预发布阶段表；
//! * 校验失败：摘要不符、包体截断（短了/多了）、失败后暂存文件不残留；
//! * 替换与回滚：成功后备份仍在、搬一半失败回滚、目标目录不可写、Windows 占用、
//!   跨卷（暂存在 A 卷、目标在 B 卷）安装成功且字节完整、中途失败不留半截文件、
//!   同卷不多搬一个字节、旧备份只留最近一份（崩溃现场的唯一旧副本例外）、
//!   跨卷守卫「探不到第二块卷」时不许静默变绿（裁决用例见 `install_xdev_cases`）；
//! * 一条龙（`job`）：只有便携单文件 exe 能原地自替换（msi/zip 在取包之前就被拒）、
//!   「没有可装更新」不碰目标、测试通道是**编译期 opt-in**（官方发布构建里那段根本编不进来，
//!   带 feature 时还要 `ULL_UPDATE_DIR` + `ULL_UPDATE_TEST_SOURCE=1` 两档全给）；
//! * 更新源：缺清单不是错误、渠道不符报错、远端地址拒绝；
//! * GitHub 源：固定清单地址、产物 URL 解析、非 stable 无更新、与 `DirSource` 对拍。
//!
//! 全程离线：所有字节都在临时目录或 127.0.0.1 的假 HTTP 服务上造，没有一个用例发公网请求。

mod channel_cases;
mod checker_cases;
mod ed25519_cases;
mod github_cases;
mod install_cases;
mod install_io_cases;
mod install_xdev_cases;
mod job_cases;
mod manifest_cases;
mod signature_cases;
mod source_cases;
mod support;
mod support_sign;
mod target_cases;
mod verify_cases;
mod version_cases;
