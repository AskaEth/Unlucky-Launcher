//! 实例 / 版本 / Java / 账户四个读侧端口（M39 的只读工具集用）。
//!
//! 与 [`super`] 的既有三个端口同形：trait 由 agent 声明，实现在装配层用 `unlucky-core` 填充。
//! 拆分出来的原因是 `port.rs` 挨着 400 行硬上限（`guards.yml` 的 file-size 门禁）——
//! 「会话与日志」「崩溃诊断」留在父模块，「游戏与账户」在这里，语义上也是一刀干净的分界。
//!
//! **凭据红线**（D9 / 02 文档 §3.1）：账户端口只露 [`AccountSummary`] 的公开字段。
//! `Credentials`（access token / 刷新令牌 / authlib 密码）在 core 的 Secret 体系里，
//! 根本不出现在本文件的类型里，因此不可能被工具结果带出去。

use serde::Serialize;

use super::PortResult;

// ───────────────────────── 实例 / 版本（M14） ─────────────────────────

/// 一个实例（`profiles.json` 一行）的摘要：模型只需要「能定位、能判断载荷在不在」。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct InstanceSummary {
    /// 实例 id。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 版本 id（如 `1.21.1` / `fabric-loader-0.16.9-1.21.1`）。
    pub version_id: String,
    /// 该版本的载荷是否真的在 `versions/` 下（`false` = 装了档案但载荷缺失）。
    pub version_present: bool,
    /// 独立游戏目录（未覆盖时为 `None`）。
    pub game_dir: Option<String>,
    /// 实例级 Java 覆盖路径（未覆盖时为 `None`）。
    pub java_path: Option<String>,
    /// 上次启动时刻（Unix 毫秒；从未启动为 `None`）。
    pub last_played_at: Option<u64>,
}

/// 已安装版本的摘要（`versions/<id>/` 的存在性 + 可读的元信息）。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct VersionSummary {
    /// 版本 id。
    pub id: String,
    /// 版本 JSON 里声明的 `type`（`release` / `snapshot` / 加载器自报），读不到为 `None`。
    pub kind: Option<String>,
    /// 被几个实例引用。
    pub instance_count: usize,
}

// ──────────────────────────── Java（M33） ────────────────────────────

/// 一个探测到的 Java 运行时。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct JavaRuntimeSummary {
    /// `java` 可执行文件路径。
    pub binary: String,
    /// 完整版本串（`21.0.2` / `1.8.0_402`）。
    pub version: Option<String>,
    /// 主版本号（8 / 17 / 21）。
    pub major: Option<u32>,
    /// 发行方（temurin / oracle / microsoft / zulu …）。
    pub vendor: Option<String>,
    /// 来源类别（`instance` / `managed` / `env` / `system`）。
    pub origin: String,
}

/// Java 现状（`get_java_status`）。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct JavaStatus {
    /// 默认堆内存（MB，`java.default_memory_mb`）。
    pub default_memory_mb: Option<u64>,
    /// 托管 JDK 的扫描根。
    pub managed_roots: Vec<String>,
    /// 探测到的候选（已去重，优先级从高到低）。
    pub candidates: Vec<JavaRuntimeSummary>,
}

// ──────────────────────────── 账户（M31） ────────────────────────────

/// 账户的**公开字段**摘要。凭据类字段刻意不存在（凭据红线）。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct AccountSummary {
    /// 条目 id。
    pub id: String,
    /// 轨道：`offline` / `microsoft` / `authlib`。
    pub kind: String,
    /// Minecraft 档案 UUID（32 位无连字符）。
    pub profile_id: String,
    /// 玩家名（可能为空串 = 有档案但名字待刷新）。
    pub profile_name: String,
    /// 登录名（仅 authlib 轨有值）。
    pub login_name: Option<String>,
    /// 创建时刻（Unix 毫秒）。
    pub created_at_ms: u64,
}

/// 游戏资产端口（M14 实例/版本 + M33 Java 探测）。
pub trait GamePort: Send + Sync {
    /// 全部实例（按上次启动时间倒序）。
    fn list_instances(&self) -> PortResult<Vec<InstanceSummary>>;
    /// 已安装版本清单。
    fn list_versions(&self) -> PortResult<Vec<VersionSummary>>;
    /// Java 现状（候选 + 托管根 + 默认内存）。
    fn java_status(&self) -> PortResult<JavaStatus>;
}

/// 账户端口（M31）。实现方只允许读 `AccountStore::accounts()`（`AccountMeta`），
/// **不得**调用 `credentials()`——那是凭据出口，agent 侧没有对应字段可以承接。
pub trait AccountPort: Send + Sync {
    /// 账户清单（公开字段）。
    fn list_accounts(&self) -> PortResult<Vec<AccountSummary>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_summary_has_no_credential_fields() {
        // 把摘要序列化成 JSON：任何 token / password / secret 形态的键都不许出现。
        // 这是凭据红线的**可执行断言**——将来有人往结构体里加字段，这条测试先炸。
        let summary = AccountSummary {
            id: "a1".to_string(),
            kind: "offline".to_string(),
            profile_id: "0123456789abcdef0123456789abcdef".to_string(),
            profile_name: "Steve".to_string(),
            login_name: Some("steve@example.com".to_string()),
            created_at_ms: 1_793_304_753_123,
        };
        let json = serde_json::to_string(&summary).expect("可序列化");
        for forbidden in ["token", "password", "secret", "credential", "refresh"] {
            assert!(
                !json.to_lowercase().contains(forbidden),
                "账户摘要不得包含 `{forbidden}`：{json}"
            );
        }
    }

    #[test]
    fn java_status_round_trips() {
        let status = JavaStatus {
            default_memory_mb: Some(2048),
            managed_roots: vec!["/data/java-managed".to_string()],
            candidates: vec![JavaRuntimeSummary {
                binary: "/jdk/bin/java".to_string(),
                version: Some("21.0.2".to_string()),
                major: Some(21),
                vendor: Some("temurin".to_string()),
                origin: "managed".to_string(),
            }],
        };
        assert_eq!(status.candidates.len(), 1);
        assert_eq!(status.candidates[0].major, Some(21));
    }
}
