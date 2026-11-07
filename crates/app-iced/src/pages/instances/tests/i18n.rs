//! 实例页的**文案守门**（M120）：中文态不许漏英文、切语言必须真的变、运行状态的数字不许变形。
//!
//! | 用例 | 抓得住什么 |
//! |---|---|
//! | [`the_chinese_page_has_no_english_words`] | 目录里某条中文忘了翻（或翻完还留着英文词） |
//! | [`every_instances_text_changes_with_the_language`] | 两套语言写成一样（= 切换语言时那一处不动，看着像「已 i18n」其实是死文案） |
//! | [`the_running_line_keeps_its_number_and_separator`] | `Running · pid 1234` 的数字 / `pid` / `·` 被翻译走样 |
//! | [`every_english_literal_in_the_page_sources_is_whitelisted`] | 页面里写死**任何**英文（旧串或新写的，白名单判定；r1 之前是黑名单，对新写的英文是哑的） |
//! | [`every_instances_key_is_referenced_by_the_page`] | 目录里加了 key、页面没人用（死 key，M117 那七条就是这么来的） |
//!
//! ## 边界（别把这张表读成「视图覆盖」）
//!
//! iced 0.14 的 `Element` **没有内省接口**（`instances/tests.rs` 里那条「为什么读源码」写的就是它），
//! 所以「上屏文本」取样只能从页面取词的那**一个入口** [`Texts`] 拿：页面用它画、测试用同一个
//! `Texts` 断言，不是另写一份平行实现。「页面又冒出一条**新**硬编码英文字面量」由
//! [`every_english_literal_in_the_page_sources_is_whitelisted`] 兜（白名单）；**中文**写死的
//! 字面量它看不见，那只能靠 `Texts::of(` 的反向锚与出图人工看。
//! 「模型这一层（快照 → 状态行 / 元信息）跟着 `Lang` 变」由 `app/view/tests.rs` 守。

use unlucky_i18n::catalog::MISSING;
use unlucky_i18n::{Catalog, Key, Lang};

use super::super::Texts;

/// 中文界面里允许出现的 ASCII 词：**品牌名与技术标识**。
///
/// 每一条都要说清为什么：
///
/// - `Java`：运行时品牌名（`JAVA` 那格的值域里也全是 `Temurin 21` 这类品牌串），翻成「爪哇」是错的；
/// - `pid`：进程号的技术标识。用户拿它去任务管理器里搜进程，翻成「进程」就丢了这条可操作性。
const ALLOWED: &[&str] = &["Java", "pid"];

/// 页面取词的五个源码文件（死 key 扫描与旧串扫描共用）。
const PAGE_SOURCES: [(&str, &str); 5] = [
    ("instances.rs", include_str!("../../instances.rs")),
    ("instances/detail.rs", include_str!("../detail.rs")),
    ("instances/demo.rs", include_str!("../demo.rs")),
    ("instances/text.rs", include_str!("../text.rs")),
    (
        "app/view/instances.rs",
        include_str!("../../../app/view/instances.rs"),
    ),
];

/// 页面五件套里**允许出现的字符串字面量**（凡是带 ASCII 单词的都必须在这张表里）。
///
/// M120 r1 复审实测出黑名单的哑点：只兜「已知的 29 条旧串」时，往页面里**新写一句英文**
/// 照样 612/612 + 12/12 全绿。判据现在反过来——**白名单**：页面源码里每个带 ASCII 单词的
/// 字面量都要在这里找到归宿，新写一句英文立刻红。
///
/// 全部 6 条都写了理由（剩下一批 `"{} · {}"` / `"{} · {:.0}%"` / `"{} {}"` 这类格式化模板
/// **连一个词都没有**，所以不需要条目）：
///
/// | 字面量 | 为什么它可以是英文 |
/// |---|---|
/// | `{date} {time} UTC` | `UTC` 是时区缩写，不是界面文案（本地时区换算属时间格式 mission） |
/// | `isolated` / `shared` | core 的隔离档位**稳定 id**（`IsolationMode::as_str`），`isolation()` 只认这两档，其余原样上屏 |
/// | `pid` / `reason` | `Catalog::render` 的**占位符名**（与 catalog 里的 `{pid}` / `{reason}` 对应） |
/// | `Instances` | `Texts::keys()` 的**族名前缀**（`Key::ALL` 的过滤条件，不是文案） |
const ALLOWED_LITERALS: [&str; 6] = [
    "{date} {time} UTC",
    "isolated",
    "shared",
    "pid",
    "reason",
    "Instances",
];

/// 抽出源码里的字符串字面量（够用的实现：本仓这五个文件没有 `\"` 转义、没有多行字符串）。
///
/// 按 `"` 切开取**奇数段**即可：`a"b"c"d"e` → 段 1 = `b`、段 3 = `d`。
fn string_literals(source: &str) -> Vec<&str> {
    source.split('"').skip(1).step_by(2).collect()
}

/// 把 `{占位符}` 抹掉再切 ASCII 词：占位符是**运行时数据**（pid / 原因 / 时间），不是文案。
fn ascii_words(text: &str) -> Vec<String> {
    let mut stripped = String::new();
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth > 0 => {}
            _ => stripped.push(ch),
        }
    }

    let mut words = Vec::new();
    let mut current = String::new();
    for ch in stripped.chars() {
        if ch.is_ascii_alphabetic() {
            current.push(ch);
        } else if !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// 有没有中日韩统一表意文字（「这条文案真的翻成中文了吗」的最直接判据）。
fn has_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(c, '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{3000}'..='\u{303F}')
    })
}

/// **中文界面无英文词**：实例页这一族的每一条中文译文，除了 [`ALLOWED`] 之外不许出现 ASCII 单词。
///
/// 抓得住：漏翻（中英写成一样）、翻了一半（`Present on disk` → `已在磁盘上 Present`）、
/// 新加 key 时中文那栏顺手抄了英文。
/// 抓不住：译文本身读着别扭；也抓不住「页面另写了一条字面量」（见模块文档的边界一节）。
///
/// **变异实测**（M120 交付时跑过）：把 `InstancesPresentOnDisk` 的中文改回 `Present on disk`，
/// 这条立刻红并指出 key 名。
#[test]
fn the_chinese_page_has_no_english_words() {
    let zh = Lang::ZhCn.catalog();
    let mut offenders = Vec::new();
    let mut checked = 0;

    for key in Texts::keys() {
        let text = zh.text(key);
        assert_ne!(text, MISSING, "{} 没有中文译文", key.name());
        checked += 1;
        for word in ascii_words(text) {
            if !ALLOWED.contains(&word.as_str()) {
                offenders.push(format!("{} · 出现 {word:?} ← 「{text}」", key.name()));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "中文界面上这些英文词会漏出来（品牌名 / 技术标识请加进 ALLOWED 并写明理由）：\n{}",
        offenders.join("\n")
    );
    assert!(
        checked >= 30,
        "实例页的 key 不该被成批删掉（只查到 {checked} 条）"
    );
}

/// **切换语言，文本真的变**：这一族的每一条中英译文都必须不同。
///
/// 抓得住：翻完没改（两栏抄同一句）——那种 key 在切语言时**一个字都不动**，界面上看起来
/// 「已经国际化」，其实那一处永远是英文。今夜反复出现的「测试绿着但什么都没守住」就是这一类。
/// 抓不住：某条译文翻错了词。
#[test]
fn every_instances_text_changes_with_the_language() {
    let en = Lang::En.catalog();
    let zh = Lang::ZhCn.catalog();

    for key in Texts::keys() {
        let (a, b) = (en.text(key), zh.text(key));
        assert_ne!(a, MISSING, "{} 没有英文译文", key.name());
        assert_ne!(b, MISSING, "{} 没有中文译文", key.name());
        assert_ne!(
            a,
            b,
            "{} 的两套语言写成了一样（{a:?}）——切语言时这一处不会变",
            key.name()
        );
        // 中文那栏必须真的有汉字——**整句只有品牌名时例外**（`InstancesMetaJava` = "Java"：
        // 那一格的值域是 `Temurin 21` 这种品牌串，翻成「爪哇」是错的）。
        let brand_only = {
            let words = ascii_words(b);
            !words.is_empty() && words.iter().all(|word| ALLOWED.contains(&word.as_str()))
        };
        assert!(
            has_cjk(b) || brand_only,
            "{} 的中文译文里一个汉字都没有：{b:?}",
            key.name()
        );
    }
}

/// **数字与分隔符不随语言变形**：`Running · pid 1234` 的两套语言只换前半句。
///
/// 抓得住：把 `pid` 翻成「进程」、把 `·` 换成 `-`、把 `{pid}` 写成固定文字、少一个空格。
/// 抓不住：真实 pid 的取值（那是 core 的事，出图那台机器上也没真跑过）。
#[test]
fn the_running_line_keeps_its_number_and_separator() {
    let en = Catalog::of(Lang::En).render(Key::InstancesRunning, &[("pid", "1234")]);
    let zh = Catalog::of(Lang::ZhCn).render(Key::InstancesRunning, &[("pid", "1234")]);

    assert_eq!(en, "Running · pid 1234");
    assert_eq!(zh, "运行中 · pid 1234");
    assert_ne!(en, zh, "切换语言时这一行要变（只换标签本体）");
    for text in [en, zh] {
        assert!(text.contains("1234"), "数字丢了：{text}");
        assert!(text.contains(" · pid "), "分隔符或 pid 变形了：{text}");
    }

    // 失败那一行同理：core 给的原因原文原样保留，只翻前半句
    let failed_en = Catalog::of(Lang::En).render(Key::InstancesFailed, &[("reason", "exit 1")]);
    let failed_zh = Catalog::of(Lang::ZhCn).render(Key::InstancesFailed, &[("reason", "exit 1")]);
    assert_eq!(failed_en, "Launch failed · exit 1");
    assert_eq!(failed_zh, "启动失败 · exit 1");
}

/// **页面里不许再写死界面文案**（白名单）：带 ASCII 单词的字面量必须在 [`ALLOWED_LITERALS`] 里。
///
/// 抓得住：①M120 之前那 29 条旧串被抄回页面（它们都带英文词 ⇒ 不在白名单 ⇒ 红）；
/// ②**任何新写的英文字面量**（r1 复审抓到的哑点：黑名单版本把 `"Ready"` 写进页面是全绿的）。
/// 抓不住：中文写死的字面量（`"实例"` 没有 ASCII 词）——那要靠 `Texts::of(` 的反向锚与出图；
/// 也抓不住夹具数据（`pages/instances/data.rs` 故意不在扫描范围：那是**用户数据**的样子，
/// 不是界面文案，见该文件的模块文档）。
///
/// **变异实测（M120 r1 交付时跑过三条）**：写回 `"ALL INSTANCES"` ⇒ 红；写进一条全新的
/// `"Ready"` ⇒ 红；把 `Texts::of(` 从实例页删掉 ⇒ 反向锚红。
#[test]
fn every_english_literal_in_the_page_sources_is_whitelisted() {
    let mut checked = 0;
    for (name, source) in PAGE_SOURCES {
        for literal in string_literals(source) {
            if ascii_words(literal).is_empty() {
                continue;
            }
            checked += 1;
            assert!(
                ALLOWED_LITERALS.contains(&literal),
                "{name} 里的字面量 {literal:?} 带 ASCII 单词但不在白名单里：界面文案要走 Texts / 目录；确属专有名词的话加进 ALLOWED_LITERALS 并写明理由"
            );
        }
    }

    // 反向锚①：扫描器真的扫到了东西（否则上面那个循环是空转）
    assert!(
        checked >= 6,
        "只扫到 {checked} 条带英文的字面量，扫描器可能失效了"
    );

    // 反向锚②：这些来源**必须**真的在用 Texts / 目录
    assert!(
        PAGE_SOURCES[0].1.contains("Texts::of("),
        "实例页没有从 Texts 取词"
    );
    assert!(
        PAGE_SOURCES[0].1.contains("Header::new(texts.title())"),
        "页头标题必须是词表里的那条（`Header::new` 里写死字符串 = 中文界面上顶着英文标题）"
    );
    assert!(
        PAGE_SOURCES[3].1.contains("Key::Instances"),
        "text.rs 没有引用本族的 key"
    );
    assert!(
        PAGE_SOURCES[4].1.contains("Key::InstancesStage"),
        "阶段标签没有走目录"
    );
}

/// **死 key 守门**：目录里每一条 `Instances*` 都必须被页面（或产品快照转换）真的引用。
///
/// M117 加的那七条就是这么沉的：目录里写着，页面上一个都没用。扫描是字面出现的
/// `Key::<名字>`，所以用宏拼 key 名能骗过它——这个局限写在这里，不假装它更强。
#[test]
fn every_instances_key_is_referenced_by_the_page() {
    for key in Texts::keys() {
        let needle = format!("Key::{}", key.name());
        assert!(
            PAGE_SOURCES
                .iter()
                .any(|(_, source)| source.contains(&needle)),
            "{needle} 在目录里定义了，但实例页没有任何地方用它（死 key，删掉或接上）"
        );
    }
}
