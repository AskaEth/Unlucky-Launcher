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
mod tests {
    use std::sync::Mutex;

    use super::{
        client_id_status, resolve_client_id, ClientIdSource, CLIENT_ID_BUILD, CLIENT_ID_ENV,
        MISSING_CLIENT_ID_MESSAGE,
    };
    use crate::auth::error::AuthError;
    use crate::auth::microsoft::MicrosoftAuth;

    /// 环境变量是**进程级**的：动它的用例必须串行，否则并行跑会互相摘/设，结论随机。
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// 用例结束后把环境变量还原（panic 也还原：`Drop` 跑得到）。
    struct EnvGuard {
        saved: Option<String>,
    }

    impl EnvGuard {
        fn clear() -> Self {
            let saved = std::env::var(CLIENT_ID_ENV).ok();
            std::env::remove_var(CLIENT_ID_ENV);
            Self { saved }
        }

        fn set(value: &str) -> Self {
            let saved = std::env::var(CLIENT_ID_ENV).ok();
            std::env::set_var(CLIENT_ID_ENV, value);
            Self { saved }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.saved.take() {
                Some(value) => std::env::set_var(CLIENT_ID_ENV, value),
                None => std::env::remove_var(CLIENT_ID_ENV),
            }
        }
    }

    #[test]
    fn missing_client_id_message_says_unconfigured_not_unimplemented() {
        // 抓得住：文案退回「本期未支持」这类说法（用户实跑到的那个 bug），
        //         或者只说「需要 id」却不给怎么配（用户下一步就卡在这里）。
        // 抓不住：文案好不好读——那要靠 review。
        assert!(MISSING_CLIENT_ID_MESSAGE.contains("客户端 id"));
        assert!(MISSING_CLIENT_ID_MESSAGE.contains("未配置"));
        assert!(!MISSING_CLIENT_ID_MESSAGE.contains("未支持"));
        assert!(!MISSING_CLIENT_ID_MESSAGE.contains("未实现"));
        assert!(MISSING_CLIENT_ID_MESSAGE.contains(CLIENT_ID_ENV));
        assert!(MISSING_CLIENT_ID_MESSAGE.contains("cargo build"));
        // 硬约束：这句话进日志、进横幅、进错误，**示例一律是 `<id>` 占位**，不出现真 id。
        assert!(MISSING_CLIENT_ID_MESSAGE.contains("<id>"));
    }

    #[test]
    fn from_env_without_client_id_is_a_config_error_not_a_feature_gap() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _guard = EnvGuard::clear();
        let result = MicrosoftAuth::from_env();
        match result {
            Err(AuthError::Unsupported(message)) => {
                // 机读码保持 `unsupported`（CLI `--json` 与既有调用方靠它分流），
                // 但**人读文案**必须说「未配置」。
                assert_eq!(message, MISSING_CLIENT_ID_MESSAGE);
                assert!(!message.contains("未支持"));
            }
            // 构建期注入过 id 时唯一合法（`ULL_MSA_CLIENT_ID` 也设进 `cargo test` 的环境就会走到这里；
            // 官方发布构建即烘了 id，但门禁里的 `cargo test` 不带该变量，故常态走上面那条。
            // 注意：把该变量导出到跑测试的 shell 会让 presenter 的 M132 入口锚**故意红**
            // （它要求「两处都没有 id」的构建，见 crates/presenter/src/host/accounts/tests.rs）。
            Ok(_) => assert!(
                CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()),
                "没配 id 却构造成功"
            ),
            Err(other) => panic!("应是 Unsupported，实际 {other:?}"),
        }
    }

    #[test]
    fn env_client_id_wins_over_build_and_the_log_says_so() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _guard = EnvGuard::set("0000-test-env-id");

        let (id, source) = resolve_client_id().expect("环境变量给了 id");
        assert_eq!(id, "0000-test-env-id");
        // 两者都存在时环境变量优先，且**不告警**，但来源必须是 env。
        assert_eq!(source, ClientIdSource::Env);

        let status = client_id_status();
        assert!(status.is_configured());
        assert_eq!(status.source(), Some(ClientIdSource::Env));
        assert_eq!(
            status.log_line(),
            "[auth] msa client_id=configured source=env"
        );

        // 有 id 时**不再报配置错误**：只验到「进得了登录流程的入口」为止，
        // `from_env` 本身零网络（它只建 reqwest 客户端，不发请求）。
        let auth = MicrosoftAuth::from_env().expect("配了 id 就该构造成功");
        assert!(auth.endpoints().profile.contains("minecraftservices.com"));
    }

    #[test]
    fn missing_client_id_log_line_is_machine_readable() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _guard = EnvGuard::clear();
        if CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()) {
            // 构建期注入过 id：这条没有「缺配置」可言，跳过。
            return;
        }
        let status = client_id_status();
        assert!(!status.is_configured());
        assert_eq!(status.source(), None);
        assert_eq!(
            status.log_line(),
            "[auth] msa client_id=missing source=none"
        );
    }

    #[test]
    fn blank_env_value_counts_as_missing() {
        // 抓得住：`ULL_MSA_CLIENT_ID=""`（空串/全空白）被当成配好了，进而拿空 id 去联网。
        let _lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let _guard = EnvGuard::set("   ");
        if CLIENT_ID_BUILD.is_some_and(|value| !value.trim().is_empty()) {
            assert_eq!(
                resolve_client_id().map(|(_, source)| source),
                Some(ClientIdSource::Build),
                "空白环境变量必须让位给构建期注入"
            );
        } else {
            assert!(resolve_client_id().is_none());
        }
    }
}
