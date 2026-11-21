//! 背景层的**像素**：真图直出（`Cover` / `Contain` / `Tile`）+ 暗化叠层 + 可选的**预模糊**。
//!
//! 与 [`super::section`](mod@super::section) 拆开只为单文件 ≤400 行的纪律；对外入口仍是
//! `pages::appearance::layer`（本文件 [`layer`] 的再导出）。
//!
//! ## 三种填充与暗化
//!
//! `Cover`/`Contain` 交给渲染器的 `ContentFit`；`Tile` 用 [`background::tiles`] 算行列后铺 N 个图块。
//! 暗化不是逐像素改色，而是**盖一层「版面底色 @ α=dim」**（[`background::dim_alpha`] 有等价性推导），
//! CPU/GPU 两档都支持。
//!
//! ## 形态 A：预模糊壁纸（M435，**默认关**）
//!
//! 开关是环境变量 [`ENV_BG_BLUR`]（`on` = 默认档）：**不设就一个像素都不变**（首刀不改现有观感；
//! 改前/改后对照图与噪声基线见 `DevDocs/artifacts/m435/`）。打开后 `art` 换成「读一次 → 缩到长边
//! [`BLUR_EDGE`] → 高斯模糊 σ → `Handle::from_rgba`」的句柄，**其余逐字不动**。
//!
//! 为什么只做一次：糊是纯 CPU 代码，但读盘 + 解码 + 卷积塞进每帧的 `view` 就是每帧几十毫秒。结果
//! 进 [`BLUR_CACHE`]（**单条**，键 = [`BlurKey`]，内存 ≤4 MB）：换壁纸（路径变）/ 同名文件被换掉
//! （`len`+`mtime` 变）/ 改 σ 要重算；**改主题 / 面板 α / 暗化 / 窗口尺寸都不重算**——前三个是糊完
//! **之后**叠上去的层，窗口尺寸由固定长边的工作副本吸收。σ 取值见 [`BLUR_SIGMA_DEFAULT`]。
//!
//! ## 形态 A 的边界（**必须一起读**）
//!
//! 1. **面板下面还有别的内容时，透不出「被糊过的下一层」**：糊的是壁纸这张图本身，不是「面板背后
//!    的所有像素」——这条降级路径的根本边界（M434 的结论）；
//! 2. **没有逐区域遮罩**：换的是整层背景图的句柄，于是壁纸**在空隙处也是糊的**。实测（1180×1700、实例
//!    页空态、`DevDocs/artifacts/m435/README.md`）：壁纸可见 81.5% = 透面板 25.9% + 直接露壁纸的**空隙
//!    55.7%**（余下 18.5% 被不透明内容完全挡住）⇒ 开起来那 55.7% 一起糊，是「糊过的壁纸」而非「玻璃」；
//! 3. **真 backdrop blur 要 vendor 补丁，本 mission 没选**：窗口 surface 纹理的 usage 只有
//!    `RENDER_ATTACHMENT`（`iced_wgpu-0.14.0/src/window/compositor.rs:329`），既不能采样也不能拷出；
//!    系统毛玻璃在 Windows 是空函数（`winit-0.30.13/src/platform_impl/windows/window.rs:132`），且
//!    DX12 surface 只报 `Opaque` ⇒ 逐像素 α 被丢掉。三条都在 M434 的调查里。

use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use iced::widget::image;
use iced::widget::{column, container, image as image_widget, space};
use iced::{ContentFit, Element, Length};

use ::image::imageops::{self, FilterType};
use ::image::{ImageReader, Limits};

use unlucky_design::color::Role;
use unlucky_presenter::prefs::BgFill;

use crate::pages::{frame, Message};
use crate::theme::Ui;

use super::background;
use super::state::State;

/// 背景层（铺满整窗、垫在最下层）。`None` = 没有可用的背景图（调用方就不叠这一层）。
///
/// 真图直出（M93 r2）：`Cover`/`Contain` 交 `ContentFit`，`Tile` 走 [`background::tiles`] 的网格，
/// 暗化 = 一层「版面底色 @ α=dim」（[`background::dim_alpha`] 有等价性说明）。M435：开关打开时图源
/// 换成预模糊句柄（`blurred`，见本文件的「形态 A」一节），读不出来就回落真图——**失败不换观感**。
#[must_use]
pub fn layer(ui: &Ui, state: &State, viewport: (f32, f32)) -> Option<Element<'static, Message>> {
    let picture = state.image()?;
    if !state.bg_enabled {
        return None;
    }

    let path = state.bg_path.trim();
    let handle = blurred(path).unwrap_or_else(|| image_handle(path));
    let art: Element<'static, Message> = match state.bg_fill {
        BgFill::Cover => fitted(handle, ContentFit::Cover),
        BgFill::Contain => fitted(handle, ContentFit::Contain),
        BgFill::Tile => tiled(handle, picture.source, viewport),
    };

    // 暗化：底色 @ α=dim 盖在图上（等价于逐像素向底色插值，见 `background::dim_alpha`）。
    let scrim_color = ui
        .color(Role::Surface)
        .scale_alpha(background::dim_alpha(state.bg_dim));
    let scrim: Element<'static, Message> = container(space::horizontal())
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_theme| frame::filled(scrim_color))
        .into();

    Some(
        iced::widget::stack![art, scrim]
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
    )
}

/// 文件字节 → iced 的图片句柄（渲染器自己解复用，不再读盘）。
fn image_handle(path: &str) -> image::Handle {
    image::Handle::from_path(path)
}

/// `Cover` / `Contain`：交给渲染器的等比缩放档。
fn fitted(handle: image::Handle, fit: ContentFit) -> Element<'static, Message> {
    image_widget(handle)
        .content_fit(fit)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// `Tile`：按 [`background::tiles`] 的网格平铺。
///
/// 三个细节都是为了「不留缝」（复审 r1 P2-1）：①单块边长 = `源尺寸 × tiles.scale`——超预算时图块
/// **等比例放大**（不是少铺），所以 `cols × 边长 ≥ 视口宽`、`rows × 边长 ≥ 视口高` 由
/// [`background::tiles`] 保证；②`ContentFit::Fill`：`None` 会按**原生尺寸**画，比格子小就留空——那道缝
/// 正是它造成的；这里格子尺寸恰好等于「等比放大后的尺寸」，`Fill` 就是等比缩放本身（不变形）；
/// ③行高显式写死（`Fixed`）、列整体 `Fill`：不靠子控件撑，布局结果与算式一致。
fn tiled(
    handle: image::Handle,
    source: (u32, u32),
    viewport: (f32, f32),
) -> Element<'static, Message> {
    let tiles = background::tiles(source, viewport);
    let (width, height) = tiles.tile_size(source);

    let rows = (0..tiles.rows)
        .map(|_| {
            let cells = (0..tiles.cols)
                .map(|_| -> Element<'static, Message> {
                    image_widget(handle.clone())
                        .content_fit(ContentFit::Fill)
                        .width(Length::Fixed(width))
                        .height(Length::Fixed(height))
                        .into()
                })
                .collect::<Vec<_>>();
            iced::widget::row(cells)
                .width(Length::Fill)
                .height(Length::Fixed(height))
                .into()
        })
        .collect::<Vec<_>>();

    column(rows).width(Length::Fill).height(Length::Fill).into()
}

// ---------------------------------------------------------------- 形态 A：预模糊

/// 预模糊开关：`ULL_APPEARANCE_BG_BLUR=<σ>`（`on` / `true` / `yes` = 默认档 [`BLUR_SIGMA_DEFAULT`]）。
/// **不设 / 空 / `0` / 负数 / 非数 = 关**。与其它 `ULL_APPEARANCE_*` 取证开关同一条纪律：只影响这次
/// 进程、不落盘 ⇒ 不在设置页里（产品的落盘键在 `launcher.json`，不在本 mission 的 scope）。
pub const ENV_BG_BLUR: &str = "ULL_APPEARANCE_BG_BLUR";

/// 模糊**工作副本**的长边（像素）：原图先缩到它，再糊。
///
/// 不按「窗宽 1:1」是因为那样一改窗口尺寸就得重烘，拖缩放手柄会连糊几十次；固定 768 让**窗口尺寸
/// 不进缓存键**，而模糊结果本身是低频的，由渲染器等比放大到 1180 dp 看不出来。
pub const BLUR_EDGE: u32 = 768;

/// 默认 σ（**工作副本像素**；`ULL_APPEARANCE_BG_BLUR=on` 取它）。
///
/// **σ 是工作副本上的量，不是屏上的 dp**：屏上 `σ_dp = σ_work × (源长边 / BLUR_EDGE) × fit 放大率`，
/// 而 fit 放大率三档各不相同——`Cover` = `max(窗宽/源宽, 窗高/源高)`、`Contain` = `min(…)`、
/// `Tile` = [`background::tiles`] 的 `scale`。所以同一个 12：取证档（1180×1700 窗 + 1920×1440 图，
/// **默认档 Cover**）是 12 × 2.5 × 1.18 ≈ **35 dp**；1920×1080 满窗 + 同尺寸图（Cover = 1）≈ **30 dp**。
///
/// 它是「糊过的壁纸」的档，不是面板玻璃的档：这一刀连空隙一起糊（见上面边界 ②），可读性由面板 α
/// 兜底，所以按「一眼看得出糊了」定。要对齐 `11-m3-glass-visual-spec.md` §4.1 的 12~24 dp，把这行改成
/// **6.2** 即可（⇒ 取证档 ≈18.3 dp、1080p 档 ≈15.5 dp，正落在 Acrylic 实测 15~20 里）。
pub const BLUR_SIGMA_DEFAULT: f32 = 12.0;

/// σ 允许区间（工作副本像素；超界按边界钳位，挡住「σ=1000 糊成单色」那类输入）。
pub const BLUR_SIGMA_RANGE: (f32, f32) = (1.0, 32.0);

/// 缓存键：同一**路径 + 文件版本 + σ** 不重算。`len`+`mtime` 一起进键是为了抓住「路径没变但壁纸
/// 被换掉」（覆盖同名图 / 壁纸软件原地改文件）；`sigma_milli` 用整数：f32 既没有 `Eq` 也带 NaN。
#[derive(Debug, Clone, PartialEq, Eq)]
struct BlurKey {
    /// 背景图路径（已 trim）。
    path: String,
    /// 文件长度（字节）。
    len: u64,
    /// 修改时间 `(秒, 纳秒)`；平台拿不到时为 `None`（只靠 `len` 判新旧）。
    modified: Option<(u64, u32)>,
    /// σ × 1000 取整。
    sigma_milli: u32,
}

/// 进程内唯一的那条缓存：**只留最近一次的结果**——这就是「有界」的全部实现（≤4 MB）。不做多槽/LRU：
/// 一次只画一张背景图，多槽只能省「切回上一张」，代价是常驻几十 MB。`view` 只读状态 ⇒ 缓存只能是模
/// 块级全局；`Mutex` 只为满足 `static` 的线程安全要求。
static BLUR_CACHE: Mutex<Option<(BlurKey, image::Handle)>> = Mutex::new(None);

/// 开关的解析结果：**进程内只读一次**（`0.0` = 关：[`blur_sigma`] 只会给 ≥1 的 σ）。环境变量是启动期输入，每帧 `getenv` 只是白花时间。
static BLUR_SIGMA: OnceLock<f32> = OnceLock::new();

/// 命中判据（抽成函数是为了单测：`Mutex` 里的全局状态没法在并行测试里安全地造）。
///
/// 键相等就交出句柄——`Handle` 克隆即共享纹理，不是像素拷贝。
fn cache_hit<'a>(
    entry: &'a Option<(BlurKey, image::Handle)>,
    key: &BlurKey,
) -> Option<&'a image::Handle> {
    entry
        .as_ref()
        .filter(|(cached, _)| cached == key)
        .map(|(_, handle)| handle)
}

/// 面板背后那层「预模糊壁纸」：`Some` = 用它；`None`（开关没开 / 读不到文件 / 解码失败）一律**回落真图**。
///
/// 没有负缓存：失败帧会重读重解（现在够不到——`layer()` 先要求 `background::load` 成功，两侧闸与解码器
/// 一致）；锁中毒时同样退回「无缓存」而不是 panic。
fn blurred(path: &str) -> Option<image::Handle> {
    // 开关在 `get_or_init` 里读：进程只 `getenv` 一次（环境变量是启动期输入）。
    let sigma = *BLUR_SIGMA
        .get_or_init(|| blur_sigma(std::env::var(ENV_BG_BLUR).ok().as_deref()).unwrap_or(0.0));
    if sigma <= 0.0 {
        return None;
    }
    let key = blur_key(path, sigma)?;

    if let Ok(cache) = BLUR_CACHE.lock() {
        if let Some(hit) = cache_hit(&cache, &key) {
            return Some(hit.clone());
        }
    }

    let handle = render_blur(path, sigma)?;
    // 烘图（几十毫秒）放在锁外：锁只护「读/写那一条」；写入即覆盖上一条，内存不涨。
    if let Ok(mut cache) = BLUR_CACHE.lock() {
        *cache = Some((key, handle.clone()));
    }
    Some(handle)
}

/// 环境变量原文 → σ（`None` = 关）：空/`0`/负数/非数不认，`on`/`true`/`yes` = 默认档，其余按数值钳位。
fn blur_sigma(raw: Option<&str>) -> Option<f32> {
    let text = raw?.trim();
    if text.is_empty() {
        return None;
    }
    let value = match text.to_ascii_lowercase().as_str() {
        "on" | "true" | "yes" => BLUR_SIGMA_DEFAULT,
        _ => text
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)?,
    };
    Some(value.clamp(BLUR_SIGMA_RANGE.0, BLUR_SIGMA_RANGE.1))
}

/// 文件元信息 → 缓存键（读不到文件 = `None`，调用方回落真图）。
fn blur_key(path: &str, sigma: f32) -> Option<BlurKey> {
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok().and_then(|time| {
        time.duration_since(UNIX_EPOCH)
            .ok()
            .map(|elapsed| (elapsed.as_secs(), elapsed.subsec_nanos()))
    });
    Some(BlurKey {
        path: path.to_owned(),
        len: meta.len(),
        modified,
        // σ 已钳在 1~32 ⇒ 1000~32000，转 u32 不回绕。
        sigma_milli: (sigma * 1000.0).round() as u32,
    })
}

/// 工作副本尺寸：长边压到 [`BLUR_EDGE`] 内，**不放大**（小图保持原尺寸）。
fn work_size(source: (u32, u32)) -> (u32, u32) {
    let (width, height) = (source.0.max(1), source.1.max(1));
    let longest = width.max(height);
    if longest <= BLUR_EDGE {
        return (width, height);
    }
    let scale = f64::from(BLUR_EDGE) / f64::from(longest);
    (
        ((f64::from(width) * scale).round() as u32).max(1),
        ((f64::from(height) * scale).round() as u32).max(1),
    )
}

/// 读盘 → 解码 → 缩到工作尺寸 → 高斯模糊 → 渲染器句柄。**整条链路只在这里跑一次**（缓存保证）。
/// 解码带 [`background`] 的两道闸（[`background::MAX_SOURCE_EDGE`] / [`background::MAX_DECODE_BYTES`]）：
/// 模糊路径是**第二遍读盘**，不能靠 `State::refresh_background` 那一遍的校验替它把关。
fn render_blur(path: &str, sigma: f32) -> Option<image::Handle> {
    let bytes = std::fs::read(path).ok()?;

    let mut limits = Limits::default();
    limits.max_image_width = Some(background::MAX_SOURCE_EDGE);
    limits.max_image_height = Some(background::MAX_SOURCE_EDGE);
    limits.max_alloc = Some(background::MAX_DECODE_BYTES);

    let mut reader = ImageReader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    reader.limits(limits);
    let decoded = reader.decode().ok()?.to_rgba8();

    let (width, height) = work_size((decoded.width(), decoded.height()));
    let small = imageops::resize(&decoded, width, height, FilterType::Triangle);
    let blurred = imageops::blur(&small, sigma);

    Some(image::Handle::from_rgba(width, height, blurred.into_raw()))
}

#[cfg(test)]
mod tests {
    //! 纯函数 / 命中判据 / 整条模糊链路的单测。**内联**而不放 `tests.rs`：新文件路径不在本 mission 的 scope。

    use super::*;

    fn key() -> BlurKey {
        key_with(10, 12_000, Some((1, 2)))
    }

    /// 造一个只改某一维的键（避开 macro 里的结构体字面量被 rustfmt 撑成 6 行）。
    fn key_with(len: u64, sigma_milli: u32, modified: Option<(u64, u32)>) -> BlurKey {
        BlurKey {
            path: "a.png".to_owned(),
            len,
            modified,
            sigma_milli,
        }
    }

    #[test]
    fn sigma_is_off_until_explicitly_switched_on_with_a_legal_value() {
        for off in ["", "  ", "0", "-4", "nan", "inf", "blur"] {
            assert_eq!(blur_sigma(Some(off)), None, "{off:?} 必须等于关");
        }
        assert_eq!(blur_sigma(None), None);
        assert_eq!(blur_sigma(Some("on")), Some(BLUR_SIGMA_DEFAULT));
        assert_eq!(blur_sigma(Some("YES")), Some(BLUR_SIGMA_DEFAULT));
        assert_eq!(blur_sigma(Some(" 18 ")), Some(18.0));
        // 超界钳位：右界挡住「糊成一块色」，左界挡住 σ=0 的无意义重算。
        assert_eq!(blur_sigma(Some("999")), Some(BLUR_SIGMA_RANGE.1));
        assert_eq!(blur_sigma(Some("0.01")), Some(BLUR_SIGMA_RANGE.0));
        // 工作副本：长边钉在 BLUR_EDGE（两个方向），小图不放大。
        assert_eq!(work_size((1920, 1080)), (768, 432));
        assert_eq!(work_size((1080, 1920)), (432, 768));
        assert_eq!(work_size((400, 300)), (400, 300));
        assert_eq!(work_size((0, 0)), (1, 1));
    }

    #[test]
    fn cache_hit_only_fires_on_the_same_key() {
        let entry = Some((key(), image::Handle::from_rgba(1, 1, vec![0, 0, 0, 255])));
        assert!(cache_hit(&entry, &key()).is_some());
        assert!(cache_hit(&None, &key()).is_none());

        // 键的四维各破一次：路径 / 文件版本 / mtime / σ（后三个只靠 key_with 改一维）。
        let other_path = BlurKey {
            path: "b.png".to_owned(),
            ..key()
        };
        assert!(cache_hit(&entry, &other_path).is_none());
        assert_ne!(key(), key_with(11, 12_000, Some((1, 2))));
        assert_ne!(key(), key_with(10, 12_000, None));
        assert_ne!(key(), key_with(10, 18_000, Some((1, 2))));
        // 文件不存在 ⇒ 连键都造不出来（调用方据此回落真图）。
        assert!(blur_key("/definitely/not/here/m435.png", 12.0).is_none());
    }

    #[test]
    fn render_blur_lands_at_the_working_size_and_follows_the_file_version() {
        // 一张 1030×515 的 png 走完整条链路（读盘 → 解码 → 缩放 → 卷积 → `from_rgba`），顺便把
        // 「一次烘图」的耗时与「同名文件被换掉 ⇒ 键必须变」这两条证据打出来。
        let path = std::env::temp_dir().join("ull-m435-blur.png");
        let pixels: Vec<u8> = (0..1030 * 515)
            .flat_map(|index| [(index % 1030) as u8, (index / 1030) as u8, 128, 255])
            .collect();
        ::image::save_buffer(&path, &pixels, 1030, 515, ::image::ColorType::Rgba8)
            .expect("写临时 png");

        let before = blur_key(&path.to_string_lossy(), BLUR_SIGMA_DEFAULT).expect("键");
        assert!(before.modified.is_some(), "键里必须真填了 mtime");
        let started = std::time::Instant::now();
        let blurred = render_blur(&path.to_string_lossy(), BLUR_SIGMA_DEFAULT);
        let elapsed = started.elapsed();
        println!(
            "M435 一次烘图 1030x515→768x384：{:.1} ms",
            elapsed.as_secs_f64() * 1e3
        );

        // 换掉文件：先改长度，再**等长**原地改内容（后者 `len` 抓不到，只有 mtime 抓得住）。
        std::fs::write(&path, vec![0_u8; 8]).expect("原地换掉这张图");
        let after = blur_key(&path.to_string_lossy(), BLUR_SIGMA_DEFAULT).expect("键");
        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&path, vec![1_u8; 8]).expect("等长原地换掉这张图");
        let same = blur_key(&path.to_string_lossy(), BLUR_SIGMA_DEFAULT).expect("键");
        let _ = std::fs::remove_file(&path);
        assert_ne!(before, after);
        assert_ne!(after, same, "等长替换只能靠 mtime 抓");

        match blurred.expect("这张 png 必须能糊出来") {
            // 长边压到 BLUR_EDGE，另一边等比。
            image::Handle::Rgba { width, height, .. } => assert_eq!((width, height), (768, 384)),
            other => panic!("应该是 RGBA 句柄，实际 {other:?}"),
        }
    }
}
