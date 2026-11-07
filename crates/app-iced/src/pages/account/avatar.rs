//! 头像位与皮肤区：**首字母块**（无皮肤时）与**真图**（第三方轨抓到皮肤时）。
//!
//! ## 为什么头像可以「真有图」
//!
//! presenter 只把皮肤的 **PNG 原图**送到 UI（它不引入图片解码依赖），裁剪/缩放全部在这里做：
//! Minecraft 皮肤的头部在 `(8,8)` 的 8×8 方块（64×64 皮肤），帽子层在 `(40,8)`，
//! 两层贴起来再放大到 64×64 就是一张真正的头像。**解码失败一律退回首字母块**——
//! 头像位不允许出现白块（M114 明文要求）。
//!
//! ## 依赖口径
//!
//! 用的是 `crates/app-iced/Cargo.toml` **既有**的 `image`（png feature，M93 背景图那一条），
//! 本 mission 没有改任何依赖表。

use std::collections::BTreeMap;

use iced::widget::image;

use unlucky_presenter::dto::SkinImage;

/// 头像边长（像素）：列表行 40、当前账户卡片 56 共用同一张 64 的图（缩下来仍然清晰）。
const HEAD_PX: u32 = 64;
/// 皮肤原图的头部坐标（64×64 皮肤的标准布局）。
const FACE: (u32, u32) = (8, 8);
/// 帽子/头发层的坐标（叠在脸上一层）。
const HAT: (u32, u32) = (40, 8);
/// 头部方块边长（8×8）。
const FACE_EDGE: u32 = 8;

/// 一个账户的解码结果（失败就是两个 `None`，UI 画首字母块）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Avatar {
    /// 裁好的头像（64×64 RGBA）
    pub head: Option<image::Handle>,
    /// 皮肤原图（皮肤预览区用）
    pub skin: Option<image::Handle>,
}

/// 解码缓存：同一个账户的同一张皮肤只解码一次。
///
/// 键 = 账户 id，值 = (皮肤来源串, 解码结果)。来源串变了（重新登录换了皮肤）就重解。
#[derive(Debug, Default)]
pub struct Cache {
    entries: BTreeMap<String, (String, Avatar)>,
}

impl Cache {
    /// 取某个账户的头像（必要时解码）。
    pub fn resolve(&mut self, id: &str, skin: Option<&SkinImage>) -> Avatar {
        let source = skin.map(|skin| skin.source.clone()).unwrap_or_default();
        let empty = skin.is_none_or(SkinImage::is_empty);
        if empty {
            // 没有皮肤 = 没有图（离线轨永远走这条；**不给假皮肤**）
            self.entries.remove(id);
            return Avatar::default();
        }
        if let Some((cached_source, avatar)) = self.entries.get(id) {
            if cached_source == &source {
                return avatar.clone();
            }
        }
        let decoded = skin.map(decode).unwrap_or_default();
        self.entries
            .insert(id.to_string(), (source, decoded.clone()));
        decoded
    }

    /// 当前缓存条目数（单测与诊断用）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 缓存是否为空。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// PNG 皮肤 → 头像（裁 + 叠帽子层 + 放大）。
fn decode(skin: &SkinImage) -> Avatar {
    let Some(decoded) = decode_png(&skin.png) else {
        return Avatar::default();
    };
    let Some(scaled) = compose_head(&decoded) else {
        return Avatar::default();
    };
    Avatar {
        head: Some(image::Handle::from_rgba(
            HEAD_PX,
            HEAD_PX,
            scaled.into_raw(),
        )),
        // 皮肤预览用**原图**（64×64）：tower 口径是「原图直接展示，不为好看伪造皮肤」
        skin: Some(image::Handle::from_bytes(skin.png.clone())),
    }
}

/// 裁 8×8 脸 + 叠 (40,8) 的帽子层 + 放大到 64×64（纯函数，单测直接验像素）。
///
/// 旧版 64×32 皮肤没有帽子层（只有脸），所以尺寸不够时只裁脸。
fn compose_head(decoded: &::image::RgbaImage) -> Option<::image::RgbaImage> {
    let (width, height) = (decoded.width(), decoded.height());
    if width < FACE.0 + FACE_EDGE || height < FACE.1 + FACE_EDGE {
        return None;
    }
    let mut face =
        ::image::imageops::crop_imm(decoded, FACE.0, FACE.1, FACE_EDGE, FACE_EDGE).to_image();
    if width >= HAT.0 + FACE_EDGE && height >= HAT.1 + FACE_EDGE {
        let hat =
            ::image::imageops::crop_imm(decoded, HAT.0, HAT.1, FACE_EDGE, FACE_EDGE).to_image();
        ::image::imageops::overlay(&mut face, &hat, 0, 0);
    }
    Some(::image::imageops::resize(
        &face,
        HEAD_PX,
        HEAD_PX,
        ::image::imageops::FilterType::Nearest,
    ))
}

/// 解 PNG（尺寸上限与背景图那条守门同源：皮肤不该有超大图）。
fn decode_png(bytes: &[u8]) -> Option<::image::RgbaImage> {
    const MAX_EDGE: u32 = 512;
    let reader = ::image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    if width == 0 || height == 0 || width > MAX_EDGE || height > MAX_EDGE {
        return None;
    }
    let reader = ::image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    Some(reader.decode().ok()?.to_rgba8())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::image::ImageEncoder;
    use ::image::{Rgba, RgbaImage};

    fn png_of(image: &RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        let encoder = ::image::codecs::png::PngEncoder::new(&mut bytes);
        encoder
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                ::image::ExtendedColorType::Rgba8,
            )
            .expect("编码 PNG");
        bytes
    }

    /// 造一张 64×64 的皮肤：脸是红色，帽子层是半透明蓝。
    fn skin_bytes() -> Vec<u8> {
        let mut skin = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 0]));
        for x in 8..16 {
            for y in 8..16 {
                skin.put_pixel(x, y, Rgba([200, 10, 10, 255]));
            }
        }
        for x in 40..48 {
            for y in 8..16 {
                skin.put_pixel(x, y, Rgba([10, 10, 200, 120]));
            }
        }
        png_of(&skin)
    }

    /// **帽子层真的叠进头像了**（复审 r1 P3-4：原用例只断言 `head.is_some()`，
    /// 把 overlay 删掉也不会红）。
    ///
    /// 夹具：脸是纯红 (200,10,10,255)，帽子层是半透明蓝 (10,10,200,120)。
    /// 叠上去之后头像像素必须**既不等于纯红、也不等于纯蓝**——那就是混合的结果。
    #[test]
    fn a_real_skin_becomes_a_head_with_the_hat_layer() {
        let avatar = decode(&SkinImage {
            png: skin_bytes(),
            source: "test".to_string(),
        });
        assert!(avatar.head.is_some(), "64×64 皮肤必须裁出头像");
        assert!(avatar.skin.is_some(), "皮肤预览要有原图");

        let decoded = decode_png(&skin_bytes()).expect("解码夹具");
        let head = compose_head(&decoded).expect("裁头");
        let pixel = head.get_pixel(0, 0).0;
        assert_ne!(pixel, [200, 10, 10, 255], "帽子层没叠上去（还是纯红）");
        assert_ne!(
            pixel,
            [10, 10, 200, 120],
            "帽子层把脸整个盖掉了（不是 blend）"
        );
        assert!(pixel[2] > 10, "蓝色分量必须被帽子层抬高：{pixel:?}");
    }

    #[test]
    fn a_broken_payload_falls_back_instead_of_panicking() {
        let avatar = decode(&SkinImage {
            png: b"not a png".to_vec(),
            source: "test".to_string(),
        });
        assert!(avatar.head.is_none(), "解不开就退回首字母块");
        assert!(avatar.skin.is_none());
    }

    #[test]
    fn the_cache_keeps_one_entry_per_account_and_drops_it_without_a_skin() {
        let mut cache = Cache::default();
        let skin = SkinImage {
            png: skin_bytes(),
            source: "a".to_string(),
        };
        let first = cache.resolve("account:a", Some(&skin));
        assert!(first.head.is_some());
        assert_eq!(cache.len(), 1);
        // 同一来源：命中缓存
        let again = cache.resolve("account:a", Some(&skin));
        assert!(again.head.is_some());
        // 没有皮肤：条目被删掉（不留「上次的图」冒充真皮肤）
        let none = cache.resolve("account:a", None);
        assert!(none.head.is_none());
        assert!(cache.is_empty());
    }
}
