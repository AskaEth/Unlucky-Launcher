//! 测试支撑：临时目录、摘要工具、进程内极简 HTTP 服务器（零额外依赖）。
//!
//! 为什么自己写 mock server 而不用 wiremock：P0 阶段控制依赖面（Win7 Tier-3 构建 +
//! 单文件体积），一个 `TcpListener` + 脚本队列足够表达「成功 / 500 / 校验不符 / 无视 Range」。
//! 服务器本体拆在 [`server`] 里（守 400 行硬线），这里只做再导出，调用方路径不变。

#![allow(dead_code)]

use std::sync::atomic::{AtomicUsize, Ordering};

mod response;
mod server;

#[allow(unused_imports)]
pub use self::response::MockResponse;
#[allow(unused_imports)]
pub use self::server::MockServer;

/// 断言用的小工具：唯一临时目录（进程内计数 + 时间戳，避免并行测试互相踩）。
pub fn temp_dir(tag: &str) -> std::path::PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let dir = std::env::temp_dir().join(format!("ull-download-test-{tag}-{stamp}-{unique}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("创建临时目录失败");
    dir
}

/// 算一段字节的 SHA-1（测试断言用）。
pub fn sha1_hex(bytes: &[u8]) -> String {
    use sha1::Digest;
    let digest = sha1::Sha1::digest(bytes);
    let mut out = String::with_capacity(40);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// 目录里（不含子目录）所有中间文件家族的名字（`.ullpart` 与 `.ullmeta.json`）。
pub fn part_files(dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(unlucky_core::download::PART_SUFFIX))
        .collect();
    names.sort();
    names
}

/// 把文件的修改时间挪到「`age` 之前」（清理/收养的年龄判定用）。
pub fn age_file(path: &std::path::Path, age: std::time::Duration) {
    let times = std::fs::FileTimes::new().set_modified(std::time::SystemTime::now() - age);
    std::fs::File::options()
        .write(true)
        .open(path)
        .expect("打开待改 mtime 的文件失败")
        .set_times(times)
        .expect("改 mtime 失败");
}
