//! 背景图：**读盘解码（png / jpeg / bmp）→ 三种填充 → 暗化系数 → 可读诊断**。
//!
//! ## 真图（M93 r2 起）
//!
//! 渲染走 iced 的 `image`（`crates/app-iced/Cargo.toml` 的 `image-without-codecs`）：`Cover` /
//! `Contain` 交给 `ContentFit`，`Tile` 用本模块的 [`tiles`] 算网格后由 `picture` 真铺图块。
//! 本模块只负责「解码一次（取色 + 诊断 + 尺寸）」与「网格/暗化的纯数学」。
//!
//! | 档 | 渲染 | 露底色 |
//! |---|---|---|
//! | `cover` | `ContentFit::Cover`（铺满、裁掉溢出） | 否 |
//! | `contain` | `ContentFit::Contain`（完整显示、等比） | **是**（上下或左右） |
//! | `tile` | 原图尺寸平铺（[`tiles`] 给行列数，上限 [`MAX_TILES`]） | 否（图比视口大时=裁切） |
//!
//! ## 解码只在这里（一次）
//!
//! 一张图会被两处用到：①渲染器（`image::Handle::from_path`，由 iced 自己解码；r2 实测
//! `from_bytes` 的异步纹理分配赶不上截图第 3 帧，画面会是空的）；②取色（`unlucky_design` 要 RGBA
//! 像素，本模块把解码结果缩到 [`MAX_EDGE`] 内交给它）。本模块的解码只为「取色 + 诊断 + 尺寸」。
//!
//! ## 暗化（`appearance.bg_dim`）
//!
//! 语义是「让背景往面板底色退」，即把版面底色（`Role::Surface`）以 α = `dim` 叠在图上——
//! 与 `rgb + (toward − rgb) × dim` **在数学上等价**（图不透明时），所以这里只给一个钳位函数
//! [`dim_alpha`]，真正的合成交给渲染器（CPU/GPU 两档都支持普通 alpha 混合）。
//! 为什么向底色退而不是「叠黑」：浅色主题下叠黑会把亮图压成暗块，深色正文反而更读不清
//! （偏离登记在 [`super`] 的偏离表 #7）。

use unlucky_i18n::catalog::Key;
use unlucky_i18n::Lang;
use unlucky_presenter::prefs::BgFill;

/// 暗化上限（core 的 `BG_DIM_MAX` = 0.95，经 `unlucky_presenter::prefs` 重导出；这里只给 f32 形态）。
pub const BG_DIM_MAX: f32 = unlucky_presenter::prefs::BG_DIM_MAX as f32;

/// 解码后保留给取色的最大边长：取色只需要色彩分布，4K 图缩到 256 边后内存从 33MB 掉到 147KB。
pub const MAX_EDGE: u32 = 256;

/// 解码**之前**允许的原始边长上限。
///
/// 复审 r1 P2-4：原来写 16_384，等于允许一张 16384×16384 的 PNG 被**整幅解成 RGBA**
/// （≈1.07 GB）再缩到 256——那是解压炸弹。现在按「真实会遇到的壁纸尺寸」定：8K（7680×4320）
/// 留一倍余量 ⇒ **8192**。再大的图直接 [`LoadError::TooLarge`] + 可读提示（不静默）。
pub const MAX_SOURCE_EDGE: u32 = 8_192;

/// 解码时允许的最大内存占用（`image::Limits::max_alloc`）。
///
/// 8192×8192 的 RGBA 是 256 MB；给到 320 MB 足够正常图，同时把「边长合法但通道数/位深
/// 变态」的图挡在分配之前（第二道闸，第一道是 [`MAX_SOURCE_EDGE`]）。
pub const MAX_DECODE_BYTES: u64 = 320 * 1024 * 1024;

/// 平铺档「小图」的判据（dp）：短边低于它就一定会被放大铺，值得给一句提示。
pub const SMALL_TILE_EDGE: f32 = 200.0;

/// 平铺档一次最多铺多少个图块（列 × 行）。超了**放大图块**而不是少铺——见 [`tiles`] 的不变量。
pub const MAX_TILES: u32 = 64;

/// 图块放大步长（每次乘这个数，直到块数落进预算）。
pub const TILE_SCALE_STEP: f32 = 1.15;

/// 图块缩放上限（**只是安全阀**，正常情况下远达不到）。
///
/// 取 1024 而不是 64：`cols·rows ≤ MAX_TILES` 与「覆盖 ≥ 视口」在**极小图**上要求很大的缩放
/// （1×1 的图在 1180×760 上要 ~118 倍才能用 64 块盖满）。取小了就等于两者不可兼得——那一侧
/// 会退化成「留缝」，正是 r1 的 P2-1。真实图片不会走到这个上限。
pub const MAX_TILE_SCALE: f32 = 1024.0;

/// 解码失败的原因（人话在 [`LoadError::note`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// 读文件失败：不存在 / 没权限 / 被占用。
    Io(String),
    /// 解码失败：格式不认识（webp/tiff 等没开编码器）/ 文件截断 / 不是图片。
    Undecodable(String),
    /// 图片头里的尺寸超限。
    TooLarge {
        /// 宽。
        width: u32,
        /// 高。
        height: u32,
    },
}

/// 一条**可翻译**诊断：`key` + 已经格式化好的参数（模板里只有 `{name}`，见 [`Key`]）。
///
/// 为什么不是 `String`：文案表在金表里（M180 `I18NAPPEAR`），本模块只给「哪条 key + 哪些值」。
/// 数字在这一层就格式化好（`{:.2}` / `{:?}` 这些格式说明词表不支持）。
///
/// 为什么 `types` 是 `String` 而不是 `&'static str`：`Io(reason)` / `Undecodable(reason)` 的原因来自
/// 磁盘与解码库（运行时字符串），只能持有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    key: Key,
    args: Vec<(&'static str, String)>,
}

impl Note {
    /// 组装一条（参数名必须与译文里的 `{name}` 逐字相同）。
    fn new(key: Key, args: Vec<(&'static str, String)>) -> Self {
        Self { key, args }
    }

    /// 这条诊断是哪条文案。
    #[must_use]
    pub fn key(&self) -> Key {
        self.key
    }

    /// 参数（名 → 已成形的值）。
    #[must_use]
    pub fn args(&self) -> Vec<(&str, &str)> {
        self.args
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect()
    }

    /// 按语言取整句（`{name}` 全替换；缺参时占位符原样留着，见 [`unlucky_i18n::catalog::Catalog::render`]）。
    #[must_use]
    pub fn render(&self, lang: Lang) -> String {
        lang.catalog().render(self.key, &self.args())
    }

    /// 英文基线里有没有这个子串。
    ///
    /// 给**跨 mission 的既有断言**用（`app/tests/appearance.rs` 查「诊断里有没有说出原因」），
    /// 那条断言不在本 mission 的 scope 里、不能改成按 key 断言。UI 一律走 [`Self::render`]。
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        self.render(Lang::En).contains(needle)
    }
}

impl LoadError {
    /// 可翻译诊断（设置页提示行与日志共用同一条 key）。
    #[must_use]
    pub fn note(&self) -> Note {
        match self {
            Self::Io(reason) => Note::new(
                Key::SettingsAppearanceBgReadFailed,
                vec![("reason", reason.clone())],
            ),
            Self::Undecodable(reason) => Note::new(
                Key::SettingsAppearanceBgDecodeFailed,
                vec![("reason", reason.clone())],
            ),
            Self::TooLarge { width, height } => Note::new(
                Key::SettingsAppearanceBgTooLarge,
                vec![
                    ("width", width.to_string()),
                    ("height", height.to_string()),
                    ("limit", MAX_SOURCE_EDGE.to_string()),
                ],
            ),
        }
    }
}

/// 一张解码好的背景图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// **原图**尺寸（像素）：填充几何按它算（`tile` = 原始尺寸平铺）。
    pub source: (u32, u32),
    /// 缩略图宽（≤ [`MAX_EDGE`]）。
    pub width: u32,
    /// 缩略图高（≤ [`MAX_EDGE`]）。
    pub height: u32,
    /// 缩略图 RGBA8（取色用；`width × height × 4`）。
    pub rgba: Vec<u8>,
}

impl Image {
    /// 从已解码的数据构造（测试也走这里，避免测试依赖磁盘）。
    #[must_use]
    pub fn from_rgba(source: (u32, u32), width: u32, height: u32, rgba: Vec<u8>) -> Self {
        Self {
            source,
            width,
            height,
            rgba,
        }
    }
}

/// 读盘 + 解码 + 缩到 [`MAX_EDGE`] 内。
///
/// # Errors
/// 见 [`LoadError`]：读不到 / 解不开 / 头部尺寸超限。**不 return 默认色**——回落的决定在
/// [`super::palette`]（它会往 `Provenance` 里留原因），这一层只管如实报告失败。
pub fn load(path: &str) -> Result<Image, LoadError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(LoadError::Io("no path configured".to_owned()));
    }
    let bytes = std::fs::read(trimmed).map_err(|error| LoadError::Io(error.to_string()))?;

    // 第一趟只读图头：先按尺寸拒掉压缩炸弹，再真解码。
    let (source_w, source_h) = header_size(&bytes)?;
    if source_w > MAX_SOURCE_EDGE || source_h > MAX_SOURCE_EDGE {
        return Err(LoadError::TooLarge {
            width: source_w,
            height: source_h,
        });
    }

    // 第二道闸（复审 r1 P2-4）：把解码器也关进同样的上限里，别只靠自己先读图头。
    let mut reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|error| LoadError::Undecodable(error.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_EDGE);
    limits.max_image_height = Some(MAX_SOURCE_EDGE);
    limits.max_alloc = Some(MAX_DECODE_BYTES);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|error| LoadError::Undecodable(error.to_string()))?;
    let (width, height) = (decoded.width().max(1), decoded.height().max(1));
    let scale = (f64::from(MAX_EDGE) / f64::from(width.max(height))).min(1.0);
    let target_w = ((f64::from(width) * scale).round() as u32).max(1);
    let target_h = ((f64::from(height) * scale).round() as u32).max(1);
    let thumb = image::imageops::resize(
        &decoded,
        target_w,
        target_h,
        image::imageops::FilterType::Triangle,
    );

    Ok(Image {
        source: (source_w, source_h),
        width: target_w,
        height: target_h,
        rgba: thumb.into_raw(),
    })
}

/// 原图尺寸（只读图头，不解像素）。
fn header_size(bytes: &[u8]) -> Result<(u32, u32), LoadError> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| LoadError::Undecodable(error.to_string()))?;
    if reader.format().is_none() {
        return Err(LoadError::Undecodable(
            "unknown image format (magic bytes not recognised)".to_owned(),
        ));
    }
    reader
        .into_dimensions()
        .map_err(|error| LoadError::Undecodable(error.to_string()))
}

/// 平铺档的**诊断**（复审 r1 P2-1）：小图会被放大图块，必须说出来而不是静默。
///
/// 判据只看图片本身（短边 < [`SMALL_TILE_EDGE`]）——视口尺寸在渲染层才知道，而这里要的是
/// 「这张图小到要放大」这个与视口无关的事实。`scale` 条数上限由 [`tiles`] 保证覆盖 ≥ 视口。
#[must_use]
pub fn small_tile_note(image: &Image, fill: BgFill) -> Option<Note> {
    if !matches!(fill, BgFill::Tile) {
        return None;
    }
    let (width, height) = (image.source.0 as f32, image.source.1 as f32);
    if width.min(height) >= SMALL_TILE_EDGE {
        return None;
    }
    Some(Note::new(
        Key::SettingsAppearanceBgSmallTile,
        vec![
            ("width", image.source.0.to_string()),
            ("height", image.source.1.to_string()),
            ("max", MAX_TILES.to_string()),
        ],
    ))
}

/// 暗化系数（叠在图上那层底色的 α）：`0` = 不暗化，上限 [`BG_DIM_MAX`]。
#[must_use]
pub fn dim_alpha(dim: f32) -> f32 {
    if dim.is_nan() {
        0.0
    } else {
        dim.clamp(0.0, BG_DIM_MAX)
    }
}

/// 平铺档的网格（列 × 行 × 图块缩放）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tiles {
    /// 列数。
    pub cols: u32,
    /// 行数。
    pub rows: u32,
    /// 单块图相对原尺寸的缩放（`1.0` = 原尺寸；超预算时**只放大图块**，不减少覆盖）。
    pub scale: f32,
}

impl Tiles {
    /// 总块数。
    #[must_use]
    pub fn count(self) -> u32 {
        self.cols * self.rows
    }

    /// 一块图的实际边长（dp）：`源尺寸 × scale`。
    #[must_use]
    pub fn tile_size(self, source: (u32, u32)) -> (f32, f32) {
        (
            source.0.max(1) as f32 * self.scale,
            source.1.max(1) as f32 * self.scale,
        )
    }

    /// 铺满后覆盖的宽高（dp）：**恒 ≥ 视口**（[`tiles`] 的硬不变量，单测钉住）。
    #[must_use]
    pub fn covered(self, source: (u32, u32)) -> (f32, f32) {
        let (w, h) = self.tile_size(source);
        (self.cols as f32 * w, self.rows as f32 * h)
    }
}

/// 平铺档的网格（原图 1 像素 = 1 dp）。
///
/// **硬不变量：`covered >= viewport`**——任何情况都不许留缝（复审 r1 P2-1 的返工点）。
///
/// r1 的写法是「超预算就减列/减行」，于是 120×120 的图需要 10×7=70 块 > [`MAX_TILES`]，
/// 列数被砍到 9 ⇒ 覆盖宽 1080 < 1180，右缘 100px 裸白。现在的做法是**放大图块步长**：
/// `scale` 从 1.0 起按 [`TILE_SCALE_STEP`] 逐级变大，直到 `ceil(视口/图块) 的乘积 ≤ 预算`。
/// 图块按同一比例放大 ⇒ 图不变形；`ceil` 保证每行/每列都盖过视口边界（多出来的部分由渲染器裁掉）。
///
/// 图比视口大时是 `1×1, scale=1.0`（= 以原尺寸裁切），与小图平铺是同一套算式。
#[must_use]
pub fn tiles(source: (u32, u32), viewport: (f32, f32)) -> Tiles {
    let mut tiles = Tiles {
        cols: 1,
        rows: 1,
        scale: 1.0,
    };

    loop {
        let (width, height) = tiles.tile_size(source);
        tiles.cols = ceil_div(viewport.0.max(1.0), width);
        tiles.rows = ceil_div(viewport.1.max(1.0), height);
        if tiles.count() <= MAX_TILES || tiles.scale >= MAX_TILE_SCALE {
            return tiles;
        }
        tiles.scale = (tiles.scale * TILE_SCALE_STEP).min(MAX_TILE_SCALE);
    }
}

fn ceil_div(value: f32, step: f32) -> u32 {
    let count = (value / step).ceil();
    if count.is_finite() && count >= 1.0 {
        count as u32
    } else {
        1
    }
}

#[cfg(test)]
mod tests;
