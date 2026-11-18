//! 对外客户端配置样例（用户把它们粘进 Claude Code / Cursor 就能连上）。
//!
//! 为什么要生成而不是写在文档里：端口与令牌是**用户本机**的值，手抄必错。
//! 两个客户端的形状不同（02 §0）：
//! - **Claude Code**：`{"type":"http","url":…}`（`type` 必填，缺了会被当成 stdio）
//! - **Cursor**：`{"url":…}`（`mcp.json` 里只认 url；多写 type 会被忽略但不报错）
//!
//! 令牌只在设置里非空时出现——回环 + 无令牌是允许的（研究里“至少一个 token”是建议不是铁律，
//! 而我们用 `McpConfig::ensure_loopback` 硬性禁掉对外监听）。

use serde_json::{json, Value};

/// 客户端配置样例（三件套：端点 URL + 两份可直接粘的 JSON）。
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigSamples {
    /// `http://127.0.0.1:<port>/mcp`。
    pub url: String,
    /// Claude Code（`claude mcp add` / `~/.claude.json` 的 `mcpServers` 片段）。
    pub claude_code: Value,
    /// Cursor（`.cursor/mcp.json` 的 `mcpServers` 片段）。
    pub cursor: Value,
}

/// 生成样例。`token` 非空时补 `headers.Authorization`。
#[must_use]
pub fn config_samples(url: &str, token: Option<&str>) -> ConfigSamples {
    let headers = token
        .filter(|t| !t.trim().is_empty())
        .map(|t| json!({ "Authorization": format!("Bearer {t}") }));

    let mut claude_entry = json!({ "type": "http", "url": url });
    let mut cursor_entry = json!({ "url": url });
    if let Some(headers) = &headers {
        claude_entry["headers"] = headers.clone();
        cursor_entry["headers"] = headers.clone();
    }

    ConfigSamples {
        url: url.to_string(),
        claude_code: json!({ "mcpServers": { "unlucky-launcher": claude_entry } }),
        cursor: json!({ "mcpServers": { "unlucky-launcher": cursor_entry } }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_needs_type_http_and_cursor_does_not() {
        let samples = config_samples("http://127.0.0.1:47823/mcp", None);
        assert_eq!(
            samples.claude_code["mcpServers"]["unlucky-launcher"]["type"],
            json!("http")
        );
        assert_eq!(
            samples.claude_code["mcpServers"]["unlucky-launcher"]["url"],
            json!("http://127.0.0.1:47823/mcp")
        );
        assert!(samples.cursor["mcpServers"]["unlucky-launcher"]
            .get("type")
            .is_none());
        assert_eq!(
            samples.cursor["mcpServers"]["unlucky-launcher"]["url"],
            json!("http://127.0.0.1:47823/mcp")
        );
    }

    #[test]
    fn token_becomes_bearer_header_and_empty_token_is_skipped() {
        let with = config_samples("http://127.0.0.1:1/mcp", Some("s3cret"));
        let header = &with.cursor["mcpServers"]["unlucky-launcher"]["headers"]["Authorization"];
        assert_eq!(header, &json!("Bearer s3cret"));

        let without = config_samples("http://127.0.0.1:1/mcp", Some("  "));
        assert!(without.claude_code["mcpServers"]["unlucky-launcher"]
            .get("headers")
            .is_none());
    }
}
