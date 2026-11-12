//! 参数占位符字典与替换。
//!
//! 占位符清单平移自 `analyse/02` §4.8（`DefaultLauncher.getConfigurations()`）：
//! 版本清单里的 `arguments.*` 用 `${...}` 引用运行时值，启动器负责在组装时替换。
//! 替换语义与 HMCL 一致：**单遍、不递归**。
//!
//! ## 未登记占位符：原样保留，但不再静默（M244）
//!
//! 未登记键（第三方清单里自造的 `${xxx}`）**仍然原样写进 argv**——第三方清单的兼容性优先，
//! 悄悄改写别人给的命令行比留着 `${xxx}` 危险得多。但「静默」是另一回事：argv、
//! `--dry-run` 与会话 JSONL 里只有一个 `${xxx}`，没有任何解释。所以渲染时会把这些键收集起来：
//!
//! - [`PlaceholderMap::render`] 每个键每次启动只往 `launcher.log` 记一行
//!   （[`crate::launch::launcher_log`] 是 `crates/core` 里唯一 GUI 与 CLI 都写、且事故之后
//!   还能翻的落点，见该模块文档）；
//! - [`PlaceholderMap::render_tracked`] / [`PlaceholderMap::unknown_keys`] 把同一份清单
//!   **结构化**交出来，给后续接 `AssemblyWarning` / `--dry-run` 留好接口。
//!
//! 键的匹配是**精确字节相等**：`${Version_Name}`、`${ version_name }` 与 `${version_name}`
//! 是三个不同的键（替换本身就不认它们，合并只会掩盖「这个串没被替换」这个事实）。日志行里
//! 会提示大小写/空白近似的那一个，方便定位笔误。

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use crate::launch::platform::PlatformCtx;
use crate::launch::request::LaunchRequest;

/// 我们支持的占位符键（不含 `${}` 包裹）。
pub const PLACEHOLDER_KEYS: &[&str] = &[
    "auth_player_name",
    "auth_session",
    "auth_access_token",
    "auth_uuid",
    "user_type",
    "user_properties",
    "version_name",
    "version_type",
    "profile_name",
    "game_directory",
    "assets_index_name",
    "game_assets",
    "assets_root",
    "natives_directory",
    "classpath",
    "classpath_separator",
    "launcher_name",
    "launcher_version",
    "library_directory",
    "libraries_directory",
    "primary_jar",
    "primary_jar_name",
    "file_separator",
    "resolution_width",
    "resolution_height",
    "language",
];

/// 一次渲染的结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderOutcome {
    /// 替换后的参数串（未登记键原样保留）。
    pub text: String,
    /// 本次渲染遇到的未登记键（不含 `${}`）；按首次出现顺序去重。
    pub unknown_keys: Vec<String>,
}

impl RenderOutcome {
    /// 记一个未登记键（已记过的不重复记）。
    fn note_unknown(&mut self, key: &str) {
        if self.unknown_keys.iter().any(|known| known == key) {
            return;
        }
        self.unknown_keys.push(key.to_string());
    }
}

/// 占位符表。
///
/// 除了键值对，还记着「渲染时见过哪些未登记键」（去重保序，[`Self::unknown_keys`] 交出）。
/// 这份记录是**附加物**：它不参与替换，也不改变替换结果。
#[derive(Debug, Default)]
pub struct PlaceholderMap {
    entries: BTreeMap<String, String>,
    reported: Mutex<Vec<String>>,
}

impl Clone for PlaceholderMap {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            reported: Mutex::new(self.reported_keys()),
        }
    }
}

impl PlaceholderMap {
    /// 空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入一个占位符（键不含 `${}`）。
    pub fn insert(&mut self, key: &str, value: impl Into<String>) {
        self.entries.insert(key.to_string(), value.into());
    }

    /// 查询。
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }

    /// 已登记的键数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 到目前为止渲染时遇到的未登记键（去重、保序）。
    ///
    /// 这是给「结构化交出口」用的：接 `AssemblyWarning` / `--dry-run` 时读这份清单即可，
    /// 不必再解析 `launcher.log`。
    pub fn unknown_keys(&self) -> Vec<String> {
        self.reported_keys()
    }

    /// 单遍替换 `${key}`；未登记键原样保留，并记一行 `launcher.log`（每个键只记一次）。
    ///
    /// 需要同一份结果的**结构化**形式（未登记键清单）时用 [`Self::render_tracked`]。
    pub fn render(&self, template: &str) -> String {
        let outcome = self.render_tracked(template);
        self.report(&outcome.unknown_keys);
        outcome.text
    }

    /// 单遍替换 `${key}`，并把本次遇到的未登记键一并交出（本方法**不**写日志）。
    pub fn render_tracked(&self, template: &str) -> RenderOutcome {
        let mut outcome = RenderOutcome {
            text: String::new(),
            unknown_keys: Vec::new(),
        };
        if !template.contains("${") {
            outcome.text.push_str(template);
            return outcome;
        }
        outcome.text.reserve(template.len() + 16);
        let mut rest = template;
        while let Some(start) = rest.find("${") {
            outcome.text.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find('}') {
                Some(end) => {
                    let key = &after[..end];
                    match self.entries.get(key) {
                        Some(value) => outcome.text.push_str(value),
                        None => {
                            outcome.text.push_str("${");
                            outcome.text.push_str(key);
                            outcome.text.push('}');
                            outcome.note_unknown(key);
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    outcome.text.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        outcome.text.push_str(rest);
        outcome
    }

    /// 登记本次遇到的未登记键，只把**新**见到的那些写进日志。
    fn report(&self, unknown: &[String]) {
        self.report_with(unknown, emit);
    }

    /// [`Self::report`] 的可注入版本：`sink` 拿到的是「本次**新**见到的键」（去重、保序）。
    ///
    /// 留这道缝只为一件事：让「哪些键真的交给了日志」有自动化证据（用例用假 sink 断言），
    /// 而不是只能靠读代码——进程级的 `launcher_log` 在 lib 测试里动不得（见 [`emit`] 的注释）。
    fn report_with(&self, unknown: &[String], mut sink: impl FnMut(&str)) {
        let fresh = {
            let mut seen = self.seen();
            collect_fresh(&mut seen, unknown)
        };
        for key in &fresh {
            sink(key);
        }
    }

    fn seen(&self) -> MutexGuard<'_, Vec<String>> {
        self.reported
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn reported_keys(&self) -> Vec<String> {
        self.seen().clone()
    }

    /// 由启动请求构造完整占位符表。
    pub fn from_request(req: &LaunchRequest, ctx: &PlatformCtx) -> Self {
        let mut map = Self::new();
        let auth = &req.auth;
        map.insert("auth_player_name", auth.player_name.clone());
        map.insert("auth_session", auth.access_token.clone());
        map.insert("auth_access_token", auth.access_token.clone());
        map.insert("auth_uuid", auth.uuid.clone());
        map.insert("user_type", auth.user_type.clone());
        map.insert("user_properties", auth.user_properties.clone());

        map.insert("version_name", req.instance.name.clone());
        map.insert("version_type", req.instance.version_type.clone());
        map.insert("profile_name", req.instance.id.clone());

        let dirs = &req.dirs;
        map.insert("game_directory", path_str(&dirs.game_dir));
        map.insert("assets_index_name", dirs.assets_index.clone());
        map.insert("game_assets", path_str(&dirs.game_assets));
        map.insert("assets_root", path_str(&dirs.assets_root));
        map.insert("natives_directory", path_str(&dirs.natives_dir));
        map.insert(
            "classpath",
            join_classpath(&req.classpath, ctx.path_separator()),
        );
        map.insert("classpath_separator", ctx.path_separator().to_string());
        map.insert("launcher_name", req.launcher.name.clone());
        map.insert("launcher_version", req.launcher.version.clone());
        map.insert("library_directory", path_str(&dirs.library_dir));
        map.insert("libraries_directory", path_str(&dirs.library_dir));
        map.insert("primary_jar", path_str(&dirs.primary_jar));
        map.insert(
            "primary_jar_name",
            dirs.primary_jar
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
        map.insert("file_separator", ctx.file_separator().to_string());
        match &req.resolution {
            Some(res) => {
                map.insert("resolution_width", res.width.to_string());
                map.insert("resolution_height", res.height.to_string());
            }
            None => {
                map.insert("resolution_width", String::new());
                map.insert("resolution_height", String::new());
            }
        }
        map.insert("language", req.language.clone());
        map
    }
}

/// 把 `keys` 登记进 `seen`，返回本次**新**见到的那些（保序；两边都按字节相等去重）。
fn collect_fresh(seen: &mut Vec<String>, keys: &[String]) -> Vec<String> {
    let mut fresh = Vec::new();
    for key in keys {
        if seen.iter().any(|known| known == key) {
            continue;
        }
        seen.push(key.clone());
        fresh.push(key.clone());
    }
    fresh
}

/// 未登记键的 `launcher.log` 行（scope 与时间戳由 `launcher_log` 加）。
///
/// 键名先做换行消毒：`launcher.log` 是**行导向**的，第三方清单完全可以在 JSON 里塞
/// `${evil\n2026-… [launch] 伪造行}`，把一个键变成两行假日志。这里换成一个空格。
fn unknown_log_line(key: &str) -> String {
    let key = key.replace(['\r', '\n'], " ");
    match near_miss(&key) {
        Some(known) => format!(
            "占位符 ${{{key}}} 未登记：已按原样保留；疑似 ${{{known}}} 的笔误\
             （替换是精确匹配，大小写/空白都算不同的键）"
        ),
        None => format!("占位符 ${{{key}}} 未登记：已按原样保留（第三方版本清单常带自有占位符）"),
    }
}

/// 大小写/首尾空白不同、但确实登记了的那个键（只用于日志提示，不参与替换与去重）。
///
/// 候选集用 [`PLACEHOLDER_KEYS`] 而不是 `self.entries`：`from_request` 登记的就是这 26 个
/// （`crates/core/tests/launch/cmdline_snapshot.rs` 的 `placeholder_table_is_complete` 守这条），
/// 而本函数是纯函数，拿不到表。
fn near_miss(key: &str) -> Option<&'static str> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return None;
    }
    PLACEHOLDER_KEYS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(trimmed))
}

/// 把一行「未登记占位符」写进 `launcher.log`。
///
/// 测试构建里是空实现：`launcher_log` 是**进程级单例**（目录 + 200 行缓冲），lib 的几百条
/// 用例在同一个进程里并行跑，本模块记的几行会落进 `launcher_log` 自己
/// `buffer_is_bounded_before_the_dir_is_known` 的窗口里（实测：`缓冲没有上限：205 行`）。
/// 让每个用例都去抢那个单例，等于把 flake 转嫁给别人。三段各有自动化证据：行文本
/// [`unknown_log_line`]、「哪些键算新」[`collect_fresh`]、「新键怎么交给 sink」
/// [`PlaceholderMap::report_with`]（用例用假 sink 断言）；只剩最后一句
/// `launcher_log::record` 由 `M244` 的一次性探针（`--test-threads=1`，跑完即删）手工验过。
#[cfg(not(test))]
fn emit(key: &str) {
    crate::launch::launcher_log::record("launch", &unknown_log_line(key));
}

/// 见上面非测试版本（测试里刻意不碰进程级日志单例）。
#[cfg(test)]
fn emit(_key: &str) {}

/// 按平台分隔符拼接 classpath。
pub fn join_classpath(paths: &[std::path::PathBuf], separator: char) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(&separator.to_string())
}

fn path_str(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests;
