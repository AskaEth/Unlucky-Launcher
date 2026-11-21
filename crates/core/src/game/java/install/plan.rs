//! `install` 的**只读取**规划段：`--dry-run` 用它算出「会装哪一版、打哪个 URL」。
//!
//! 从 `install.rs` 拆出来的直接原因是守 400 行硬线（M280 r1 给回退链加了守卫后那边到 401 行），
//! 但**不是纯搬移**，三处语义变了，别照着「搬移」去读它：
//!
//! 1. 签名从 `plan(engine, location, major)` 变成 `plan(engine, location, &InstallRequest)`
//!    —— 版本这一维（`request.version`）现在参与选择；
//! 2. 函数体不再自己写 `for` + `continue`，改成走 [`super::chain::chain_fetch`]，
//!    **与 `install` 共用同一份判据**（取版本 + 「版本必须对得上」的兜底闸）；
//! 3. `request.priority`（设置里的 `java.vendorPriority`）现在参与选择顺序；旧实现用
//!    `vendor::order(&[], domestic)`，等于忽略这条设置。
//!
//! 与 `install/one.rs`（真装的一步）、`install/chain.rs`（判据与骨架）同一个拆法。

use std::path::PathBuf;

use super::super::error::{JavaError, JavaErrorKind};
use super::super::extract;
use super::super::location::JavaInstallRoot;
use super::super::release::JdkRelease;
use super::super::source;
use super::super::target::AdoptiumTarget;
use super::{chain::chain_fetch, chain_order, InstallRequest};
use crate::download::DownloadEngine;

/// 只做规划不落盘：返回将要安装的版本、目标目录与「是否已经装好」。
///
/// `--dry-run` 用它：`request.version` 与 `--vendor` 都参与选择（钉了版本就把这一维钉死），
/// 所以「最后会打哪个 URL、装哪个版本串」在下载之前就能看见——这也是不联网时
/// 唯一能验证版本选择逻辑的输出。
///
/// # Errors
///
/// 与 [`crate::game::java::install::install`] 的查询段相同（平台不支持 / 网络失败 / 该家没有这个版本）。
pub async fn plan(
    engine: &DownloadEngine,
    location: &JavaInstallRoot,
    request: &InstallRequest,
) -> Result<(JdkRelease, PathBuf, bool), JavaError> {
    let target = AdoptiumTarget::host()?;
    let domestic = engine.region().is_domestic();
    let wanted = request.version.as_deref();
    let candidates = chain_order(request, domestic);
    let (hit, failures) = chain_fetch(&candidates, wanted, |vendor| {
        source::fetch(engine, vendor, request.major, target, wanted)
    })
    .await;
    match hit {
        Some((_, release)) => {
            let root = location.jdk_dir(release.major, release.vendor, &release.version);
            let ready = extract::is_ready(&root, release.sha256());
            Ok((release, root, ready))
        }
        None => Err(JavaError::new(
            JavaErrorKind::Metadata,
            format!(
                "没有可用的 Java {}（{} ）",
                request.major,
                failures
                    .iter()
                    .map(|(vendor, reason)| format!("{}：{reason}", vendor.id()))
                    .collect::<Vec<_>>()
                    .join("；")
            ),
        )),
    }
}
