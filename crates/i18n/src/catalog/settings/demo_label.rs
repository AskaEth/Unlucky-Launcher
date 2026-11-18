//! 设置页六个分组「演示行」的**标签**一族（M314 r1，复审的事实修正）。
//!
//! ## 为什么只搬标签，不搬值
//!
//! `pages/settings/*.rs` 的 `Group.rows` 是「没有 presenter 快照时」的退回卡片
//! （`settings/mod.rs` 的 `frame::demo_card`），设置页首帧与 `pages_demo` 落在这条路上。
//! 它的一行是 `(标签, 值)`：
//!
//! | 列 | 例子 | 能不能建 key |
//! |---|---|---|
//! | **标签** | `Density` / `Corner style` / `Provider` | **能**：它是界面文案，中英必须不同 |
//! | 值 | `0.2.2` / `https://api.deepseek.com/v1` / `deepseek-chat` / `100%` / `On` | **不能**：locale 中立的数据 |
//!
//! `catalog/tests.rs` 的 `chinese_catalog_is_actually_chinese` 要求**每条 key 的中英必须不同**，
//! 所以值那一列建 key 只会逼出假译文。于是本域**只收标签**，`Group.rows` 的类型是
//! `&'static [(Key, &'static str)]`（标签走词表、值仍是数据）——这也正是复审 r1 指出的
//! 「那条硬理由只覆盖一半」的修法。
//!
//! ## 为什么挂在链尾
//!
//! 与 [`super::accent`] / [`super::bg_note`] 同一条：`settings_*_entries` 都在装配链中间，往那里
//! 追加会把其后所有 key 在 `Key::ALL` 里整体平移 ⇒ 金表（逐 index 逐字节）整片红。本域挂在链尾，
//! 金表只需在 `tail2::ROWS` 末尾追加。
//!
//! ## 顺序 = `pages/settings/registry.rs` 的 `GROUPS` 顺序
//!
//! General → Appearance → Java（空）→ Downloads → Update → AI / About。Java 组没有演示行
//! （M175 按「宁可空着，也不留一句会撒谎的演示值」清空），所以这里没有它的 key。

macro_rules! settings_demo_label_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── General 组（`pages/settings/general.rs`）
            SettingsDemoStartupPage => ("Startup page", "启动页"),
            SettingsDemoCloseButton => ("Close button", "关闭按钮"),
            // ── Appearance 组（`pages/settings/appearance.rs`）
            SettingsDemoDensity => ("Density", "密度"),
            SettingsDemoCornerStyle => ("Corner style", "圆角风格"),
            SettingsDemoMotion => ("Motion", "动效"),
            SettingsDemoTextScale => ("Text scale", "文字缩放"),
            // ── Downloads 组（`pages/settings/download.rs`）
            SettingsDemoConcurrency => ("Concurrency", "并发数"),
            SettingsDemoRetries => ("Retries per source", "单源尝试次数"),
            SettingsDemoGhProxy => ("GitHub proxy prefix", "GitHub 加速前缀"),
            SettingsDemoGhProxyOff => ("Turn it off", "关掉它"),
            SettingsDemoGhProxyOverride => ("Replace it", "换掉它"),
            // ── Update 组（`pages/settings/update.rs`）
            SettingsDemoCurrentVersion => ("Current version", "当前版本"),
            SettingsDemoChannel => ("Channel", "更新通道"),
            SettingsDemoCheckOnStartup => ("Check for updates on startup", "启动时检查更新"),
            // ── AI / About 组（`pages/settings/ai_about.rs`）
            SettingsDemoProvider => ("Provider", "提供方"),
            SettingsDemoEndpoint => ("Endpoint", "接口地址"),
            SettingsDemoModel => ("Model", "模型"),
        } }
    };
}

pub(crate) use settings_demo_label_entries;

#[cfg(test)]
mod tests {
    use crate::catalog::{Catalog, Key};
    use crate::Lang;

    /// 有没有汉字。
    fn is_han(ch: char) -> bool {
        ('\u{4e00}'..='\u{9fff}').contains(&ch)
    }

    /// **这一族的 17 条标签的中文栏必须真有汉字**（M314 r2 的 p2-1）。
    ///
    /// 为什么需要它（缺口是实测出来的，不是猜的）：`assert_ne!` 只抓得住「中文栏整条照抄英文」，
    /// 抓不住**ASCII 变体**——把 `("Density", "密度")` 写成 `("Density", "Density ")`，再把金表
    /// 跟着改成一致（作者连写两处的真实情形），`chinese_catalog_is_actually_chinese` 与金表逐字节
    /// **一起**放行，中文界面上那一行就成了英文。牙口是「中文里有没有汉字」，不是「与英文是否相同」。
    ///
    /// **变异实测**（M314 r2，独立复跑）：`SettingsDemoDensity` 的中文改 `"Density "` + 同步改
    /// `golden/tail2.rs` 那一行 ⇒ **36 passed / 1 failed**，红在本文件，原文
    /// 「SettingsDemoDensity 的中文里一个汉字都没有：Density  」——前 36 条（含 `assert_ne!` 与
    /// 金表逐字节）全绿，正是这个缺口。
    ///
    /// 抓不住：译文语义对不对（机器只能证明「有汉字」）；也抓不住标签被译成另一种非拉丁文。
    #[test]
    fn every_demo_label_is_translated_into_chinese() {
        let zh = Catalog::of(Lang::ZhCn);
        let mut count = 0;
        for key in Key::ALL
            .iter()
            .filter(|k| k.name().starts_with("SettingsDemo"))
        {
            count += 1;
            let chinese = zh.text(*key);
            assert!(
                chinese.chars().any(is_han),
                "{} 的中文里一个汉字都没有：{chinese}",
                key.name()
            );
        }
        // 条数也要钉：以后往这一族加标签却忘了它被本用例覆盖（例如改了前缀），这里会先红。
        assert_eq!(count, 17, "演示行标签应当正好 17 条");
    }
}
