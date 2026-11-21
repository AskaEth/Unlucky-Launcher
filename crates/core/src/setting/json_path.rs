//! 点分键（`download.concurrency`）与嵌套 JSON 之间的最小工具集。
//!
//! 单独成文件：这些函数是 [`super::document`] 的机械部件，和「配置语义」无关，
//! 也是容易出边界 bug（空路径、中间层不是对象）的地方。

use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::catalog::SettingSchema;
use super::document::LoadProblem;
use super::schema::{SCHEMA_KEY, SCHEMA_VERSION_KEY};

/// 点分键 → 路径段。
pub(crate) fn segments(key: &str) -> Vec<&str> {
    key.split('.').filter(|part| !part.is_empty()).collect()
}

/// 沿路径取嵌套值。
pub(crate) fn lookup<'a>(root: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let (head, tail) = path.split_first()?;
    let value = root.get(*head)?;
    if tail.is_empty() {
        Some(value)
    } else {
        lookup(value.as_object()?, tail)
    }
}

/// 沿路径写嵌套值（中间层不是对象则重建）。
pub(crate) fn insert_at(root: &mut Map<String, Value>, path: &[&str], value: Value) {
    let Some((head, tail)) = path.split_first() else {
        return;
    };
    if tail.is_empty() {
        root.insert((*head).to_owned(), value);
        return;
    }
    let entry = root
        .entry((*head).to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(Map::new());
    }
    if let Some(child) = entry.as_object_mut() {
        insert_at(child, tail, value);
    }
}

/// 深合并（`extra` 覆盖 `target` 的同名叶子）。
pub(crate) fn merge_maps(target: &mut Map<String, Value>, extra: &Map<String, Value>) {
    for (key, value) in extra {
        if let Some(Value::Object(existing)) = target.get_mut(key) {
            if let Some(incoming) = value.as_object() {
                merge_maps(existing, incoming);
                continue;
            }
        }
        target.insert(key.clone(), value.clone());
    }
}

/// 收集 schema 不认识的键（整棵保留），并把形状问题记进 `problems`。
pub(crate) fn collect_unknown(
    prefix: &[&str],
    node: &Map<String, Value>,
    schema: &SettingSchema,
    problems: &mut Vec<LoadProblem>,
) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in node {
        if prefix.is_empty() && (key == SCHEMA_KEY || key == SCHEMA_VERSION_KEY) {
            continue;
        }
        let mut path = prefix.to_vec();
        path.push(key.as_str());
        let dotted = path.join(".");
        if schema.is_known(&dotted) {
            continue;
        }
        if schema.has_prefix(&path) {
            match value.as_object() {
                Some(child) => {
                    let nested = collect_unknown(&path, child, schema, problems);
                    if !nested.is_empty() {
                        out.insert(key.clone(), Value::Object(nested));
                    }
                }
                None => problems.push(LoadProblem::InvalidShape {
                    key: path.join("."),
                    reason: "期望对象（其下还有已知键）".to_owned(),
                }),
            }
            continue;
        }
        problems.push(LoadProblem::UnknownKey {
            key: path.join("."),
        });
        out.insert(key.clone(), value.clone());
    }
    out
}

/// 把嵌套对象摊平成点分叶子集合。
pub(crate) fn flatten(map: &Map<String, Value>) -> BTreeSet<String> {
    fn walk(prefix: &str, node: &Map<String, Value>, out: &mut BTreeSet<String>) {
        for (key, value) in node {
            let dotted = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{prefix}.{key}")
            };
            match value.as_object() {
                Some(child) if !child.is_empty() => walk(&dotted, child, out),
                _ => {
                    out.insert(dotted);
                }
            }
        }
    }
    let mut out = BTreeSet::new();
    walk("", map, &mut out);
    out
}
