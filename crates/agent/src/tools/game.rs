//! 实例 / 版本 / Java 三个只读工具（M39 任务 3 的读侧）。
//!
//! 全部只读、全部走 [`crate::port::GamePort`]：agent 不自己去翻 `versions/` 目录，
//! 也不自己扫 JDK——那两件事的真相源在 M14（game）与 M33（java），重复一份必然漂。
//!
//! 输出形状刻意做成「摘要 + 上限」：实例可能几十个、Java 候选可能十几个，
//! 一次全塞给模型等于烧 token（02 §3.2 的「单次返回上限防 token 爆炸」同理）。

use std::sync::Arc;

use serde_json::{json, Value};

use crate::port::GamePort;
use crate::registry::schema::{integer, object};
use crate::registry::{
    RegistryError, ToolCtx, ToolError, ToolFn, ToolOutput, ToolRegistry, ToolSpec,
};

/// 实例列表工具名。
pub const LIST_INSTANCES: &str = "list_instances";
/// 版本列表工具名。
pub const LIST_VERSIONS: &str = "list_versions";
/// Java 现状工具名。
pub const GET_JAVA_STATUS: &str = "get_java_status";
/// 单次最多返回多少个实例/版本。
pub const MAX_ROWS: usize = 50;

/// 注册三个只读工具。
///
/// # Errors
/// 工具名重复/schema 非法时返回 [`RegistryError`]。
pub fn register(registry: &mut ToolRegistry, port: Arc<dyn GamePort>) -> Result<(), RegistryError> {
    registry.register(list_instances_spec(Arc::clone(&port)))?;
    registry.register(list_versions_spec(Arc::clone(&port)))?;
    registry.register(java_status_spec(port))
}

fn limit_arg(args: &Value) -> usize {
    args.get("limit")
        .and_then(Value::as_u64)
        .map_or(MAX_ROWS, |n| usize::try_from(n).unwrap_or(MAX_ROWS))
        .clamp(1, MAX_ROWS)
}

fn limit_schema() -> Value {
    integer(
        &format!("最多返回多少条（1~{MAX_ROWS}，默认 {MAX_ROWS}）"),
        1,
        MAX_ROWS as i64,
    )
}

fn list_instances_spec(port: Arc<dyn GamePort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let limit = limit_arg(args);
        let mut instances = port.list_instances().map_err(ToolError::Port)?;
        let total = instances.len();
        instances.truncate(limit);
        Ok(ToolOutput::ok(
            json!({
                "total": total,
                "shown": instances.len(),
                "truncated": total > instances.len(),
                "instances": instances,
            }),
            format!("列出 {}/{} 个实例", instances.len(), total),
        ))
    });

    ToolSpec::read_only(
        LIST_INSTANCES,
        "列出启动器里的游戏实例（id / 名称 / 版本 id / 载荷是否已安装 / 独立游戏目录 / 实例级 Java）。要看某个实例的日志先用 list_game_sessions。",
        object(&[("limit", limit_schema())], &[]),
        handler,
    )
}

fn list_versions_spec(port: Arc<dyn GamePort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, args: &Value| {
        let limit = limit_arg(args);
        let mut versions = port.list_versions().map_err(ToolError::Port)?;
        let total = versions.len();
        versions.truncate(limit);
        Ok(ToolOutput::ok(
            json!({
                "total": total,
                "shown": versions.len(),
                "truncated": total > versions.len(),
                "versions": versions,
            }),
            format!("列出 {}/{} 个已安装版本", versions.len(), total),
        ))
    });

    ToolSpec::read_only(
        LIST_VERSIONS,
        "列出已安装的游戏版本（`versions/<id>/` 下真实存在的载荷），带类型与被几个实例引用。判断「实例的载荷缺没缺」时配合 list_instances 的 version_present 一起看。",
        object(&[("limit", limit_schema())], &[]),
        handler,
    )
}

fn java_status_spec(port: Arc<dyn GamePort>) -> ToolSpec {
    let handler: ToolFn = Arc::new(move |_ctx: &ToolCtx, _args: &Value| {
        let status = port.java_status().map_err(ToolError::Port)?;
        let majors: Vec<String> = status
            .candidates
            .iter()
            .filter_map(|c| c.major.map(|m| m.to_string()))
            .collect();
        Ok(ToolOutput::ok(
            json!({
                "default_memory_mb": status.default_memory_mb,
                "managed_roots": status.managed_roots,
                "candidate_count": status.candidates.len(),
                "majors": majors,
                "candidates": status.candidates,
            }),
            format!(
                "Java 候选 {} 个（主版本 {}）",
                status.candidates.len(),
                if majors.is_empty() {
                    "无".to_string()
                } else {
                    majors.join("/")
                }
            ),
        ))
    });

    ToolSpec::read_only(
        GET_JAVA_STATUS,
        "读取 Java 现状：托管扫描根、默认堆内存、探测到的全部运行时（路径 / 版本 / 主版本 / 发行方 / 来源）。启动报「不支持的 class 版本」「找不到 Java」时先看这里。",
        object(&[], &[]),
        handler,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::MockGame;

    fn registry() -> ToolRegistry {
        let mut registry = ToolRegistry::new();
        register(&mut registry, Arc::new(MockGame::new())).expect("注册应成功");
        registry
    }

    fn call(registry: &ToolRegistry, name: &str, args: Value) -> Value {
        let spec = registry.get(name).expect("工具存在").clone();
        let ctx = ToolCtx {
            agent_session_id: "s1".to_string(),
            confirmed: false,
        };
        (spec.handler)(&ctx, &args).expect("应成功").content
    }

    #[test]
    fn lists_instances_with_limit() {
        let content = call(&registry(), LIST_INSTANCES, json!({ "limit": 1 }));
        assert_eq!(content["total"], json!(1));
        assert_eq!(content["instances"][0]["id"], json!("inst-1"));
        assert_eq!(content["truncated"], json!(false));
    }

    #[test]
    fn lists_versions() {
        let content = call(&registry(), LIST_VERSIONS, json!({}));
        assert_eq!(content["total"], json!(2));
        assert_eq!(content["versions"][0]["id"], json!("1.20.1-forge-47.2.20"));
    }

    #[test]
    fn java_status_summarises_majors() {
        let content = call(&registry(), GET_JAVA_STATUS, json!({}));
        assert_eq!(content["candidate_count"], json!(2));
        assert_eq!(content["majors"], json!(["17", "21"]));
        assert_eq!(content["default_memory_mb"], json!(2048));
    }

    #[test]
    fn port_errors_are_passed_through() {
        let mut registry = ToolRegistry::new();
        register(
            &mut registry,
            Arc::new(MockGame::failing("versions 目录不可读")),
        )
        .expect("注册应成功");
        let spec = registry.get(LIST_VERSIONS).expect("工具存在").clone();
        let ctx = ToolCtx {
            agent_session_id: "s1".to_string(),
            confirmed: false,
        };
        let err = (spec.handler)(&ctx, &json!({})).unwrap_err();
        assert_eq!(err.code(), "port_error");
        assert!(err.message().contains("versions 目录不可读"));
    }
}
