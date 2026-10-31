//! 版本列表与下载分类映射（下载页）。

use crate::dto::{AncientKind, VersionCategory, VersionKind as DtoVersionKind, VersionSummary};
use crate::state::AppState;
use crate::{UiVersion, VersionKind};

/// 分类侧栏 → 该分类下的版本（远古版按子类细分）。
pub fn in_category(category: VersionCategory, kind: DtoVersionKind) -> bool {
    match (category, kind) {
        (VersionCategory::Release, DtoVersionKind::Release) => true,
        (VersionCategory::Snapshot, DtoVersionKind::Snapshot) => true,
        (VersionCategory::Ancient(expected), DtoVersionKind::Ancient(actual)) => expected == actual,
        // 未接线的分类（整合包 / Mod）不显示任何版本，而不是把全部版本倒进去
        (VersionCategory::Modpack | VersionCategory::Mod, _) => false,
        _ => false,
    }
}

/// 版本行（下载页与新建向导共用）。
pub fn version_rows(versions: &[VersionSummary]) -> Vec<UiVersion> {
    versions
        .iter()
        .map(|version| UiVersion {
            id: version.id.clone().into(),
            kind: version_kind(version.kind),
            released: version.released.clone().into(),
            installed: version.installed,
            ancient: version.kind.is_ancient(),
        })
        .collect()
}

/// 下载页版本行（按当前分类 + 「仅已装」+ 版本号筛选词过滤）。
pub fn versions(state: &AppState) -> Vec<UiVersion> {
    let query = state.download_query.trim().to_lowercase();
    let filtered: Vec<VersionSummary> = state
        .versions
        .iter()
        .filter(|version| in_category(state.prefs.download_category, version.kind))
        .filter(|version| !state.installed_only || version.installed)
        .filter(|version| query.is_empty() || version.id.to_lowercase().contains(&query))
        .cloned()
        .collect();
    version_rows(&filtered)
}

fn version_kind(kind: DtoVersionKind) -> VersionKind {
    match kind {
        DtoVersionKind::Release => VersionKind::Release,
        DtoVersionKind::Snapshot => VersionKind::Snapshot,
        DtoVersionKind::Ancient(AncientKind::Classic) => VersionKind::AncientClassic,
        DtoVersionKind::Ancient(AncientKind::OldAlpha) => VersionKind::AncientAlpha,
        DtoVersionKind::Ancient(AncientKind::OldBeta) => VersionKind::AncientBeta,
        DtoVersionKind::Ancient(AncientKind::Unlisted) => VersionKind::AncientUnlisted,
    }
}

#[cfg(test)]
mod tests {
    use super::versions;
    use crate::dto::{VersionKind, VersionSummary};
    use crate::state::AppState;

    fn state_with_versions() -> AppState {
        let mut state = AppState::new();
        let row = |id: &str, installed: bool| VersionSummary {
            id: id.to_string(),
            kind: VersionKind::Release,
            released: "2026-01-01".to_string(),
            installed,
        };
        state.versions = vec![
            row("1.21.9", false),
            row("1.20.4", true),
            row("1.20.1", true),
        ];
        state
    }

    #[test]
    fn installed_only_filter_hides_uninstalled_versions() {
        let mut state = state_with_versions();
        assert_eq!(versions(&state).len(), 3);

        state.installed_only = true;
        let filtered = versions(&state);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|version| version.installed));
    }

    #[test]
    fn query_filters_by_version_id_substring() {
        let mut state = state_with_versions();
        state.download_query = "1.20".to_string();
        let filtered = versions(&state);
        assert_eq!(filtered.len(), 2);

        state.download_query = "1.21".to_string();
        state.installed_only = true;
        // 「仅已装」优先：1.21.9 未安装 → 空列表
        assert!(versions(&state).is_empty());
    }
}
