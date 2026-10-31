//! P0 假数据：实例 / 版本 / 日志 / 诊断 / 工具调用样例。
//!
//! 只被 [`crate::mock`] 与阶段 B 的 main.rs 使用；真实后端接入后整文件可删。

use crate::dto::{
    AncientKind, ChatMessage, ChatRole, Diagnosis, DiagnosisReport, InstanceRuntime,
    InstanceSummary, LoaderInfo, LoaderKind, LogLevel, LogLine, LogOutcome, LogSession, SessionId,
    ToolCallView, VersionCategory, VersionKind, VersionSummary,
};

/// P0 演示实例：覆盖空闲 / 失败两种磁贴状态（运行中由启动脚本产生）。
pub fn mock_instances() -> Vec<InstanceSummary> {
    vec![
        InstanceSummary {
            id: "survival".to_string(),
            name: "生存整合包".to_string(),
            mc_version: "1.20.4".to_string(),
            loader: LoaderInfo {
                kind: LoaderKind::Fabric,
                version: "0.16.5".to_string(),
            },
            last_played: Some("2026-10-29T19:22:00Z".to_string()),
            runtime: InstanceRuntime::Idle,
            icon: None,
            isolation: "isolated".to_string(),
            version_id: "1.20.4".to_string(),
            version_present: true,
        },
        InstanceSummary {
            id: "vanilla-latest".to_string(),
            name: "原版最新".to_string(),
            mc_version: "1.21.9".to_string(),
            loader: LoaderInfo {
                kind: LoaderKind::Vanilla,
                version: String::new(),
            },
            last_played: Some("2026-10-28T21:03:00Z".to_string()),
            runtime: InstanceRuntime::Idle,
            icon: None,
            isolation: "isolated".to_string(),
            version_id: "1.21.9".to_string(),
            version_present: true,
        },
        InstanceSummary {
            id: "ancient-1.7.10".to_string(),
            name: "远古 1.7.10".to_string(),
            mc_version: "1.7.10".to_string(),
            loader: LoaderInfo {
                kind: LoaderKind::Forge,
                version: "10.13.4.1614".to_string(),
            },
            last_played: None,
            runtime: InstanceRuntime::Idle,
            icon: None,
            isolation: "shared".to_string(),
            version_id: "1.7.10".to_string(),
            version_present: false,
        },
        InstanceSummary {
            id: "modded-dev".to_string(),
            name: "开发测试".to_string(),
            mc_version: "1.21.1".to_string(),
            loader: LoaderInfo {
                kind: LoaderKind::NeoForge,
                version: "21.1.72".to_string(),
            },
            last_played: Some("2026-10-27T12:40:00Z".to_string()),
            runtime: InstanceRuntime::Failed {
                reason: "exit0 静默失败".to_string(),
            },
            icon: None,
            isolation: "isolated".to_string(),
            version_id: "1.21.1-neoforge-21.1.72".to_string(),
            version_present: true,
        },
    ]
}

/// P0 演示版本列表（含远古版三子类，验证分类侧栏与警示徽标）。
pub fn mock_versions() -> Vec<VersionSummary> {
    fn row(id: &str, kind: VersionKind, released: &str, installed: bool) -> VersionSummary {
        VersionSummary {
            id: id.to_string(),
            kind,
            released: released.to_string(),
            installed,
        }
    }
    vec![
        row("1.21.9", VersionKind::Release, "2026-09-20", false),
        row("1.21.8", VersionKind::Release, "2026-07-11", true),
        row("1.20.4", VersionKind::Release, "2023-12-07", true),
        row("1.21.9-pre3", VersionKind::Snapshot, "2026-09-01", false),
        row(
            "rd-132211",
            VersionKind::Ancient(AncientKind::Classic),
            "2009-05-13",
            false,
        ),
        row(
            "a1.2.6",
            VersionKind::Ancient(AncientKind::OldAlpha),
            "2010-08-11",
            false,
        ),
        row(
            "b1.7.3",
            VersionKind::Ancient(AncientKind::OldBeta),
            "2011-07-08",
            false,
        ),
    ]
}

/// 日志会话样例（✓ / ✗ / ⚠ 三种结局各一条）。
pub fn mock_log_sessions() -> Vec<LogSession> {
    vec![
        LogSession {
            id: "s-1029-1922".to_string(),
            instance: "生存整合包".to_string(),
            when: "10-29 19:22".to_string(),
            outcome: LogOutcome::Ok,
            duration_secs: 3_780,
        },
        LogSession {
            id: "s-1029-1840".to_string(),
            instance: "开发测试".to_string(),
            when: "10-29 18:40".to_string(),
            outcome: LogOutcome::Crashed,
            duration_secs: 62,
        },
        LogSession {
            id: "s-1029-1755".to_string(),
            instance: "原版最新".to_string(),
            when: "10-29 17:55".to_string(),
            outcome: LogOutcome::SilentFail,
            duration_secs: 8,
        },
    ]
}

/// 日志行样例（含一条 ERROR 供高亮验证）。
pub fn mock_log_lines() -> Vec<LogLine> {
    fn line(text: &str, level: LogLevel) -> LogLine {
        LogLine {
            text: text.to_string(),
            level,
        }
    }
    vec![
        line(
            "[19:22:01] [main/INFO] Launching Minecraft 1.20.4",
            LogLevel::Info,
        ),
        line("[19:22:02] [main/INFO] Loading 42 mods", LogLevel::Info),
        line(
            "[19:22:03] [Render thread/WARN] GL vendor: unknown",
            LogLevel::Warn,
        ),
        line(
            "[19:22:05] [Render thread/ERROR] OpenGL 2.1 not supported",
            LogLevel::Error,
        ),
        line("[19:22:05] [main/INFO] Stopping!", LogLevel::Info),
    ]
}

/// 崩溃诊断样例（验证日志页诊断卡片 + 「让 AI 详解」入口）。
pub fn mock_diagnosis(session: SessionId) -> DiagnosisReport {
    DiagnosisReport {
        session,
        diagnosis: Diagnosis {
            rule: "gpu.driver_old".to_string(),
            title: "显卡驱动过旧（OpenGL 版本不足）".to_string(),
            confidence: 92,
            hint: "升级显卡驱动，或在启动参数里改用软件渲染".to_string(),
            fix: Some("switch_software_renderer".to_string()),
        },
    }
}

/// 工具调用条目样例（AI 面板静态填充用）。
pub fn mock_tool_message() -> ChatMessage {
    ChatMessage {
        role: ChatRole::Tool,
        text: "读到 128 行，无 ERROR".to_string(),
        tool: Some(ToolCallView {
            name: "read_game_log".to_string(),
            ok: true,
            summary: "读到 128 行，无 ERROR".to_string(),
        }),
        streaming: false,
    }
}

/// 分类 → 该分类下的假版本（下载页切换分类时用）。
pub fn mock_versions_for(category: VersionCategory) -> Vec<VersionSummary> {
    mock_versions()
        .into_iter()
        .filter(|version| match category {
            VersionCategory::Release => version.kind == VersionKind::Release,
            VersionCategory::Snapshot => version.kind == VersionKind::Snapshot,
            VersionCategory::Ancient(kind) => version.kind == VersionKind::Ancient(kind),
            VersionCategory::Modpack | VersionCategory::Mod => false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::dto::{AncientKind, VersionCategory, VersionKind};
    use crate::mock_data::{mock_instances, mock_versions_for};

    #[test]
    fn mock_instances_cover_the_tile_states() {
        let instances = mock_instances();
        assert_eq!(instances.len(), 4);
        assert!(instances.iter().any(|instance| matches!(
            instance.runtime,
            crate::dto::InstanceRuntime::Failed { .. }
        )));
    }

    #[test]
    fn versions_are_filtered_by_category() {
        let release = mock_versions_for(VersionCategory::Release);
        assert!(release
            .iter()
            .all(|version| version.kind == VersionKind::Release));
        assert_eq!(release.len(), 3);

        let classic = mock_versions_for(VersionCategory::Ancient(AncientKind::Classic));
        assert_eq!(classic.len(), 1);
        assert_eq!(classic[0].id, "rd-132211");

        assert!(mock_versions_for(VersionCategory::Mod).is_empty());
    }
}
