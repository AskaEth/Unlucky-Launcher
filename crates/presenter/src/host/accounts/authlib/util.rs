//! authlib 轨的 HTTP 基座与纯函数小工具：`clientToken` 生成、GET 文本/字节、base64 解码、按已知值脱敏。
//!
//! 从 `authlib.rs` 拆出来守 400 行硬线。网络只走**既有下载引擎**的客户端
//! （`host::engine::engine(env).client()`）：代理/超时/证书栈与下载器同源，本 crate 不新增依赖。

use unlucky_core::auth::microsoft::now_ms;
use unlucky_core::download::blocking::block_on;

use crate::dto::REDACTED;

use super::super::super::engine;
use super::super::super::env::HostEnv;

/// 一次 GET 的超时上限（秒）：服务器不可达时要能失败，而不是挂住 UI 的登录面板。
pub(super) const HTTP_TIMEOUT_SECS: u64 = 15;

/// 单次 GET 的响应体上限（字节）：用户能填任意 URL，服务发现 JSON/皮肤 PNG 都远小于它。
pub(super) const MAX_BODY_BYTES: usize = 512 * 1024;
/// base64 解码输出上限（textures 属性是个小 JSON）。
pub(super) const MAX_BASE64_BYTES: usize = 64 * 1024;

/// 启动器侧生成的 `clientToken`（Yggdrasil 要求刷新/校验都带，且服务端回传必须一致）。
pub(super) fn client_token() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut out = String::with_capacity(32);
    for salt in 0..2u64 {
        // `RandomState` 每个实例自带随机种子（标准库给的是进程级随机），不引第三方随机库。
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u64(now_ms() ^ salt);
        hasher.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", hasher.finish()));
    }
    out
}

/// GET 文本（状态码 + 正文）。
///
/// 客户端来自**既有的下载引擎**（`host::engine::engine`）：代理/超时/证书栈与本仓的下载器
/// 完全同源，不在这里另起一套 HTTP 配置（本 crate 也没有 reqwest 依赖，拿到的只是它的 `&Client`）。
pub(super) fn get_text(env: &HostEnv, url: &str) -> Result<(u16, String), String> {
    let engine = engine::engine(env);
    let client = engine.client();
    let url = url.to_string();
    block_on(async move {
        let reply = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|error| format!("服务器不可达：{error}"))?;
        let status = reply.status().as_u16();
        let text = reply
            .text()
            .await
            .map_err(|error| format!("读取响应失败：{error}"))?;
        Ok((status, text))
    })
}

/// GET 字节（皮肤图片）。
pub(super) fn get_bytes(env: &HostEnv, url: &str) -> Result<(u16, Vec<u8>), String> {
    let engine = engine::engine(env);
    let client = engine.client();
    let url = url.to_string();
    block_on(async move {
        let reply = client
            .get(&url)
            .timeout(std::time::Duration::from_secs(HTTP_TIMEOUT_SECS))
            .send()
            .await
            .map_err(|error| format!("皮肤图片不可达：{error}"))?;
        let status = reply.status().as_u16();
        let bytes = reply
            .bytes()
            .await
            .map_err(|error| format!("读取皮肤图片失败：{error}"))?
            .to_vec();
        if bytes.len() > MAX_BODY_BYTES {
            return Err(format!(
                "图片过大（{} 字节，上限 {MAX_BODY_BYTES}）",
                bytes.len()
            ));
        }
        Ok((status, bytes))
    })
}

/// base64 解码（标准字母表，容忍 URL-safe 变体与缺失填充）。
///
/// 为什么自己写：textures 属性是 base64 编码的 JSON，而本 crate 不能引第三方解码库
/// （依赖最小化）；这段是 40 行的确定性算法，配单测比拖一个依赖更划算。
pub(super) fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer: u32 = 0;
    let mut bits = 0u32;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            other => return Err(format!("base64 里有非法字符：{:?}", other as char)),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).unwrap_or(0));
        }
        if out.len() > MAX_BASE64_BYTES {
            return Err(format!("base64 输出超过上限 {MAX_BASE64_BYTES} 字节"));
        }
    }
    if out.is_empty() {
        return Err("base64 内容为空".to_string());
    }
    Ok(out)
}

/// 错误文案里可能混进已知凭据值：按值再兜一遍（形状脱敏由 `AuthlibLoginView::failed` 做）。
#[must_use]
pub fn scrub_known(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        return message.to_string();
    }
    message.replace(secret, REDACTED)
}
