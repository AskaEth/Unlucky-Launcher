//! 「检查 → 取包 → 长度/SHA256 双检 → 原子替换」的一条龙：**GUI 与 CLI 的唯一入口**。
//!
//! # 为什么要有本模块
//!
//! mission 的硬要求是「GUI 与 CLI 行为一致，不许两套逻辑」。两个壳子各自拼一遍「选源 + 定目标 +
//! 暂存 + 校验 + 替换」，迟早会漂：一处忘了摘要校验，一处把暂存放在数据根之外，一处把「没有更新」
//! 当成错误。所以这些决定收成一个 [`Job`]：
//!
//! ```text
//! Job { source, target, staging, dest }
//!      ├─ check(current, channel)  → CheckOutcome   （版本比较/渠道准入都在 checker）
//!      └─ apply(current, channel)  → Applied        （复查 → 取包 → 双检 → 替换）
//! ```
//!
//! # 应用为什么先复查一次
//!
//! 用户点「安装」离上次检查可能已过几分钟，期间发布侧可能换了包。清单只有几 KB，复查一次换来
//! 两个好处：装的一定是**此刻**最新的那一份；`apply` 自带「现在到底有没有得装」的答案，
//! 不必让调用方信任内存里的旧候选。
//!
//! # 失败时的文件去哪
//!
//! 校验失败：暂存文件由 [`stage`] 就地删掉（[`super::verify::StagedPackage`] 的 `Drop` 再兜一层），
//! **不留在原地**。替换失败：由 [`install_staged`] 回滚，旧版本仍在原路径上。两件事都不需要调用方记得做。
//!
//! # 换源
//!
//! 生产源是 D50 定死的 GitHub Releases（[`default_source`]）；`ULL_UPDATE_DIR` 指向本地目录时
//! 改走 [`super::source::DirSource`]——那是取证/内网手工分发的口子，**不是**第二个更新源。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::download::DownloadEngine;

use super::channel::Channel;
use super::checker::{CheckOutcome, UpdateChecker};
use super::error::UpdateError;
use super::github::GitHubReleaseSource;
use super::install::{install_staged, InstallReceipt, ReplacePolicy};
use super::manifest::Artifact;
use super::source::{DirSource, UpdateSource};
use super::target::{PackageFormat, Target};
use super::verify::stage;
use super::version::Version;

/// 本机更新源覆盖（取证 / 内网手工分发）：指向一个 `DirSource` 根目录（`<root>/stable.json`）。
///
/// **只在调试构建里生效**（判据与理由见 [`local_dir_override`]）：发布构建忽略它，
/// 真实网络下 `releases/latest` 是唯一入口。它的用途是把「发现新版本」这条分支在没有稳定
/// Release 的情况下也能整环验一遍（本地清单 + 本地产物，一个字节都不出网）。
pub const UPDATE_DIR_ENV: &str = "ULL_UPDATE_DIR";

/// 一次应用的结局。
#[derive(Debug)]
pub enum Applied {
    /// 替换成功（新版本已经就位，等重启）。
    Installed {
        /// 收据（目标路径 + 旧版本备份 + 字节数）。
        receipt: InstallReceipt,
        /// 装上的版本号。
        version: String,
    },
    /// 复查时已经没有可装的更新（例如这期间别人替你升过）。
    Nothing(CheckOutcome),
}

/// 一次检查/应用的全部输入。
///
/// 不含任何 UI 状态：谁在查、查完怎么展示是壳层的事。字段都可读，日志/诊断要什么有什么。
pub struct Job {
    source: Box<dyn UpdateSource>,
    target: Target,
    staging: PathBuf,
    dest: PathBuf,
}

impl std::fmt::Debug for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Job")
            .field("source", &self.source.source_id())
            .field("target", &self.target.as_str())
            .field("staging", &self.staging)
            .field("dest", &self.dest)
            .finish()
    }
}

impl Job {
    /// 显式装配（测试用：源、目标平台、暂存、替换目标全都自己给）。
    #[must_use]
    pub fn new(
        source: Box<dyn UpdateSource>,
        target: Target,
        staging: impl Into<PathBuf>,
        dest: impl Into<PathBuf>,
    ) -> Self {
        Self {
            source,
            target,
            staging: staging.into(),
            dest: dest.into(),
        }
    }

    /// 本进程的默认装配：目标平台 = 编译目标，替换目标 = 本进程 exe，
    /// 暂存 = `<cache_root>/update/staging`。
    #[must_use]
    pub fn for_process(source: Box<dyn UpdateSource>, cache_root: &Path) -> Self {
        Self::new(
            source,
            Target::current(),
            staging_dir(cache_root),
            process_dest(),
        )
    }

    /// 换替换目标（`ull update apply --dest <path>`、单测都走它）。
    #[must_use]
    pub fn with_dest(mut self, dest: impl Into<PathBuf>) -> Self {
        self.dest = dest.into();
        self
    }

    /// 源 id（日志/事件里的 `source` 字段）。
    #[must_use]
    pub fn source_id(&self) -> &str {
        self.source.source_id()
    }

    /// 绑定的目标平台。
    #[must_use]
    pub const fn target(&self) -> Target {
        self.target
    }

    /// 暂存目录。
    #[must_use]
    pub fn staging(&self) -> &Path {
        &self.staging
    }

    /// 替换目标（本进程 exe）。
    #[must_use]
    pub fn dest(&self) -> &Path {
        &self.dest
    }

    /// 查一次更新。
    ///
    /// # Errors
    ///
    /// 清单取不到 / 清单不合法 / 渠道不符都返回 [`UpdateError`]；「没有更新」不是错误，
    /// 走 [`CheckOutcome::UpToDate`]。
    pub fn check(&self, current: &Version, channel: Channel) -> Result<CheckOutcome, UpdateError> {
        UpdateChecker::new(SourceRef(&*self.source), self.target).check(current, channel)
    }

    /// 下载 → 双检 → 原子替换。
    ///
    /// # Errors
    ///
    /// 复查失败、取包失败、长度/摘要不符、替换失败都返回 [`UpdateError`]；
    /// 「复查之后没有可装的更新」不是错误，走 [`Applied::Nothing`]。
    pub fn apply(&self, current: &Version, channel: Channel) -> Result<Applied, UpdateError> {
        let outcome = self.check(current, channel)?;
        let Some(candidate) = outcome.candidate() else {
            return Ok(Applied::Nothing(outcome));
        };
        // P3-6：清单只有 `target` 参与挑选（`Release::artifact_for` 只比平台），所以清单可以给
        // win-x64 挂一个 msi；以前这里会把它直接 rename 成本进程的 exe（D50 明说「不做 MSI」）。
        // 先按包形态挡一次，**挡在取包之前**：既不浪费下载，也不会在目标路径上留下任何东西。
        ensure_self_replaceable(&candidate.artifact)?;
        let staged = stage(&*self.source, &candidate.artifact, &self.staging)?;
        let version = candidate.release.version.to_string();
        match install_staged(&staged, &self.dest, ReplacePolicy::default()) {
            Ok(receipt) => Ok(Applied::Installed { receipt, version }),
            Err(failure) => Err(failure.error),
        }
    }
}

/// 这个包形态能不能**原地**变成启动器本体。
///
/// 只有「便携单文件 exe」（[`PackageFormat::PortableExe`]）可以：
///
/// | 形态 | 为什么不行 |
/// |---|---|
/// | `msi` / `dmg` / `deb` / `rpm` | D50 明说「不做 MSI」——那是系统包管理器的活（`needs_installer()`） |
/// | `portable-zip` | 是压缩包，不是启动器本体：rename 到 exe 路径只会得到一个跑不起来的文件 |
/// | `appimage` | Linux 的可执行镜像，Windows/macOS 上没有意义；且本模块只做「同目录改名换 exe」 |
pub(super) fn self_replaceable(format: PackageFormat) -> bool {
    matches!(format, PackageFormat::PortableExe)
}

/// 包形态不合规就拒绝（错误分类是 `InvalidManifest`：清单挂错了东西，换源/重试都没用）。
fn ensure_self_replaceable(artifact: &Artifact) -> Result<(), UpdateError> {
    if self_replaceable(artifact.format) {
        return Ok(());
    }
    Err(UpdateError::invalid_manifest(format!(
        "发布里的 {} 产物是 {} 形态，不能原地替换启动器：本启动器只做便携单文件 exe 的自替换\
         （安装器形态请用系统包管理器；压缩包请解压后再用）",
        artifact.target.as_str(),
        artifact.format.as_str()
    )))
}

/// `&dyn UpdateSource` 也能当源用（[`UpdateChecker`] 是泛型的，这里省掉一次装箱）。
struct SourceRef<'a>(&'a dyn UpdateSource);

impl UpdateSource for SourceRef<'_> {
    fn source_id(&self) -> &str {
        self.0.source_id()
    }

    fn fetch_manifest(
        &self,
        channel: Channel,
    ) -> Result<Option<super::manifest::ReleaseManifest>, UpdateError> {
        self.0.fetch_manifest(channel)
    }

    fn fetch_artifact(
        &self,
        artifact: &super::manifest::Artifact,
        dest: &Path,
    ) -> Result<u64, UpdateError> {
        self.0.fetch_artifact(artifact, dest)
    }
}

/// 本进程要替换的目标（拿不到 `current_exe` 时留一个占位名，让错误信息里至少有个名字）。
#[must_use]
pub fn process_dest() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("unlucky-launcher"))
}

/// 暂存目录（`<cache_root>/update/staging`）。
#[must_use]
pub fn staging_dir(cache_root: &Path) -> PathBuf {
    cache_root.join("update").join("staging")
}

/// 生产源：GitHub Releases（D50）；**只在调试构建里**，`ULL_UPDATE_DIR` 设了就走本地目录源。
#[must_use]
pub fn default_source(engine: Arc<DownloadEngine>) -> Box<dyn UpdateSource> {
    match local_dir_override() {
        Some(root) => Box::new(DirSource::new(root)),
        None => Box::new(GitHubReleaseSource::new(engine)),
    }
}

/// `ULL_UPDATE_DIR` 的落点：**只认调试构建**。
///
/// # 为什么判据是 `debug_assertions` 而不是别的
///
/// 这个后门能绕过 GitHub 源：谁能让启动器读到一份本地清单，谁就能让它把任意字节 rename 成本进程
/// 的 exe。清单里的摘要也来自同一份清单，所以**这条路上没有可信根**——它不是权限提升（要能设用户
/// 环境变量 + 用户自己点「下载并安装」），但发布产物里没有任何理由保留它。
///
/// 三档选一，我选了「按构建画像判」：
///
/// | 判据 | 为什么不选 |
/// |---|---|
/// | 命令行显式参数（`--update-dir`） | 要给 GUI 与 CLI 各加一条参数、还要穿到 `Job` 里；而它只服务取证/内网，不该出现在正式用法里 |
/// | 运行期开关（设置键 / 环境变量再加一层鉴权） | 等于「发布版也留着这条路」，只是多了一道可被一起设好的门 |
/// | **`cfg!(debug_assertions)`** | 发布构建（`--release`）里这个常量是 `false`，整段分支被常量折叠掉——**release 二进制里这条路径根本不存在**，而且是编译期事实，不靠用户/环境配合 |
///
/// 调试构建仍认它：本 mission 的「发现新版本 → 下载 → 双检 → 自替换」整环就是用 `cargo build`
/// 出来的二进制 + 本地夹具验的（真实 GitHub 上 v0.2.2 还是 prerelease，取不到 stable 清单）。
fn local_dir_override() -> Option<std::path::PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let root = std::env::var(UPDATE_DIR_ENV).ok()?;
    let root = root.trim();
    if root.is_empty() {
        return None;
    }
    Some(std::path::PathBuf::from(root))
}
