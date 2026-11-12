//! 安装选项的决策单测（M105）：兼容性真值表 + 确认键门 + 默认加载器版本。
//!
//! 全部是**纯函数**（`OptionsView::of` 吃 `AppState` 与 `VersionRow`，不碰 iced），
//! 所以不开图形环境也能钉住「什么组合能装、什么组合按下不去」。
//!
//! **为什么用 `AppState` 而不是 mock 后端**：加载器版本列表是快照字段
//! （`state.loader_versions`），灌一条 `CoreEvent::LoaderVersions` 就能造出三种状态
//! （没查过 / 查到了 / 取不到），比 mock 更直接，也不依赖 `MockBackend` 的脚本。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{
    AncientKind, AppEvent, CoreEvent, LoaderKind, VersionKind, VersionSummary,
};
use unlucky_presenter::state::AppState;

use super::model::{release_rank, Incompat, InstallDraft, Loader, LoaderVersionsView, OptionsView};
use crate::pages::download::model::VersionRow;

pub(super) fn row(id: &str, kind: VersionKind) -> VersionRow {
    VersionRow::of(
        &VersionSummary {
            id: id.to_string(),
            kind,
            released: "2026-09-20".to_string(),
            installed: false,
        },
        Lang::En.catalog(),
    )
}

pub(super) fn state_with(kind: LoaderKind, mc: &str, items: Result<Vec<&str>, &str>) -> AppState {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::LoaderVersions {
        kind,
        mc_version: mc.to_string(),
        result: items
            .map(|list| {
                list.into_iter()
                    .map(str::to_string)
                    .collect::<Vec<String>>()
            })
            .map_err(str::to_string),
    }));
    state
}

pub(super) fn draft(loader: Loader) -> InstallDraft {
    InstallDraft {
        selected: Some("1.20.1".to_string()),
        loader,
        loader_version: String::new(),
        instance_name: String::new(),
    }
}

/// 版本号解析：只有纯数字三段（或缺末段）才算正式版。
#[test]
fn release_rank_only_accepts_plain_release_ids() {
    assert_eq!(release_rank("1.20.1"), Some((1, 20, 1)));
    assert_eq!(release_rank("1.20"), Some((1, 20, 0)), "缺末段按 0 算");
    assert_eq!(release_rank("24w14a"), None, "快照不是正式版号");
    assert_eq!(release_rank("rd-132211"), None);
    assert_eq!(
        release_rank("1.20.1-pre3"),
        None,
        "预发布不许当正式版放过去"
    );
    assert_eq!(release_rank("1.20.1.2"), None, "多一段不认");
}

/// 兼容性真值表（任务书点名的「老版本 × NeoForge」在这条里）。
#[test]
fn loader_support_matrix_is_explicit() {
    // 原版永远可以
    for id in ["1.21.9", "1.12.2", "rd-132211"] {
        let kind = if id.starts_with("rd") {
            VersionKind::Ancient(AncientKind::Classic)
        } else {
            VersionKind::Release
        };
        assert_eq!(Loader::Vanilla.supports(id, kind), Ok(()));
    }
    // NeoForge 要 1.20.1+
    assert_eq!(
        Loader::NeoForge.supports("1.20.1", VersionKind::Release),
        Ok(())
    );
    assert_eq!(
        Loader::NeoForge.supports("1.12.2", VersionKind::Release),
        Err(Incompat::NeedsAtLeast("1.20.1")),
        "老版本 × NeoForge 必须在选中那一刻就说清，而不是点安装才失败"
    );
    // Fabric/Quilt 要 1.14+
    assert_eq!(
        Loader::Fabric.supports("1.20.1", VersionKind::Release),
        Ok(())
    );
    assert_eq!(
        Loader::Fabric.supports("1.12.2", VersionKind::Release),
        Err(Incompat::NeedsAtLeast("1.14"))
    );
    // Forge 任何正式版都行，但预览版/远古版没有 maven 条目
    assert_eq!(
        Loader::Forge.supports("1.12.2", VersionKind::Release),
        Ok(())
    );
    assert_eq!(
        Loader::Forge.supports("24w14a", VersionKind::Snapshot),
        Err(Incompat::Unsupported)
    );
    // LiteLoader 的官方版本表只有 1.5.2 ~ 1.12.2（16 个原版版本），两头都按「没有构建」拒。
    for id in ["1.5.2", "1.7.10", "1.12.2"] {
        assert_eq!(
            Loader::LiteLoader.supports(id, VersionKind::Release),
            Ok(()),
            "{id} 官方的确有 LiteLoader"
        );
    }
    for id in ["1.4.7", "1.13", "1.20.1"] {
        assert_eq!(
            Loader::LiteLoader.supports(id, VersionKind::Release),
            Err(Incompat::Unsupported),
            "{id} 上游没有 LiteLoader 构建"
        );
    }
    assert_eq!(
        Loader::LiteLoader.supports("24w14a", VersionKind::Snapshot),
        Err(Incompat::Unsupported)
    );
    // 远古版：五家全都不支持
    let ancient = ("a1.2.6", VersionKind::Ancient(AncientKind::OldAlpha));
    for loader in [
        Loader::Fabric,
        Loader::Quilt,
        Loader::Forge,
        Loader::NeoForge,
        Loader::LiteLoader,
    ] {
        assert_eq!(
            loader.supports(ancient.0, ancient.1),
            Err(Incompat::Unsupported)
        );
    }
}

/// 不兼容的组合：给具名提示，并且**确认键按下不去**。
#[test]
fn incompatible_combination_disables_confirm_with_a_named_reason() {
    let state = AppState::new();
    let options = OptionsView::of(
        &row("1.12.2", VersionKind::Release),
        &state,
        &draft(Loader::NeoForge),
        Lang::En.catalog(),
    );
    assert!(!options.can_confirm, "不兼容就不许按下确认");
    let problem = options.problem.expect("要给出具名原因");
    assert!(problem.contains("NeoForge"), "{problem}");
    assert!(problem.contains("1.20.1"), "{problem}");
}

/// 默认加载器版本 = 列表里最新的一条（core 的默认口径）；用户选过就用用户的。
#[test]
fn loader_version_defaults_to_the_newest_and_honours_the_choice() {
    let state = state_with(LoaderKind::Fabric, "1.20.1", Ok(vec!["0.16.5", "0.16.4"]));
    let options = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &state,
        &draft(Loader::Fabric),
        Lang::En.catalog(),
    );
    assert!(options.can_confirm);
    assert_eq!(options.loader_version, "0.16.5", "没选过就用最新");
    assert_eq!(options.loader_version_arg(), Some("0.16.5"));

    let mut chosen = draft(Loader::Fabric);
    chosen.loader_version = "0.16.4".to_string();
    let options = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &state,
        &chosen,
        Lang::En.catalog(),
    );
    assert_eq!(options.loader_version, "0.16.4", "用户选的就是用户选的");
}

/// 三条「列表还没到位」的状态都不许按下确认，且各自说清是哪种。
#[test]
fn versions_pending_or_failed_blocks_confirm() {
    let bare = AppState::new();
    let loading = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &bare,
        &draft(Loader::Fabric),
        Lang::En.catalog(),
    );
    assert_eq!(loading.versions, LoaderVersionsView::Loading);
    assert!(!loading.can_confirm, "还没查过就不许装");

    let failed = state_with(LoaderKind::Fabric, "1.20.1", Err("network down"));
    let options = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &failed,
        &draft(Loader::Fabric),
        Lang::En.catalog(),
    );
    assert!(!options.can_confirm);
    assert!(
        options
            .problem
            .as_deref()
            .is_some_and(|p| p.contains("network down")),
        "取不到要给具名原因（不是空下拉）"
    );

    let empty = state_with(LoaderKind::Fabric, "1.20.1", Ok(vec![]));
    let options = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &empty,
        &draft(Loader::Fabric),
        Lang::En.catalog(),
    );
    assert!(!options.can_confirm, "官方没有版本就不许装");
    assert!(options.problem.is_some());
}

/// 缓存键带原版版本：A 版本的列表不会挂到 B 版本下面。
#[test]
fn cached_versions_are_keyed_by_minecraft_version() {
    let state = state_with(LoaderKind::Fabric, "1.20.1", Ok(vec!["0.16.5"]));
    let other = OptionsView::of(
        &row("1.21.9", VersionKind::Release),
        &state,
        &draft(Loader::Fabric),
        Lang::En.catalog(),
    );
    assert_eq!(
        other.versions,
        LoaderVersionsView::Loading,
        "换原版版本就得重新查，不许复用上一版的结果"
    );
}

/// 原版：不查列表、不需要加载器版本，确认键直接可用。
#[test]
fn vanilla_needs_no_loader_version_at_all() {
    let state = AppState::new();
    let options = OptionsView::of(
        &row("1.20.1", VersionKind::Release),
        &state,
        &draft(Loader::Vanilla),
        Lang::En.catalog(),
    );
    assert!(options.can_confirm);
    assert_eq!(options.versions, LoaderVersionsView::Ready(Vec::new()));
    assert_eq!(options.loader_version, "");
    assert_eq!(options.loader_version_arg(), None);
}

/// 已装的版本：确认键给「已安装」禁用态（列表行上也有勾，按钮再点一次不会更清楚）。
#[test]
fn installed_versions_cannot_be_confirmed_again() {
    let state = AppState::new();
    let mut installed = row("1.20.1", VersionKind::Release);
    installed.installed = true;
    let options = OptionsView::of(
        &installed,
        &state,
        &draft(Loader::Vanilla),
        Lang::En.catalog(),
    );
    assert!(options.installed);
    assert!(!options.can_confirm);
}

/// 中性说明（M222）：只有 LiteLoader 带「这里只叠原版」那条，且它**不影响**确认键。
///
/// 抓得住：说明被塞进 `problem`（会让用户以为不能装、确认键按不下去）；或者说明跑到别家
/// 加载器下面（Forge/Fabric 没有这条边界）。
#[test]
fn liteloader_carries_a_neutral_note_without_blocking_confirm() {
    let state = state_with(
        LoaderKind::LiteLoader,
        "1.12.2",
        Ok(vec!["1.12.2-SNAPSHOT"]),
    );
    let options = OptionsView::of(
        &row("1.12.2", VersionKind::Release),
        &state,
        &draft(Loader::LiteLoader),
        Lang::En.catalog(),
    );
    let note = options.note.as_deref().expect("LiteLoader 应带中性说明");
    // 文案在 D30 五审后压成**单行**（两行 28px 是滚动区最大的可压项），断言随之改为
    // 「说清只支持原版父链、且确实只有一行」；完整命令行说法在 `ull help install` 里。
    assert!(note.contains("Vanilla parent only"), "{note}");
    assert_eq!(note.lines().count(), 1, "必须是一行：{note}");
    assert!(options.problem.is_none(), "中性说明不能变成错误");
    assert!(options.can_confirm, "说明不该禁用确认键");

    // 中文目录同样给中文说明，且仍然不禁用确认键。
    let zh = state_with(
        LoaderKind::LiteLoader,
        "1.12.2",
        Ok(vec!["1.12.2-SNAPSHOT"]),
    );
    let options = OptionsView::of(
        &row("1.12.2", VersionKind::Release),
        &zh,
        &draft(Loader::LiteLoader),
        Lang::ZhCn.catalog(),
    );
    let note = options.note.as_deref().expect("中文也要有说明");
    assert!(note.contains("只叠原版"), "{note}");
    assert!(options.can_confirm);

    // 别家加载器不带这条说明。
    for loader in [Loader::Vanilla, Loader::Fabric, Loader::Forge] {
        let state = state_with(loader.kind(), "1.20.1", Ok(Vec::new()));
        let options = OptionsView::of(
            &row("1.20.1", VersionKind::Release),
            &state,
            &draft(loader),
            Lang::En.catalog(),
        );
        assert!(
            options.note.is_none(),
            "{loader:?} 不该带 LiteLoader 的说明"
        );
    }
}
