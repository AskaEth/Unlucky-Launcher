//! JDK 发行版（vendor）抽象与用户可调优先级链（D23）。
//!
//! 四家发行版的**默认顺序按地区分区**（用户定案 + research/06 §5.1 实测修正）：
//!
//! ```text
//! 国内：Temurin(TUNA) → Oracle OpenJDK(华为云→南大) → MS OpenJDK(境外 CDN) → Zulu
//! 国外：Temurin        → MS OpenJDK                 → Oracle OpenJDK       → Zulu
//! ```
//!
//! 国内把 Oracle 提到 MS 之前的依据是实测差 28×（Oracle 国内 11.5 MB/s vs MS 410 KB/s）；
//! 用户原话是「Temurin → MS → Oracle」，但同一句话里给了「优先级可调」，所以这只是默认值，
//! 设置键 `java.vendorPriority` 可以整体覆盖（D23 定稿）。
//!
//! 这里的表只描述**能力与顺序**，不碰网络：怎么查、怎么下在 `oracle` / `microsoft` /
//! `zulu` / `api`（Temurin）四个模块里。

/// 设置键 `java.vendorPriority` 的允许值（顺序即默认序的覆盖来源）。
///
/// `termux` 只在 Termux 宿主上可用（系统包管理器安装），默认序里不出现；
/// 显式写进优先级或 `--vendor termux` 才走它。
pub const VENDOR_IDS: &[&str] = &["temurin", "oracle", "microsoft", "zulu", "termux"];

/// 一家 JDK 发行版。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum JdkVendor {
    /// Eclipse Temurin（Adoptium），v1 唯一实现。
    Temurin,
    /// Oracle OpenJDK（`download.java.net` GA，只发首个 GA + ≤2 个 update）。
    Oracle,
    /// Microsoft OpenJDK（`aka.ms` 模板 + VS CDN，无 8）。
    Microsoft,
    /// Azul Zulu（第四备选，API 元数据 + CDN 直链）。
    Zulu,
    /// Termux 的系统 openjdk（`pkg install openjdk-<major>`；只在 Termux 里可用）。
    Termux,
}

impl JdkVendor {
    /// 全部发行版（枚举顺序，不是优先级顺序）。
    pub const ALL: [Self; 5] = [
        Self::Temurin,
        Self::Oracle,
        Self::Microsoft,
        Self::Zulu,
        Self::Termux,
    ];

    /// 稳定 id（目录名段、`--vendor` 取值、设置值、JSON 字段）。
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Temurin => "temurin",
            Self::Oracle => "oracle",
            Self::Microsoft => "microsoft",
            Self::Zulu => "zulu",
            Self::Termux => "termux",
        }
    }

    /// 展示名（人读输出 + `java vendors`）。
    #[must_use]
    pub const fn display(self) -> &'static str {
        match self {
            Self::Temurin => "Eclipse Temurin",
            Self::Oracle => "Oracle OpenJDK",
            Self::Microsoft => "Microsoft OpenJDK",
            Self::Zulu => "Azul Zulu",
            Self::Termux => "Termux OpenJDK",
        }
    }

    /// 解析 `--vendor` / 设置值（容忍常见别名，大小写不敏感）。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "temurin" | "adoptium" | "eclipse" | "eclipse-temurin" => Some(Self::Temurin),
            "oracle" | "oracle_openjdk" | "openjdk" | "java.net" => Some(Self::Oracle),
            "microsoft" | "ms" | "msopenjdk" | "ms_openjdk" | "microsoft-openjdk" => {
                Some(Self::Microsoft)
            }
            "zulu" | "azul" => Some(Self::Zulu),
            "termux" | "termux-openjdk" | "pkg" => Some(Self::Termux),
            _ => None,
        }
    }

    /// 该发行版能不能装这个大版本。
    ///
    /// Temurin / Zulu 覆盖 8 起的全部 GA；Oracle 的 archive 从 9 起（8 由 Temurin 负责）；
    /// MS 只发 11/17/21/25（**已停发 8**，官方页面直接让去下 Temurin 8）。
    #[must_use]
    pub const fn supports(self, major: u32) -> bool {
        match self {
            Self::Temurin | Self::Zulu => major >= 8,
            Self::Oracle => major >= 9,
            Self::Microsoft => matches!(major, 11 | 17 | 21 | 25),
            Self::Termux => matches!(major, 17 | 21 | 25),
        }
    }

    /// 已知在发的 GA 大版本（`java vendors` 展示与设置页提示用；不是硬上限）。
    #[must_use]
    pub const fn hint_majors(self) -> &'static [u32] {
        match self {
            Self::Temurin => &[8, 11, 17, 21, 25],
            Self::Oracle => &[17, 21, 25],
            Self::Microsoft => &[11, 17, 21, 25],
            Self::Zulu => &[8, 11, 17, 21, 25],
            Self::Termux => &[17, 21, 25],
        }
    }

    /// 国内默认序（用户定案 + 实测修正）。
    #[must_use]
    pub const fn domestic_default() -> [Self; 4] {
        [Self::Temurin, Self::Oracle, Self::Microsoft, Self::Zulu]
    }

    /// 国外默认序（用户原话顺序）。
    #[must_use]
    pub const fn global_default() -> [Self; 4] {
        [Self::Temurin, Self::Microsoft, Self::Oracle, Self::Zulu]
    }

    /// 按地区取默认序。
    #[must_use]
    pub const fn default_order(domestic: bool) -> [Self; 4] {
        if domestic {
            Self::domestic_default()
        } else {
            Self::global_default()
        }
    }
}

/// 用户优先级（设置值）→ 实际尝试顺序。
///
/// 语义刻意做成「**补集追加**」而不是「必须写全」：用户写了 `["oracle"]` 就只把 oracle 提上来，
/// 其余按地区默认序跟在后面。理由——写全四家是负担，漏写更不该等于「其余不试」。
#[must_use]
pub fn order(priority: &[JdkVendor], domestic: bool) -> Vec<JdkVendor> {
    let mut out: Vec<JdkVendor> = Vec::with_capacity(JdkVendor::ALL.len());
    for vendor in priority {
        if !out.contains(vendor) {
            out.push(*vendor);
        }
    }
    for vendor in JdkVendor::default_order(domestic) {
        if !out.contains(&vendor) {
            out.push(vendor);
        }
    }
    out
}

/// 解析设置值里的字符串数组（未知项忽略；顺序保留）。
#[must_use]
pub fn parse_priority(items: &[String]) -> Vec<JdkVendor> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Some(vendor) = JdkVendor::parse(item) {
            if !out.contains(&vendor) {
                out.push(vendor);
            }
        }
    }
    out
}

/// 尝试顺序 → 稳定 id 列表（JSON / 人读输出）。
#[must_use]
pub fn ids(vendors: &[JdkVendor]) -> Vec<&'static str> {
    vendors.iter().map(|vendor| vendor.id()).collect()
}

#[cfg(test)]
mod tests {
    use super::{ids, order, parse_priority, JdkVendor};

    #[test]
    fn domestic_and_global_defaults_follow_d23() {
        assert_eq!(
            ids(&order(&[], true)),
            vec!["temurin", "oracle", "microsoft", "zulu"],
            "国内：Oracle 在 MS 之前（实测差 28×）"
        );
        assert_eq!(
            ids(&order(&[], false)),
            vec!["temurin", "microsoft", "oracle", "zulu"],
            "国外：维持用户原话 Temurin→MS→Oracle"
        );
    }

    #[test]
    fn user_priority_leads_and_the_rest_follows_as_a_complement() {
        let priority = parse_priority(&["microsoft".to_string()]);
        assert_eq!(
            ids(&order(&priority, true)),
            vec!["microsoft", "temurin", "oracle", "zulu"],
            "只写一家也要把其余按默认序补齐"
        );
        let full = parse_priority(&["zulu".to_string(), "oracle".to_string(), "zulu".to_string()]);
        assert_eq!(
            ids(&order(&full, true)),
            vec!["zulu", "oracle", "temurin", "microsoft"],
            "重复项去重，未列出的追加在后面"
        );
    }

    #[test]
    fn unknown_setting_values_are_ignored() {
        let parsed = parse_priority(&["graalvm".to_string(), "AZUL".to_string()]);
        assert_eq!(parsed, vec![JdkVendor::Zulu]);
    }

    #[test]
    fn vendor_ids_majors_and_aliases() {
        assert_eq!(JdkVendor::parse("Adoptium"), Some(JdkVendor::Temurin));
        assert_eq!(JdkVendor::parse("ms"), Some(JdkVendor::Microsoft));
        assert_eq!(JdkVendor::parse("graalvm"), None);
        // MS 没有 8：这是「回退链要跳过某家」的真实判据。
        assert!(!JdkVendor::Microsoft.supports(8));
        assert!(JdkVendor::Microsoft.supports(21));
        assert!(JdkVendor::Oracle.supports(9));
        assert!(!JdkVendor::Oracle.supports(8), "8 交给 Temurin");
        assert!(JdkVendor::Temurin.supports(8));
        for vendor in JdkVendor::ALL {
            assert!(
                vendor.hint_majors().contains(&21),
                "{} 应发 21",
                vendor.id()
            );
        }
    }
}
