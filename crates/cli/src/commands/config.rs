//! `config get|set`：走 `core::setting` 的 schema API（单一真相源）。
//!
//! # 落点（M40：与数据根解析同源）
//!
//! - **数据根键**（[`DATA_ROOT_KEYS`]：`core.data_mode` / `core.data_custom_root` /
//!   `core.launcher_home` / `core.game_home`）→ 便携目录那份 `launcher.json`
//!   （`<exe 目录>/.ULL/config/launcher.json`）。这正是 `Ctx::resolve` 决定数据根时读的
//!   同一份文件（落点由 `Ctx::setting_path_for` 给出），所以 `config get core.data_mode`
//!   展示的就是真正生效的值，`config set` 也真的影响下一次解析；
//! - 其余键 → 解析后的数据根（`<data>/.ULL/config/launcher.json`）。
//!
//! 之前两条路各看一份文件：`--data-dir` / standard 模式下「改了 `core.data_mode` 没反应、
//! `config get` 还报另一份文件里的值」（finding `…-improve-ull-config-get-set-launcher-json`）。
//!
//! 实例级配置（`config/instances/<id>.json`）留给实例管理 mission，这里不半途实现。
//!
//! **凭据纪律**（D9 / `02-ai-agent-design.md` §3.1）：schema 标 `High` 的敏感键
//! （`ai.api_key` / `download.proxy_url` / `java.extra_args` …）在 read 路径上一律掩码——
//! 单键 get、无键全量 dump、set 的旧值回显**三条**都要掩，`--show-secret` 才放行明文。
//! 判定叠两道：schema 的 `Sensitivity::High` + agent 的 `redact::is_sensitive_key` 词表。

use serde_json::{json, Map, Value};
use unlucky_core::setting::catalog::SettingSchema;
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::spec::Sensitivity;
use unlucky_core::setting::store::{SaveOutcome, SettingStore, SkipReason};

use crate::context::{is_data_root_key, Ctx, DATA_ROOT_KEYS};
use crate::error::CliError;
use crate::output::Report;

/// 敏感值在输出里的占位。
const MASK: &str = "<redacted>";

mod prefs_migration;

use prefs_migration::migrate_update_prefs;

/// `ull config get [<key>] [--show-secret]`。
///
/// # Errors
///
/// 未知键、配置不可读时返回用户错误。
pub fn get(ctx: &Ctx, key: Option<&str>, show_secret: bool) -> Result<Report, CliError> {
    match key {
        Some(key) => get_one(ctx, key, show_secret),
        None => dump(ctx, show_secret),
    }
}

/// 单键 get：读该键自己的落点文件（数据根键走便携目录那份）。
fn get_one(ctx: &Ctx, key: &str, show_secret: bool) -> Result<Report, CliError> {
    // 先跑一次遗留偏好迁移（幂等；没有旧文件时只是一次 stat）。必须在 `load` **之前**：
    // 迁移写的正是这份 `launcher.json`，先读后迁会让这一条输出还是旧值。
    let migration = migrate_update_prefs(ctx);
    let path = ctx.setting_path_for(key);
    let store = SettingStore::load(&path, launcher_schema());
    let schema = store.doc().schema().clone();

    let Some(value) = store.effective(key) else {
        return Err(
            CliError::user("unknown_key", format!("配置键 `{key}` 不在 schema 里"))
                .with_hint("`%prog% config get`（不带键）可以看到全部生效键"),
        );
    };

    let sensitive = is_sensitive(&schema, key);
    let hidden = sensitive && !show_secret;
    let shown = if hidden {
        Value::String(MASK.to_string())
    } else {
        value.clone()
    };
    let problems = render_problems(&store);
    let mut data = json!({
        "key": key,
        "value": shown,
        "redacted": hidden,
        "sensitive": sensitive,
        "path": path.display().to_string(),
        "problems": problems,
    });
    let mut human = vec![format!(
        "{key} = {}{}",
        render_value(&shown),
        if hidden {
            "（敏感键；加 --show-secret 显示明文）"
        } else {
            ""
        }
    )];
    human.push(format!("来自 {}", path.display()));
    if let Some(note) = migration {
        human.push(note.human);
        if let Some(object) = data.as_object_mut() {
            object.insert("migration".to_string(), note.data);
        }
    }
    for problem in &problems {
        human.push(format!("问题：{problem}"));
    }
    human.extend(source_note(ctx, key));
    Ok(Report::new(data, human))
}

/// 无键全量 dump：以数据根那份为底，`core.*` 数据根键换成便携目录那份的生效值。
fn dump(ctx: &Ctx, show_secret: bool) -> Result<Report, CliError> {
    let migration = migrate_update_prefs(ctx);
    let path = ctx.launcher_config();
    let store = SettingStore::load(&path, launcher_schema());
    let schema = store.doc().schema().clone();

    let core_path = ctx
        .portable_launcher_config()
        .filter(|core| core.as_path() != path.as_path());
    let core_store = core_path
        .as_ref()
        .map(|core| SettingStore::load(core, launcher_schema()));

    let mut document = store.doc().to_value();
    if let Some(core) = &core_store {
        // 只叠加**便携目录那份里真有的键**（默认值不落盘，dump 也只展示文件里有的键）；
        // 生效数据根在下面的 `data_root` 里单独给，不靠这里的默认值推。
        for key in DATA_ROOT_KEYS {
            if let Some(value) = core.doc().get(key) {
                overlay(&mut document, key, value.clone());
            }
        }
    }
    let mut problems = render_problems(&store);
    if let Some(core) = &core_store {
        problems.extend(render_problems(core));
    }

    let (document, masked) = if show_secret {
        (document, false)
    } else {
        mask_document(&schema, &document)
    };

    let text = match serde_json::to_string_pretty(&document) {
        Ok(text) => text,
        Err(error) => return Err(CliError::internal("config_serialize", error.to_string())),
    };
    let mut human: Vec<String> = Vec::new();
    human.push(format!("配置 {}（{:?}）", path.display(), store.compat()));
    if let Some(core) = &core_path {
        human.push(format!("数据根键（core.*）读的是 {}", core.display()));
    }
    human.push(format!(
        "数据根：{}（launcher home {}，游戏目录 {}）",
        ctx.data.mode.as_str(),
        ctx.data.launcher_home.display(),
        ctx.data.game_home.display()
    ));
    human.extend(text.lines().map(str::to_string));
    if let Some(note) = &migration {
        human.push(note.human.clone());
    }
    for problem in &problems {
        human.push(format!("问题：{problem}"));
    }
    if masked {
        human.push("（敏感值已掩码；加 --show-secret 显示明文）".to_string());
    }
    let mut data = json!({
        "path": path.display().to_string(),
        "core_path": core_path.as_ref().map(|core| core.display().to_string()),
        "config": document,
        "redacted": masked,
        "problems": problems,
        "data_root": {
            "mode": ctx.data.mode.as_str(),
            "launcher_home": ctx.data.launcher_home.display().to_string(),
            "game_home": ctx.data.game_home.display().to_string(),
        },
    });
    if let Some(note) = migration {
        if let Some(object) = data.as_object_mut() {
            object.insert("migration".to_string(), note.data);
        }
    }
    Ok(Report::new(data, human))
}

/// `ull config set <key> <value>`。
///
/// 敏感键的**旧值回显要掩码**（新值是用户刚敲进来的，照常回显）。
///
/// # Errors
///
/// 键不在 schema、值不合法、文件只读/不可读、写盘失败时返回用户错误。
pub fn set(ctx: &Ctx, key: &str, raw: &str) -> Result<Report, CliError> {
    // 同一份遗留偏好先迁进来：否则用户「set 一个、get 另一个」会看到两份真相。
    let migration = migrate_update_prefs(ctx);
    let path = ctx.setting_path_for(key);
    let mut store = SettingStore::load(&path, launcher_schema());
    let sensitive = is_sensitive(store.doc().schema(), key);
    let value = parse_value(raw);
    let old = store.effective(key).cloned().map(|old| {
        if sensitive {
            Value::String(MASK.to_string())
        } else {
            old
        }
    });
    store.set(key, value.clone()).map_err(|error| {
        CliError::user("config_set", error.to_string())
            .with_hint(format!("键与取值范围见 {}", path.display()))
    })?;

    let outcome = store.save().map_err(|error| {
        CliError::user(
            "config_save",
            format!("写 {} 失败：{error}", path.display()),
        )
    })?;
    match &outcome {
        SaveOutcome::Saved(saved) => {
            let mut data = json!({
                "key": key,
                "old": old,
                "new": value,
                "saved": saved.display().to_string(),
            });
            let mut human = vec![
                format!(
                    "{key} = {}（原值 {}）",
                    render_value(&value),
                    render_value(&old.unwrap_or(Value::Null))
                ),
                format!("已保存：{}", saved.display()),
            ];
            if let Some(note) = migration {
                human.push(note.human);
                if let Some(object) = data.as_object_mut() {
                    object.insert("migration".to_string(), note.data);
                }
            }
            human.extend(source_note(ctx, key));
            Ok(Report::new(data, human))
        }
        SaveOutcome::Skipped(SkipReason::NotDirty) => {
            let mut human = vec![format!("{key} 已经是该值，未写盘")];
            if let Some(note) = &migration {
                human.push(note.human.clone());
            }
            Ok(Report::new(
                json!({ "key": key, "new": value, "saved": null }),
                human,
            ))
        }
        SaveOutcome::Skipped(reason) => {
            let error = CliError::user(
                "config_read_only",
                format!("{} 当前不可写（{}）", path.display(), reason.as_str()),
            );
            // 不可写常常不是权限问题，而是文件内容让加载降级（`$schema` 认不出 / 版本更高）。
            let problems = render_problems(&store);
            Err(if problems.is_empty() {
                error.with_hint("修好文件权限/内容后重试")
            } else {
                error.with_hint(problems.join("；"))
            })
        }
    }
}

/// 数据根键的落点说明（其余键不需要：它们的落点就是数据根本身）。
fn source_note(ctx: &Ctx, key: &str) -> Option<String> {
    if !is_data_root_key(key) {
        return None;
    }
    if ctx.data_dir_forced() {
        return Some(
            "本次由 --data-dir 指定数据根：该键在下次不带 --data-dir 运行时生效".to_string(),
        );
    }
    if ctx.setting_path_for(key) != ctx.launcher_config() {
        return Some("该键决定数据根，落点是便携目录那份（与数据根解析同源）".to_string());
    }
    None
}

/// 加载问题 → 人读一行（`problems` 通道与人类可读输出共用）。
fn render_problems(store: &SettingStore) -> Vec<String> {
    store.problems().iter().map(ToString::to_string).collect()
}

/// 把 `key` 的生效值写进 dump 文档（点分键 → 嵌套对象）。
fn overlay(root: &mut Value, key: &str, value: Value) {
    let mut segments = key.split('.').filter(|part| !part.is_empty());
    let Some(head) = segments.next() else {
        return;
    };
    let rest: Vec<&str> = segments.collect();
    let Some(map) = root.as_object_mut() else {
        return;
    };
    if rest.is_empty() {
        map.insert(head.to_string(), value);
        return;
    }
    overlay(
        map.entry(head.to_string())
            .or_insert_with(|| Value::Object(Map::new())),
        &rest.join("."),
        value,
    );
}

/// 值先按 JSON 解析（数字/布尔/对象），失败按字符串。
fn parse_value(raw: &str) -> Value {
    serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// 敏感键：schema 标 `High`，或键名本身命中 agent 的敏感词表（双保险）。
fn is_sensitive(schema: &SettingSchema, key: &str) -> bool {
    matches!(schema.sensitivity(key), Some(Sensitivity::High))
        || unlucky_agent::redact::is_sensitive_key(key)
}

/// 递归掩码渲染后的配置文档，返回（掩码后的文档，是否掩过）。
///
/// `to_value()` 是**嵌套对象**（点分键展开成层级），所以按路径下钻；
/// 命中敏感路径时整棵子树一起掩码，数组元素沿用父路径继续下钻。
fn mask_document(schema: &SettingSchema, document: &Value) -> (Value, bool) {
    mask_at(schema, "", document)
}

fn mask_at(schema: &SettingSchema, path: &str, value: &Value) -> (Value, bool) {
    if !path.is_empty() && is_sensitive(schema, path) {
        return (Value::String(MASK.to_string()), true);
    }
    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len());
            let mut masked = false;
            for (key, child) in map {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let (child, child_masked) = mask_at(schema, &child_path, child);
                masked |= child_masked;
                out.insert(key.clone(), child);
            }
            (Value::Object(out), masked)
        }
        Value::Array(items) => {
            let mut masked = false;
            let redacted: Vec<Value> = items
                .iter()
                .map(|item| {
                    let (item, item_masked) = mask_at(schema, path, item);
                    masked |= item_masked;
                    item
                })
                .collect();
            (Value::Array(redacted), masked)
        }
        other => (other.clone(), false),
    }
}

fn render_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests;
