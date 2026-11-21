//! **N2 守门**：presenter 产出的界面字符串不许含 CJK（汉字 / 假名 / 全角标点）。
//!
//! ## 为什么需要这条
//!
//! `unlucky_i18n::bridge` 的模块文档最后一行写着「后续在 `crates/presenter` 里加一条守门测试
//! （例如：`*_view` 的字符串字段不许含 CJK 字符）」——这条就是那个「后续」。它要防的现场是
//! 「英文界面里冒出整句中文」（R1 说界面文案只从 `i18n::catalog` 的 key 来，R2 说 core/presenter
//! 的中文是日志 / CLI / 诊断报告的文案）。**这条测试是抽样哨兵，不是那条规则的执行器。**
//!
//! ## ⚠ 覆盖面：探了哪些、没探哪些（M383 r2 复审 p2）
//!
//! **被探的 8 个类型逐个列名**（不写「探了 N 个」这类会随新增而变谎的话）：
//! `Diagnosis`、`DiagnosisReport`、`LaunchFailureView`、`LogSession`、`LogLine`、`VersionSummary`、
//! `NoticeView`（以上 7 个在 `dto/`），外加 `toast::Toast`（在 `crate::toast`，不在 `dto/`）。
//!
//! 这 8 个里**只有 `LaunchFailureView` 与 `NoticeView` 两个是 `*View`**；`dto/` 下**其余 `*View`
//! 一律未探**（`dto/` 共 58 个 `*View`，例如 `JavaPageView` / `AdoptProblemView` /
//! `InstallFailureView` / `ContentResults`）。非 `*View` 的结构体同样只有上面 5 个被探到。
//!
//! 抓得住：列名的那 8 个类型的**字符串字段**里出现 CJK。探针用穷尽解构构造，所以给它们**加一个
//! 字段**时本文件编译不过（逼一次判断）。抓不住：**新增一个 DTO 类型**（没有全局遍历，加了不会被
//! 抓到），以及**方法返回的字符串**——判定按字段，`fn label() -> &'static str` 这类不在字段里。
//!
//! 已知漏网的样本就在这次新建的同族文件里：`crates/presenter/src/prefs/ui_keys.rs` 的
//! `TileSize::label()` 返回 `"小"` / `"中"` / `"大"`，是 presenter 产出的、要上设置页分段控件的
//! 中文。它是**既存问题**（M383 只是把 `TileSize` 从 `prefs.rs` 纯搬移进 `ui_keys.rs`）。
//! 它曾被记在「与 64 条诊断规则 title/hint 同批清债」（N3），但 M392 的 scope 不含
//! `prefs/ui_keys.rs`，所以**它还在**：今天仍然是一份未修的 N3 残余（`label()` 是方法返回的
//! 字符串，本文件的判据按字段扫，本来也抓不到它）。
//!
//! 结论：本文件绿只说明「上面列名的 8 个类型，此刻字段里没有白名单之外的 CJK」。它**不是**
//! 「presenter 不再产中文」，也不是任何形式的全量保证。
//!
//! ## 判据复用
//!
//! 码位区间逐位抄自 `crates/i18n/src/bridge.rs:117` 的 `is_cjk`（`0x2E80..=0x9FFF` 含 CJK 部首 /
//! 假名 / 统一表意文字与 `0x3000` 段标点，外加 `0xF900..=0xFAFF` 兼容区、`0xFF00..=0xFFEF` 全角）。
//! 那边是**私有** fn，真正复用要给它 presenter 加 `unlucky-i18n` 的 dev-dependency——那条路要改
//! `crates/presenter/Cargo.toml`，M383 的 scope 不含它。抄一份的代价写在这里：**改判据要两边一起改**
//! （`bridge.rs` 的区间与这里的区间，任一改了就漂，且没有任何测试抓得住这种漂——除非把依赖加上）。
//!
//! ## 白名单是**具名字段**，不是「跳过所有 title」
//!
//! 白名单的键形状固定为 `<类型>.<字段>`，理由分三类，逐条写在 [`KNOWN_CJK_FIELDS`] 旁边：
//!
//! | 类别 | 例子 | 还要修吗 |
//! |---|---|---|
//! | 用户数据 | `LogSession.instance`（实例是用户起的名字） | 永不修 |
//! | 原始载荷 | `LogLine.text`（游戏自己打印的行） | 永不修 |
//! | **具名透传** | `Diagnosis.system_text`（目录没条目时 core 的原话） | 那 10 条内置结论做完模板后删 |
//! | **已登记的技术债** | `LaunchFailureView.detail` / `Toast.text` / `NoticeView.*`（presenter 拼句） | 修完删白名单条目 |
//!
//! 一个字段在不在白名单里，由**字段名**决定，所以「新增一个 title 字段」不会自动被放行——
//! 它会红，然后有人必须来判一次。
//!
//! ## M392：`Diagnosis` 从「3 条豁免」变成「1 条具名透传」
//!
//! 之前 `Diagnosis.title` / `.hint` / `.fix` 三条都靠白名单放行，而那三条里**只有 `fix` 该豁免**
//! （它是 6 个 ASCII 工具 id，`FixActionKind::agent_tool()`）。`title` / `hint` 不是「暂时改不动」，
//! 是**设计错了**：它们装的是 core 规则表的中文原文，界面直接画 ⇒ 英文界面冒整句中文。
//!
//! 现在那两个字段**删掉**了（界面按 `cause_key` 从 i18n 目录取文案，见
//! `unlucky_presenter::dto::logs` 的 `Diagnosis` 文档），`fix` 那条例外也不再需要——
//! 白名单里那条 `Diagnosis.fix` 其实是**给测试夹具自己开的口子**：夹具把 `fix` 写成了中文散文
//! 「打开显卡厂商的驱动下载页」，而生产中这个字段永远是 `java.set` 这种工具 id。
//! 夹具改回真工具 id，条目就删掉了。净账：**−3（title/hint/fix）、+1（system_text）**。
//!
//! ## 红样本
//!
//! [`the_red_sample_a_chinese_system_text_fails_the_guard`] 拿内置结论的 `system_text`（core 拼好的
//! 中文句子）当红样本：它证明判据**真的会红**（而不是一份永远绿的清单），同时证明那个字段确实靠
//! 白名单才放行——把 `Diagnosis.system_text` 从白名单删掉，主用例立刻红。
//! [`a_rule_diagnosis_only_leaks_through_the_named_passthrough`] 反向钉住「除 `system_text` 之外
//! 一个都不许有中文」：规则结论的其它字段必须是机器串。

/// 是不是 CJK 字符（判据同 `unlucky_i18n::bridge::is_cjk`，见模块文档）。
///
/// `pub(crate)` 是给 `probes` 子模块用的（`probes.rs` 里 `use super::is_cjk`）：判据只有这一份。
/// 注意本文件是**测试 target 的 crate root**，在根模块上写 `pub(super)` 会报
/// 「too many leading `super` keywords」。
pub(crate) fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

/// 已知**含中文且今天改不动**的字段（键形状 `<类型>.<字段>`）。
///
/// 每一条都要能说出「谁在产这段中文」与「它何时消失」，说不出的不许加。
const KNOWN_CJK_FIELDS: &[&str] = &[
    // —— 用户数据 / 原始载荷：这两类永远豁免（翻译它们等于篡改用户的东西）——
    // 实例名是用户起的（`生存` 是默认名）。
    "LogSession.instance",
    // 游戏自己的日志行（含 mod 打印的中文）。
    "LogLine.text",
    // —— 唯一一条「具名透传」的豁免（M392）——
    // `core::diagnose` 的内置结论（`signal.*` ×6 与 `hs_err.*` ×4）没有目录条目：句子是
    // `format!` 拼的、带动态参数（退出摘要 / 错误行数 / hs_err 的模块名与失败帧），塞不进模板。
    // 界面口径：中文界面直出这句（保持既有行为，零倒退），英文界面画
    // `Key::CoreTextUntranslated`（`[system text not translated]`）。
    // **消失条件**：那 10 条结论做成模板 key + 参数透传（另一条 mission）之后删掉这条。
    "Diagnosis.system_text",
    // —— 技术债：core/presenter 的失败原文（N1 的文案边界契约没有调用点）——
    // `LaunchFailureView.detail` 是 core 的错误原文（`版本 JSON 没有 mainClass` 这种）。
    // 结构化成 `LaunchFailureStage` + 变量是 N1/P2 的事；`stage` 这一半已经是枚举了。
    "LaunchFailureView.detail",
    // presenter 自己拼的 Toast（`host/` 里 `format!("拉取版本清单失败：{reason}")` 共数十处）。
    // 改法是整条走 i18n key（N1），不是逐句翻译。
    "Toast.text",
    // 通知条标题 / 正文：同 Toast，都是 presenter 拼句。
    "NoticeView.title",
    "NoticeView.body",
];

#[path = "view_cjk/probes.rs"]
mod probes;

use probes::*;
/// **红样本**：内置结论的 `system_text`（core 拼好的中文句子）真的会红，而它靠白名单才绿。
///
/// 这条用例的三个断言合起来才是证据：
/// 1. 传**空白名单**时判据真的报出来（否则主用例的绿是假的——判据恒返回空也一样绿）；
/// 2. 报的字段就是 `Diagnosis.system_text`（不是别的字段顺手红）；
/// 3. 白名单里确实有这一条（否则主用例早该红，说明主用例没扫这个字段）。
///
/// M392 之前这条红样本打的是 `Diagnosis.title`；那个字段已经**删掉**（界面不再画 core 的中文
/// 结论），白名单也从 3 条 `Diagnosis.*` 降到 1 条具名透传。守据没变，红样本跟着换成今天的形状。
#[test]
fn the_red_sample_a_chinese_system_text_fails_the_guard() {
    let report = chinese_diagnosis();
    let probes = diagnosis_probes(&report);

    let red = violations(&probes, &[]);
    assert!(
        !red.is_empty(),
        "红样本必须让判据变红：中文的诊断结论竟然被判为合格"
    );
    let fields: Vec<&str> = red.iter().map(|(field, _)| *field).collect();
    assert!(
        fields.contains(&"Diagnosis.system_text"),
        "红样本必须从 Diagnosis.system_text 报出来，实际：{fields:?}"
    );
    assert!(
        KNOWN_CJK_FIELDS.contains(&"Diagnosis.system_text"),
        "主用例之所以还绿，靠的就是这条白名单；它不在列表里说明主用例没扫到这个字段"
    );

    // 同一条结论换成英文 ⇒ 判据立刻绿（证明报的是内容而不是字段名）。
    let mut english = chinese_diagnosis();
    english.diagnosis.system_text = Some("Exited with code 0 but never really started".to_string());
    let green = violations(&diagnosis_probes(&english), &[]);
    assert!(
        green.is_empty(),
        "换成英文之后不该再有违规，实际：{green:?}"
    );
}

/// **规则结论**（目录命中那一类）在空白名单下必须**一条都不报** —— 即使它的 `system_text`
/// 还是 core 的中文原话。
///
/// 这条钉的是 M392 的取舍：`system_text` 不是「允许 DTO 装中文」，而是「目录没条目时那张底牌」。
/// `rule_diagnosis()` 的 `system_text` 是中文，但它被白名单放行**只是因为那条具名条目**；
/// 换成空白名单会红一次 —— 于是「加字段塞中文」这条路仍然会被看见。
#[test]
fn a_rule_diagnosis_only_leaks_through_the_named_passthrough() {
    let report = rule_diagnosis();
    let probes = diagnosis_probes(&report);

    let red = violations(&probes, &[]);
    let fields: Vec<&str> = red.iter().map(|(field, _)| *field).collect();
    assert_eq!(
        fields,
        vec!["Diagnosis.system_text"],
        "规则结论里除了 system_text 之外的字段都必须是机器串（实际：{fields:?}）"
    );

    let green = violations(&probes, KNOWN_CJK_FIELDS);
    assert!(
        green.is_empty(),
        "白名单应当只放过 system_text 这一条，实际：{green:?}"
    );
}

/// 主守门：白名单**之外**的字段一个 CJK 都不许有。
#[test]
fn view_strings_carry_no_cjk_outside_the_named_allowlist() {
    let mut probes = clean_probes();
    let report = chinese_diagnosis();
    probes.extend(diagnosis_probes(&report));
    let found = violations(&probes, KNOWN_CJK_FIELDS);
    assert!(
        found.is_empty(),
        "这些字段漏了中文（要么改产出侧，要么在白名单里具名登记并写清债）：{found:?}"
    );
}

/// 白名单的键形状必须是 `<类型>.<字段>`：不许出现通配或裸字段名。
///
/// 抓得住：有人把「跳过所有 title」写成 `"title"` 或 `"*.title"` —— 那等于把守门废掉一半。
#[test]
fn the_allowlist_is_field_level_not_wildcards() {
    for field in KNOWN_CJK_FIELDS {
        assert!(
            field.contains('.'),
            "白名单条目必须是 <类型>.<字段>：{field}"
        );
        assert!(!field.contains('*'), "白名单不许用通配：{field}");
        let (type_name, field_name) = field.split_once('.').expect("已断言含 .");
        assert!(
            type_name.chars().all(|ch| ch.is_ascii_alphanumeric()),
            "类型名应是一个 DTO 名：{field}"
        );
        assert!(
            field_name
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'),
            "字段名应是 snake_case：{field}"
        );
    }
    // 用户数据 / 原始载荷那两条**永不修**的豁免也在这里重申一遍：它们是决定，不是遗忘。
    assert!(KNOWN_CJK_FIELDS.contains(&"LogSession.instance"));
    assert!(KNOWN_CJK_FIELDS.contains(&"LogLine.text"));
}

/// 判据自身的边界（照抄 `bridge.rs` 那段码位，随手钉一遍防止抄错）。
#[test]
fn the_cjk_predicate_matches_the_i18n_boundary() {
    for ch in ['中', '文', 'あ', 'ア', '（', '：', '，', '　'] {
        assert!(is_cjk(ch), "{ch} 应判为 CJK");
    }
    for ch in ['A', 'z', '0', '-', ':', '/', '.', '~', 'é'] {
        assert!(!is_cjk(ch), "{ch} 不该判为 CJK");
    }
    // 品牌专名与版本号必须放行（它们本来就要原样显示）。
    for text in [
        "Fabric",
        "NeoForge 21.1.72",
        "1.20.4",
        "optifine_1.20.4_HD_U_I6",
    ] {
        assert!(!text.chars().any(is_cjk), "品牌/版本串不该含 CJK：{text}");
    }
}
