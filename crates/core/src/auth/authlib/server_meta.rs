//! 认证服务器元数据里我们真正需要的字段（服务发现的产物）。
//!
//! authlib-injector 规范里根路径 `/` 返回 `{ meta, skinDomains, signaturePublickey }`
//! （`analyse/04` §6.3）。本期**只保存**名字/非邮箱登录/skinDomains：
//! `signaturePublickey` 与 `prefetched` 的 Base64 属于注入参数（后续 mission），
//! 现在存了也没人消费，不如不存。

use serde_json::Value;

/// 服务器元数据（缺失字段一律回落默认值）。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthlibServerMeta {
    /// 服务器显示名（缺省回落 URL）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 是否允许非邮箱登录名（决定输入框校验）。
    #[serde(default)]
    pub non_email_login: bool,
    /// `skinDomains`（纹理白名单，注入参数会用；本期只保存）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skin_domains: Vec<String>,
}

impl AuthlibServerMeta {
    /// 从根元数据 JSON 解析（容错：缺字段就用默认值）。
    #[must_use]
    pub fn from_value(value: &Value) -> Self {
        let meta = value.get("meta");
        let name = meta
            .and_then(|meta| meta.get("serverName"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let non_email_login = meta
            .and_then(|meta| meta.get("feature.non_email_login"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let skin_domains = value
            .get("skinDomains")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            name,
            non_email_login,
            skin_domains,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AuthlibServerMeta;
    use serde_json::json;

    #[test]
    fn parses_full_metadata() {
        let value = json!({
            "meta": {
                "serverName": "Example Skin Site",
                "feature.non_email_login": true
            },
            "skinDomains": ["example.com", "*.example.com"],
            "signaturePublickey": "-----BEGIN PUBLIC KEY-----"
        });
        let meta = AuthlibServerMeta::from_value(&value);
        assert_eq!(meta.name.as_deref(), Some("Example Skin Site"));
        assert!(meta.non_email_login);
        assert_eq!(meta.skin_domains.len(), 2);
    }

    #[test]
    fn tolerates_missing_fields() {
        let meta = AuthlibServerMeta::from_value(&json!({}));
        assert_eq!(meta, AuthlibServerMeta::default());
        assert!(meta.name.is_none());
        assert!(!meta.non_email_login);
        assert!(meta.skin_domains.is_empty());
    }
}
