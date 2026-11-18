//! 「源选择」的两个显式设置键 → [`RegionProfile`] 之上的覆盖层（M294）。
//!
//! 两个键：`download.region`（`auto` / `cn` / `global`）与 `download.mirror_preference`
//! （`auto` / `official` / `mirror`）。在本轮之前它们**一个引擎侧消费者都没有**——
//! `grep -rn "mirror_preference\|Region::" crates/core/src | grep -v setting/keys` 是零命中，
//! 设置页画着三档 chip，改它不改任何一次下载。本模块就是那个消费者。
//!
//! # 为什么两个键都落到同一个开关上
//!
//! 引擎里「官方在前还是镜像在前」**只有一个**开关：[`RegionProfile::mode`]（`mirrors.rs` 的
//! `expand()` 与 `gh_proxy_candidates()` 都只认它）。两个键问的是同一件事的两个侧面——
//! 「我在哪个地区」（决定默认值）与「我要谁在前」（更具体的显式选择）——所以两者都落成
//! 「给 `mode` 叠一层显式结论」，**不另造第二套排序开关**。再造一层的下场是「改 A 被 B 盖住、
//! 界面上看不出为什么」，正是本任务要治的那类病。
//!
//! 两者的分工也据此定死：`region` 回答「我在不在大中华区」（`greater_china`，并给出该地区的
//! 默认模式），`mirror_preference` 只回答「谁在前」（`mode`），**不动地区判定**。
//!
//! # 优先级（唯一一条可读规则）
//!
//! ```text
//! 显式 mirror_preference（official / mirror）   ← 最具体，永远优先
//!   > 显式 region（cn / global）
//!     > 自动探测（时区 + 语言 + 系统信号）与 region.json 的 24h 缓存   ← 今天的行为
//! ```
//!
//! `auto` 等于「这一层不发言」，退到下一层。两个键都不设、或都写 `auto` ⇒ 覆盖层是恒等映射，
//! 与 M294 之前**逐值相同**（守卫：`preference/tests.rs` 的 `not_set_changes_nothing`）。
//!
//! # 回落规则（手改文件 / 老配置 / 未知值）
//!
//! 只认 schema 登记的那几个词（`setting::keys::REGIONS` / `MIRROR_PREFERENCES`），大小写与前后
//! 空白不敏感。其余一切（`""` / `"cn2"` / `"bogus"`，以及非字符串值——宿主侧 `effective_str`
//! 给出 `None`）**一律当 `auto`**，静默退到下一层：下载不能因为一个设置字符串就停下。
//! 这与 `gh_proxy_override` 的「空串≠关」是同一条纪律的两面。
//!
//! # 不落盘
//!
//! 覆盖只发生在**读时**（`shared::build` 装配引擎那一刻，与 `apply_gh_proxy` 同一个位置）。
//! `region.json` 里仍然只有自动判定的结论——把显式值写回去会让 24h 重判永久失效。

use super::region::{RegionMode, RegionProfile};

/// `download.region` 的三档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegionPreference {
    /// 不发言：用自动探测（或 `region.json` 的 24h 缓存）的结论。
    #[default]
    Auto,
    /// `cn`：人在大中华区 ⇒ 默认镜像优先。
    GreaterChina,
    /// `global`：人在大中华区以外 ⇒ 默认直连。
    Global,
}

impl RegionPreference {
    /// 稳定字符串 id（与 schema 的取值逐字相同；日志/诊断用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::GreaterChina => "cn",
            Self::Global => "global",
        }
    }

    /// 设置原文 → 档位（未知值 = [`Self::Auto`]，见模块头的回落规则）。
    #[must_use]
    pub fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "cn" => Self::GreaterChina,
            "global" => Self::Global,
            _ => Self::Auto,
        }
    }

    /// 叠到已有判定上：`cn` ⇒ 大中华区 + 镜像优先；`global` ⇒ 非大中华区 + 直连；
    /// `auto` ⇒ **原样返回**（一个字段都不动）。
    #[must_use]
    pub fn apply(self, profile: RegionProfile) -> RegionProfile {
        match self {
            Self::Auto => profile,
            Self::GreaterChina => profile.overridden(RegionMode::MirrorFirst, true),
            Self::Global => profile.overridden(RegionMode::Direct, false),
        }
    }
}

/// `download.mirror_preference` 的三档。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MirrorPreference {
    /// 不发言：跟着 [`RegionPreference`] 的结论走（镜像侧再按健康度排序）。
    #[default]
    Auto,
    /// `official`：官方源永远第一条。
    Official,
    /// `mirror`：镜像永远第一条（官方兜底）。
    Mirror,
}

impl MirrorPreference {
    /// 稳定字符串 id（与 schema 的取值逐字相同；日志/诊断用）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Official => "official",
            Self::Mirror => "mirror",
        }
    }

    /// 设置原文 → 档位（未知值 = [`Self::Auto`]）。
    #[must_use]
    pub fn from_raw(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "official" => Self::Official,
            "mirror" => Self::Mirror,
            _ => Self::Auto,
        }
    }

    /// 叠到已有判定上：只改「谁在前」（`mode`），**不动** `greater_china`——那是
    /// [`RegionPreference`] 的事。`auto` ⇒ 原样返回。
    #[must_use]
    pub fn apply(self, profile: RegionProfile) -> RegionProfile {
        let greater_china = profile.greater_china;
        match self {
            Self::Auto => profile,
            Self::Official => profile.overridden(RegionMode::Direct, greater_china),
            Self::Mirror => profile.overridden(RegionMode::MirrorFirst, greater_china),
        }
    }
}

/// 两层覆盖按**固定顺序**叠：`region` 先、`mirror_preference` 后 ⇒ 后者更具体、永远优先。
#[must_use]
pub fn apply(
    profile: RegionProfile,
    region: RegionPreference,
    mirror: MirrorPreference,
) -> RegionProfile {
    mirror.apply(region.apply(profile))
}

#[cfg(test)]
#[path = "preference/tests.rs"]
mod tests;
