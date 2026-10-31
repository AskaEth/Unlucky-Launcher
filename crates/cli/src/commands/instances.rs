//! `instances list`：实例与已安装版本的总览。
//!
//! 安装/登记在 [`super::install`]；这里只读（`profiles.json` + `versions/` 的存在性）。
//! 读 `profiles.json` 的实现委托给 `core::game::install::instance`，避免 CLI 自带一份
//! 解析逻辑（同一份文件两处解析迟早会漂移）。

use std::path::PathBuf;

use serde_json::{json, Value};
use unlucky_core::game::install::instance::read_profiles as read_profiles_from_layout;
use unlucky_core::game::ProfileList;

use crate::context::Ctx;
use crate::error::CliError;
use crate::output::Report;

/// `ull instances list`。
///
/// # Errors
///
/// `profiles.json` 读不了或解析失败时返回用户错误。
pub fn list(ctx: &Ctx) -> Result<Report, CliError> {
    let profiles = read_profiles(ctx)?;
    let game = ctx.game();
    let installed = game.installed_versions().unwrap_or_default();

    let mut rows: Vec<Value> = Vec::new();
    let mut body: Vec<String> = Vec::new();
    for profile in &profiles.profiles {
        let present = game.is_version_installed(&profile.version_id);
        body.push(format!(
            "  {}  [{}]  {}  {}",
            profile.name,
            profile.version_id,
            if present { "已安装" } else { "载荷缺失" },
            profile.id.as_str()
        ));
        rows.push(json!({
            "id": profile.id.as_str(),
            "name": profile.name,
            "version_id": profile.version_id,
            "version_present": present,
            "game_dir": option_path(profile.game_dir.as_ref()),
            "java_path": option_path(profile.java_path.as_ref()),
            "last_played_at": profile.last_played_at,
        }));
    }

    let data = json!({
        "data_root": ctx.data.launcher_home.display().to_string(),
        "game_root": ctx.data.game_home.display().to_string(),
        "instances": rows,
        "installed_versions": installed,
    });

    let mut human = vec![format!(
        "实例 {} 个（数据根 {}）",
        profiles.profiles.len(),
        ctx.data.launcher_home.display()
    )];
    if body.is_empty() {
        human.push("  （空）用 `ull install <version>` 登记第一个实例".to_string());
    }
    human.extend(body);
    if installed.is_empty() {
        human.push("已安装版本: （无）".to_string());
    } else {
        human.push(format!("已安装版本: {}", installed.join(", ")));
    }
    Ok(Report::new(data, human))
}

/// 读实例清单（文件不存在 = 空列表，不算错误）。
///
/// # Errors
///
/// 读盘失败或 JSON 非法时返回用户错误。
pub fn read_profiles(ctx: &Ctx) -> Result<ProfileList, CliError> {
    let path = ctx.profiles_config();
    read_profiles_from_layout(&ctx.layout()).map_err(|error| {
        CliError::user(
            "profiles_parse",
            format!("读实例清单 {} 失败：{error}", path.display()),
        )
    })
}

fn option_path(path: Option<&PathBuf>) -> Option<String> {
    path.map(|path| path.display().to_string())
}
