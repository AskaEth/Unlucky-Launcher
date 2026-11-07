//! 文案边界（示例契约）：core / presenter 的**状态**怎么进 UI。
//!
//! ## 现状（为什么需要这份契约）
//!
//! UI 文案硬编码英文，而 `core` / `presenter` 产出的状态串是中文，于是界面上已经出现
//! `Launch failed · exit0 静默失败` 这种**一句话里中英混排**的东西，加载器徽标也在 `原版` / `Fabric`
//! 之间混用。根因不是「译文不够」，是**边界没划**：谁产出文案、谁负责翻译，没有规则。
//!
//! ## 三条规则
//!
//! **R1 · UI 侧不产生文案，只翻译 key。**
//!    UI 不许把片段拼成句子（`"Launch failed · " + reason`），也不许写中文字面量。
//!    一句话 = 一个 key（[`crate::catalog`]），带变量的用 `{name}` 交给
//!    [`crate::catalog::Catalog::render`]。拼接一旦发生，任何一套语言都拼不对（语序、标点、
//!    量词都不一样），而且混排会立刻出现在截图里。
//!
//! **R2 · core 的中文串不进 UI。**
//!    core / presenter 的中文（`原版`、`静默失败（exit 0 但游戏没起来）`、
//!    `缺少库文件 foo.jar（…）`）是**日志 / CLI / 诊断报告**的文案，不是界面文案。
//!    界面只能通过两条合法通道拿到「要显示什么」：
//!    1. **结构化出口**（首选）：枚举 / DTO 字段 → 映射到 [`crate::Key`]。
//!       例：`presenter::dto::LoaderKind::as_str()` 的稳定 id（`vanilla` / `fabric`）→
//!       [`crate::Key::LoaderVanilla`]；`LogOutcome` 三态 → `LaunchOutcome*` 三 key。
//!       好处是**新增一个枚举变体时编译器会逼你补映射**（`match` 非穷尽即报错）。
//!    2. **过渡表**（[`classify`]）：只覆盖「已知会漏到界面的少数几个串」，逐条硬编码。
//!       它是**债**，不是终点——新出现的未知串一律走第三条。
//!
//! **R3 · 认不出的串，不许画原文。**
//!    [`CoreText::Untranslated`] 就是那种情况：UI 只许显示 [`crate::Key::CoreTextUntranslated`]
//!    这个**故意难看**的兜底（`[system text not translated]`），并把原文写进日志。
//!    这条比「先画上去以后再翻」严格，是因为后者永远不会被回头修——难看才会被修。
//!    同时它是一件**上游缺陷的证据**：说明 core 欠一个结构化出口，应当报 finding 而不是在 UI 贴创可贴。
//!
//! ## 品牌专名（唯一的豁免）
//!
//! `Fabric` / `Forge` / `NeoForge` / `Quilt` / `OptiFine` / `LiteLoader` 是专有名词，**不翻译**
//! （翻成「锻造」就没人认识了）。它们可以原样显示，但必须经 [`CoreText::Brand`] 判定——
//! 也就是说「原样显示」是一个**显式决定**，不是「反正没译文就画原文」的默认行为。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：本表里列到的串（映射正确、未知串不被放行）；
//! - **抓不住**：core 明天新加一句中文——本 crate 不依赖 core（零依赖纪律），没有跨 crate 的一致性检查。
//!   那条只能靠「core 侧只给枚举」的纪律 + 后续在 `crates/presenter` 里加一条守门测试
//!   （例如：`*_view` 的字符串字段不许含 CJK 字符）。

use crate::catalog::Key;

/// 一段来自 `core` / `presenter` 的显示文本该怎么处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreText {
    /// 品牌专名：原样显示（不翻译，见模块文档的豁免）。
    Brand(&'static str),
    /// 有对应 key：按当前语言取词。
    Key(Key),
    /// 不认识：只许显示 [`Key::CoreTextUntranslated`]，原文进日志。
    Untranslated,
}

/// 已知会漏到界面的中文状态串 → key。
///
/// 每一条都得在 `crates/presenter` / `crates/core` 里找得到出处（注释里给位置），
/// 不许凭想象加——凭空加的条目会让 [`classify`] 假装覆盖了实际没覆盖的东西。
const LEGACY_STATUS: &[(&str, Key)] = &[
    // `presenter::dto::instances::LoaderKind::label()`（全仓唯一那份显示词表）
    ("原版", Key::LoaderVanilla),
    // `presenter::host::job.rs`：`LogOutcome::SilentFail` 的展示文字
    ("静默失败（exit 0 但游戏没起来）", Key::LaunchOutcomeSilent),
    // `presenter::mock_data.rs` 里的演示数据
    ("exit0 静默失败", Key::LaunchOutcomeSilent),
];

/// 品牌专名（原样显示；**不得**翻译或拼进句子，UI 拼装仍然禁止）。
const BRANDS: &[&str] = &[
    "Fabric",
    "Forge",
    "NeoForge",
    "Quilt",
    "OptiFine",
    "LiteLoader",
];

/// 契约总入口：core / presenter 的显示文本 → 该画什么。
///
/// 这是**示例契约**：形状（三分类 + 未知不放行）比表里的具体条目重要，接线 mission 按这个形状
/// 把每条 UI 路径改过来，条目随上游结构化出口的落地逐条删掉。
#[must_use]
pub fn classify(text: &str) -> CoreText {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return CoreText::Untranslated;
    }

    if let Some((_, key)) = LEGACY_STATUS.iter().find(|(legacy, _)| *legacy == trimmed) {
        return CoreText::Key(*key);
    }

    if let Some(brand) = BRANDS.iter().find(|brand| **brand == trimmed) {
        return CoreText::Brand(brand);
    }

    // `Fabric 0.16.5` 这类「品牌 + 版本号」也是合法显示：品牌之后只跟一个版本号时放行。
    // 注意这里是**保守**放行：只有「一个词 + 空格 + 版本号」的形状才算，含中文的串绝不会进来。
    if let Some((head, tail)) = trimmed.split_once(' ') {
        if let Some(brand) = BRANDS.iter().find(|brand| **brand == head) {
            if !tail.is_empty() && !tail.contains(char::is_whitespace) && !tail.chars().any(is_cjk)
            {
                return CoreText::Brand(brand);
            }
        }
    }

    CoreText::Untranslated
}

/// 是否 CJK 字符（`classify` 的保守放行判据用）。
fn is_cjk(ch: char) -> bool {
    // 0x2E80..=0x9FFF 已含 CJK 部首 / 假名 / 统一表意文字与 0x3000 段的中文标点，不再单列。
    matches!(ch as u32, 0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_chinese_status_strings_map_to_keys() {
        // 抓得住：过渡表条目的映射写错（把「原版」映到别的 key），以及「同一段意思两个 key」的漂移。
        // 抓不住：core 那边改了这个中文串的措辞——那时这里会落到 Untranslated（**故意的**，
        //         让漏译可见，而不是继续拿旧文案糊过去）。
        assert_eq!(classify("原版"), CoreText::Key(Key::LoaderVanilla));
        assert_eq!(
            classify("静默失败（exit 0 但游戏没起来）"),
            CoreText::Key(Key::LaunchOutcomeSilent)
        );
        assert_eq!(
            classify("exit0 静默失败"),
            CoreText::Key(Key::LaunchOutcomeSilent)
        );
        assert_eq!(
            classify(" 原版 "),
            CoreText::Key(Key::LoaderVanilla),
            "前后空白要吃掉"
        );
    }

    #[test]
    fn brands_pass_through_and_are_never_translated() {
        // 抓得住：品牌名被当成待翻译文案（`Fabric` → 某个 key 的译文就没人认识它了），
        //         以及「品牌 + 版本号」这种合法徽标被判成未翻译。
        // 抓不住：品牌名与版本号之间该用什么分隔（M3 令牌 / 排版层的事，不是本契约的事）。
        assert_eq!(classify("Fabric"), CoreText::Brand("Fabric"));
        assert_eq!(classify("Fabric 0.16.5"), CoreText::Brand("Fabric"));
        assert_eq!(classify("NeoForge 21.1.72"), CoreText::Brand("NeoForge"));
        assert_eq!(classify("Fabric 中文"), CoreText::Untranslated);
        assert_eq!(classify("Fabric 0.16.5 beta"), CoreText::Untranslated);
        assert_eq!(classify("FabricX"), CoreText::Untranslated);
    }

    #[test]
    fn unknown_core_prose_is_refused_instead_of_printed_raw() {
        // 抓得住：R3——未知中文（core 的诊断prose、`describe_warning` 的拼句）被放行到界面，
        //         那正是 `Launch failed · exit0 静默失败` 这类混排的来源。
        // 抓不住：UI 拿到 Untranslated 之后有没有真的只显示兜底（页面侧测试 + 截图验收）。
        let leaks = [
            "缺少库文件 lwjgl.jar（C:\\Users\\x\\.minecraft\\libs\\lwjgl.jar）",
            "版本 JSON 没有 mainClass",
            "退出码为 0 但疑似静默失败（3 条错误行）：游戏可能刚起来就被 mod 异常终止",
            "",
            "   ",
        ];
        for leak in leaks {
            assert_eq!(classify(leak), CoreText::Untranslated, "{leak}");
        }
    }

    #[test]
    fn classify_never_returns_a_raw_unknown_string() {
        // 抓得住：将来给 `CoreText` 加了「原样字符串」变体（那等于把 R3 废掉）。
        // 抓不住：枚举里已有变体被误用——那要靠调用点 review。
        let odd = "some untranslated english";
        assert_eq!(classify(odd), CoreText::Untranslated);
    }
}
