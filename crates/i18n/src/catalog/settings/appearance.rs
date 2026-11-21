//! 设置页「Appearance」分组文案（M116 落了圆角一族；M180 `I18NAPPEAR` 补齐全组）。
//!
//! 加 key 时把条目写进 `$($acc)*` 后面即可（形状见 [`crate::catalog`] 的「域装配」一节）。
//!
//! ## 两批 key：圆角（M116）在前，其余（M180）在后
//!
//! M116 只落了圆角那 5 条；本分组其余控件（整窗/面板透明度、背景图、填充、暗化、配色来源）
//! 是 M93 落的，文案一直是英文基线字面量。M172 审计量出这一页 27 处文案里 22 处硬编码、中文界面
//! 本地化 0/27。M180 把**控件文案 + 运行时诊断 + 配色回落提示**全部搬进本文件：
//!
//! | 批次 | 条目 | 来源 |
//! |---|---|---|
//! | M116 | `Corner` … `CornerSuspended` | `pages/appearance/section/window.rs` 的圆角一段 |
//! | M180 | `WindowOpacity` … `PaletteAdjusted` | 卡片各段标签/说明 + `state.rs` / `background.rs` / `palette.rs` 的诊断 prose |
//!
//! ## 为什么诊断也在这里（而不是留在 `state.rs`）
//!
//! `pages/appearance/state.rs` 已经 390 行、贴着 400 行硬线，放不下任何一张文案表。所以那一层只
//! 给 **key + 已格式化好的参数**（`background::Note`），句子一律在本文件里。
//!
//! ## 数值参数：模板里只有 `{name}`，格式在 Rust 侧做
//!
//! [`crate::catalog::Catalog::render`] 只做 `{name}` → 值的字面替换，**不认** `{:.2}` 这类格式说明。
//! 所以「位置/尺寸/上限」这些数字由调用方先 `format!("{:.2}")` 好再当参数传进来：
//!
//! | key | 原字面量 | 现在 |
//! |---|---|---|
//! | `SettingsAppearancePanelOpacityForcedValue` | `format!("100% (forced from {:.2})")` | `{requested}` = `format!("{:.2}")` |
//! | `SettingsAppearanceWindowOpacityHint` | `Floor {:.2}` = `WINDOW_ALPHA_FLOOR` | `{floor}` |
//! | `SettingsAppearancePanelOpacityHint` | `Floor 0.40` = `PANEL_ALPHA_FLOOR` | `{floor}` |
//! | `SettingsAppearanceBgDimHint` | `Max 0.95` = `BG_DIM_MAX` | `{max}` |
//! | `SettingsAppearanceBgTooLarge` | `(limit 8192 px per edge)` = `MAX_SOURCE_EDGE` | `{limit}` |
//! | `SettingsAppearanceBgSmallTile` | `(max 64 tiles …)` = `MAX_TILES` | `{max}` |
//!
//! 常量走占位符而不是写进译文，是为了「改常量不用改两套译文」（M180 任务口径）。
//!
//! ## 文案口径（不许暗示所有平台一致）
//!
//! 用户定案：Win11 / Server 2025 开系统圆角，Win10 可以不开，其他系统按窗口合成器的默认规则。
//! 所以说明句必须写清「谁负责画圆」而不是「我们支持圆角」——后半句在 Win10 上是假的。

macro_rules! settings_appearance_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            SettingsAppearanceCorner => ("Window corners", "窗口圆角"),
            SettingsAppearanceCornerSystem => ("Follow system", "跟随系统"),
            SettingsAppearanceCornerSquare => ("Square", "直角"),
            SettingsAppearanceCornerHint => (
                "On Windows 11 and Server 2025 we ask the system for rounded corners; Windows 10 keeps square corners. On macOS and Linux the desktop's own rules apply. We never draw corners ourselves.",
                "Windows 11 与 Windows Server 2025 上我们请求系统处理外框圆角；Windows 10 保持直角；macOS 与 Linux 交给桌面合成器自己的规则；我们不自己画圆角。"
            ),
            SettingsAppearanceCornerSuspended => (
                "Not in effect right now: window transparency is below 100%. Setting it back to 100% restores the rounded corners.",
                "当前因整窗透明度不生效：把整窗透明度调回 100% 圆角即恢复。"
            ),
            // ---- M180 `I18NAPPEAR`：卡片内文案（① 整窗 / ③ 面板 / ④~⑧ 背景图 / ⑨ 配色来源）
            SettingsAppearanceWindowOpacity => ("Window transparency", "整窗透明度"),
            SettingsAppearanceWindowOpacityHint => (
                "Applies to the whole window. Floor {floor}; 1.00 = fully opaque.",
                "作用于整个窗口。下限 {floor}；1.00 = 完全不透明。"
            ),
            SettingsAppearancePanelOpacity => ("Panel transparency", "面板透明度"),
            SettingsAppearancePanelOpacityHint => (
                "Only the panel/card layer. Floor {floor}; independent of window transparency.",
                "只作用于面板 / 卡片这一层。下限 {floor}；与整窗透明度互不影响。"
            ),
            SettingsAppearancePanelOpacityForcedValue => (
                "100% (forced from {requested})",
                "100%（强制，原请求值 {requested}）"
            ),
            SettingsAppearancePanelOpacityForcedHint => (
                "Software renderer: panels and glass stay fully opaque (window transparency still works).",
                "软件渲染：面板与玻璃效果恒为不透明（整窗透明度仍然有效）。"
            ),
            SettingsAppearanceBackgroundImage => ("Background image", "背景图"),
            SettingsAppearanceOff => ("Off", "关"),
            SettingsAppearanceOn => ("On", "开"),
            SettingsAppearanceImagePath => ("Image path", "图片路径"),
            SettingsAppearancePaletteLevel => ("Colours: {level}", "配色：{level}"),
            SettingsAppearanceBgFill => ("Fill", "填充方式"),
            SettingsAppearanceBgFillCover => ("Cover", "铺满"),
            SettingsAppearanceBgFillContain => ("Contain", "完整显示"),
            SettingsAppearanceBgFillTile => ("Tile", "平铺"),
            SettingsAppearanceBgDim => ("Dim", "暗化"),
            SettingsAppearanceBgDimHint => (
                "Blends the image toward the panel colour so text stays readable. Max {max}.",
                "把背景图往面板底色上退，让文字读得清。上限 {max}。"
            ),
            SettingsAppearancePaletteSource => ("Colour source", "配色来源"),
            SettingsAppearancePaletteBackground => ("Background", "取自背景图"),
            SettingsAppearancePaletteWallpaper => ("Wallpaper", "取自壁纸"),
            SettingsAppearancePalettePreset => ("Preset", "内置色卡"),
            // ---- M180：运行时诊断（`background.rs` 的三种读盘失败 + 小图平铺）。
            //
            // M180 r1 复审 P2-1：软件渲染那句**只有一条**，就挂在面板那一行（`PanelOpacityForcedHint`）
            // 上——它挨着被禁用的滑块，正好回答「为什么拖不动」；这里不再有第二句。
            SettingsAppearanceBgReadFailed => (
                "Background image could not be read: {reason}",
                "背景图读不出来：{reason}"
            ),
            SettingsAppearanceBgDecodeFailed => (
                "Background image could not be decoded ({reason}; this build decodes PNG, JPEG and BMP)",
                "背景图解不开（{reason}；这个构建只支持 PNG、JPEG 与 BMP）"
            ),
            SettingsAppearanceBgTooLarge => (
                "Background image is too large to decode: {width}×{height} (limit {limit} px per edge)",
                "背景图太大，解不开：{width}×{height}（每边上限 {limit} 像素）"
            ),
            SettingsAppearanceBgSmallTile => (
                "Small image ({width}×{height}): tiles are scaled up so the window stays covered (max {max} tiles, no gaps).",
                "图片很小（{width}×{height}）：图块会被放大，保证窗口铺满（最多 {max} 块，不留缝）。"
            ),
            // ---- M180：配色回落提示（`palette.rs` 把 design 的 `hint_key()` 翻成下面这些）
            SettingsAppearancePaletteBgUndecodable => (
                "Background image could not be read; colours fall back.",
                "背景图读不出来，配色回落到下一级。"
            ),
            SettingsAppearancePaletteBgTooSmall => (
                "Background image is too small to sample; colours fall back.",
                "背景图太小，取不了色，配色回落到下一级。"
            ),
            SettingsAppearancePaletteBgBadDimensions => (
                "Background image data looks inconsistent; colours fall back.",
                "背景图数据前后不一致，配色回落到下一级。"
            ),
            SettingsAppearancePaletteBgNoOpaquePixels => (
                "Background image is almost fully transparent; colours fall back.",
                "背景图几乎全透明，配色回落到下一级。"
            ),
            SettingsAppearancePaletteBgLowChroma => (
                "Background image is too monochrome to tint from; colours fall back.",
                "背景图太单调（彩度不足），取不出配色，回落到下一级。"
            ),
            SettingsAppearancePaletteBgContrast => (
                "Background image colours would be unreadable; colours fall back.",
                "按背景图取的颜色读不清，配色回落到下一级。"
            ),
            SettingsAppearancePaletteWallpaperUndecodable => (
                "Desktop wallpaper could not be read.",
                "桌面壁纸读不出来。"
            ),
            SettingsAppearancePaletteWallpaperTooSmall => (
                "Desktop wallpaper is too small to sample.",
                "桌面壁纸太小，取不了色。"
            ),
            SettingsAppearancePaletteWallpaperBadDimensions => (
                "Desktop wallpaper data looks inconsistent.",
                "桌面壁纸数据前后不一致。"
            ),
            SettingsAppearancePaletteWallpaperNoOpaquePixels => (
                "Desktop wallpaper is almost fully transparent.",
                "桌面壁纸几乎全透明。"
            ),
            SettingsAppearancePaletteWallpaperLowChroma => (
                "Desktop wallpaper is too monochrome to tint from.",
                "桌面壁纸太单调（彩度不足），取不出配色。"
            ),
            SettingsAppearancePaletteWallpaperContrast => (
                "Desktop wallpaper colours would be unreadable; colours fall back.",
                "按桌面壁纸取的颜色读不清，配色回落到下一级。"
            ),
            SettingsAppearancePalettePresetContrast => (
                "The built-in card needed a small contrast correction.",
                "内置色卡做了一点对比度修正。"
            ),
            SettingsAppearancePaletteAdjusted => (
                "A few colours were nudged for readability.",
                "有几个颜色为了可读性做了微调。"
            ),
        } }
    };
}

pub(crate) use settings_appearance_entries;

#[cfg(test)]
mod tests {
    use crate::catalog::{placeholders, Catalog, Key};
    use crate::Lang;

    /// 本域的 key（`Key::ALL` 里按名字前缀筛；不手工列表，漏一条就少一条守卫）。
    fn appearance_keys() -> Vec<Key> {
        Key::ALL
            .iter()
            .copied()
            .filter(|key| key.name().starts_with("SettingsAppearance"))
            .collect()
    }

    /// 有没有汉字（与 `pages/settings/tests/appearance_section.rs` 的 `is_han` 同一段码位）。
    fn is_han(ch: char) -> bool {
        ('\u{4e00}'..='\u{9fff}').contains(&ch)
    }

    /// **M180 守门 ②**：外观分组的每条 key 都必须真翻过——英文照抄进中文表就地判红。
    ///
    /// 抓得住：① 新加 key 时中文栏直接抄英文（`chinese_catalog_is_actually_chinese` 也抓这条，
    /// 但那条在 `catalog/tests.rs`、不属于本 mission 的 scope，而且不查「中文字里有没有汉字」）；
    /// ② 中文译文变成纯 ASCII（比如只写了 `PNG` / `Preset` 这种专名）；③ 占位符集合两边不一致
    /// （`{floor}` 只在一边有 ⇒ `render()` 吐出半截句子）。
    /// 抓不住：译文翻得对不对（语义只能人审）。
    ///
    /// 变异证明：把任一条的第二个字符串换成第一个（`("Cover", "Cover")`）⇒ 本条立刻红（`assert_ne!`
    /// 与 `is_han` 命中同一行）。
    #[test]
    fn every_appearance_key_is_translated_into_chinese() {
        let keys = appearance_keys();
        assert!(
            keys.len() >= 40,
            "外观分组至少 40 条 key（M180 落地时是 45 条），实际 {}：\
             key 被搬走或前缀改名了？",
            keys.len()
        );

        let en = Catalog::of(Lang::En);
        let zh = Catalog::of(Lang::ZhCn);
        for key in keys {
            let (english, chinese) = (en.text(key), zh.text(key));
            assert_ne!(english, chinese, "{} 的中文是英文照抄", key.name());
            assert!(
                chinese.chars().any(is_han),
                "{} 的中文里一个汉字都没有：{chinese}",
                key.name()
            );
            let mut left = placeholders(english);
            let mut right = placeholders(chinese);
            left.sort_unstable();
            right.sort_unstable();
            assert_eq!(left, right, "{} 的占位符两套译文不一致", key.name());
        }
    }

    /// 数值参数必须走占位符，不许把常量写进译文（改常量时不该还要改两套译文）。
    #[test]
    fn numeric_limits_are_placeholders_not_text() {
        let en = Catalog::of(Lang::En);
        for (key, needle, placeholder) in [
            (Key::SettingsAppearanceWindowOpacityHint, "0.08", "floor"),
            (Key::SettingsAppearancePanelOpacityHint, "0.40", "floor"),
            (Key::SettingsAppearanceBgDimHint, "0.95", "max"),
            (Key::SettingsAppearanceBgTooLarge, "8192", "limit"),
            (Key::SettingsAppearanceBgSmallTile, "64", "max"),
        ] {
            let text = en.text(key);
            assert!(
                !text.contains(needle),
                "{} 把常量 {needle} 写死在译文里了（应当走 {{{placeholder}}}）",
                key.name()
            );
            assert!(
                placeholders(text).contains(&placeholder),
                "{} 缺 {{{placeholder}}} 占位符：{text}",
                key.name()
            );
        }
    }
}
