//! install profile 的 `processors[]` 与 `data` 表：**纯解析，不动作**。
//!
//! 字段含义照 Forge installer spec（与 HMCL `ForgeNewInstallProfile.Processor/Datum` 一致）：
//!
//! ```json
//! "processors": [{
//!   "sides": ["client"],                                   // 缺省 = 两侧都跑
//!   "jar": "net.minecraftforge:installertools:1.3.0",      // 可执行 jar（主类在 MANIFEST 里）
//!   "classpath": ["net.minecraftforge:jarsplitter:1.1.4"], // 依赖 jar（jar 自己不在里面）
//!   "args": ["--task", "MCP_DATA", "--output", "{MAPPINGS}"],
//!   "outputs": {"{MAPPINGS}": "ab12…（期望 SHA-1）"}
//! }],
//! "data": {"MAPPINGS": {"client": "[de.oceanlabs.mcp:mcp_config:1.20.1@zip]", "server": "…"}}
//! ```
//!
//! 三条容易看错的点：
//!
//! 1. `data` 的每条是**按侧**的（`client` / `server`），client 侧只取 `.client`；
//! 2. 官方只写 `jar`，**不写主类**——主类在 jar 的 `META-INF/MANIFEST.MF`（`Main-Class`）；
//!    少数 fork 会额外给 `mainClass`，所以这里留了口子；
//! 3. `classpath` **不含** `jar` 自己（Forge 的约定），拼 `-cp` 时要补上。

use std::collections::BTreeMap;

use serde::Deserialize;

/// install profile 里的一个 processor。
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Processor {
    /// 生效侧（`client` / `server`；空表示两侧都要）。
    #[serde(default)]
    pub sides: Vec<String>,
    /// 可执行 jar 的 maven 坐标。
    #[serde(default)]
    pub jar: Option<String>,
    /// 主类覆盖（官方 profile 不写，留给少数 fork）。
    #[serde(default, rename = "mainClass")]
    pub main_class: Option<String>,
    /// 依赖 jar 的 maven 坐标（**不含** `jar` 自己）。
    #[serde(default)]
    pub classpath: Vec<String>,
    /// 命令行参数（可含 `{VAR}` / `[坐标]` / `'字面量'` 三种占位）。
    #[serde(default)]
    pub args: Vec<String>,
    /// 产物表：路径占位 → 期望 SHA-1（既有产物可用于幂等跳过）。
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
}

impl Processor {
    /// 是否在 client 侧生效。
    #[must_use]
    pub fn applies_to_client(&self) -> bool {
        self.sides.is_empty() || self.sides.iter().any(|side| side == "client")
    }

    /// `jar` 坐标（去掉首尾空白；缺失或空串返回 `None`）。
    #[must_use]
    pub fn jar_coordinate(&self) -> Option<&str> {
        self.jar
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    /// 主类覆盖（缺失或空串返回 `None`）。
    #[must_use]
    pub fn main_class(&self) -> Option<&str> {
        self.main_class
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

/// `data` 表的一条：按侧给值（client 侧只用 `.client`）。
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct Datum {
    /// client 侧的值（`[坐标]` / `'字面量'` / installer 内的文件路径）。
    #[serde(default)]
    pub client: Option<String>,
    /// server 侧的值（本期不跑 server 安装，只解析不动作）。
    #[serde(default)]
    pub server: Option<String>,
}

impl Datum {
    /// client 侧的值（空串视为没有）。
    #[must_use]
    pub fn client_value(&self) -> Option<&str> {
        self.client
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::Processor;

    #[test]
    fn sides_empty_means_both_and_client_subset_is_recognised() {
        let both: Processor = serde_json::from_str(r#"{"jar": "g:a:1"}"#).expect("应可解析");
        assert!(both.applies_to_client());
        let server: Processor =
            serde_json::from_str(r#"{"sides": ["server"], "jar": "g:a:1"}"#).expect("应可解析");
        assert!(!server.applies_to_client());
        let client: Processor =
            serde_json::from_str(r#"{"sides": ["client"], "jar": "g:a:1"}"#).expect("应可解析");
        assert!(client.applies_to_client());
    }

    #[test]
    fn blank_jar_and_main_class_are_treated_as_absent() {
        let processor: Processor =
            serde_json::from_str(r#"{"jar": " ", "mainClass": ""}"#).expect("应可解析");
        assert!(processor.jar_coordinate().is_none());
        assert!(processor.main_class().is_none());
    }
}
