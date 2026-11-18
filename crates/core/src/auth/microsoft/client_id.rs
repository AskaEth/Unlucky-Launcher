//! Azure 公共客户端 id 的**配置状态**：从哪来、配没配、启动日志那一行长什么样。
//!
//! ## 为什么单独一块（M117）
//!
//! 这是**公共客户端 id**（非机密，随二进制一起分发），可以来自两处：运行期环境变量 [`CLIENT_ID_ENV`]，
//! 或构建期 `option_env!` 注入（[`CLIENT_ID_BUILD`]）——官方发布构建在构建时烘入它（M149），
//! 所以官方产物开箱即用；环境变量仍优先，可覆盖。**两处都没有**时登录是「没配置」，
//! 不是「没实现」——这两件事在用户眼里完全不同，文案与日志都必须分清：
//!
//! | 现象 | 该说什么 |
//! |---|---|
//! | 两处都没配 | [`MISSING_CLIENT_ID_MESSAGE`]：未配置 + 怎么配（含可照抄的命令） |
//! | 配了但登录失败 | 走 `AuthError` 的正常分流（网络/拒绝/无拥有权…），**不许**再提「未实现」 |
//!
//! [`ClientIdStatus::log_line`] 是给启动日志的那一行：`[auth] msa client_id=… source=…`。
//! 有了它，「未配置」与「配了但登录失败」在日志里一眼可分，不用再猜。
//!
//! 本模块**零网络**：只读环境变量与编译期常量，连 HTTP 客户端都不建。

/// 运行期环境变量名（配 client id）。
pub const CLIENT_ID_ENV: &str = "ULL_MSA_CLIENT_ID";

/// 构建期可注入的 client id（`ULL_MSA_CLIENT_ID=<id> cargo build`）。
///
/// 官方发布构建就是这么烘的（`.github/workflows/release-build.yml` 给构建步设了同名环境变量，M149；
/// 值来自仓库 Secret，不进 git 文件），所以官方产物走到这里恒为 `Some`；从源码自建时才可能为空。
pub const CLIENT_ID_BUILD: Option<&str> = option_env!("ULL_MSA_CLIENT_ID");

/// 两处都没配时给用户看的那句话（CLI / GUI / 启动日志共用，**不含任何真实 id**）。
///
/// 措辞纪律（M117）：说「未配置」，不说「未支持」——登录链路是完整的，缺的只是这一项；
/// 并且句子里带**可照抄的命令**，因为失败横幅往往是用户唯一会看到的那段字。
pub const MISSING_CLIENT_ID_MESSAGE: &str =
    "微软登录未配置：缺少 Azure 公共客户端 id（登录功能已实现，只差这一项配置）。\
构建时注入：`ULL_MSA_CLIENT_ID=<id> cargo build`；或启动前设置同名环境变量 `ULL_MSA_CLIENT_ID=<id>`。";

/// client id 从哪来（启动日志与排障用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientIdSource {
    /// 运行期环境变量 `ULL_MSA_CLIENT_ID`（**优先于**构建期注入）。
    Env,
    /// 构建期 `option_env!` 注入。
    Build,
}

impl ClientIdSource {
    /// 启动日志里的机读标签（`env` / `build`）。
    #[must_use]
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Build => "build",
        }
    }
}

/// client id 的配置状态：配没配 + 从哪来（`None` = 两处都没有）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientIdStatus {
    source: Option<ClientIdSource>,
}

impl ClientIdStatus {
    /// 拿到 id 了没（`false` = 需要配置，不是功能缺失）。
    #[must_use]
    pub const fn is_configured(self) -> bool {
        self.source.is_some()
    }

    /// 来源（`None` = 两处都没有）。
    #[must_use]
    pub const fn source(self) -> Option<ClientIdSource> {
        self.source
    }

    /// 启动日志那一行：`[auth] msa client_id=configured|missing source=env|build|none`。
    ///
    /// 判据（M117）：日后看日志能直接分辨「未配置」与「配置了但登录失败」。
    #[must_use]
    pub fn log_line(self) -> String {
        format!(
            "[auth] msa client_id={} source={}",
            if self.source.is_some() {
                "configured"
            } else {
                "missing"
            },
            self.source.map_or("none", ClientIdSource::tag)
        )
    }
}

/// 当前进程看到的 client id 与来源（**只读环境/编译期常量，不联网**）。
///
/// 优先级：环境变量 > 构建期注入。两者都存在时**不告警**（M117）：环境变量就是显式覆盖，
/// 来源如实报 [`ClientIdSource::Env`]，日志里看得到。
#[must_use]
pub fn resolve_client_id() -> Option<(String, ClientIdSource)> {
    let from_env = std::env::var(CLIENT_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty());
    let from_build = CLIENT_ID_BUILD.filter(|value| !value.trim().is_empty());
    match (from_env, from_build) {
        (Some(value), _) => Some((value, ClientIdSource::Env)),
        (None, Some(value)) => Some((value.to_string(), ClientIdSource::Build)),
        (None, None) => None,
    }
}

/// 只问「配没配 / 从哪来」——启动日志与 GUI 启动行用它（不建 HTTP 客户端、不联网）。
#[must_use]
pub fn client_id_status() -> ClientIdStatus {
    ClientIdStatus {
        source: resolve_client_id().map(|(_, source)| source),
    }
}

#[cfg(test)]
#[path = "client_id/tests.rs"]
mod tests;
