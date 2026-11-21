//! 设置页「数据目录」分组（M397 从 `host/settings.rs` 整段搬来，函数体逐字不动）。

use std::path::Path;

use crate::dto::{SettingGroupView, SettingRowView};

use super::super::env::HostEnv;
use super::{exe_dir, selected};

/// 数据目录（D6 三模式）。**M216 ①**：`core.data_custom_root` 读**写回的那份**（便携配置），否则
/// 改完自定义根页面读「解析后」那份、输入凭空消失；`core.data_mode` 仍显示生效模式。
pub(super) fn data_group(env: &HostEnv) -> SettingGroupView {
    data_group_at(env, exe_dir().as_deref())
}

/// [`data_group`] 的参数化入口（`exe_dir` 决定自定义根那行读哪份配置；单测注入用）。
pub(super) fn data_group_at(env: &HostEnv, exe_dir: Option<&Path>) -> SettingGroupView {
    let modes = ["portable", "standard", "custom"];
    let mode = env.mode().as_str().to_string();
    let custom_root = exe_dir
        .map(super::env::request_from_portable)
        .and_then(|request| request.custom_root)
        .map(|path| path.display().to_string())
        .unwrap_or_default();
    SettingGroupView::new("data", "数据目录（D6 三模式）")
        .with(SettingRowView::choice(
            "core.data_mode",
            "存储模式",
            "便携（运行目录）/ 标准（系统应用数据目录）/ 自定义",
            &modes,
            selected(&modes, &mode),
        ))
        .with(SettingRowView::text(
            "core.data_custom_root",
            "自定义根目录",
            "custom 模式生效（绝对路径）",
            &custom_root,
        ))
        .with(SettingRowView::read_only(
            "启动器数据目录",
            &env.data.launcher_home.display().to_string(),
        ))
        .with(SettingRowView::read_only(
            "游戏目录",
            &env.data.game_home.display().to_string(),
        ))
        .with(SettingRowView::read_only(
            "生效模式",
            &format!("{}（改数据根需重启启动器）", env.mode()),
        ))
}
