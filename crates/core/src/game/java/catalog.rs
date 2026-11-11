//! 可下载 JDK 大版本目录（M197）：设置页「下载一个 JDK」的来源表。
//!
//! 用户原话（2026-11-08）：「我们内置了那么多镜像源 甚至用户不能手动选择指定版本下载。」
//! 这一层只回答两个问题：**可选的有哪些大版本**、**每个大版本由哪几家发行版承接**。
//! 它不联网、不落盘、不认识 UI——真正查版本与下载仍在 [`super::install`] / [`super::source`]。
//!
//! ## 为什么目录要写在这里而不是 UI 里
//!
//! 「哪几家能装 8」是 [`JdkVendor::supports`] 的知识（Oracle 从 9 起、MS 只发 11/17/21/25），
//! 写进 UI 就是第二份真相：两边一旦不一致，界面会给出一个「点了必然失败」的按钮。
//! 所以本模块只做「目录 × 发行版能力」的组合，UI 拿到的就是回退链里真实会试的那几家。
//!
//! ## 基准版本
//!
//! [`BASELINE_MAJOR`]（17）是 D13 定的基准：缺 Java 时先装它。它同时也是目录的一员，
//! 但不是唯一一员——`26.1` 起 MC 要 25，`1.12.2` 只能吃 8，所以要能挑。

use super::vendor::{self, JdkVendor};

/// D13 的基准 JDK 大版本（没有上下文时「装哪个」的默认答案）。
pub const BASELINE_MAJOR: u32 = 17;

/// 一个可选的大版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JdkChoice {
    /// 大版本号（`17`）。
    pub major: u32,
    /// 是不是 LTS（展示用徽标；不参与任何判定）。
    pub lts: bool,
    /// 是不是 D13 基准版本。
    pub baseline: bool,
}

/// 目录（升序）：8 / 11 / 17 / 21 / 25。
///
/// 8 不是「顺便带上」的：1.12.2 及更早只能吃 Java 8（LaunchWrapper），而 25 是 `26.1` 起
/// 的下限（见 `mapping`）。少任何一端，用户在最老的实例上就只能手动去装 JDK。
pub const CHOICES: [JdkChoice; 5] = [
    JdkChoice {
        major: 8,
        lts: false,
        baseline: false,
    },
    JdkChoice {
        major: 11,
        lts: true,
        baseline: false,
    },
    JdkChoice {
        major: 17,
        lts: true,
        baseline: true,
    },
    JdkChoice {
        major: 21,
        lts: true,
        baseline: false,
    },
    JdkChoice {
        major: 25,
        lts: true,
        baseline: false,
    },
];

/// 全部可选大版本（升序）。
#[must_use]
pub const fn choices() -> &'static [JdkChoice] {
    &CHOICES
}

/// 目录里有没有这个大版本（`Some` = 这一档可选）。
#[must_use]
pub fn find(major: u32) -> Option<JdkChoice> {
    CHOICES.into_iter().find(|choice| choice.major == major)
}

/// 某个大版本能由哪几家发行版承接（顺序 = 回退链顺序）。
///
/// 与 [`super::install::install`] 的过滤同源——`vendor::order` 给出这条链，`supports` 去掉
/// 发不出这个大版本的那几家。于是 GUI 上「能选哪几家」与「真会试哪几家」不会各说一套。
#[must_use]
pub fn vendors_for(major: u32, priority: &[JdkVendor], domestic: bool) -> Vec<JdkVendor> {
    vendor::order(priority, domestic)
        .into_iter()
        .filter(|vendor| vendor.supports(major))
        .collect()
}

/// 「该装哪个大版本」：把当前实例的最低要求对齐到目录里的一档，没有上下文时给基准版本。
///
/// 对齐方向是**向上取整**（1.17 要 16 → 目录里的 17），因为装低一档一定跑不起来；
/// 要求超过目录上限（例如 26.1 的 25 已在目录里；将来 28 就超出）时给基准版本，
/// 由页面把「这个版本装什么都不对」说清楚——而不是悄悄装个用不上的。
#[must_use]
pub fn default_major(required: Option<u32>) -> u32 {
    let Some(required) = required else {
        return BASELINE_MAJOR;
    };
    CHOICES
        .iter()
        .map(|choice| choice.major)
        .find(|major| *major >= required)
        .unwrap_or(BASELINE_MAJOR)
}

#[cfg(test)]
mod tests {
    use super::{choices, default_major, find, vendors_for, BASELINE_MAJOR};
    use crate::game::java::vendor::{ids, JdkVendor};

    /// 目录必须覆盖用户点名的三档（8 / 17 / 21），并钉住基准版本。
    ///
    /// 变异证明：从 `CHOICES` 里删掉 8 那一项 ⇒ 本用例红（`find(8)` 为 `None`）。
    #[test]
    fn the_catalog_covers_eight_seventeen_and_twenty_one() {
        for major in [8, 17, 21] {
            assert!(find(major).is_some(), "目录里必须有 Java {major}");
        }
        assert_eq!(BASELINE_MAJOR, 17, "D13 的基准版本是 17");
        assert!(find(BASELINE_MAJOR).expect("基准版本在目录里").baseline);
        assert_eq!(
            choices().len(),
            5,
            "目录是 8/11/17/21/25 五档，改动要同步本断言与 UI 文案"
        );
        assert!(
            choices()
                .windows(2)
                .all(|pair| pair[0].major < pair[1].major),
            "目录必须升序（UI 直接按这个顺序画）"
        );
    }

    /// 「8 只能由 Temurin / Zulu 承接」：Oracle 从 9 起、MS 已停发 8。
    ///
    /// 变异证明：把 `vendors_for` 里的 `supports` 过滤删掉 ⇒ 本用例红（8 会列出 oracle/microsoft）。
    #[test]
    fn java_eight_is_only_served_by_the_vendors_that_still_ship_it() {
        assert_eq!(ids(&vendors_for(8, &[], true)), vec!["temurin", "zulu"]);
        assert_eq!(
            ids(&vendors_for(21, &[], true)),
            vec!["temurin", "oracle", "microsoft", "zulu"],
            "21 四家都在（国内默认序）"
        );
        assert!(
            !vendors_for(8, &[], true).contains(&JdkVendor::Microsoft),
            "微软已停发 8"
        );
    }

    /// 用户优先级要真的排在最前面（与 `install` 的链同源）。
    #[test]
    fn the_user_priority_leads_the_vendor_list() {
        let priority = vec![JdkVendor::Zulu];
        assert_eq!(ids(&vendors_for(17, &priority, true))[0], "zulu");
        assert_eq!(ids(&vendors_for(17, &priority, true)).len(), 4);
    }

    /// 「该装哪个」对齐到目录里的一档：向上取整，没有上下文给基准，超出目录给基准。
    #[test]
    fn the_default_major_rounds_up_into_the_catalog() {
        assert_eq!(default_major(None), 17);
        assert_eq!(default_major(Some(8)), 8, "1.12.2 → 8");
        assert_eq!(default_major(Some(16)), 17, "1.17 要 16 → 目录里的 17");
        assert_eq!(default_major(Some(17)), 17);
        assert_eq!(default_major(Some(21)), 21, "1.20.5+ → 21");
        assert_eq!(default_major(Some(25)), 25, "26.1 → 25");
        assert_eq!(default_major(Some(99)), 17, "超出目录时退回基准");
    }
}
