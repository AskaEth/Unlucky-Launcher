//! 极简 PNG 编码器（只做「无压缩 stored deflate」），给截图验收用。
//!
//! 为什么不引 `image` / `png` crate：截屏只是开发期验收工具（`--screenshot`），
//! 为它加一个依赖违反「新依赖过 deny.toml 白名单」的成本收益。PNG 规范里 zlib 流允许
//! **stored（未压缩）块**，于是我们只需要 CRC32 + Adler32 两个校验和就能产出合法 PNG——
//! 文件大一些，但任何看图工具都认。
//!
//! 输出：8bit RGBA（color type 6），逐行 filter=0（None）。

use slint::{Rgba8Pixel, SharedPixelBuffer};

/// 把 RGBA8 像素缓冲写成 PNG 字节流。
#[must_use]
pub fn encode_rgba8(buffer: &SharedPixelBuffer<Rgba8Pixel>) -> Vec<u8> {
    let width = buffer.width();
    let height = buffer.height();
    let pixels = buffer.as_slice();

    // 原始扫描线：每行前面加一个 filter 字节 0
    let stride = width as usize * 4;
    let mut raw = Vec::with_capacity((stride + 1) * height as usize);
    for y in 0..height as usize {
        raw.push(0u8);
        for x in 0..width as usize {
            let pixel = pixels[y * width as usize + x];
            raw.extend_from_slice(&[pixel.r, pixel.g, pixel.b, pixel.a]);
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 位深 8、RGBA、deflate、无交错
    push_chunk(&mut out, b"IHDR", &ihdr);
    push_chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    push_chunk(&mut out, b"IEND", &[]);
    out
}

fn push_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// zlib 容器 + 全 stored deflate 块（每块上限 65535 字节）。
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 65_535 * 5 + 16);
    out.extend_from_slice(&[0x78, 0x01]); // CMF/FLG：deflate、32K 窗口、无字典、最快
    let mut offset = 0usize;
    while offset < raw.len() {
        let len = (raw.len() - offset).min(65_535);
        let last = offset + len >= raw.len();
        out.push(u8::from(last)); // BFINAL + BTYPE=00
        let len_u16 = len as u16;
        out.extend_from_slice(&len_u16.to_le_bytes());
        out.extend_from_slice(&(!len_u16).to_le_bytes());
        out.extend_from_slice(&raw[offset..offset + len]);
        offset += len;
    }
    if raw.is_empty() {
        out.push(1);
        out.extend_from_slice(&[0, 0, 0xff, 0xff]);
    }
    out.extend_from_slice(&adler32(raw).to_be_bytes());
    out
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(r: u8, g: u8, b: u8) -> Rgba8Pixel {
        Rgba8Pixel { r, g, b, a: 255 }
    }

    #[test]
    fn header_and_chunks_are_well_formed() {
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(3, 2);
        buffer.make_mut_slice().fill(pixel(1, 2, 3));
        let bytes = encode_rgba8(&buffer);
        assert_eq!(
            &bytes[..8],
            &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]
        );
        // IHDR：宽 3 高 2、8bit、RGBA(6)
        assert_eq!(&bytes[8..12], &13u32.to_be_bytes());
        assert_eq!(&bytes[12..16], b"IHDR");
        assert_eq!(&bytes[16..20], &3u32.to_be_bytes());
        assert_eq!(&bytes[20..24], &2u32.to_be_bytes());
        assert_eq!(bytes[24], 8);
        assert_eq!(bytes[25], 6);
        // 结尾是 IEND 块
        assert_eq!(&bytes[bytes.len() - 8..bytes.len() - 4], b"IEND");
    }

    #[test]
    fn checksums_match_known_vectors() {
        // CRC32("IEND") 是公认值 0xAE426082（写进 IEND 块尾）
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        // Adler32("Wikipedia") = 0x11E60398
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
    }

    #[test]
    fn empty_buffer_still_produces_valid_stream() {
        let buffer = SharedPixelBuffer::<Rgba8Pixel>::new(0, 0);
        let bytes = encode_rgba8(&buffer);
        assert!(bytes.windows(4).any(|window| window == b"IDAT"));
    }
}
