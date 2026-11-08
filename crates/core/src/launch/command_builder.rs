//! `CommandBuilder`：命令行组装器与“默认值抑制”语义。
//!
//! 平移自 `analyse/02` §4.2（`CommandBuilder.java`）——这是 HMCL 命令行正确性的基石，
//! 也是“用户参数永远优先、我们只补缺”的唯一可靠实现：
//!
//! - [`CommandBuilder::add`] 无条件追加（占位符已替换）；
//! - [`CommandBuilder::add_without_parsing`] 原样追加（已带引号或变量的串）；
//! - [`CommandBuilder::add_default`] 若已存在同前缀参数则不追加（opt=前缀，如 `-Xmx`）；
//! - [`CommandBuilder::add_unstable_flag`] / [`CommandBuilder::add_unstable_default`]
//!   按 `-XX:key=value` 与 `-XX:±key` 两种形态判重，避免同时出现 `+UseG1GC` 与 `-UseG1GC`；
//! - [`CommandBuilder::add_all_default`] 把一组参数整体按“默认值”处理（`-D` 拆键值、
//!   `-XX:` 走 unstable 分支、`-Xmx/-Xms/-Xmn/-Xss` 拆前缀、`-cp` 成对处理，其余要求完全相同）；
//! - `@argfile` 感知：Java 9+ 的 `@options-file` 展开后的参数登记进 `external`，
//!   让后续默认值同样被抑制。
//!
//! 与 HMCL 的差异（已记录在 `DevDocs/SubAgentsTalk/launch-接口约定.md` §7）：我们直接 spawn
//! 子进程、不经过 shell，因此不做批处理/Shell 转义；转义只在“生成启动脚本”（P1）时需要。

use std::path::Path;

use crate::launch::placeholders::PlaceholderMap;

/// 命令行组装器。
#[derive(Debug, Clone, Default)]
pub struct CommandBuilder {
    args: Vec<String>,
    external: Vec<String>,
    placeholders: PlaceholderMap,
}

impl CommandBuilder {
    /// 空构造器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 带占位符表的构造器。
    pub fn with_placeholders(placeholders: PlaceholderMap) -> Self {
        Self {
            args: Vec::new(),
            external: Vec::new(),
            placeholders,
        }
    }

    /// 占位符表。
    pub fn placeholders(&self) -> &PlaceholderMap {
        &self.placeholders
    }

    /// 追加一个参数（替换占位符）。
    pub fn add(&mut self, arg: &str) {
        let rendered = self.placeholders.render(arg);
        self.args.push(rendered);
    }

    /// 追加一组参数（替换占位符）。
    pub fn add_all(&mut self, args: &[String]) {
        for arg in args {
            self.add(arg);
        }
    }

    /// 原样追加（不替换占位符）。
    pub fn add_without_parsing(&mut self, arg: impl Into<String>) {
        self.args.push(arg.into());
    }

    /// 原样追加一组。
    pub fn add_all_without_parsing(&mut self, args: &[String]) {
        for arg in args {
            self.add_without_parsing(arg.clone());
        }
    }

    /// 原样追加一组，并读取其中的 `@argfile`：
    /// 文件里展开出来的参数登记为 `external`，后续 [`Self::add_default`] 会被它们抑制。
    pub fn add_all_without_parsing_and_read_external(
        &mut self,
        args: &[String],
        read_argfile: &dyn Fn(&str) -> Option<String>,
    ) {
        for arg in args {
            self.add_without_parsing(arg.clone());
            if let Some(path) = arg.strip_prefix('@') {
                if let Some(contents) = read_argfile(path) {
                    for extra in parse_argfile(&contents) {
                        self.external.push(extra);
                    }
                }
            }
        }
    }

    /// 已存在以 `prefix` 开头的参数则返回它，否则追加 `prefix + value` 并返回 `None`。
    ///
    /// 返回值的语义与 HMCL 一致：`None` = “是我加的”，`Some(已有值)` = “用户已经指定过”。
    pub fn add_default(&mut self, prefix: &str, value: &str) -> Option<String> {
        if let Some(existing) = self.find_starting_with(prefix) {
            return Some(existing.to_string());
        }
        let rendered = self.placeholders.render(&format!("{prefix}{value}"));
        self.args.push(rendered);
        None
    }

    /// `-XX:key=value` 形态的默认值；已存在该 key 的任何形态则不追加。
    pub fn add_unstable_default(&mut self, key: &str, value: &str) -> Option<String> {
        if let Some(existing) = self.find_unstable(key) {
            return Some(existing.to_string());
        }
        let rendered = self.placeholders.render(&format!("-XX:{key}={value}"));
        self.args.push(rendered);
        None
    }

    /// `-XX:+key` / `-XX:-key` 形态的默认值；已存在该 key 的任何形态则不追加。
    pub fn add_unstable_flag(&mut self, key: &str, enabled: bool) -> Option<String> {
        if let Some(existing) = self.find_unstable(key) {
            return Some(existing.to_string());
        }
        let sign = if enabled { '+' } else { '-' };
        self.args.push(format!("-XX:{sign}{key}"));
        None
    }

    /// 一组参数整体按“默认值”处理。
    pub fn add_all_default(&mut self, args: &[String]) {
        let mut index = 0;
        while index < args.len() {
            let raw = args[index].trim();
            if raw.is_empty() {
                index += 1;
                continue;
            }
            let resolved = self.placeholders.render(raw);
            if matches!(raw, "-cp" | "-classpath") {
                let exists = self.find_starting_with(raw).is_some();
                if !exists {
                    self.args.push(resolved);
                    if let Some(value) = args.get(index + 1) {
                        let value = self.placeholders.render(value);
                        self.args.push(value);
                    }
                }
                index += 2;
                continue;
            }
            if self.try_add_key_value_default(&resolved) {
                index += 1;
                continue;
            }
            let exists = self
                .args
                .iter()
                .chain(self.external.iter())
                .any(|arg| arg == &resolved);
            if !exists {
                self.args.push(resolved);
            }
            index += 1;
        }
    }

    /// 已存在的、以 `prefix` 开头的参数。
    pub fn find_starting_with(&self, prefix: &str) -> Option<&str> {
        self.args
            .iter()
            .chain(self.external.iter())
            .find(|arg| arg.starts_with(prefix))
            .map(String::as_str)
    }

    /// 已存在的、提到 `key` 的 `-XX:` 参数（两种形态都算）。
    pub fn find_unstable(&self, key: &str) -> Option<&str> {
        self.args
            .iter()
            .chain(self.external.iter())
            .find(|arg| unstable_key(arg) == Some(key))
            .map(String::as_str)
    }

    /// 是否已存在完全相同的参数。
    pub fn contains_exact(&self, arg: &str) -> bool {
        self.args.iter().any(|existing| existing == arg)
    }

    /// 删除禁用参数（例如 Java ≥9 上的 `-Xincgc`），返回被删掉的项。
    pub fn remove_forbidden(&mut self, forbidden: &[&str]) -> Vec<String> {
        let mut removed = Vec::new();
        self.args.retain(|arg| {
            if forbidden.contains(&arg.as_str()) {
                removed.push(arg.clone());
                false
            } else {
                true
            }
        });
        removed
    }

    /// 当前参数列表。
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// 取走参数列表。
    pub fn into_args(self) -> Vec<String> {
        self.args
    }

    /// 人读形式（含空格的参数加双引号），用于日志与快照测试。
    pub fn display_args(&self) -> String {
        self.args
            .iter()
            .map(|arg| quote_for_display(arg))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn try_add_key_value_default(&mut self, resolved: &str) -> bool {
        if let Some(rest) = resolved.strip_prefix("-D") {
            if let Some(eq) = rest.find('=') {
                let split = 2 + eq + 1;
                let prefix = resolved[..split].to_string();
                let value = resolved[split..].to_string();
                let _ = self.add_default(&prefix, &value);
                return true;
            }
        }
        if let Some(rest) = resolved.strip_prefix("-XX:") {
            if let Some(eq) = rest.find('=') {
                let _ = self.add_unstable_default(&rest[..eq], &rest[eq + 1..]);
                return true;
            }
            if let Some(signless) = rest.strip_prefix(['+', '-']) {
                let _ = self.add_unstable_flag(signless, rest.starts_with('+'));
                return true;
            }
        }
        for prefix in ["-Xmx", "-Xms", "-Xmn", "-Xss"] {
            if let Some(value) = resolved.strip_prefix(prefix) {
                let _ = self.add_default(prefix, value);
                return true;
            }
        }
        false
    }
}

fn unstable_key(arg: &str) -> Option<&str> {
    let rest = arg.strip_prefix("-XX:")?;
    let body = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    Some(body.split('=').next().unwrap_or(body))
}

/// 把 `minecraftArguments` 之类的一整串拆成参数（引号感知，反斜杠转义 `"` 与 `\`）。
pub fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut has_token = false;
    let mut in_single = false;
    let mut in_double = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
                has_token = true;
            }
            '"' if !in_single => {
                in_double = !in_double;
                has_token = true;
            }
            '\\' if !in_single => match chars.peek() {
                Some(&next) if next == '"' || next == '\\' => {
                    current.push(next);
                    chars.next();
                    has_token = true;
                }
                _ => {
                    current.push(ch);
                    has_token = true;
                }
            },
            c if c.is_whitespace() && !in_single && !in_double => {
                if has_token {
                    out.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            c => {
                current.push(c);
                has_token = true;
            }
        }
    }
    if has_token {
        out.push(current);
    }
    out
}

/// 解析 `@argfile` 内容：忽略空行与 `#` 注释行，每行再走 [`tokenize`]。
pub fn parse_argfile(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.extend(tokenize(line));
    }
    out
}

/// 从磁盘读取 `@argfile`（读不到就当没有；不因用户文件缺失而中断启动）。
pub fn read_argfile(path: &str) -> Option<String> {
    let path = path.strip_prefix('@').unwrap_or(path);
    std::fs::read(Path::new(path))
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

/// 人读引用：含空白或引号的参数加双引号并转义内部引号。
pub fn quote_for_display(arg: &str) -> String {
    if arg.is_empty() {
        return "\"\"".to_string();
    }
    if arg.contains([' ', '\t', '"']) {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}
