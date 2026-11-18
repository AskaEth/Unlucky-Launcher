//! 六维修复动作本体：枚举、维度、以及「代入捕获组 → agent 工具参数」的物化。
//!
//! 六个维度与 analyse/02 §7.7 的动作分类一一对应，也是 M16 `ToolSpec` 的参数来源
//! （契约见同目录 `CONTRACT.md` §3）。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::fix_params::{AdjustMemory, ChangeGraphics, ChangeJava, Reinstall, RemoveMod, SeekHelp};
use super::substitute;

/// 六维修复动作（YAML 里 `kind:` 选维度）。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FixAction {
    /// 换 Java。
    ChangeJava(ChangeJava),
    /// 调内存。
    AdjustMemory(AdjustMemory),
    /// 改图形设置。
    ChangeGraphics(ChangeGraphics),
    /// 删/禁/换 mod。
    RemoveMod(RemoveMod),
    /// 重装/修组件。
    Reinstall(Reinstall),
    /// 求助上报。
    SeekHelp(SeekHelp),
}

/// 六维动作的维度枚举（不含参数，便于判等/去重/映射工具）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixActionKind {
    /// 换 Java。
    ChangeJava,
    /// 调内存。
    AdjustMemory,
    /// 改图形设置。
    ChangeGraphics,
    /// 删/禁/换 mod。
    RemoveMod,
    /// 重装/修组件。
    Reinstall,
    /// 求助上报。
    SeekHelp,
}

impl FixActionKind {
    /// 稳定字符串（诊断输出的 `kind` 字段，与 YAML 的 `kind:` 一致）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChangeJava => "change_java",
            Self::AdjustMemory => "adjust_memory",
            Self::ChangeGraphics => "change_graphics",
            Self::RemoveMod => "remove_mod",
            Self::Reinstall => "reinstall",
            Self::SeekHelp => "seek_help",
        }
    }

    /// 建议的 agent 工具 id。
    ///
    /// M16 的 P0 工具只有 5 个（`get_config`/`set_config`/`list_game_sessions`/`read_game_log`/
    /// `diagnose_crash`），因此它按 `kind` 桥接；这里的 id 是 P1 落地细分工具时的直接映射。
    #[must_use]
    pub const fn agent_tool(self) -> &'static str {
        match self {
            Self::ChangeJava => "java.set",
            Self::AdjustMemory => "memory.set",
            Self::ChangeGraphics => "graphics.set",
            Self::RemoveMod => "mod.remove",
            Self::Reinstall => "component.reinstall",
            Self::SeekHelp => "diagnostics.report",
        }
    }

    /// 全部六维（测试与 UI 遍历用）。
    #[must_use]
    pub const fn all() -> [Self; 6] {
        [
            Self::ChangeJava,
            Self::AdjustMemory,
            Self::ChangeGraphics,
            Self::RemoveMod,
            Self::Reinstall,
            Self::SeekHelp,
        ]
    }
}

/// 已做模板替换/单位换算的动作，可直接当 agent 工具参数。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MaterializedFixAction {
    /// 维度。
    pub kind: FixActionKind,
    /// agent 工具 id（见 [`FixActionKind::agent_tool`]）。
    pub tool: &'static str,
    /// 扁平字符串参数表。
    pub args: BTreeMap<String, String>,
}

impl FixAction {
    /// 维度。
    #[must_use]
    pub const fn kind(&self) -> FixActionKind {
        match self {
            Self::ChangeJava(_) => FixActionKind::ChangeJava,
            Self::AdjustMemory(_) => FixActionKind::AdjustMemory,
            Self::ChangeGraphics(_) => FixActionKind::ChangeGraphics,
            Self::RemoveMod(_) => FixActionKind::RemoveMod,
            Self::Reinstall(_) => FixActionKind::Reinstall,
            Self::SeekHelp(_) => FixActionKind::SeekHelp,
        }
    }

    /// 代入规则命名捕获组，产出 agent 工具可直接用的参数表。
    #[must_use]
    pub fn materialize(&self, groups: &BTreeMap<String, String>) -> MaterializedFixAction {
        let kind = self.kind();
        let mut args = BTreeMap::new();
        match self {
            Self::ChangeJava(p) => {
                let derived = p
                    .class_file_version_group
                    .as_ref()
                    .and_then(|group| groups.get(group))
                    .and_then(|raw| raw.trim().parse::<u32>().ok())
                    .and_then(class_file_to_java_major);
                put_opt(&mut args, "required_major", p.required_major.or(derived));
                put_opt(&mut args, "max_major", p.max_major);
                put_str(&mut args, "vendor", p.vendor.as_str());
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
            Self::AdjustMemory(p) => {
                put_opt(&mut args, "suggested_xmx_mb", p.suggested_xmx_mb);
                put_opt(&mut args, "min_xmx_mb", p.min_xmx_mb);
                put_bool(&mut args, "need_64bit_java", p.need_64bit_java);
                put_bool(&mut args, "increase_system_swap", p.increase_system_swap);
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
            Self::ChangeGraphics(p) => {
                put_str(&mut args, "setting", p.setting.as_str());
                put_bool(&mut args, "update_driver", p.update_driver);
                put_opt_str(&mut args, "suggested_value", p.suggested_value.clone());
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
            Self::RemoveMod(p) => {
                let ids = p
                    .mod_ids
                    .iter()
                    .map(|id| substitute(id, groups))
                    .collect::<Vec<_>>();
                if !ids.is_empty() {
                    args.insert("mod_ids".to_owned(), ids.join(","));
                }
                put_str(&mut args, "action", p.action.as_str());
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
            Self::Reinstall(p) => {
                put_str(&mut args, "component", p.component.as_str());
                put_opt_str(&mut args, "path_hint", render_opt(&p.path_hint, groups));
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
            Self::SeekHelp(p) => {
                put_bool(&mut args, "export_diagnostics", p.export_diagnostics);
                if !p.needs_more_info.is_empty() {
                    args.insert("needs_more_info".to_owned(), p.needs_more_info.join("; "));
                }
                put_opt_str(&mut args, "note", render_opt(&p.note, groups));
            }
        }
        MaterializedFixAction {
            kind,
            tool: kind.agent_tool(),
            args,
        }
    }
}

/// 可选的模板字段：`Some` 时做 `{group}` 替换，`None` 保持 `None`。
fn render_opt(value: &Option<String>, groups: &BTreeMap<String, String>) -> Option<String> {
    value.as_ref().map(|text| substitute(text, groups))
}

fn put_str(args: &mut BTreeMap<String, String>, key: &str, value: &str) {
    args.insert(key.to_owned(), value.to_owned());
}

fn put_opt_str(args: &mut BTreeMap<String, String>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        args.insert(key.to_owned(), value);
    }
}

fn put_opt(args: &mut BTreeMap<String, String>, key: &str, value: Option<u32>) {
    if let Some(value) = value {
        args.insert(key.to_owned(), value.to_string());
    }
}

fn put_bool(args: &mut BTreeMap<String, String>, key: &str, value: bool) {
    args.insert(key.to_owned(), value.to_string());
}

/// `class file version 52` → `Java 8`（HMCL `getJavaVersionFromMajorVersion` 同式）。
#[must_use]
pub fn class_file_to_java_major(class_file_version: u32) -> Option<u32> {
    class_file_version
        .checked_sub(44)
        .filter(|major| *major > 0)
}
