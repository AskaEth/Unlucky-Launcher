//! `Catalog` 的守门测试（从 `catalog.rs` 原样搬出来：那边已经贴着 400 行硬上限，
//! 而页面每加一批 key 都要在这个模块里占一行）。
//!
//! **零改动搬运**：用例、注释、断言一字未动；`use super::*` 指向的仍是 `crate::catalog`。
//! M94（下载页）与 M95（日志页）几乎同时撞到这堵墙、各自搬了一次：两条 branch 的**用例部分
//! 逐字相同**（解冲突时按 M94 那份合并，测试数量 7 条不变）。

use super::*;

/// 手写一套**故意缺条**的目录（不经过 `catalog!`，模拟后续 mission 的注入表）。
const PARTIAL_ZH: &[(Key, &str)] = &[
    (Key::NavInstances, "实例"),
    (Key::NavLogs, "日志"),
    (Key::NavSettings, "设置"),
    (Key::NavDownload, "下载"),
    (Key::InstancesNew, "新建实例"),
    (Key::InstancesImport, "导入"),
    (Key::InstancesBack, "返回实例列表"),
    (Key::InstancesPlay, "启动"),
    (Key::InstancesEmptyTitle, "未选择实例"),
    (
        Key::InstancesEmptyBody,
        "在左侧选择一个实例，查看它的版本、加载器与启动选项。",
    ),
    (Key::LoaderVanilla, "原版"),
    (Key::LaunchOutcomeNormal, "正常结束"),
    (Key::LaunchOutcomeCrash, "游戏崩溃"),
    // 缺：LaunchOutcomeSilent / LaunchFailedWithCode / FontCjk* / Settings* / CoreTextUntranslated
];

#[test]
fn built_in_catalogs_have_every_key() {
    // 抓得住：`catalog!` 的成对生成被绕开（有人改回「两套各写一遍」并漏了一条 / 抄重了一条）。
    //         判据是**集合相等**：`Key` 枚举拦住了「表里有、枚举没有」，这里拦反方向。
    // 抓不住：译文对不对（这只能是人工 / 后续的语言审查）。
    for lang in Lang::ALL {
        let catalog = Catalog::of(*lang);
        assert_eq!(catalog.missing(), Vec::new(), "{}", catalog.lang().tag());
        assert_eq!(catalog.diagnose(), None);

        let mut keys = catalog
            .entries()
            .iter()
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "{} 目录里有重复条目", lang.tag());
        assert_eq!(
            keys.len(),
            Key::ALL.len(),
            "{} 条目数与 key 数不等",
            lang.tag()
        );

        for key in Key::ALL {
            assert!(
                catalog.get(*key).is_some(),
                "{} 缺 {}",
                lang.tag(),
                key.name()
            );
        }
    }
    assert!(!Key::ALL.is_empty());
}

#[test]
fn key_names_are_unique_and_stable() {
    // 抓得住：同一个 key 被写成两个变体（名字重复会让日志/诊断指认失准）。
    // 抓不住：重命名变体对下游源码的影响（那是编译错误，本来就跑不过）。
    let mut names = Key::ALL.iter().map(|key| key.name()).collect::<Vec<_>>();
    let total = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), total);
    assert_eq!(Key::NavInstances.name(), "NavInstances");
}

#[test]
fn chinese_catalog_is_actually_chinese() {
    // 抓得住：整句漏译（英文被抄进中文表）；一条都没有说明这张表是真翻过的。
    // 抓不住：语义翻错、语气不合（机器能查的只有「与英文不同」这一层）。
    let en = Catalog::of(Lang::En);
    let zh = Catalog::of(Lang::ZhCn);
    for key in Key::ALL {
        let english = en.text(*key);
        assert_ne!(english, zh.text(*key), "{} 未翻译", key.name());
        assert!(!english.is_empty() && !zh.text(*key).is_empty());
    }
}

#[test]
fn placeholder_sets_match_across_languages() {
    // 抓得住：一处文案的 `{code}` / `{when}` 在另一套译文里丢了或写错名——
    //         那会让 render() 输出半截句子，或让参数静默失效。
    // 抓不住：占位符本身是不是正确的变量（那是调用方的职责，由页面侧测试守）。
    let en = Catalog::of(Lang::En);
    let zh = Catalog::of(Lang::ZhCn);
    for key in Key::ALL {
        let mut left = placeholders(en.text(*key));
        let mut right = placeholders(zh.text(*key));
        left.sort_unstable();
        right.sort_unstable();
        assert_eq!(left, right, "{}", key.name());
    }
    assert_eq!(placeholders("exit {code} at {when}"), vec!["code", "when"]);
    assert_eq!(placeholders("没有占位符"), Vec::<&str>::new());
}

#[test]
fn missing_rows_are_diagnosed_in_plain_words() {
    // 抓得住：缺译诊断的**可读性**（必须点名语言、key 名与英文原文）——注入目录缺条时靠它定位。
    // 抓不住：内置目录真的缺译（内置表由 catalog! 结构保证，见上一条测试）。
    let partial = Catalog::from_entries(Lang::ZhCn, PARTIAL_ZH);
    assert_eq!(partial.missing().len(), Key::ALL.len() - PARTIAL_ZH.len());

    let report = partial.diagnose().expect("应当报缺译");
    assert!(report.starts_with("zh-CN 缺 "), "{report}");
    assert!(report.contains("LaunchOutcomeSilent"), "{report}");
    assert!(
        report.contains("Suspected silent failure"),
        "诊断要带上英文原文当参考：{report}"
    );
    assert!(
        !report.contains("NavInstances"),
        "已译的 key 不该出现在报告里：{report}"
    );
}

#[test]
fn missing_translation_never_falls_back_silently() {
    // 抓得住：缺译被静默顶替（回落英文 / 回落中文 / 返回空串）——那会产出「半句中文半句英文」的界面。
    // 抓不住：MISSING 标记真的被画到屏幕上（那是 UI 侧截图验收的事）。
    let partial = Catalog::from_entries(Lang::ZhCn, PARTIAL_ZH);
    assert_eq!(partial.get(Key::LaunchOutcomeSilent), None);
    assert_eq!(partial.text(Key::LaunchOutcomeSilent), MISSING);
    assert_eq!(
        partial.render(
            Key::LaunchFailedWithCode,
            &[("code", "0"), ("reason", "JVM 拒绝启动")]
        ),
        "⟪missing:LaunchFailedWithCode⟫"
    );
}

#[test]
fn render_substitutes_and_keeps_unfilled_placeholders_visible() {
    // 抓得住：占位符替换的回归（漏参时**保留** `{code}` 而不是清空、不 panic）。
    // 抓不住：调用方传没传对值（`("code", "0")` 是不是真的退出码）。
    let zh = Catalog::of(Lang::ZhCn);
    assert_eq!(
        zh.render(
            Key::LaunchFailedWithCode,
            &[("code", "0"), ("reason", "mod 冲突")]
        ),
        "启动失败（退出码 0）：mod 冲突"
    );
    assert_eq!(
        zh.render(Key::LaunchFailedWithCode, &[("code", "0")]),
        "启动失败（退出码 0）：{reason}"
    );
    assert_eq!(zh.text(Key::InstancesNew), "新建实例");
    assert_eq!(zh.lang(), Lang::ZhCn);
}

/// M99：把单张条目表按域拆到 `catalog/<域>.rs` 之后，`key -> (en, zh-CN)` 必须**逐条逐字节**不变。
///
/// **抓得住**：① 某个 key 整条被搬丢（金表有、现在没有 ⇒ 条数或 key 名对不上）；② 两条 key 的译文
/// 互抄 / 同 key 被覆盖成别人的译文（该行 en 或 zh 不等）；③ 某条译文改了一个字符（逐字节比，
/// **不是**数条数——「A 丢了一条、B 复制成两条」条数照旧）；④ 域的顺序被调换（`Key` 的 `Ord` 与
/// `entries()` 的声明顺序都靠它，金表按原顺序比）。
/// **抓不住**：译文本身翻得对不对——金表是「拆分前那一份」的快照，只能证明没变，不能证明翻得好。
///
/// 金表怎么来的：拆分前在这里临时加过一条 `println!("GOLD\t{}\t{}\t{}", …)` 的用例，把
/// `Key::ALL` ×（`Catalog::of(En)`, `Catalog::of(ZhCn)`）导出成 72 行 TSV，再原样抄成
/// `catalog/golden.rs`；那条导出用例随后已删除（不在提交里）。
#[test]
fn split_keeps_every_translation() {
    let en = Catalog::of(Lang::En);
    let zh = Catalog::of(Lang::ZhCn);
    let actual = Key::ALL
        .iter()
        .map(|key| (key.name(), en.text(*key), zh.text(*key)))
        .collect::<Vec<_>>();

    assert_eq!(
        actual.len(),
        super::golden::len(),
        "key 条数与拆分前不一致（某条被搬丢，或多出来一条）"
    );

    for (index, expected) in super::golden::rows().enumerate() {
        let got = actual[index];
        assert_eq!(
            got.0, expected.0,
            "第 {index} 条的 key 变了：拆分的域顺序被打乱，或某条被搬到了别处"
        );
        assert_eq!(got.1, expected.1, "{} 的英文译文变了", expected.0);
        assert_eq!(got.2, expected.2, "{} 的中文译文变了", expected.0);
    }
}

/// 用户可见文案里**不许**出现内部决策编号（`(D13)` / `(M175)` 这类）。
///
/// 为什么要在目录这一层守：`(D13)` 对用户没有任何意义（他不知道 D13 是什么），而这类串一旦
/// 混进来就再没人记得删。M197 出图时实测到下载目录的基准版本徽标写着 `基准版本（D13）`。
///
/// 变异证明：把 `SettingsJavaDownloadBaseline` 的译文改回 `Baseline (D13)` / `基准版本（D13）`
/// ⇒ 本用例红（中英两侧都扫）。判据是 `(`/`（` + `D`/`M` + 数字，不误伤 `(404)` / `(5xx / 429)`
/// 这类真对用户有意义的括号短语。
#[test]
fn no_user_visible_string_mentions_an_internal_decision_id() {
    for (key, en, zh) in super::golden::rows().copied() {
        for (side, text) in [("en", en), ("zh-CN", zh)] {
            assert!(
                !mentions_decision_id(text),
                "{key} 的{side}译文里出现了内部决策编号：{text}"
            );
        }
    }
}

/// 用户可见文案里**不许**出现 markdown 反引号。
///
/// 渲染层没有内联代码通道（M175 r3 复审已核实 `RichText|markdown|Span::|spans(` 全树只命中
/// 进度条与注释），反引号会原样上屏。M175 修过三处、M197 又修了三处；M168 的
/// `FontCjkStatusHint` 是最后一条（M197 复审 r1 指出的），这里一并钉住。
///
/// 变异证明：给任意一条译文加一对反引号 ⇒ 本用例红。
#[test]
fn no_user_visible_string_carries_markdown_backticks() {
    for (key, en, zh) in super::golden::rows().copied() {
        for (side, text) in [("en", en), ("zh-CN", zh)] {
            assert!(
                !text.contains('`'),
                "{key} 的{side}译文里还有 markdown 反引号：{text}"
            );
        }
    }
}

/// 串里有没有「(D13)」「（M175）」这类内部编号（全角半角括号都算）。
fn mentions_decision_id(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    chars.windows(3).any(|window| {
        (window[0] == '(' || window[0] == '（')
            && (window[1] == 'D' || window[1] == 'M')
            && window[2].is_ascii_digit()
    })
}
