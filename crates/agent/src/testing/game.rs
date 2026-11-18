//! 实例 / 版本 / Java / 账户四个只读端口的测试替身。

use crate::port::game::{
    AccountPort, AccountSummary, GamePort, InstanceSummary, JavaRuntimeSummary, JavaStatus,
    VersionSummary,
};
use crate::port::PortResult;

/// 内存游戏端口：一个 1.20.1 Forge 实例 + 两个已安装版本 + 两个 Java 候选。
#[derive(Debug, Clone)]
pub struct MockGame {
    instances: Vec<InstanceSummary>,
    versions: Vec<VersionSummary>,
    java: JavaStatus,
    fail: Option<String>,
}

impl Default for MockGame {
    fn default() -> Self {
        Self::new()
    }
}

impl MockGame {
    /// 默认假数据（够跑通 `list_instances` / `list_versions` / `get_java_status`）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            instances: vec![InstanceSummary {
                id: "inst-1".to_string(),
                name: "1.20.1 Forge".to_string(),
                version_id: "1.20.1-forge-47.2.20".to_string(),
                version_present: true,
                game_dir: None,
                java_path: Some(r"D:\jdk17\bin\java.exe".to_string()),
                last_played_at: Some(1_793_304_753_123),
            }],
            versions: vec![
                VersionSummary {
                    id: "1.20.1-forge-47.2.20".to_string(),
                    kind: Some("release".to_string()),
                    instance_count: 1,
                },
                VersionSummary {
                    id: "1.21.1".to_string(),
                    kind: Some("release".to_string()),
                    instance_count: 0,
                },
            ],
            java: JavaStatus {
                default_memory_mb: Some(2048),
                managed_roots: vec![r"D:\ULL\.ULL\data\java-managed".to_string()],
                candidates: vec![
                    JavaRuntimeSummary {
                        binary: r"D:\ULL\.ULL\data\java-managed\jdk-17-temurin\bin\java.exe"
                            .to_string(),
                        version: Some("17.0.13".to_string()),
                        major: Some(17),
                        vendor: Some("temurin".to_string()),
                        origin: "managed".to_string(),
                    },
                    JavaRuntimeSummary {
                        binary: "/usr/bin/java".to_string(),
                        version: Some("21.0.2".to_string()),
                        major: Some(21),
                        vendor: None,
                        origin: "PATH".to_string(),
                    },
                ],
            },
            fail: None,
        }
    }

    /// 让三个查询全部返回同一个错误（测端口错误透传）。
    #[must_use]
    pub fn failing(message: &str) -> Self {
        Self {
            fail: Some(message.to_string()),
            ..Self::new()
        }
    }

    fn fail(&self) -> Option<String> {
        self.fail.clone()
    }
}

impl GamePort for MockGame {
    fn list_instances(&self) -> PortResult<Vec<InstanceSummary>> {
        self.fail().map_or_else(|| Ok(self.instances.clone()), Err)
    }

    fn list_versions(&self) -> PortResult<Vec<VersionSummary>> {
        self.fail().map_or_else(|| Ok(self.versions.clone()), Err)
    }

    fn java_status(&self) -> PortResult<JavaStatus> {
        self.fail().map_or_else(|| Ok(self.java.clone()), Err)
    }
}

/// 内存账户端口：一个离线账户。**只有公开字段**，凭据无处可放。
#[derive(Debug, Clone)]
pub struct MockAccounts {
    accounts: Vec<AccountSummary>,
    fail: Option<String>,
}

impl Default for MockAccounts {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAccounts {
    /// 一个离线账户（Steve）。
    #[must_use]
    pub fn new() -> Self {
        Self {
            accounts: vec![AccountSummary {
                id: "acc-1".to_string(),
                kind: "offline".to_string(),
                profile_id: "8667ba71b85a4004af54457a9734eed7".to_string(),
                profile_name: "Steve".to_string(),
                login_name: None,
                created_at_ms: 1_793_304_753_123,
            }],
            fail: None,
        }
    }

    /// 空账户列表（测「没有账户」的提示文案）。
    #[must_use]
    pub fn empty() -> Self {
        Self {
            accounts: Vec::new(),
            fail: None,
        }
    }

    /// 让查询失败（测端口错误透传）。
    #[must_use]
    pub fn failing(message: &str) -> Self {
        Self {
            fail: Some(message.to_string()),
            ..Self::new()
        }
    }
}

impl AccountPort for MockAccounts {
    fn list_accounts(&self) -> PortResult<Vec<AccountSummary>> {
        self.fail
            .clone()
            .map_or_else(|| Ok(self.accounts.clone()), Err)
    }
}
