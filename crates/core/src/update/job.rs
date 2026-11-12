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
//! 生产源是 D50 定死的 GitHub Releases（[`default_source`]）；**测试通道**（带 `update-test-source`
//! feature 编出来的二进制 + [`UPDATE_DIR_ENV`] + [`UPDATE_TEST_SOURCE_ENV`] 三档全给）时改走
//! [`super::source::DirSource`]——那是取证/内网手工分发的口子，**不是**第二个更新源。判据、隔离
//! 手段与残余风险见 [`test_source_root`]。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::download::DownloadEngine;

use super::channel::Channel;
use super::checker::{CheckOutcome, UpdateChecker};
use super::error::UpdateError;
use super::github::GitHubReleaseSource;
use super::install::{install_staged, InstallReceipt, ReplacePolicy};
use super::manifest::Artifact;
#[cfg(feature = "update-test-source")]
use super::source::DirSource;
use super::source::UpdateSource;
use super::target::{PackageFormat, Target};
use super::verify::stage;
use super::version::Version;

/// 测试通道的源根（取证 / 内网手工分发）：指向一个 `DirSource` 根目录（`<root>/stable.json`）。
///
/// 只在**带 `update-test-source` feature** 编出来的二进制里生效，且**单独设它没有用**：还要
/// [`UPDATE_TEST_SOURCE_ENV`]（判据见 `test_source_root`）。默认构建里这两个变量纯粹是死字符串。
pub const UPDATE_DIR_ENV: &str = "ULL_UPDATE_DIR";

/// 测试通道的显式承认开关：值必须是 `1`，[`UPDATE_DIR_ENV`] 才会被认（同样只在 feature 构建里）。
///
/// 两个变量分开是为了「路径」与「我知道我在用测试源」这两件事各自留痕：只漏出一个路径
/// （继承的环境、别的脚本 export 过）不足以把启动器的更新源改走本地目录。
pub const UPDATE_TEST_SOURCE_ENV: &str = "ULL_UPDATE_TEST_SOURCE";

/// [`UPDATE_TEST_SOURCE_ENV`] 唯一认的值（`true` / `yes` / `on` 一律不算：手误不该开闸）。
#[cfg(feature = "update-test-source")]
const TEST_SOURCE_ACK: &str = "1";

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

    fn fetch_manifest_documents(
        &self,
        channel: Channel,
    ) -> Result<Option<super::source::ManifestDocuments>, UpdateError> {
        self.0.fetch_manifest_documents(channel)
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

/// 生产源：GitHub Releases（D50）；**带 `update-test-source` feature 的构建**里，测试通道开启时
/// （`test_source_root`）走本地目录源。
#[must_use]
pub fn default_source(engine: Arc<DownloadEngine>) -> Box<dyn UpdateSource> {
    // 测试通道（M169）：整段只在带 feature 的构建里存在。默认关 ⇒ 官方发布产物里
    // `test_source_root` 根本编不出来，`DirSource` 也进不了这条入口（编译期事实，不看运行时）。
    #[cfg(feature = "update-test-source")]
    if let Some(root) = test_source_root() {
        return Box::new(DirSource::new(root));
    }
    Box::new(GitHubReleaseSource::new(engine))
}

/// 测试通道的判据：**feature 编进来 + 两档环境变量给全**，才返回本地源根。
///
/// # 为什么判据从「只看构建画像」改成「feature + 两档显式承认」
///
/// M159 的判据是 `cfg!(debug_assertions)`：`--release` 里整段被常量折叠掉，于是**发布产物没法
/// 指向本地更新源**，而发布产物正是 release，`ull update apply` 又会替换本进程 exe——这条链在
/// release 上从来没有端到端证据（M167 的结论）。要补这一环，判据必须与「是不是 debug」脱钩。
///
/// M169 的三档（tower 定案 B，理由不是省事而是攻击面）：
///
/// | 档 | 怎么给 | 作用 |
/// |---|---|---|
/// | 编译期 | `--features update-test-source` | 这段代码**才存在**；官方发布构建不带它 |
/// | 路径 | `ULL_UPDATE_DIR=<root>` | 本地源根（`<root>/stable.json` + 产物） |
/// | 承认 | `ULL_UPDATE_TEST_SOURCE=1` | 「我知道这是测试通道」；值必须是 `1`，`true`/`yes` 不算 |
///
/// 只给路径（或只设环境变量）什么都不会发生：**默认构建里这段函数根本不存在**。
///
/// # 隔离手段（逐条；别只信「有闸门」）
///
/// 1. **编译期不存在**：默认 features 关，官方发布二进制里没有这条路径——这是 README 里
///    「release builds do not contain that path at all」这句承诺的落点。
/// 2. **不是默认**：feature 打开之后，还要两个环境变量同时给；判据是纯函数
///    （`test_source_root_from`），每种半开都有单测。
/// 3. **不是用户面开关**：没有设置键、没有命令行参数、不落盘；每次 [`default_source`] 重读环境
///    变量，同一个进程里改了就立刻生效/失效（不留缓存）。
/// 4. **输出里点名**：`ull update check --json` 的信封里 `data.source` 是 `file:<root>`、
///    `data.test_source` 是 `true`（判据派生自**实际用了哪个源**，不是回读环境变量），人读输出另有
///    一行「测试通道生效」。
/// 5. **发布流程不碰它**：`packaging/check_test_source_off.py` 扫 `.github/workflows/**`、
///    `packaging/*.py|*.sh` 与各 `Cargo.toml` 的 `default`，任何一处打开它就红（CI 的
///    release-guard job 跑它）。
///
/// # 这条闸门**不是**权限边界
///
/// 谁能在 Windows 上给 GUI 设进程环境变量，谁就已经能以该用户身份运行任意程序。真正要守的是
/// 「**官方二进制里不存在第二条更新源**」——第 1 条（编译期）才是这件事的判据，第 2-5 条是纵深。
#[cfg(feature = "update-test-source")]
pub(super) fn test_source_root() -> Option<PathBuf> {
    let dir = std::env::var(UPDATE_DIR_ENV).ok();
    let ack = std::env::var(UPDATE_TEST_SOURCE_ENV).ok();
    test_source_root_from(dir.as_deref(), ack.as_deref())
}

/// [`test_source_root`] 的纯判据（不碰进程环境：单测直接喂两档，并行跑也不会互相干扰）。
///
/// 空路径按「没给」处理（`ULL_UPDATE_DIR=` 这种写法很常见，不该被当成「当前目录」）；
/// 承认档两侧空白忽略、大小写不忽略（`1` 就是 `1`）。
#[cfg(feature = "update-test-source")]
pub(super) fn test_source_root_from(dir: Option<&str>, ack: Option<&str>) -> Option<PathBuf> {
    if ack.map(str::trim) != Some(TEST_SOURCE_ACK) {
        return None;
    }
    let root = dir?.trim();
    if root.is_empty() {
        return None;
    }
    Some(PathBuf::from(root))
}
