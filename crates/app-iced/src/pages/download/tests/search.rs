//! 搜索 × 主版本桶的一致性判据（复审 P2-1）+ 复数口径（P2-4）。
//!
//! 这两条都是「一屏之内两句话打架」那类问题：命中数说 N 个、列表却 0 行；或者英文写 `1 matches`。
//! 所以断言都写成**不变式**（对一张输入矩阵扫），而不是挑一个例子。

use unlucky_i18n::Lang;
use unlucky_presenter::dto::{AppEvent, CoreEvent, VersionKind, VersionSummary};
use unlucky_presenter::state::AppState;

use super::super::group;
use super::super::model::{Category, Download};
use super::super::options::InstallDraft;
use super::super::rows;
use super::version;

fn state_with(versions: Vec<VersionSummary>, query: &str) -> AppState {
    let mut state = AppState::new();
    state.apply(AppEvent::Core(CoreEvent::VersionListChanged(versions)));
    state.download_query = query.to_string();
    state
}

/// 一份带三个主版本桶的清单（`1.21` / `1.20` / `24w`）。
fn three_buckets() -> Vec<VersionSummary> {
    vec![
        version("1.21.9", VersionKind::Release, false),
        version("1.21.8", VersionKind::Release, true),
        version("1.20.4", VersionKind::Release, true),
        version("1.20.1", VersionKind::Release, false),
        version("24w14a", VersionKind::Snapshot, false),
    ]
}

/// **P2-1 的核心不变式**：搜索有命中时，列表要么有行，要么落到「命中在别的类型里」那条口径。
///
/// 扫「四个分类 × 五个桶值 × 四个查询词」：任何一格出现「hits > 0 且可见行为空 **且**
/// `only_elsewhere()` 为假」就是自相矛盾的一屏——那正是复审复现的形态：选中 `1.21` 后搜 `1.20`，
/// `rows` 非空所以 `only_elsewhere()` 也是假，于是页面落到「没有版本」的空态，而同屏写着「2 matches」。
///
/// （分类这一层本来就有 `only_elsewhere()`：站在「正式版」搜 `24w` 时列表空是**对的**，
/// 页面会说「N 个命中在别的类型里」并给出口——那不是矛盾，所以判据里要把它排除。）
///
/// 抓得住：把 `group::active_bucket` 的搜索放宽去掉（那一行删掉，这一格立刻红）。
/// 抓不住：像素上这两句话是否真的没同屏（那要出图，见 `DevDocs/artifacts/m107/`）。
#[test]
fn a_search_hit_never_lands_on_an_empty_list() {
    // A 组：**有搜索词**。桶可以是任意值（包括「桶列表里不存在」的过期值），分类也随便切——
    //    只要 hits > 0，可见行就不许为空。这一组是 P2-1 的正面判据。
    for category in Category::ALL {
        for bucket in [None, Some("1.21"), Some("1.20"), Some("24w"), Some("9.99")] {
            for query in ["1.20", "1.21", "24w", "zzz"] {
                let state = state_with(three_buckets(), query);
                let mut model = Download::from_snapshot(
                    &state,
                    category,
                    &InstallDraft::default(),
                    Lang::En.catalog(),
                );
                model.selected_major = bucket.map(str::to_string);

                let visible = rows::visible(&model);
                let contradiction = model.hits > 0 && visible.is_empty() && !model.only_elsewhere();
                assert!(
                    !contradiction,
                    "分类 {category:?} / 桶 {bucket:?} / 查询 {query:?}：命中数 {} 但可见行 0                      且没有「命中在别的类型」的口径 ⇒ 同屏自相矛盾",
                    model.hits
                );
            }
        }
    }

    // B 组：**没有搜索词**。只有「桶真的在当前分类里」时才有这条承诺；桶是过期值时列表空是
    // 正确行为（真正要防的是「选中项不在桶列表里」的幽灵态，那由 `rebuild_after_query` 的校验
    // 与 `a_stale_bucket_choice_cannot_hide_the_hits` 两条一起管）。
    for category in Category::ALL {
        let state = state_with(three_buckets(), "");
        let model = Download::from_snapshot(
            &state,
            category,
            &InstallDraft::default(),
            Lang::En.catalog(),
        );
        for bucket in model
            .buckets
            .iter()
            .filter_map(|bucket| bucket.key.as_deref())
        {
            let mut filtered = Download::from_snapshot(
                &state,
                category,
                &InstallDraft::default(),
                Lang::En.catalog(),
            );
            filtered.selected_major = Some(bucket.to_string());
            assert!(
                !rows::visible(&filtered).is_empty(),
                "分类 {category:?} / 真实桶 {bucket:?}：画出来的桶点下去居然是空列表"
            );
        }
    }
}

/// 一个**过期的桶选择**（模型里有个桶列表里不存在的 key）也挡不住搜索命中。
///
/// 这种 key 正常路径上不会出现（`rebuild_after_query` 会校验、点选只可能点到真实桶），但它是
/// 「幽灵选中」的极端形态：搜索结果必须仍然可见，否则用户会看着「N 个命中 + 空列表」。
#[test]
fn a_stale_bucket_choice_cannot_hide_the_hits() {
    let state = state_with(three_buckets(), "1.20");
    let mut model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    model.selected_major = Some("9.99".to_string());

    assert_eq!(model.hits, 2);
    assert_eq!(rows::visible(&model).len(), 2, "搜索期间过期桶不生效");
    assert_ne!(
        group::active_bucket(&model),
        Some("9.99"),
        "过期桶不许成为生效值"
    );
}

/// 搜索期间第二级桶**放宽到「全部」**（`active_bucket` = None），清空后原来的桶回来。
///
/// 抓得住：有人「修好」一致性的办法是把 `selected_major` 一起清掉（那会让用户的桶选择丢失，
/// 清空搜索后回不到原位）。
#[test]
fn searching_relaxes_the_bucket_without_losing_the_choice() {
    let state = state_with(three_buckets(), "1.20");
    let mut model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    model.selected_major = Some("1.21".to_string());

    assert!(model.has_query());
    assert_eq!(
        model.selected_major.as_deref(),
        Some("1.21"),
        "选择本身留着"
    );
    assert_eq!(
        group::active_bucket(&model),
        None,
        "搜索期间生效值是「全部」"
    );
    assert_eq!(rows::visible(&model).len(), 2, "两个 1.20 的行都看得见");
    assert!(group::bucket_selected(&model, None), "侧栏点亮「全部版本」");

    // 清空搜索：生效值回到用户原来选的那个桶，而它还在（buckets 由全量行聚合）
    let state = state_with(three_buckets(), "");
    let mut model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    model.selected_major = Some("1.21".to_string());
    assert_eq!(group::active_bucket(&model), Some("1.21"));
    assert_eq!(rows::visible(&model).len(), 2, "1.21 桶里两个正式版");
}

/// 没有搜索词时第二级桶照常过滤（这条不许被「放宽」误伤）。
#[test]
fn without_a_query_the_bucket_still_filters() {
    let state = state_with(three_buckets(), "");
    let mut model = Download::from_snapshot(
        &state,
        Category::All,
        &InstallDraft::default(),
        Lang::En.catalog(),
    );
    model.selected_major = Some("1.20".to_string());

    assert_eq!(rows::visible(&model).len(), 2);
    assert!(!group::bucket_selected(&model, None), "「全部」不点亮");
    assert!(group::bucket_selected(&model, Some("1.20")));
}

/// 复数口径：`1` 走单数 key，`>1` 走复数 key（复审 P2-4）。
///
/// 抓得住：把 `hits_text` 的 `count == 1` 分支删掉（英文重新变成「1 matches」）。
/// 抓不住：中文的「个」是否自然（中文两边同形，由译审看）。
#[test]
fn the_search_counts_are_pluralised() {
    let en = Lang::En.catalog();
    let zh = Lang::ZhCn.catalog();

    assert_eq!(rows::hits_text(en, 1), "1 match");
    assert_eq!(rows::hits_text(en, 2), "2 matches");
    assert_eq!(rows::hits_text(en, 0), "0 matches");
    assert_eq!(rows::hits_text(zh, 1), "1 个匹配");

    assert_eq!(
        rows::elsewhere_text(en, 1),
        "1 version matches, but it is in another type."
    );
    assert_eq!(
        rows::elsewhere_text(en, 3),
        "3 versions match, but they are in another type."
    );
    assert_eq!(
        rows::elsewhere_text(zh, 1),
        "有 1 个版本匹配，但它在别的类型里。"
    );
    assert_ne!(
        rows::elsewhere_text(zh, 1),
        rows::elsewhere_text(zh, 3),
        "中文也在两套 key 之间切换（不是硬编码同一个串）"
    );
}
