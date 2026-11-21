//! `download::font` 的测试夹具（回环 HTTP 服务端 / 假字体字节 / 临时数据根）。
//!
//! 从 `tests.rs` 拆出只为守 400 行硬线（复审 r2 加了两条时限用例之后越线）。这里只有夹具，
//! 断言全在 `tests.rs`——两个回环服务端（`serve` 正常、`serve_trickle` 只爬）是「缓存/校验/时限」
//! 三组守卫的地基，改坏它们等于把守卫变成空转（复审 r2 P2-1 就是这么发现上一版的
//! CRLF 字面量写坏了的）。

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::super::super::verify;
use super::super::{FontSpec, HashAlgo};

/// 每个用例一个独立数据根（共享引擎的登记处是进程级的，键撞了会互相干扰）。
pub(super) fn root(tag: &str) -> PathBuf {
    static SEQ: AtomicUsize = AtomicUsize::new(0);
    let seq = SEQ.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ull-font-{tag}-{}-{seq}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("建数据根");
    dir
}

/// 一段「像字体」的字节：sfnt 头 + 一张表记录 + 载荷。
///
/// 为什么造它而不是塞随机字节：`looks_like_sfnt` 是落盘前的一道闸，测试若拿 HTML 当正常内容，
/// 「下载成功」这条路径就永远走不到。
pub(super) fn fake_font(payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"\x00\x01\x00\x00"); // sfnt 版本
    bytes.extend_from_slice(&1u16.to_be_bytes()); // numTables
    bytes.extend_from_slice(&[0u8; 6]); // searchRange / entrySelector / rangeShift
    bytes.extend_from_slice(b"TEST"); // 表名
    bytes.extend_from_slice(&0u32.to_be_bytes()); // checksum
    bytes.extend_from_slice(&28u32.to_be_bytes()); // offset
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

pub(super) fn sha256(bytes: &[u8]) -> String {
    verify::digest_bytes(HashAlgo::Sha256, bytes)
}

/// 回环 HTTP 服务：不管请求什么，都回同一份字节；记下处理了几条请求。
///
/// 只讲 HTTP/1.1 keep-alive，够用来判「第二次调用有没有再发请求」。
pub(super) fn serve(body: Vec<u8>, expected_requests: usize) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
    let addr = listener.local_addr().expect("取端口");
    let served = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&served);

    std::thread::spawn(move || {
        let mut requests = 0usize;
        while requests < expected_requests {
            let Ok((mut socket, _)) = listener.accept() else {
                break;
            };
            let _ = socket.set_read_timeout(Some(Duration::from_secs(10)));
            let mut seen = Vec::new();
            loop {
                let mut chunk = [0u8; 1024];
                match socket.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => seen.extend_from_slice(&chunk[..read]),
                }
                if !seen.windows(4).any(|window| window == b"\r\n\r\n") {
                    continue;
                }
                requests += 1;
                counter.store(requests, Ordering::SeqCst);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: font/ttf\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                if socket.write_all(head.as_bytes()).is_err() || socket.write_all(&body).is_err() {
                    break;
                }
                seen.clear();
                if requests >= expected_requests {
                    return;
                }
            }
        }
    });

    (format!("http://{addr}/cjk.ttf"), served)
}

/// 一个**不会**有人监听的地址：证明「没联网」比证明「联网成功」更有力。
pub(super) fn dead_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("占一个端口");
    let addr = listener.local_addr().expect("取端口");
    drop(listener);
    format!("http://{addr}/cjk.ttf")
}

/// 爬行服务端的取证计数器：`header` = 真的发过响应头，`dripped` = 真的挤出过几个字节。
///
/// 为什么把这两个数字**带出去**：这类 `serve_*` 夹具坏掉时，用例会静默变成另一种场景
/// （复审 r2 P2-1 就是：请求头永远读不完 ⇒ 一个字节不发 ⇒ 用例证明的是「连接挂着」，而不是
/// 「活着但在爬」）。用例拿到这两个数就能断言「服务端确实在说话」。
pub(super) struct TrickleProof {
    /// 响应头是否真的写出去了。
    pub header: Arc<std::sync::atomic::AtomicBool>,
    /// 写出去了几个正文字节。
    pub dripped: Arc<AtomicUsize>,
}

/// 由 URL 候选 + 期望字节算 pin（哈希取这批字节的 sha256）。
pub(super) fn spec_for(urls: Vec<String>, bytes: &[u8]) -> FontSpec {
    FontSpec::new(urls, sha256(bytes), bytes.len() as u64).expect("pin 应自洽")
}

/// 爬行服务端：回完头之后每 100 ms 只挤 1 字节，**永远不结束**。
///
/// 这就是复审 r1 P2-2 实测到的那条路径（Fastly 的 IPv6 边缘 ≈67 KB/s）：引擎的 `read_timeout`
/// 是 30 s **空闲**超时，而字节一直在动 ⇒ 引擎自己永远不会判它失败。只有我们这一层的整体时限
/// 能打断它——这条服务端存在的唯一目的就是造出「活着但爬」的连接。
///
/// 注意这里的 `\r\n` 必须是**转义写法**：上一版这个函数里写成了真实换行字节（`b"\n\n"`，2 字节）
/// 却拿 4 字节窗口去比，`PartialEq` 先比长度 ⇒ 恒 false ⇒ 请求头永远读不完、一个字节都不发，
/// 用例证明的其实是另一件事（复审 r2 P2-1）。`serve()` 那边的写法是对的，别动它。
pub(super) fn serve_trickle() -> (String, TrickleProof) {
    let header_sent = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let dripped = Arc::new(AtomicUsize::new(0));
    let (header_flag, drip_flag) = (Arc::clone(&header_sent), Arc::clone(&dripped));
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定回环端口");
    let addr = listener.local_addr().expect("取端口");
    std::thread::spawn(move || {
        let Ok((mut socket, _)) = listener.accept() else {
            return;
        };
        let _ = socket.set_read_timeout(Some(Duration::from_secs(5)));
        // 先把请求头读干净（不读完就写响应，某些栈会 RST）。
        let mut seen = Vec::new();
        loop {
            let mut chunk = [0u8; 1024];
            match socket.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(read) => seen.extend_from_slice(&chunk[..read]),
            }
            if seen.windows(4).any(|window| window == b"\r\n\r\n") {
                break;
            }
        }
        // 声明一个够大的长度，然后开始「爬」：每 100 ms 一个字节。
        if socket
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: font/ttf\r\nContent-Length: 1000000\r\n\r\n",
            )
            .is_err()
        {
            return;
        }
        header_flag.store(true, Ordering::SeqCst);
        for _ in 0..600 {
            if socket.write_all(&[0u8]).is_err() {
                return;
            }
            drip_flag.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(100));
        }
    });
    (
        format!("http://{addr}/cjk.ttf"),
        TrickleProof {
            header: header_sent,
            dripped,
        },
    )
}
