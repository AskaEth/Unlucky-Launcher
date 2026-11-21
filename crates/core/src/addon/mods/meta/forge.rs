//! `META-INF/mods.toml` / `META-INF/neoforge.mods.toml` 的**手写 TOML 子集**（M312 · MODDUP）。
//!
//! # 为什么不是完整 TOML 解析器
//!
//! 引 `toml` 这个依赖要改 `crates/core/Cargo.toml` 与 `Cargo.lock`，而这两份文件都不在
//! M312 的改动范围内（scope 只到 `src/`）。所以这里只实现 `mods.toml` **真正用到**的那一小块：
//!
//! | 认 | 形状 |
//! |---|---|
//! | 表头 | `[[mods]]` / `[[dependencies.<modid>]]` / `[[dependencies]]` |
//! | 标量 | `key = "串"`（`\\` `\"` `\n` `\t` `\r` `\uXXXX` 转义）/ `key = true\|false` / `key = 裸字面量` |
//! | 注释 | 行尾 `#`（引号里的 `#` 不算） |
//! | 忽略 | `[单括号表]`、多行数组的续行、认不出的行——**跳过，不猜** |
//!
//! 这个子集是**故意窄**的：认不出的行等于没看见，代价是偶尔漏一条前置。反过来（猜）会把
//! 一段不认识的语法当成 id，那才是真误报——报告里一条"缺前置 X"会让用户去删一个没问题的 mod。
//!
//! # 前置的必需位只有一条判据
//!
//! ```text
//! mandatory 键存在 ⇒ 用它
//! 否则            ⇒ type == "required"
//! ```
//!
//! `type = "incompatible" | "discouraged"` 的条目**不是前置**（是冲突声明），直接丢掉——
//! 把它们算成"缺失的前置"是最容易犯的错，`mods.toml` 里这两种恰好写在同一张表里。

use super::{non_empty, version, MetaSource, ModDependency, ModMeta};

/// 解析一份 `mods.toml`（Forge 与 NeoForge 共用这份语法）。
///
/// 没有 `[[mods]]` 表、或其中没有 `modId` ⇒ `None`（这不是我们认识的 Forge 元数据）。
pub(super) fn mods_toml(text: &str) -> Option<ModMeta> {
    let mut section = Section::Other;
    let mut main: Option<Identity> = None;
    let mut deps: Vec<RawDep> = Vec::new();

    for raw in text.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = double_header(line) {
            section = match header {
                "mods" if main.is_none() => Section::Main,
                "dependencies" => {
                    deps.push(RawDep::default());
                    Section::Deps
                }
                other => match other.strip_prefix("dependencies.") {
                    Some(owner) => {
                        deps.push(RawDep {
                            owner: non_empty(owner),
                            ..RawDep::default()
                        });
                        Section::Deps
                    }
                    None => Section::Other,
                },
            };
            if matches!(section, Section::Main) {
                main = Some(Identity::default());
            }
            continue;
        }
        // 单括号表（`[something]`）与多行数组的续行都落到这里：换节 / 跳过。
        if line.starts_with('[') {
            section = Section::Other;
            continue;
        }
        let Some((key, value)) = assignment(line) else {
            continue;
        };
        match &mut section {
            Section::Main => {
                if let Some(draft) = main.as_mut() {
                    apply_identity(draft, key, &value);
                }
            }
            Section::Deps => {
                // 表头那一刻已经 push 过一条，这里把键落在**当前那条**上。
                if let Some(raw) = deps.last_mut() {
                    apply_dep(raw, key, &value);
                }
            }
            Section::Other => {}
        }
    }

    let main = main?;
    let id = main.id.and_then(|id| non_empty(&id))?.to_ascii_lowercase();
    let deps = deps
        .into_iter()
        .filter(|raw| match raw.owner.as_deref() {
            // `[[dependencies]]`（不带 owner 的老写法）与"声明自己的前置"两种都收。
            None => true,
            Some(owner) => owner.eq_ignore_ascii_case(&id),
        })
        .filter_map(dependency)
        .collect();

    Some(ModMeta {
        id,
        name: main.name,
        version: main.version,
        deps,
        source: MetaSource::Forge,
    })
}

/// 当前在哪个表里。
enum Section {
    /// 第一个 `[[mods]]`（身份与前置换取都以它为准）。
    Main,
    /// `[[dependencies(.<modid>)]]`（每遇到一个表头就新起一条 [`RawDep`]，后续键落在那条上）。
    Deps,
    /// 其它一切（含第二个及以后的 `[[mods]]`）。
    Other,
}

/// `[[mods]]` 那一块里我们要的三个字段。
#[derive(Default)]
struct Identity {
    id: Option<String>,
    name: Option<String>,
    version: Option<String>,
}

/// 一条还没定必需位的依赖声明。
#[derive(Default)]
struct RawDep {
    owner: Option<String>,
    id: Option<String>,
    mandatory: Option<bool>,
    kind: Option<String>,
    range: Option<String>,
}

fn apply_identity(draft: &mut Identity, key: &str, value: &Value) {
    match (key, value) {
        ("modId", Value::Str(text)) => draft.id = Some(text.clone()),
        ("displayName", Value::Str(text)) => draft.name = non_empty(text),
        ("version", Value::Str(text)) => draft.version = version(text),
        _ => {}
    }
}

fn apply_dep(draft: &mut RawDep, key: &str, value: &Value) {
    match (key, value) {
        ("modId", Value::Str(text)) => draft.id = Some(text.clone()),
        ("mandatory", Value::Bool(flag)) => draft.mandatory = Some(*flag),
        ("type", Value::Str(text)) => draft.kind = Some(text.clone()),
        ("versionRange", Value::Str(text)) => draft.range = Some(text.clone()),
        _ => {}
    }
}

/// 一条依赖声明 → 一条前置（`None` = 它不是前置：冲突声明 / 没有 id）。
fn dependency(raw: RawDep) -> Option<ModDependency> {
    let id = raw.id.and_then(|id| non_empty(&id))?.to_ascii_lowercase();
    let kind = raw.kind.unwrap_or_default().to_ascii_lowercase();
    // `incompatible` / `discouraged` 是**冲突**声明，不是前置：把它当缺失项报出去等于
    // 让用户去装一个"装了就崩"的 mod。
    if matches!(kind.as_str(), "incompatible" | "discouraged") {
        return None;
    }
    // 必需位的**唯一**判据：显式 `mandatory` 优先，否则看 `type`。
    let mandatory = raw.mandatory.unwrap_or(kind == "required");
    let mut dep = if mandatory {
        ModDependency::required(id)
    } else {
        ModDependency::optional(id)
    };
    if let Some(range) = raw.range.and_then(|range| non_empty(&range)) {
        dep = dep.with_range(range);
    }
    Some(dep)
}

/// 一行的 `键 = 值`（键是裸键；值只认串 / 布尔 / 裸字面量）。
fn assignment(line: &str) -> Option<(&str, Value)> {
    let (key, rest) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim();
    let value = if let Some(body) = rest.strip_prefix('"') {
        Value::Str(quoted(body)?)
    } else if rest.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if rest.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else {
        Value::Bare
    };
    Some((key, value))
}

/// 一个 TOML 标量（`Bare` 只用于"认出来了但不是我们要的键"）。
enum Value {
    /// 带引号的串（已反转义）。
    Str(String),
    /// 布尔。
    Bool(bool),
    /// 裸字面量（数字 / 数组 / 日期……认出来了但不是我们要的键，不取值）。
    Bare,
}

/// 读一个已经去掉开头引号的串，直到收尾的 `"`（不收尾 ⇒ 认不出）。
fn quoted(body: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                // `\uXXXX`：转义本身解不出也没关系，跳过四个十六进制位即可（id 不会用到它）。
                'u' => {
                    for _ in 0..4 {
                        chars.next()?;
                    }
                }
                other => out.push(other),
            },
            other => out.push(other),
        }
    }
    None
}

/// 去掉行尾注释（引号里的 `#` 是正文，不算注释）。
fn strip_comment(line: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        match ch {
            _ if escaped => escaped = false,
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// `[[表名]]` → 表名。
fn double_header(line: &str) -> Option<&str> {
    line.strip_prefix("[[")?.strip_suffix("]]").map(str::trim)
}
