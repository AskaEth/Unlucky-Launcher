//! 诊断卡文案解析的守门测试（从 `diagnostic.rs` 拆出：那边撞上了 400 行红线，M392）。
//!
//! 测的是 `diagnostic::texts_of` 的几条口径（M395 `BUILTIN10` 起不再是三条）：目录命中 ⇒ 两种语言
//! 都走目录；**内置结论同样走目录**（中文必须与 core 原话逐字相等）；参数缺了（老会话）⇒ 中文画
//! core 原话、英文画兜底 key；目录里没有的结论 ⇒ 同上。以及**目录 ↔ 规则表逐条对齐**（那条只能在
//! 看得见 `unlucky-core` 与 `unlucky_i18n` 的地方跑）。

use std::collections::BTreeMap;

use unlucky_core::diagnose::loader::RuleLoader;

use super::diagnostic::texts_of;
use super::model::{Confidence, Finding, Severity};

/// **目录 ↔ 规则表逐条对齐**（M392；M400 `DIAGHINT` 收紧）：这是「中文侧只是副本」那句话的执行器。
///
/// 为什么必须在这里：要同时看得见 `unlucky_core`（规则表）与 `unlucky_i18n`（目录）。
/// `unlucky-presenter` 不依赖 i18n，`unlucky-core` 的 `Cargo.toml` 不在本 mission 的 scope 里，
/// 所以不可能放在它们任何一边。
///
/// **M400 把判据从「两边一致」收紧成「两边都必须有」**：M392 那版判的是
/// `keys.hint.is_some() == rule.cause.detail.is_some()`，形状上允许一条规则**既没有 `detail`
/// 也没有 `hint`**——那 21 条规则当时正是这个形状，于是守卫对它们静默放行。现在每条规则都必须
/// 有 `detail` 与 `hint`，且中文逐字相等（同一句话存两份，改一边就要改另一边）。
///
/// 抓得住：YAML 改了 `summary` / `detail` 而目录没跟上（或反过来）、`cause.key` 的命名族
/// 被改动、某条规则被删了而键表还留着、**新增一条规则忘了写 `detail`**（或缺 `hint`）、
/// **删掉某条规则的 `detail`**——这一支先红在循环前那句总量断言「无 detail 的规则数必须是 0」
/// 上（M400 r1 复审 P2-2：它原先挂在循环之后，于是被循环里的逐条 panic 抢先，永远红不了）。
/// 抓不住：英文翻得准不准（人工评审）、建议是否真的可操作（人工评审）。
#[test]
fn diagnose_catalog_matches_the_rule_table() {
    let loader = RuleLoader::embedded();
    let zh = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::ZhCn);

    // **总量先判（M400 r1 复审 P2-2 修的就是这里）**：这条断言原本挂在循环之后，于是它**永远
    // 不会红**——循环里那句 `unwrap_or_else(panic!)` 对缺 `detail` 的规则先炸，计数恒为 `0 == 0`。
    // 挪到循环之前它才真的是「整批删 / 整批漏写时先红在总数上，读的人不用翻 64 次」。
    // 只多一次 `.count()`（64 条的比较），不把主循环拆成两趟。
    let without_detail = loader
        .rule_set()
        .rules()
        .iter()
        .filter(|compiled| compiled.rule().cause.detail.is_none())
        .count();
    assert_eq!(
        without_detail, 0,
        "无 detail 的规则数必须是 0（M400 补齐了最后 21 条）"
    );

    let mut seen = 0;

    for compiled in loader.rule_set().rules() {
        let rule = compiled.rule();
        let id = compiled.id();
        seen += 1;

        assert_eq!(
            rule.cause.key,
            format!("diagnose.cause.{id}"),
            "规则 {id} 的 cause.key 不在命名族里"
        );

        let keys = unlucky_i18n::catalog::diagnose_keys(id)
            .unwrap_or_else(|| panic!("规则 {id} 在诊断键表里没有条目"));
        assert_eq!(
            keys.title.name(),
            format!("DiagnoseTitle{}", pascal(id)),
            "规则 {id} 的标题键不是机械推导出来的变体"
        );
        assert_eq!(
            Some(zh.text(keys.title)),
            Some(rule.cause.summary.as_str()),
            "规则 {id} 的 YAML summary 与目录中文不逐字相等"
        );
        // 「每条规则都必须有一条能照着做的建议」的两个半边，缺哪边都当场说清是哪边。
        let detail = rule.cause.detail.as_deref().unwrap_or_else(|| {
            panic!("规则 {id} 的 YAML 里没有 cause.detail：每条规则都必须给一条建议")
        });
        let hint = keys.hint.unwrap_or_else(|| {
            panic!("规则 {id} 在诊断键表里没有 hint：每条规则都必须有中英两份建议")
        });
        assert_eq!(
            zh.text(hint),
            detail,
            "规则 {id} 的 YAML detail 与目录中文不逐字相等（同一句话存两份，只改一边就会红）"
        );
    }

    assert_eq!(seen, 64, "嵌入式规则表的条数变了");
}

/// 英文标题必须**代入捕获组**：不代就画出字面的 `{name}`。
#[test]
fn english_titles_substitute_the_capture_groups() {
    let finding = Finding {
        rule: "integrity.unsatisfied_link".to_string(),
        cause_key: "diagnose.cause.integrity.unsatisfied_link".to_string(),
        severity: Severity::Critical,
        confidence_band: Confidence::High,
        confidence: 85,
        groups: BTreeMap::from([("name".to_string(), "lwjgl_opengl.dll".to_string())]),
        system_text: None,
        action: None,
    };

    let (title, hint) = texts_of(
        &unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::En),
        &finding,
    );
    assert!(
        title.contains("lwjgl_opengl.dll"),
        "英文标题没代入 {{name}}：{title}"
    );
    assert!(!title.contains('{'), "标题里还剩占位符：{title}");
    let hint = hint.expect("这条规则有 detail，提示不该缺席");
    assert!(!hint.contains('{'), "提示里还剩占位符：{hint}");
    assert!(
        title
            .chars()
            .all(|ch| !matches!(ch as u32, 0x2E80..=0x9FFF)),
        "英文标题里出现了 CJK：{title}"
    );
}

/// **内置结论的口径（M395 `BUILTIN10`）**：10 条全部走目录模板，中文与 core 原话**逐字相等**。
///
/// 这是塔的硬门禁「中文零倒退」的执行器，也是「模板真的在渲染」的证明（不是退回 `system_text`）：
///
/// 1. 逐条比 `texts_of(zh).0 == core 的 message`——模板 + `groups` 渲染出来必须**一个字不差**；
/// 2. 英文侧不许剩 `{…}` 占位符、不许有 CJK、也不许是「未翻译的系统文案」兜底 key；
/// 3. 改一个参数值，中文标题必须跟着变——否则上面两条可能只是「恰好退回了原话」。
///
/// 抓得住：core 的 `format!` 与目录 zh 漂了（改了一边没改另一边）、参数名对不上（画 `{exit_code}`）、
/// 目录里漏一条内置结论（英文变成兜底 key）、`groups` 没传到页面模型。
/// 抓不住：英文翻得准不准（人工评审）、图标/排版。
#[test]
fn builtin_conclusions_read_the_same_in_chinese() {
    let zh = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::ZhCn);
    let en = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::En);

    // 夹具不是手抄的：`demo::builtin()` 跑真引擎（理由见 `pages/logs/demo.rs` 的文档）。
    // 那一档靠 `ULL_LOGS_FIXTURE=builtin` 挑，取值解析也在这里守一次（拼错会静默退成 crash）。
    assert_eq!(
        super::demo::State::of_env_value(Some("builtin")),
        super::demo::State::Builtin,
        "出图通道的取值必须认 builtin"
    );
    let findings = super::demo::builtin().findings;
    let rules: Vec<&str> = findings.iter().map(|f| f.rule.as_str()).collect();
    assert_eq!(
        rules,
        vec![
            "signal.access_violation",
            "signal.dll_not_found",
            "signal.stack_buffer_overrun",
            "signal.sigkill",
            "signal.jvm_error",
            "signal.silent_failure",
            "hs_err.access_violation",
            "hs_err.out_of_memory",
            "hs_err.stack_overflow",
            "hs_err.internal_error",
        ],
        "10 条内置结论一条都不能少（少了就说明它们又变成「没人译」的那半）"
    );

    for finding in &findings {
        let original = finding
            .system_text
            .as_deref()
            .expect("真产品路径（presenter）会给 core 的原话");
        let (title, hint) = texts_of(&zh, finding);
        assert_eq!(
            title, original,
            "{} 的中文界面与 core 原话不逐字相等（模板或参数漂了）",
            finding.rule
        );
        assert!(hint.is_none(), "内置结论没有第二段建议：{}", finding.rule);

        let (english, _) = texts_of(&en, finding);
        assert!(
            !english.contains('{'),
            "{} 的英文模板还有没代进去的占位符：{english}",
            finding.rule
        );
        assert!(
            !english
                .chars()
                .any(|ch| matches!(ch as u32, 0x2E80..=0x9FFF)),
            "{} 的英文里出现了 CJK：{english}",
            finding.rule
        );
        assert_ne!(
            english,
            en.text(unlucky_i18n::catalog::Key::CoreTextUntranslated),
            "{} 还没进目录（英文界面只能画兜底 key）",
            finding.rule
        );
    }

    // **分支证明**：改一个参数值，中文标题必须跟着变。若上面那条相等只是「退回了原话」，
    // 这里会红（原话是 core 拼死的，不会跟着 `groups` 变）。
    let mut probe = findings
        .iter()
        .find(|finding| finding.rule == "signal.silent_failure")
        .expect("静默失败那条必须在")
        .clone();
    probe
        .groups
        .insert("error_lines".to_owned(), "99".to_owned());
    let (title, _) = texts_of(&zh, &probe);
    assert!(
        title.contains("99 条错误行"),
        "参数没有真的代进模板（渲染路径可能退回了 system_text）：{title}"
    );
}

/// **core 不知道模块名 / 失败帧时**：空串参数由目录补词，两种语言都读得通。
///
/// 这一支最容易被写成「core 顺手填个『未知模块』」——那就是把中文塞进英文界面。core 传空串，
/// 词在目录里（`DiagnoseBuiltinUnknownModule` / `DiagnoseBuiltinUnknownFrame`）。
#[test]
fn a_builtin_without_a_module_gets_the_word_from_the_catalog() {
    let report = unlucky_core::diagnose::hs_err::HsErrReport {
        kind: unlucky_core::diagnose::hs_err::HsErrKind::AccessViolation,
        ..Default::default()
    };
    let mut input = unlucky_core::diagnose::engine::DiagnosisInput::new();
    input.push_hs_err(report);
    let core = unlucky_core::diagnose::engine::synthetic_findings(&input)
        .into_iter()
        .find(|finding| finding.rule_id == "hs_err.access_violation")
        .expect("没有失败帧的 hs_err 也要给结论");
    assert_eq!(
        core.groups.get("module_name").map(String::as_str),
        Some(""),
        "core 不知道时传空串（不是中文词）"
    );

    let finding = super::demo::core_finding(&core);
    let zh = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::ZhCn);
    let en = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::En);
    let (chinese, _) = texts_of(&zh, &finding);
    assert_eq!(chinese, core.message, "中文侧仍要与 core 原话逐字相等");
    assert!(chinese.contains("未知模块"), "{chinese}");
    let (english, _) = texts_of(&en, &finding);
    assert!(english.contains("unknown module"), "{english}");
    assert!(!english.contains('{'), "{english}");
}

/// **老会话（缺模板参数）不倒退**：M395 之前写的 JSONL 没有内置结论的参数。
///
/// 照直渲染会画出 `{error_lines}` 字面量——那正是硬门禁要防的「中文界面倒退」。所以参数缺时退回
/// core 的中文整句（英文侧照旧诚实兜底）。
#[test]
fn a_builtin_conclusion_without_params_falls_back_to_the_core_text() {
    let original = "退出码为 0 但疑似静默失败（3 条错误行）：游戏可能刚起来就被 mod 异常终止";
    let finding = Finding {
        rule: "signal.silent_failure".to_string(),
        cause_key: "diagnose.cause.signal.silent_failure".to_string(),
        severity: Severity::Warning,
        confidence_band: Confidence::Low,
        confidence: 40,
        groups: BTreeMap::new(),
        system_text: Some(original.to_string()),
        action: None,
    };

    let zh = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::ZhCn);
    let en = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::En);
    assert_eq!(texts_of(&zh, &finding).0, original, "中文界面必须零倒退");
    assert_eq!(
        texts_of(&en, &finding).0,
        en.text(unlucky_i18n::catalog::Key::CoreTextUntranslated),
        "英文界面应承认这句没译文"
    );
    // 提示：模板参数缺 ⇒ 也不画 hint（内置结论本来就没有第二段）。
    assert!(texts_of(&zh, &finding).1.is_none());
}

/// 目录里**没有**条目的结论（未来的新规则）：中文画 core 原话、英文画兜底 key。
#[test]
fn a_cause_outside_the_catalog_still_shows_the_core_text_in_chinese() {
    let finding = Finding {
        rule: "future.rule_not_in_catalog".to_string(),
        cause_key: "diagnose.cause.future.rule_not_in_catalog".to_string(),
        severity: Severity::Info,
        confidence_band: Confidence::High,
        confidence: 90,
        groups: BTreeMap::new(),
        system_text: Some("core 说的一句话".to_string()),
        action: None,
    };
    let zh = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::ZhCn);
    let en = unlucky_i18n::catalog::Catalog::of(unlucky_i18n::Lang::En);
    assert_eq!(texts_of(&zh, &finding).0, "core 说的一句话");
    assert_eq!(
        texts_of(&en, &finding).0,
        en.text(unlucky_i18n::catalog::Key::CoreTextUntranslated)
    );
}

/// `PascalCase`：`.` 与 `_` 都是分词边界（与 `catalog/diagnose.rs` 的变体名同一规则）。
fn pascal(rule: &str) -> String {
    rule.split(['.', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| part[..1].to_uppercase() + &part[1..])
        .collect()
}
