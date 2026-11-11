//! 实例 / 版本 / Java 端口（M14 + M33）。

use std::path::PathBuf;

use unlucky_agent::port::game::{
    GamePort, InstanceSummary, JavaRuntimeSummary, JavaStatus, VersionSummary,
};
use unlucky_agent::port::PortResult;
use unlucky_core::game::java::{self, JavaOrigin};
use unlucky_core::game::{install::instance::read_profiles, GameLayout};
use unlucky_core::platform::data_dir::DataMode;
use unlucky_core::platform::layout::LauncherLayout;
use unlucky_core::platform::paths::{Platform, ProcessEnv};
use unlucky_core::setting::keys::launcher_schema;
use unlucky_core::setting::store::SettingStore;

/// 实例 / 版本 / Java 的只读端口。
#[derive(Debug, Clone)]
pub struct CoreGame {
    layout: LauncherLayout,
    game: GameLayout,
    launcher_config: PathBuf,
    mode: DataMode,
    platform: Platform,
}

impl CoreGame {
    /// 装配：`layout` 是 `.ULL/`，`game` 是 `.minecraft/`。
    #[must_use]
    pub fn new(
        layout: LauncherLayout,
        game: GameLayout,
        mode: DataMode,
        platform: Platform,
    ) -> Self {
        let launcher_config = layout.launcher_config();
        Self {
            layout,
            game,
            launcher_config,
            mode,
            platform,
        }
    }

    fn store(&self) -> SettingStore {
        SettingStore::load(&self.launcher_config, launcher_schema())
    }

    fn managed_roots(&self) -> Vec<PathBuf> {
        let store = self.store();
        let install_root = store
            .doc()
            .effective_str("java.installRoot")
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(PathBuf::from);
        java::location::scan_roots(
            self.mode,
            &self.layout,
            install_root.as_deref(),
            self.platform,
            &ProcessEnv,
        )
    }
}

impl GamePort for CoreGame {
    fn list_instances(&self) -> PortResult<Vec<InstanceSummary>> {
        let profiles =
            read_profiles(&self.layout).map_err(|error| format!("读实例清单失败：{error}"))?;
        let mut out: Vec<InstanceSummary> = profiles
            .profiles
            .iter()
            .map(|profile| InstanceSummary {
                id: profile.id.as_str().to_string(),
                name: profile.name.clone(),
                version_id: profile.version_id.clone(),
                version_present: self.game.is_version_installed(&profile.version_id),
                game_dir: profile.game_dir.as_ref().map(|p| p.display().to_string()),
                java_path: profile.java_path.as_ref().map(|p| p.display().to_string()),
                last_played_at: profile.last_played_at.as_deref().and_then(parse_rfc3339_ms),
            })
            .collect();
        // `profiles.json` 的顺序是 UI 顺序；这里按最近启动时间倒序（最近的先看）。
        out.sort_by_key(|instance| std::cmp::Reverse(instance.last_played_at));
        Ok(out)
    }

    fn list_versions(&self) -> PortResult<Vec<VersionSummary>> {
        let versions = self
            .game
            .installed_versions()
            .map_err(|error| format!("读已安装版本失败：{error}"))?;
        let profiles = read_profiles(&self.layout).map_err(|error| error.to_string())?;
        Ok(versions
            .into_iter()
            .map(|id| {
                let instance_count = profiles
                    .profiles
                    .iter()
                    .filter(|profile| profile.version_id == id)
                    .count();
                VersionSummary {
                    id,
                    kind: None,
                    instance_count,
                }
            })
            .collect())
    }

    fn java_status(&self) -> PortResult<JavaStatus> {
        let roots = self.managed_roots();
        let mut candidates = java::candidates_in_roots(&roots);
        if let Ok(profiles) = read_profiles(&self.layout) {
            for profile in &profiles.profiles {
                if let Some(path) = &profile.java_path {
                    candidates.push(java::probe(path, JavaOrigin::Instance));
                }
            }
        }
        let candidates = java::discover::dedupe(candidates);
        let summary: Vec<JavaRuntimeSummary> = candidates
            .iter()
            .map(|runtime| JavaRuntimeSummary {
                binary: runtime.binary.display().to_string(),
                version: runtime.version.clone(),
                major: runtime.major,
                vendor: runtime.vendor.clone(),
                origin: runtime.origin.category().to_string(),
            })
            .collect();

        let default_memory = self
            .store()
            .effective("java.default_memory_mb")
            .and_then(serde_json::Value::as_u64);

        Ok(JavaStatus {
            default_memory_mb: default_memory,
            managed_roots: roots
                .iter()
                .map(|root| root.display().to_string())
                .collect(),
            candidates: summary,
        })
    }
}

/// RFC3339 → Unix 毫秒（解析失败返回 `None`，不让一个坏时间戳毁掉整个列表）。
///
/// 只处理 launch 写出来的那一种形态（总是 UTC、总是 `Z` 结尾）。不引第三方日期库：
/// agent 侧只需要「排序用的毫秒」，多一棵依赖树不值当。
fn parse_rfc3339_ms(text: &str) -> Option<u64> {
    let body = text.trim().trim_end_matches('Z');
    let (date, time) = body.split_once('T')?;
    let mut date_parts = date.split('-');
    let year: i64 = date_parts.next()?.parse().ok()?;
    let month: i64 = date_parts.next()?.parse().ok()?;
    let day: i64 = date_parts.next()?.parse().ok()?;
    let mut time_parts = time.split(':');
    let hour: i64 = time_parts.next()?.parse().ok()?;
    let minute: i64 = time_parts.next()?.parse().ok()?;
    let seconds_text = time_parts.next()?;
    let (secs, millis) = match seconds_text.split_once('.') {
        Some((secs, frac)) => (secs.parse::<i64>().ok()?, {
            let padded = format!("{frac:0<3}");
            padded.get(..3)?.parse::<i64>().ok()?
        }),
        None => (seconds_text.parse::<i64>().ok()?, 0),
    };

    let days = days_from_civil(year, month, day);
    let total_ms = (((days * 24 + hour) * 60 + minute) * 60 + secs) * 1000 + millis;
    u64::try_from(total_ms).ok()
}

/// Howard Hinnant 的 `days_from_civil`（1970-01-01 为 0）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_launch_timestamps() {
        assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00.000Z"), Some(0));
        // 与既有审计测试同源：1_793_304_753_123 = 2026-10-29T20:12:33.123Z
        assert_eq!(
            parse_rfc3339_ms("2026-10-29T20:12:33.123Z"),
            Some(1_793_304_753_123)
        );
        assert_eq!(parse_rfc3339_ms("not-a-time"), None);
    }
}
