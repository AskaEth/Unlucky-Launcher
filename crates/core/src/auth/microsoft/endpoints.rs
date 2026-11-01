//! 微软端点表：**直连**（不走镜像，承 mission Context 与 `analyse/04` §4 的端点族区分）。
//!
//! 两条流**共用同一个端点族**（`research/08` §2 硬性约束 1：租户只能用 `consumers`）：
//! - 设备码流：`devicecode` + `token`；
//! - 授权码流：`authorize`（浏览器打开）+ 同一个 `token`（`grant_type=authorization_code`）。
//!
//! 都走 `login.microsoftonline.com/consumers/oauth2/v2.0/*`：用 `common` 或 AAD 租户 id
//! 拿 `XboxLive.signin` 会直接被拒（`analyse/04` §4 的「第一个坑」）。
//!
//! 端点集中成一张表是为了**测试可替换**：整表指向一个进程内 mock 服务器即可覆盖全链，
//! 生产默认走 [`MicrosoftEndpoints::consumers`]。

/// 微软账户 OAuth 与 Xbox / Minecraft 服务端点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftEndpoints {
    /// 设备码申请（`POST`，表单 `client_id` + `scope`）。
    pub device_code: String,
    /// 授权码流的授权页（`GET`，浏览器打开；`research/08` §3 方案 A）。
    pub authorize: String,
    /// 换 token / 刷新 token / 授权码换 token（`POST`，表单）。
    pub token: String,
    /// Xbox Live 用户认证（`POST` JSON）。
    pub xbl_authenticate: String,
    /// XSTS 授权（`POST` JSON；无 Xbox 档案时回 **401** 带 `XErr`）。
    pub xsts_authorize: String,
    /// `login_with_xbox`（`POST` JSON，`identityToken = "XBL3.0 x=<uhs>;<xsts>"`）。
    pub login_with_xbox: String,
    /// 拥有权校验 `entitlements/mcstore`（`GET`，必须 200）。
    pub entitlements_mcstore: String,
    /// 拥有权细分 `entitlements/license`（`GET`，`/minecraft/profile` 404 时复判用）。
    pub entitlements_license: String,
    /// 档案 `minecraft/profile`（`GET`，带 Bearer）。
    pub profile: String,
}

impl MicrosoftEndpoints {
    /// 生产端点（微软消费版账户，直连）。
    #[must_use]
    pub fn consumers() -> Self {
        Self {
            device_code: "https://login.microsoftonline.com/consumers/oauth2/v2.0/devicecode"
                .to_string(),
            authorize: "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize"
                .to_string(),
            token: "https://login.microsoftonline.com/consumers/oauth2/v2.0/token".to_string(),
            xbl_authenticate: "https://user.auth.xboxlive.com/user/authenticate".to_string(),
            xsts_authorize: "https://xsts.auth.xboxlive.com/xsts/authorize".to_string(),
            login_with_xbox: "https://api.minecraftservices.com/authentication/login_with_xbox"
                .to_string(),
            entitlements_mcstore: "https://api.minecraftservices.com/entitlements/mcstore"
                .to_string(),
            entitlements_license: "https://api.minecraftservices.com/entitlements/license"
                .to_string(),
            profile: "https://api.minecraftservices.com/minecraft/profile".to_string(),
        }
    }

    /// 把整表指向同一个基址（测试用；各端点用自己的路径后缀区分）。
    #[must_use]
    pub fn all_on(base: &str) -> Self {
        let join = |path: &str| format!("{}/{path}", base.trim_end_matches('/'));
        Self {
            device_code: join("devicecode"),
            authorize: join("authorize"),
            token: join("token"),
            xbl_authenticate: join("xbl"),
            xsts_authorize: join("xsts"),
            login_with_xbox: join("login_with_xbox"),
            entitlements_mcstore: join("entitlements/mcstore"),
            entitlements_license: join("entitlements/license"),
            profile: join("profile"),
        }
    }

    /// 所有端点（调试/诊断用）。
    #[must_use]
    pub fn all(&self) -> [&str; 9] {
        [
            &self.device_code,
            &self.authorize,
            &self.token,
            &self.xbl_authenticate,
            &self.xsts_authorize,
            &self.login_with_xbox,
            &self.entitlements_mcstore,
            &self.entitlements_license,
            &self.profile,
        ]
    }
}

impl Default for MicrosoftEndpoints {
    fn default() -> Self {
        Self::consumers()
    }
}

#[cfg(test)]
mod tests {
    use super::MicrosoftEndpoints;

    #[test]
    fn consumers_endpoints_stay_on_two_families() {
        let endpoints = MicrosoftEndpoints::consumers();
        assert!(endpoints.device_code.contains("login.microsoftonline.com"));
        assert!(endpoints.token.contains("/consumers/oauth2/v2.0/token"));
        // 授权码流的授权页与设备码流同族同租户（`research/08` §2：只能用 consumers）。
        assert!(endpoints
            .authorize
            .contains("/consumers/oauth2/v2.0/authorize"));
        assert!(endpoints
            .xbl_authenticate
            .contains("user.auth.xboxlive.com"));
        assert!(endpoints.xsts_authorize.contains("xsts.auth.xboxlive.com"));
        // 微软 OAuth 两流都不走 HMCL 的 login.live.com 端点族。
        assert!(endpoints
            .all()
            .iter()
            .all(|url| !url.contains("login.live.com")));
    }

    #[test]
    fn all_on_keeps_paths_distinct() {
        let endpoints = MicrosoftEndpoints::all_on("http://127.0.0.1:1234/");
        assert_eq!(endpoints.device_code, "http://127.0.0.1:1234/devicecode");
        assert_eq!(endpoints.authorize, "http://127.0.0.1:1234/authorize");
        assert_eq!(
            endpoints.entitlements_license,
            "http://127.0.0.1:1234/entitlements/license"
        );
    }
}
