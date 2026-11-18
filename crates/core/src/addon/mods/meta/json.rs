//! JSON 那两份元数据的解析：`fabric.mod.json` 与 `mcmod.info`（M312 · MODDUP）。
//!
//! 两个函数的契约与 [`super::read`] 一致：**认不出就给 `None`**，绝不 panic、绝不猜。
//! 字段读不全（比如没有 `name`）不算认不出——id 才是身份，其余都可以缺。

use serde_json::Value;

use super::{non_empty, version, MetaSource, ModDependency, ModMeta};

/// `fabric.mod.json`。
///
/// - 身份：`id`（必需，丢了就算认不出）+ `name` / `version`（可缺）；
/// - 前置：`depends` = **必需**，`recommends` = **可选**（Fabric 的官方口径）；
/// - `suggests` 太弱（作者自己都说"随便"）、`breaks` 不是前置（是冲突），两者都不收。
pub(super) fn fabric(text: &str) -> Option<ModMeta> {
    let root: Value = serde_json::from_str(text).ok()?;
    let object = root.as_object()?;
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .and_then(non_empty)?
        .to_ascii_lowercase();

    let mut deps = Vec::new();
    push_deps(object.get("depends"), true, &mut deps);
    push_deps(object.get("recommends"), false, &mut deps);

    Some(ModMeta {
        id,
        name: object
            .get("name")
            .and_then(Value::as_str)
            .and_then(non_empty),
        version: object
            .get("version")
            .and_then(Value::as_str)
            .and_then(version),
        deps,
        source: MetaSource::Fabric,
    })
}

/// 旧版 FML 的 `mcmod.info`（顶层是数组，或 `{"modList": [...]}` 两种都见过）。
///
/// 前置的两栏语义是 FML 自己的：`requiredMods` = 硬前置；`dependencies` = 软依赖，
/// 但**带 `@` 前缀**的条目例外——那是"其实必需"的老写法（`"@othermod"`）。
pub(super) fn legacy(text: &str) -> Option<ModMeta> {
    let root: Value = serde_json::from_str(text).ok()?;
    let list = match &root {
        Value::Array(items) => items,
        Value::Object(map) => map.get("modList")?.as_array()?,
        _ => return None,
    };
    let first = list.first()?.as_object()?;
    let id = first
        .get("modid")
        .or_else(|| first.get("modId"))
        .and_then(Value::as_str)
        .and_then(non_empty)?
        .to_ascii_lowercase();

    let mut deps = Vec::new();
    for spec in strings(first.get("requiredMods")) {
        push_legacy(&spec, true, &mut deps);
    }
    for spec in strings(first.get("dependencies")) {
        push_legacy(&spec, false, &mut deps);
    }

    Some(ModMeta {
        id,
        name: first
            .get("name")
            .and_then(Value::as_str)
            .and_then(non_empty),
        version: first
            .get("version")
            .and_then(Value::as_str)
            .and_then(version),
        deps,
        source: MetaSource::Legacy,
    })
}

/// `{"id": ">=1.0"}` 这种表：键是前置 id，值是版本范围（串或串数组）。
fn push_deps(value: Option<&Value>, mandatory: bool, out: &mut Vec<ModDependency>) {
    let Some(object) = value.and_then(Value::as_object) else {
        return;
    };
    for (id, spec) in object {
        let Some(id) = non_empty(id).map(|id| id.to_ascii_lowercase()) else {
            continue;
        };
        let mut dep = if mandatory {
            ModDependency::required(id)
        } else {
            ModDependency::optional(id)
        };
        if let Some(range) = range_of(spec) {
            dep = dep.with_range(range);
        }
        out.push(dep);
    }
}

/// 版本范围的原文（数组取拼接，方便人读；本仓不解表达式）。
fn range_of(spec: &Value) -> Option<String> {
    match spec {
        Value::String(text) => non_empty(text),
        Value::Array(items) => {
            let parts = items.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join(", "))
        }
        _ => None,
    }
}

/// `"othermod@1.0"` / `"@othermod"` / `"othermod@[1.0,2.0)"` 三种形状。
fn push_legacy(spec: &str, mandatory: bool, out: &mut Vec<ModDependency>) {
    let spec = spec.trim();
    let (mandatory, body) = match spec.strip_prefix('@') {
        Some(rest) => (true, rest.trim()),
        None => (mandatory, spec),
    };
    let (id, range) = match body.split_once('@') {
        Some((id, range)) => (id.trim(), Some(range.trim())),
        None => (body, None),
    };
    let Some(id) = non_empty(id).map(|id| id.to_ascii_lowercase()) else {
        return;
    };
    let mut dep = if mandatory {
        ModDependency::required(id)
    } else {
        ModDependency::optional(id)
    };
    if let Some(range) = range.and_then(non_empty) {
        dep = dep.with_range(range);
    }
    out.push(dep);
}

/// 一个 JSON 值里的字符串数组（非字符串条目直接跳过）。
fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
