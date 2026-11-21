//! 「用户自带 Key」这一档：Key 的类型与「有没有配」的状态。
//!
//! # 为什么 Key 是一个类型而不是 `String`
//!
//! 它是本模块里唯一**不能出现在日志 / 错误 / DTO** 的字段。用 newtype 把两件事变成类型事实：
//!
//! 1. [`ApiKey`] **没有**公开的取值方法（只有 `pub(crate)` 的 `ApiKey::expose`，拿给请求头用），
//!    于是本 crate 之外的代码即使拿到一个 `ApiKey` 也印不出来；
//! 2. `Debug` 是手写的 `ApiKey(<redacted>)`，`#[derive(Debug)]` 会把它打进任何一行 `{:?}` ——
//!    包括 `CurseForge` 自己的 Debug、测试失败的断言输出、以及日志里顺手打的请求结构。
//!
//! # 为什么不内置一把
//!
//! CurseForge 官方 API 要求 `x-api-key`，闭源启动器把自家 Key 编进二进制。本项目**不这样做**：
//! 那是别人的配额与授权，分发一份进去等于把用户的检索挂在别人的凭据上，Key 一旦被吊销，
//! 所有人一起坏。所以 Key 只能来自用户：宿主先读设置 `online.curseforge.api_key`，环境变量
//! `ULL_CF_API_KEY` 只是兜底（**core 既不读环境变量也不读设置**：一个库不该在初始化时偷偷摸
//! 进程环境，也不该知道宿主的配置落在哪）。
//!
//! 那条优先级本身在 [`resolve_key`] 里**只写一遍**（GUI 与 CLI 都调它），变量名在
//! [`API_KEY_ENV`] 里也只写一遍；两个宿主各自只做「取两个裸值 + 调它」。
//!
//! 于是「没配 Key」是一个**正常状态**，不是配置错误：[`super::CfErrorKind::NotConfigured`]
//! 就是它的具名表达，客户端在发请求**之前**就返回它。

use std::fmt;

use super::CfError;

/// 用户自带的 CurseForge API Key。
///
/// 构造只做「非空白」检查：Key 的形状我们没有权威定义（站点没说长度与字符集），本地校验一旦
/// 猜错就是把正确的 Key 拒掉。真正的校验留给站点（401/403 → [`super::CfErrorKind::InvalidKey`]）。
#[derive(Clone)]
pub struct ApiKey(String);

impl ApiKey {
    /// 从用户输入构造（宿主从设置 `online.curseforge.api_key` 读，`ULL_CF_API_KEY` 兜底）。
    ///
    /// # Errors
    /// 空白串（含 `""`）返回 [`CfErrorKind::NotConfigured`]：**空 Key 与没配 Key 是同一件事**，
    /// 让它变成 [`super::CfErrorKind::InvalidKey`] 会误导用户去"换一把"。
    ///
    /// [`CfErrorKind::NotConfigured`]: super::CfErrorKind::NotConfigured
    pub fn new(raw: impl Into<String>) -> Result<Self, CfError> {
        let raw = raw.into();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(CfError::not_configured());
        }
        Ok(Self(trimmed.to_string()))
    }

    /// 请求头里的值。
    ///
    /// **只给本 crate 的请求拼装点用**（`pub(crate)`）：crate 之外没有「把 Key 取出来」的路，
    /// 于是「顺手把它塞进错误串 / 日志」在类型层就不成立。
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey(<redacted>)")
    }
}

/// 兜底那一档的环境变量名（`ULL_CF_API_KEY`）。
///
/// 只是把**名字**收在一处：本 crate 不读它（[`resolve_key`] 收的是值，不是变量名），
/// 宿主各读一次再递进来——于是这个字符串不会在 GUI 与 CLI 里各写一遍、慢慢漂成两个名字。
pub const API_KEY_ENV: &str = "ULL_CF_API_KEY";

/// 「有没有 Key」的**唯一**判据：设置优先 → 环境变量兜底。
///
/// # 纯函数
///
/// 两个参数都是**值**：本函数不读环境变量、不读设置、不碰 IO（[`resolve_key`] 的可测性靠这一条，
/// core「既不读环境变量也不读设置」的纪律也靠它）。宿主只负责「从自己的配置里取两个裸值」。
///
/// # 为什么必须只有一处
///
/// 这条优先级曾经写过三遍（presenter 的检索、presenter 的宿主三函数、CLI 的导入 fetcher），
/// 于是「设置里填了 Key」在一处算有、在另两处算没有——同一份配置两种结论。
/// 现在 GUI（`presenter::host::addon::sources::api_key`）与 CLI（`commands::pack`）都只调它，
/// 把优先级改反（环境变量优先）会同时打红两侧的用例。
///
/// # 「空白」与「没配」是同一件事
///
/// 空串 / 空白串一律按没配处理，判据**复用 [`ApiKey::new`]**（它本来就是这么定义的），
/// 所以这里不再写一遍 `trim` + `is_empty`：那一档只有 [`ApiKey::new`] 一处。
#[must_use]
pub fn resolve_key(setting_value: Option<&str>, env_value: Option<&str>) -> Option<ApiKey> {
    setting_value
        .and_then(|raw| ApiKey::new(raw).ok())
        .or_else(|| env_value.and_then(|raw| ApiKey::new(raw).ok()))
}

/// Key 的配置状态（页面据此决定「检索按钮能不能点」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CfKeyState {
    /// 没配（每一次调用都会得到 [`super::CfErrorKind::NotConfigured`]，**不发请求**）。
    Missing,
    /// 配了（内容仍可能 401/403，那由站点的回答决定）。
    Configured,
}

impl CfKeyState {
    /// 稳定字符串 id（日志 / DTO 用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Configured => "configured",
        }
    }

    /// 配了吗。
    #[must_use]
    pub const fn is_configured(self) -> bool {
        matches!(self, Self::Configured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用例要能说出**哪一档**赢了，所以读回明文（`expose` 是 `pub(crate)`，本模块内可见）。
    fn value_of(key: Option<ApiKey>) -> Option<String> {
        key.map(|key| key.expose().to_string())
    }

    /// 设置优先、环境变量兜底 —— **本仓唯一一处**表达这条优先级。
    ///
    /// 抓得住：把 `resolve_key` 的两支对调（环境变量优先）⇒ 本用例红；两侧（GUI / CLI）的用例
    /// 也各有一条跟着红，因为两边都只调这一个函数。
    #[test]
    fn the_setting_wins_and_the_env_var_is_only_the_fallback() {
        assert_eq!(
            value_of(resolve_key(Some("from-settings"), Some("from-env"))).as_deref(),
            Some("from-settings"),
            "两档都有值时必须取设置那一档"
        );
        assert_eq!(
            value_of(resolve_key(None, Some("from-env"))).as_deref(),
            Some("from-env"),
            "设置没写时环境变量兜底"
        );
        assert_eq!(
            value_of(resolve_key(Some("from-settings"), None)).as_deref(),
            Some("from-settings"),
            "只有设置时不必依赖环境变量"
        );
    }

    /// 「空白」与「没配」是同一件事 —— **两档都**这样，否则「设置里留了个空格」会把兜底档整个挡死。
    ///
    /// 抓得住：滤空那一行被去掉（`Some("   ")` 变成 `Some(ApiKey("   "))`）或只在设置那一档滤空
    /// （空串环境变量被当成 Key）⇒ 红。M316 在这条上栽过一次。
    #[test]
    fn blank_counts_as_missing_on_both_sources() {
        assert!(resolve_key(None, None).is_none(), "两档都没有 ⇒ 没配");
        assert!(resolve_key(Some(""), Some("")).is_none(), "空串不是 Key");
        assert!(
            resolve_key(Some("   "), Some(" \t\n ")).is_none(),
            "空白不是 Key"
        );
        assert_eq!(
            value_of(resolve_key(Some("   "), Some("from-env"))).as_deref(),
            Some("from-env"),
            "设置里只留了空白 ⇒ 仍要走到兜底那一档"
        );
        assert_eq!(
            value_of(resolve_key(Some("from-settings"), Some("  "))).as_deref(),
            Some("from-settings"),
            "空白的环境变量不许顶掉真实的设置值"
        );
    }

    /// 递进来的值会被 `trim`（Key 的明文只到 `ApiKey` 为止，拼请求头时不该带首尾空白）。
    #[test]
    fn the_value_is_trimmed() {
        assert_eq!(
            value_of(resolve_key(Some("  spaced-key  "), None)).as_deref(),
            Some("spaced-key")
        );
        assert_eq!(
            value_of(resolve_key(None, Some("\tenv-key\n"))).as_deref(),
            Some("env-key")
        );
    }

    /// 变量名只登记一次（GUI 与 CLI 都读这个常量；改名要在这里一并改）。
    #[test]
    fn the_env_var_name_is_the_documented_one() {
        assert_eq!(API_KEY_ENV, "ULL_CF_API_KEY");
    }
}
