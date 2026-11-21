//! 在线内容源的凭据键（M317）：CurseForge 官方 API 的 `x-api-key`。
//!
//! 从 `keys.rs` 拆出（那边贴着 400 行硬线）：主表只留一行 `.with_all(online::specs())`。
//!
//! ## 为什么是「用户自带 Key」，不是内置一把
//!
//! CurseForge 官方 API 的每次请求都要 `x-api-key`。闭源启动器把自家 Key 编进二进制；本项目
//! **不这样做**：那是别人的配额与授权，分发一份进去等于全世界的检索挂在同一把钥上，钥一被吊销
//! 所有人一起坏。于是 Key 只能来自用户，本键就是它落盘的地方。
//!
//! ## 形状照 `ai.api_key`（同一套敏感键纪律）
//!
//! 1. **默认 `null`**（= 没配）：老配置文件没有这个键时的行为与「用户不填」逐字相同——CF 源与
//!    CF 整合包的 `files[]` 补全如实报「缺 Key」（`cf_not_configured` / `fetch_not_configured`），
//!    不是报「包坏了」；
//! 2. [`AiPolicy::locked`]：AI 永远不能读写它（D9「AI 永不触凭据」），所以它也不进 `get_config`
//!    的 schema 白名单；
//! 3. **不落日志**：本键的读取点只有一处（宿主把值塞进 `addon::curseforge::ApiKey`），而
//!    `ApiKey` 没有公开取值方法、`Debug` 恒打 `<redacted>`——「顺手把它打进错误串 / 日志」在
//!    类型层就不成立。
//!
//! ## 迁移：纯增量 minor，不需要迁移步骤
//!
//! `1.10.0` 只登记这一个键，出厂值为 `null`。[`super::super::document`] 的「默认值不落盘」让
//! 老文件缺键即取默认，迁移链（按 major 走）不需要新步骤。

use crate::setting::spec::{AiPolicy, SettingSpec, ValueKind};

/// 键名（宿主与用例都从这里取，避免同一个字符串写两遍写飘）。
pub const CURSEFORGE_API_KEY: &str = "online.curseforge.api_key";

/// 本键域的登记项（主表用 `.with_all(online::specs())` 接上）。
#[must_use]
pub fn specs() -> Vec<SettingSpec> {
    vec![SettingSpec::new(
        CURSEFORGE_API_KEY,
        ValueKind::Text { max_len: 256 },
        serde_json::Value::Null,
        AiPolicy::locked(),
        "CurseForge 官方 API Key（用户自带，curseforge.com 账号后台生成）；空 = 没配，CurseForge 源与整合包里的 CF 条目会如实报「缺 Key」而不是报失败",
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 与 `ai.api_key` 同一套形状：默认 `null`、AI 不可写、分级 High。
    ///
    /// 抓得住：有人把它改成 `AiPolicy::sensitive()`（AI 可写 ⇒ 凭据进了 AI 工具链）⇒ 红；
    /// 改成有非空默认值（等于替用户编了一把钥）⇒ 红。
    #[test]
    fn the_curseforge_key_is_locked_and_defaults_to_absent() {
        let spec = specs().pop().expect("一条");
        assert_eq!(spec.key, CURSEFORGE_API_KEY);
        assert_eq!(spec.key, "online.curseforge.api_key");
        assert_eq!(spec.default, serde_json::Value::Null, "默认 = 没配");
        assert!(!spec.ai.writable, "凭据键不许 AI 写（locked）");
        assert_eq!(spec.ai.sensitivity, crate::setting::spec::Sensitivity::High);
        // 用户填的值仍要过类型检查（长 Key 不许无声截断）。
        assert!(spec.kind.check(&json!("cf-key-123")).is_ok());
        assert!(spec.kind.check(&json!("x".repeat(257))).is_err());
    }
}
