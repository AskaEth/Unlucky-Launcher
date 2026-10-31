//! 参数占位符字典与替换。
//!
//! 占位符清单平移自 `analyse/02` §4.8（`DefaultLauncher.getConfigurations()`）：
//! 版本清单里的 `arguments.*` 用 `${...}` 引用运行时值，启动器负责在组装时替换。
//! 替换语义与 HMCL 一致：**单遍、不递归**；未定义的占位符原样保留（便于发现清单漏项）。

use std::collections::BTreeMap;
use std::path::Path;

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

/// 占位符表。
#[derive(Debug, Clone, Default)]
pub struct PlaceholderMap {
    entries: BTreeMap<String, String>,
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

    /// 单遍替换 `${key}`；未定义键原样保留。
    pub fn render(&self, template: &str) -> String {
        if !template.contains("${") {
            return template.to_string();
        }
        let mut out = String::with_capacity(template.len() + 16);
        let mut rest = template;
        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find('}') {
                Some(end) => {
                    let key = &after[..end];
                    match self.entries.get(key) {
                        Some(value) => out.push_str(value),
                        None => {
                            out.push_str("${");
                            out.push_str(key);
                            out.push('}');
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str(&rest[start..]);
                    rest = "";
                }
            }
        }
        out.push_str(rest);
        out
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
