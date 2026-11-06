//! 系统语言的**真值源**（D28）：Windows 走 `reg.exe` 读注册表——**零 `unsafe`、零 FFI、零新依赖**。
//!
//! ## 为什么是子进程而不是 FFI
//!
//! 先例是 M73 的壁纸探测（`crates/core/src/platform/wallpaper/windows.rs`）：同样是「Windows 上
//! 只有注册表知道答案」的问题，它用 `reg.exe query` 解决，`unsafe` 一个没加。本 crate 的 `unsafe`
//! 只允许出现在 [`crate::platform::win32`]（D36 的纪律），所以这里照抄那条路。
//!
//! 为什么不 `GetUserDefaultLocaleName`：它要 FFI，而它对**本用途**并不比注册表准——Windows 的
//! 「区域格式」与「显示语言」本来就是两个设置，两者都有各自的说法（见下面的诚实边界）。
//!
//! ## 读什么、按什么顺序
//!
//! 1. `HKCU\Control Panel\International` 的 `LocaleName`——Win7+ 都有，值就是 `zh-CN` 这种标签，
//!    [`unlucky_i18n::Lang::from_locale`] 直接吃得下；
//! 2. 读不到 `LocaleName` 时退到同一个键的 `Locale`（8 位十六进制 LCID，如 `00000804`），
//!    用一张**很短的**表翻成标签；表里没有的返回 `None`（宁可英文，也不猜错语种）。
//!
//! 优先级链在 [`unlucky_i18n::lang::resolve`]：**`ui.language` 显式值 > 本模块 > 环境变量 > 英文**。
//!
//! ## 诚实边界（抓得住什么 / 抓不住什么）
//!
//! - **抓得住**：`reg query` 输出的解析（含 CRLF、空值、值名大小写）、`LocaleName` 缺失时退 LCID
//!   的分支、命令失败/缺失时的 `None`；
//! - **抓不住**：①`LocaleName` 是**区域格式**，用户完全可能在设置里把「显示语言」改成英文而区域
//!   仍是 `zh-CN`（或反过来）——真要读显示语言得 `GetUserDefaultUILanguage`（FFI）或多语言
//!   用户界面注册表（`...\Control Panel\Desktop` 的 `PreferredUILanguages`，本 mission 未做）；
//!   ②策略禁用 `reg.exe` / Wine 环境 → `None` → 落到环境变量 → 英文；
//!   ③代码页（`chcp`）与这里无关，注册表值不受它影响。

use unlucky_i18n::lang::LocaleSource;

/// 语言相关的注册表键。
const INTERNATIONAL_KEY: &str = r"HKCU\Control Panel\International";

/// 首选值名：值是 BCP-47 风格标签（`zh-CN`）。
const LOCALE_NAME_VALUE: &str = "LocaleName";

/// 兜底值名：值是 8 位十六进制 LCID（`00000804`）。
const LOCALE_VALUE: &str = "Locale";

/// 常见 LCID → 语言的短表（**故意短**：不认识的返回 `None`，让链路落到英文）。
///
/// 覆盖面：中文四地 + 英语三地 + 几个大语种。这张表只在 `LocaleName` 缺失时用到，
/// 而 `LocaleName` 从 Win7 起就有，所以它实际上是「老系统 / 被裁剪的系统」专用。
const LCID_TAGS: &[(&str, &str)] = &[
    ("00000804", "zh-CN"),
    ("00000404", "zh-TW"),
    ("00000c04", "zh-HK"),
    ("00001004", "zh-SG"),
    ("00000409", "en-US"),
    ("00000809", "en-GB"),
    ("00000c09", "en-AU"),
    ("00000411", "ja-JP"),
    ("00000412", "ko-KR"),
    ("00000407", "de-DE"),
    ("0000040c", "fr-FR"),
    ("00000410", "it-IT"),
    ("00000c0a", "es-ES"),
    ("00000416", "pt-BR"),
    ("00000419", "ru-RU"),
    ("00000415", "pl-PL"),
    ("0000041f", "tr-TR"),
    ("00000406", "da-DK"),
    ("0000040b", "fi-FI"),
    ("00000413", "nl-NL"),
];

/// 跑外部命令的接口（注入点：单测不 spawn 进程）。
pub trait RegIo {
    /// 跑一条命令取 stdout；命令缺失 / 非零退出 / 空输出都返回 `None`。
    fn run(&self, program: &str, args: &[&str]) -> Option<String>;
}

/// 真实实现：`reg.exe` 子进程（非 Windows 上恒 `None`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct HostReg;

impl RegIo for HostReg {
    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        if !cfg!(windows) {
            // 其它平台的「系统语言」真值源不是注册表；`resolve` 会继续往环境变量走。
            return None;
        }

        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .ok()?;
        // `Command::output` 对非零退出同样返回 `Ok`，退出码要单独判（`reg.exe` 查不到值就是非零）。
        if !output.status.success() || output.stdout.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

/// 系统语言源（实现 [`LocaleSource`]，日志里自称 `registry`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemLocale;

impl LocaleSource for SystemLocale {
    fn locale(&self) -> Option<String> {
        detect_with(&HostReg)
    }

    fn label(&self) -> &'static str {
        "registry"
    }
}

/// 用给定 IO 读一次系统语言（`LocaleName` 优先，退 `Locale` 的 LCID）。
pub fn detect_with(io: &dyn RegIo) -> Option<String> {
    let by_name = io
        .run(
            "reg.exe",
            &["query", INTERNATIONAL_KEY, "/v", LOCALE_NAME_VALUE],
        )
        .and_then(|text| parse_reg_sz(&text, LOCALE_NAME_VALUE))
        .filter(|value| looks_like_tag(value));
    if by_name.is_some() {
        return by_name;
    }

    io.run("reg.exe", &["query", INTERNATIONAL_KEY, "/v", LOCALE_VALUE])
        .and_then(|text| parse_reg_sz(&text, LOCALE_VALUE))
        .and_then(|lcid| lcid_tag(&lcid))
        .map(str::to_owned)
}

/// 取 `reg query <键> /v <值名>` 输出里某个 `REG_SZ` 值的原文。
///
/// 真机输出形如（本机 Windows 11 24H2 实测，CRLF）：
///
/// ```text
/// HKEY_CURRENT_USER\Control Panel\International
///     LocaleName    REG_SZ    zh-CN
/// ```
///
/// 值名大小写不敏感（`reg.exe` 原样回显查询时给的名字）；值可能含空格，所以「值名 + 类型」
/// 之后整段取原文，不 `split_whitespace` 再拼（会吃掉连续空格）。
pub fn parse_reg_sz(text: &str, value_name: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let name_end = trimmed.find(char::is_whitespace)?;
        if !trimmed[..name_end].eq_ignore_ascii_case(value_name) {
            return None;
        }
        let after_name = trimmed[name_end..].trim_start();
        let kind_end = after_name.find(char::is_whitespace)?;
        let value = after_name[kind_end..].trim();
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// LCID（十六进制原文，如 `00000804`）→ 标签（大小写不敏感）。
pub fn lcid_tag(lcid: &str) -> Option<&'static str> {
    LCID_TAGS
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(lcid.trim()))
        .map(|(_, tag)| *tag)
}

/// 像不像一个语言标签（`zh-CN` / `en` / `zh-Hans-CN`）。
///
/// 只拦明显不是标签的东西（空值、带空格、带路径分隔符）——`reg.exe` 在某些策略下会回
/// `REG_SZ` 空串或一长串本地化文本，那些不该被当成语言喂给 `from_locale`。
fn looks_like_tag(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 35
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 Windows 11 24H2 实测抓下来的两段原文（含 `\r\n`）。
    const REAL_LOCALE_NAME: &str = "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    LocaleName    REG_SZ    zh-CN\r\n\r\n";
    const REAL_LOCALE: &str = "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    Locale    REG_SZ    00000804\r\n\r\n";

    struct FakeReg {
        name: Option<&'static str>,
        locale: Option<&'static str>,
        fail: bool,
    }

    impl FakeReg {
        fn both(name: Option<&'static str>, locale: Option<&'static str>) -> Self {
            FakeReg {
                name,
                locale,
                fail: false,
            }
        }
    }

    impl RegIo for FakeReg {
        fn run(&self, program: &str, args: &[&str]) -> Option<String> {
            assert_eq!(program, "reg.exe", "程序名写错就不是注册表读了");
            assert_eq!(args[0], "query");
            assert_eq!(args[1], INTERNATIONAL_KEY);
            assert_eq!(args[2], "/v");
            if self.fail {
                return None;
            }

            let raw = match args[3] {
                LOCALE_NAME_VALUE => self.name?,
                LOCALE_VALUE => self.locale?,
                other => panic!("没料到的值名：{other}"),
            };
            Some(format!(
                "\r\nHKEY_CURRENT_USER\\Control Panel\\International\r\n    {}    REG_SZ    {raw}\r\n\r\n",
                args[3]
            ))
        }
    }

    #[test]
    fn real_registry_output_parses_to_the_locale_tag() {
        // 抓得住：解析器与真机 `reg query` 输出格式之间的契约（CRLF、前导空白、值名与类型之间的
        //         多个空格）。这两段夹具是真机抓的原文，不是我编的。
        // 抓不住：这台机器的值本身会变（用户改区域设置）——测试只钉格式，不钉具体语言。
        assert_eq!(
            parse_reg_sz(REAL_LOCALE_NAME, LOCALE_NAME_VALUE).as_deref(),
            Some("zh-CN")
        );
        assert_eq!(
            parse_reg_sz(REAL_LOCALE, LOCALE_VALUE).as_deref(),
            Some("00000804")
        );
        // 值名大小写不敏感（reg.exe 会原样回显查询里给的名字）
        assert_eq!(
            parse_reg_sz(REAL_LOCALE_NAME, "localename").as_deref(),
            Some("zh-CN")
        );
        // 空值 = 没设（不是空串语言）
        let empty = "    LocaleName    REG_SZ    \r\n";
        assert_eq!(parse_reg_sz(empty, LOCALE_NAME_VALUE), None);
        // 别的值名不该被误取
        assert_eq!(parse_reg_sz(REAL_LOCALE_NAME, "Locale"), None);
    }

    #[test]
    fn lcid_fallback_covers_chinese_and_english() {
        // 抓得住：LCID 兜底表的关键格子（中文四地 + 英语三地）被改动或删掉；
        //         以及「不认识就 None」这条（不许猜成某个语言）。
        // 抓不住：这 20 条之外的语言——没有 `LocaleName` 的老系统上它们会落到英文。
        assert_eq!(lcid_tag("00000804"), Some("zh-CN"));
        assert_eq!(lcid_tag("00000804\r"), Some("zh-CN"), "要去空白");
        assert_eq!(lcid_tag("00000404"), Some("zh-TW"));
        assert_eq!(lcid_tag("00000C04"), Some("zh-HK"), "大小写不敏感");
        assert_eq!(lcid_tag("00001004"), Some("zh-SG"));
        assert_eq!(lcid_tag("00000409"), Some("en-US"));
        assert_eq!(lcid_tag("00000411"), Some("ja-JP"));
        assert_eq!(lcid_tag("00000437"), None, "表外语言必须 None");
        assert_eq!(lcid_tag(""), None);
    }

    #[test]
    fn detect_prefers_locale_name_and_falls_back_to_lcid() {
        // 抓得住：两条取值路径的顺序（有 LocaleName 就不该再看 LCID），以及命令失败时的 None。
        // 抓不住：真机注册表里到底有什么（下面的 windows_live 测试才碰真机）。
        assert_eq!(
            detect_with(&FakeReg::both(Some("zh-CN"), Some("00000409"))).as_deref(),
            Some("zh-CN"),
            "LocaleName 优先"
        );
        assert_eq!(
            detect_with(&FakeReg::both(None, Some("00000804"))).as_deref(),
            Some("zh-CN"),
            "没有 LocaleName 才退 LCID"
        );
        assert_eq!(detect_with(&FakeReg::both(None, None)), None);
        assert_eq!(
            detect_with(&FakeReg {
                name: Some("zh-CN"),
                locale: Some("00000804"),
                fail: true,
            }),
            None,
            "命令失败就是 None，不许猜"
        );
        // 值名对但值不像标签（策略回了一串本地化文本）→ 不许当成语言
        assert_eq!(detect_with(&FakeReg::both(Some("中文(简体)"), None)), None);
    }

    #[test]
    #[cfg(windows)]
    fn windows_live_registry_query_returns_a_tag() {
        // 抓得住：`reg.exe` 这条路在本机真的通（spawn 成功、退出码为 0、输出能被解析）。
        // 抓不住：它读到的「区域格式」是否等于用户想用的**界面语言**（见模块文档的诚实边界）。
        let tag = SystemLocale.locale().expect("本机应当读得到 LocaleName");
        assert!(!tag.is_empty());
        assert!(looks_like_tag(&tag), "解析出来的不是语言标签：{tag:?}");
        assert_eq!(SystemLocale.label(), "registry");
    }
}
