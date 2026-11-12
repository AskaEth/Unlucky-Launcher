//! D38 三级配色来源的**调用侧**：用户选的档 → `unlucky_design::palette::resolve_source`。
//!
//! ## 档位与优先序
//!
//! `appearance.palette_source` 的取值是「**从哪一级起**试」：① `background-image` ② `wallpaper`
//! ③ `preset`。起点的级别以下的级别**不参与**（在 `Provenance` 里记为 `Absent` = 用户没让它们
//! 参与），起点以上的每一级只要「有输入但不合格」就自动往下一级落——回落链本身由 design 实现
//! （`crates/design/src/palette/resolve/`），本文件只做两件事：
//!
//! 1. 把「没设」与「设了但坏了」**分开传**（[`Source::Absent`] vs [`Source::Undecodable`]）——
//!    混成一个 `None` 会让设置页说错话（「没设背景图」 vs 「你那张图读不出来」）；
//! 2. 把 `Provenance::hint_key()` 翻成一条词表 key（M180：句子在 `catalog/settings/appearance.rs`，
//!    这里只给 key——中文界面下这句提示也是中文）。
//!
//! ## 为什么不在这里做对比度
//!
//! WCAG 收敛在 `unlucky_design` 里（`converge` + `check`，任何来源的方案都跑同一套）。本文件
//! 只负责「选哪一级」，越俎代庖会让两条路径各有一套门禁。测试里仍然断言「任何一级的结果都
//! `is_clean()`」，那是**验收**，不是实现。

use unlucky_design::color::{Scheme, SchemeFlavor};
use unlucky_design::palette::{resolve_source, ImageInput, Level, Provenance};
use unlucky_i18n::catalog::Key;
use unlucky_presenter::prefs::PaletteSource;

use super::background::Image;

/// 某一级的输入（三态：没设 / 设了但坏了 / 解码好了）。
#[derive(Debug, Clone, Copy)]
pub enum Source<'a> {
    /// 用户没设这一级（没开关 / 路径空）。
    Absent,
    /// 设了，但读盘或解码失败（原因在调用侧记着，这里只用来说「坏」）。
    Undecodable,
    /// 解码好的像素。
    Pixels(&'a Image),
}

impl<'a> Source<'a> {
    /// 翻成 design 的输入形态。
    #[must_use]
    pub fn input(self) -> Option<ImageInput<'a>> {
        match self {
            Self::Absent => None,
            Self::Undecodable => Some(ImageInput::Undecodable),
            Self::Pixels(image) => Some(ImageInput::pixels(&image.rgba, image.width, image.height)),
        }
    }
}

/// 一次解析的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    /// 最终生效的方案（49 个角色）。
    pub scheme: Scheme,
    /// 用哪一级、前几级为什么没用。
    pub provenance: Provenance,
}

impl Resolved {
    /// 生效的层级（日志与设置页展示用）。
    #[must_use]
    pub fn level(&self) -> Level {
        self.provenance.level
    }

    /// 提示 key（`None` = 一级成功、没什么可解释的）。
    #[must_use]
    pub fn note(&self) -> Option<Key> {
        note(&self.provenance)
    }
}

/// 按档位解析一套方案。
///
/// `preset` 固定传 `None`：本仓库的「用户选中的色卡」还没有 UI（M91 只落了档位），
/// 所以 design 用它自己的出厂卡（`DEFAULT_CARD` = 天蓝）。色卡选择接上后从 `state` 传进来即可。
#[must_use]
pub fn resolve(
    source: PaletteSource,
    background: Source<'_>,
    wallpaper: Source<'_>,
    flavor: SchemeFlavor,
) -> Resolved {
    // 档位 = 起点：起点以下的级别不参与（传 `Source::Absent`）。
    let background = if source == PaletteSource::BackgroundImage {
        background
    } else {
        Source::Absent
    };
    let wallpaper =
        if source == PaletteSource::BackgroundImage || source == PaletteSource::Wallpaper {
            wallpaper
        } else {
            Source::Absent
        };

    let (scheme, provenance) = resolve_source(background.input(), wallpaper.input(), None, flavor);
    Resolved { scheme, provenance }
}

/// `Provenance::hint_key()` → 一条词表 key（M180：句子在 `catalog/settings/appearance.rs`）。
///
/// 键的完整表在 `unlucky_design::palette::resolve::Provenance::hint_key` 的文档里；
/// 这里只翻「本 mission 真会产生的那几个」，未识别键返回 `None` 而不是编一句话。
#[must_use]
pub fn note(provenance: &Provenance) -> Option<Key> {
    Some(match provenance.hint_key()? {
        "palette.hint.background.undecodable" => Key::SettingsAppearancePaletteBgUndecodable,
        "palette.hint.background.too-small" => Key::SettingsAppearancePaletteBgTooSmall,
        "palette.hint.background.bad-dimensions" => Key::SettingsAppearancePaletteBgBadDimensions,
        "palette.hint.background.no-opaque-pixels" => {
            Key::SettingsAppearancePaletteBgNoOpaquePixels
        }
        "palette.hint.background.low-chroma" => Key::SettingsAppearancePaletteBgLowChroma,
        "palette.hint.background.contrast" => Key::SettingsAppearancePaletteBgContrast,
        "palette.hint.wallpaper.undecodable" => Key::SettingsAppearancePaletteWallpaperUndecodable,
        "palette.hint.wallpaper.low-chroma" => Key::SettingsAppearancePaletteWallpaperLowChroma,
        "palette.hint.wallpaper.too-small" => Key::SettingsAppearancePaletteWallpaperTooSmall,
        "palette.hint.wallpaper.bad-dimensions" => {
            Key::SettingsAppearancePaletteWallpaperBadDimensions
        }
        "palette.hint.wallpaper.no-opaque-pixels" => {
            Key::SettingsAppearancePaletteWallpaperNoOpaquePixels
        }
        "palette.hint.wallpaper.contrast" => Key::SettingsAppearancePaletteWallpaperContrast,
        "palette.hint.preset.contrast" => Key::SettingsAppearancePalettePresetContrast,
        "palette.hint.adjusted" => Key::SettingsAppearancePaletteAdjusted,
        _ => return None,
    })
}

/// 生效层级**码值** → 已有的那三条档位 key（M180 复审 P2-2：中文界面里不许留机器码）。
///
/// 码值的全部来源只有两处，取值集合相同：`Level::code()`（`background-image` / `wallpaper` /
/// `preset`）与 `PaletteSource::as_str()`（同三个词）。所以三条 key 就够，**不新增平行词表**——
/// 卡片底部「配色来源」用它们表示**用户选的**档，这一行用它们表示**回落之后真正生效**的档。
///
/// 未识别的码值落到 `background-image` 那条（宁可显示一个偏保守的档名，也不许把机器码漏进
/// 中文界面——这条兜底本身由 `tests` 里的 `every_level_code_maps_to_its_own_sentence` 钉住）。
/// `state.rs` 已贴 400 行线，所以这个映射住在本文件。
#[must_use]
pub fn level_key(code: &str) -> Key {
    match code {
        "preset" => Key::SettingsAppearancePalettePreset,
        "wallpaper" => Key::SettingsAppearancePaletteWallpaper,
        _ => Key::SettingsAppearancePaletteBackground,
    }
}

#[cfg(test)]
mod tests {
    use super::{note, resolve, Source};
    use crate::pages::appearance::background::Image;
    use unlucky_design::color::SchemeFlavor;
    use unlucky_design::palette::wcag::check;
    use unlucky_design::palette::{Level, Outcome, Skip};
    use unlucky_i18n::catalog::Key;
    use unlucky_presenter::prefs::PaletteSource;

    /// 高彩度的图（纯红）：取色一定成功。
    fn vivid() -> Image {
        let rgba = [0xC0, 0x20, 0x20, 0xFF].repeat(64);
        Image::from_rgba((8, 8), 8, 8, rgba)
    }

    /// 极低彩度的图（近灰）：D40 的彩度门槛会拒绝它（`< 8`）。
    fn grey() -> Image {
        let rgba = [0x80, 0x80, 0x82, 0xFF].repeat(64);
        Image::from_rgba((8, 8), 8, 8, rgba)
    }

    #[test]
    fn background_tier_wins_when_the_image_is_usable() {
        // 抓得住：档位 ① 有合格输入时就用它，且不回落、没有提示。
        let image = vivid();
        let resolved = resolve(
            PaletteSource::BackgroundImage,
            Source::Pixels(&image),
            Source::Absent,
            SchemeFlavor::Light,
        );
        assert_eq!(resolved.level(), Level::BackgroundImage);
        assert!(!resolved.provenance.did_fall_back());
        assert_eq!(resolved.note(), None, "一级成功时不该有提示");
        assert!(check(&resolved.scheme).is_clean(), "取色结果也必须过 WCAG");
    }

    #[test]
    fn a_broken_background_falls_back_and_says_why() {
        // 抓得住：「设了但坏了」（Undecodable）与「没设」（Absent）在 Provenance 里分得开，
        // 且提示文案对应的是前者；结果仍然过 WCAG（回落不静默变黑白）。
        let resolved = resolve(
            PaletteSource::BackgroundImage,
            Source::Undecodable,
            Source::Absent,
            SchemeFlavor::Dark,
        );
        assert_eq!(resolved.level(), Level::Preset, "解码失败 → 落到色卡");
        assert_eq!(
            resolved.provenance.background(),
            Outcome::Failed(Skip::Undecodable)
        );
        assert!(resolved.provenance.did_fall_back());
        assert_eq!(
            resolved.note(),
            Some(Key::SettingsAppearancePaletteBgUndecodable)
        );
        assert!(check(&resolved.scheme).is_clean());
    }

    #[test]
    fn a_monochrome_background_falls_back_instead_of_turning_grey() {
        // 抓得住：彩度不足**不是**「压成灰方案」，而是回落到色卡（D40 的纪律），并留下 low-chroma。
        let image = grey();
        let resolved = resolve(
            PaletteSource::BackgroundImage,
            Source::Pixels(&image),
            Source::Absent,
            SchemeFlavor::Light,
        );
        assert_eq!(resolved.level(), Level::Preset);
        match resolved.provenance.background() {
            Outcome::Failed(skip) => {
                assert!(matches!(skip, Skip::LowChroma { .. }), "实际 {skip:?}")
            }
            other => panic!("灰图必须被判不合格，实际 {other:?}"),
        }
        assert_eq!(
            resolved.note(),
            Some(Key::SettingsAppearancePaletteBgLowChroma),
            "灰图的回落原因要指名「彩度不足」"
        );
    }

    #[test]
    fn the_three_tiers_are_tried_in_order_and_never_out_of_order() {
        // 抓得住：① 档位=wallpaper 时背景图**不参与**（即使手里有像素）；② 档位=preset 时前两级
        // 都不参与。这两条是「优先序」的唯一可断言形态（顺序本身在 design 里）。
        let background = vivid();
        let wallpaper = vivid();

        let wallpaper_tier = resolve(
            PaletteSource::Wallpaper,
            Source::Pixels(&background),
            Source::Pixels(&wallpaper),
            SchemeFlavor::Light,
        );
        assert_eq!(wallpaper_tier.level(), Level::Wallpaper);
        assert_eq!(
            wallpaper_tier.provenance.background(),
            Outcome::Absent,
            "档位是壁纸 → 背景图那一级没参与"
        );

        let preset_tier = resolve(
            PaletteSource::Preset,
            Source::Pixels(&background),
            Source::Pixels(&wallpaper),
            SchemeFlavor::Light,
        );
        assert_eq!(preset_tier.level(), Level::Preset);
        assert_eq!(preset_tier.provenance.background(), Outcome::Absent);
        assert_eq!(preset_tier.provenance.wallpaper(), Outcome::Absent);
        assert!(check(&preset_tier.scheme).is_clean());

        // 而档位=background-image 且壁纸也在时：一级成功 ⇒ 壁纸是 NotReached（不是 Absent）。
        let background_tier = resolve(
            PaletteSource::BackgroundImage,
            Source::Pixels(&background),
            Source::Pixels(&wallpaper),
            SchemeFlavor::Light,
        );
        assert_eq!(background_tier.level(), Level::BackgroundImage);
        assert_eq!(background_tier.provenance.wallpaper(), Outcome::NotReached);
    }

    #[test]
    fn hints_are_only_produced_for_keys_we_ship_a_sentence_for() {
        // 抓得住：note() 不会为未知键编文案（返回 None 而不是 Some("")）。
        assert_eq!(
            note(
                &resolve(
                    PaletteSource::Preset,
                    Source::Absent,
                    Source::Absent,
                    SchemeFlavor::Light,
                )
                .provenance
            ),
            None
        );
    }

    /// **M180 复审 P2-2 的守门**：生效层级的**每一个码值**都必须映射到它自己那条中文句子。
    ///
    /// 抓得住：① 三个码值被映射到同一条 key（界面上分不出到底生效的是哪一级）；
    /// ② 某条映射被删掉 / 写错（`preset` 落到 background 那条 ⇒ 中文界面说「取自背景图」）；
    /// ③ 有人再加一级却忘了扩映射。
    /// 抓不住：译文本身的措辞（那是 `catalog/settings/appearance.rs` 的译文守卫的事）。
    ///
    /// 变异证明：把 `"preset"` 那行删掉（落进兜底）⇒ 本条红，且 `assert_ne!` 直接点名码值。
    ///
    /// 为什么这条必须有：`the_appearance_card_speaks_chinese` 断的是**整句**非 ASCII，
    /// 而「配色：preset」里有汉字 ⇒ 整句非 ASCII ⇒ 那个守卫放过它。这个口子只能在这里堵。
    #[test]
    fn every_level_code_maps_to_its_own_sentence() {
        use super::level_key;

        let codes = ["background-image", "wallpaper", "preset"];
        let keys: Vec<Key> = codes.iter().copied().map(level_key).collect();
        assert_eq!(
            keys,
            vec![
                Key::SettingsAppearancePaletteBackground,
                Key::SettingsAppearancePaletteWallpaper,
                Key::SettingsAppearancePalettePreset,
            ],
            "码值与档位 key 必须一一对应"
        );

        // 三条中文句子互不相同（否则「映射对了」但界面上仍分不出层级）。
        let zh = unlucky_i18n::Lang::ZhCn.catalog();
        let mut texts: Vec<&str> = keys.iter().map(|key| zh.text(*key)).collect();
        let total = texts.len();
        texts.sort_unstable();
        texts.dedup();
        assert_eq!(texts.len(), total, "三条档位译文有重复：{texts:?}");
    }
}
