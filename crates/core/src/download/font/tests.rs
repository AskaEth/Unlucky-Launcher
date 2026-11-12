//! `download::font` 的守卫（三条都能变红，变异证明见 M168 报告）。
//!
//! ## 离线纪律
//!
//! 服务端是 `127.0.0.1` 上的回环监听器，把**内存里的假字体字节**当响应体发出去：不碰任何
//! 外网站点，跑 `cargo test` 不需要网络。真 CDN 的字节有没有变、17 MiB 要下多久，这些**不在**
//! 本文件的能力范围内（报告里写明）。

use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::*;

#[path = "tests/support.rs"]
mod support;

use support::{dead_url, fake_font, root, serve, serve_trickle, sha256, spec_for};

/// 守卫 ③：**缓存命中不联网**。
///
/// 抓得住：把 `fetch_blocking` 开头的缓存分支删掉——URL 指向的死人端口会被真的去连，本用例红。
/// 抓不住：真机上 17 MiB 文件读盘有多快。
#[test]
fn cache_hit_never_touches_the_network() {
    let dir = root("cache");
    let body = fake_font(b"cached payload");
    let spec = spec_for(vec![dead_url()], &body);
    let path = cache_path(&dir, &spec);
    std::fs::create_dir_all(path.parent().unwrap()).expect("建字体目录");
    std::fs::write(&path, &body).expect("预置缓存");

    let fetched = fetch_blocking(&dir, &spec).expect("缓存命中应成功");
    assert!(fetched.from_cache, "必须报「来自缓存」");
    assert_eq!(fetched.bytes, body);
    assert_eq!(fetched.source, "cache");
    std::fs::remove_dir_all(&dir).ok();
}

/// 守卫 ②：**sha256 不符时拒绝落盘**（正式路径上什么都不许留下）。
///
/// **两层校验是互备的**（复审 r3 顺手项①：上一版这里写「只拆一层也会红」是假的，复审变异实测
/// 单独拆掉引擎的 `with_sha256` 或单独短路本模块复验，本用例都**仍然绿**——另一层会接住）。
/// 本用例真正抓的是**两层同时被拆**：那时坏字节会被当成字体返回、`cache_path` 也会落盘，断言在
/// `expect_err` 处红（r1 的变异证明就是这么打的）。
///
/// 抓不住：单层的实现细节（比如 `verify::matches` 的大小写处理）——那要各自的单元用例。
#[test]
fn sha256_mismatch_is_refused_and_nothing_lands() {
    let dir = root("mismatch");
    let served = fake_font(b"what the cdn claims");
    let expected = fake_font(b"what the pin expects");
    let (url, _served) = serve(served.clone(), 4);
    let spec = FontSpec::new(vec![url], sha256(&expected), served.len() as u64).expect("pin");

    let error = fetch_blocking(&dir, &spec).expect_err("哈希不符必须失败");
    assert_eq!(error.class(), "integrity", "{error}");
    let path = cache_path(&dir, &spec);
    assert!(
        !path.exists(),
        "校验不过的字体不许出现在 {}",
        path.display()
    );
    // 字体目录里什么都不该留下：复验在写盘之前，所以连中间态都不存在。
    let leftovers = std::fs::read_dir(path.parent().unwrap())
        .map_or(0, |entries| entries.filter_map(Result::ok).count());
    assert_eq!(leftovers, 0, "校验不过时字体目录必须干净");
    std::fs::remove_dir_all(&dir).ok();
}

/// 守卫 ③ 的另一半：**下过一次就再也不下**（同进程第二次调用零请求）。
///
/// 抓得住：缓存写失败（换名没做）/ 缓存文件名不带哈希（pin 换了还命中旧文件）/ 每次调用都
/// 重新下载——第一条与第三条都会让 `served == 2`，本用例红。
#[test]
fn the_second_call_is_served_from_the_cache() {
    let dir = root("second");
    let body = fake_font(b"payload of the second call");
    // 服务端最多伺候 4 条请求：若第二次调用又联网，计数会变成 2。
    let (url, served) = serve(body.clone(), 4);
    let spec = spec_for(vec![url], &body);

    let first = fetch_blocking(&dir, &spec).expect("第一次应成功");
    assert!(
        !first.from_cache,
        "第一次必须是真下载（否则这条用例什么也没证明"
    );
    assert_eq!(first.bytes, body);

    let second = fetch_blocking(&dir, &spec).expect("第二次应命中缓存");
    assert!(second.from_cache, "第二次不许再联网");
    assert_eq!(
        served.load(Ordering::SeqCst),
        1,
        "整个过程只该有一条 HTTP 请求"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 坏缓存**自愈**：哈希不符的旧文件要被删掉重下，而不是被当成命中，也不是永远挡路。
#[test]
fn a_corrupt_cache_file_is_healed_by_a_fresh_download() {
    let dir = root("heal");
    let body = fake_font(b"fresh bytes");
    let spec = spec_for(vec![dead_url()], &body);
    let path = cache_path(&dir, &spec);
    std::fs::create_dir_all(path.parent().unwrap()).expect("建字体目录");
    std::fs::write(&path, fake_font(b"rotten bytes")).expect("预置坏缓存");

    // 坏缓存 + 死人 URL ⇒ 只能报网络错（若把坏文件当命中会返回 Ok，本用例红）。
    let error = fetch_blocking(&dir, &spec).expect_err("坏缓存不该被当成命中");
    assert_eq!(error.class(), "network", "{error}");
    assert!(!path.exists(), "坏缓存必须被删掉");

    // 换成真服务端：同一次进程里重来一次就能落地（自愈）。
    let (url, _served) = serve(body.clone(), 4);
    let spec = spec_for(vec![url], &body);
    let fetched = fetch_blocking(&dir, &spec).expect("自愈后应成功");
    assert!(!fetched.from_cache);
    assert!(path.exists());
    std::fs::remove_dir_all(&dir).ok();
}

/// CDN 回了一个 HTML 错误页（哈希还对不上/对得上都一样）：不许当字体落盘。
///
/// 抓得住：`looks_like_sfnt` 这道闸被删掉——哈希正确时它会让一堆 HTML 进字体系统。
/// 抓不住：真字体里子集不全这类内容问题（那要 cmap 检查）。
#[test]
fn an_html_error_page_is_not_a_font() {
    let dir = root("html");
    let body = b"<html><body>403 Forbidden</body></html>".to_vec();
    let (url, _served) = serve(body.clone(), 4);
    let spec = spec_for(vec![url], &body);

    let error = fetch_blocking(&dir, &spec).expect_err("HTML 不是字体");
    assert_eq!(error.class(), "integrity", "{error}");
    assert!(!cache_path(&dir, &spec).exists());
    std::fs::remove_dir_all(&dir).ok();
}

/// 守卫（M168 复审 r2 P2-1/P2-3）：**单次尝试的时限会打断爬行连接并转下一个候选**。
///
/// 这个场景现在是真的：第一个候选是 `serve_trickle()`（发真实 CRLF 响应头，之后每 100 ms 一个
/// 字节），第二个候选是正常服务端。
///
/// 抓得住：①`serve_trickle` 的响应头写错（上一版就是这个病：真实换行字节 + 4 字节窗口 ⇒ 恒 false
/// ⇒ 一个字节都不发）——那样第一个候选会在**读不到响应头**时按网络错失败，用例里
/// `trickle_served` 的「服务端真的发过头」这条断言会红；②`budget` 不是 `min(单次, 剩余)` 而是
/// 直接用总预算 ⇒ 本用例会等到总预算（5 s）才换源，`elapsed < 3 s` 红；③时限被归成校验失败。
/// 抓不住：默认值（120 s / 360 s）在真弱网下够不够（那要真 CDN，见报告「未验」）。
#[test]
fn a_trickling_candidate_is_cut_by_the_attempt_deadline() {
    let dir = root("deadline-attempt");
    let good = fake_font(b"served by the second candidate");
    let (good_url, _served) = serve(good.clone(), 4);
    let (trickle_url, proof) = serve_trickle();
    let spec = FontSpec::new(
        vec![trickle_url, good_url],
        sha256(&good),
        good.len() as u64,
    )
    .expect("pin");

    let started = std::time::Instant::now();
    let fetched = fetch_blocking_with_limits(
        &dir,
        &spec,
        Duration::from_millis(300),
        Duration::from_secs(5),
    )
    .expect("第二个候选应当成功");
    assert_eq!(fetched.bytes, good);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "单次上限该在 300 ms 处切断第一个候选，实际 {:?}",
        started.elapsed()
    );
    assert!(cache_path(&dir, &spec).exists());
    // **这条是「夹具没坏」的判据**（复审 r2 P2-1）：第一个候选必须真的发过响应头、真的挤过字节。
    // 少了这两条，一个「什么都不发的服务端」也能让本用例绿——那证明的是「连接挂着被掐断」，
    // 而不是我们想证明的「活着但在爬」。
    assert!(
        proof.header.load(Ordering::SeqCst),
        "爬行服务端必须真的发出响应头"
    );
    assert!(
        proof.dripped.load(Ordering::SeqCst) >= 1,
        "爬行服务端必须真的挤出过字节（否则引擎的空闲超时那条路径根本没被造出来）"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// 守卫（M168 复审 r3 P2-3）：**最后一个候选拿整个剩余预算**（`is_last` 那一支）。
///
/// 形状是刻意挑的：**单候选** + `attempt = 300 ms` / `total = 5 s`。
/// * 有 `is_last`：最后一个（也是唯一一个）候选拿到剩下的 ~5 s ⇒ 只在**总时限**处被打断，
///   耗时 ≈5 s；
/// * 没有 `is_last`（一律 `attempt.min(remaining)`）：300 ms 就切，耗时 ≈0.3 s。
///
/// 两种实现的解不同，于是这条用例能分辨它们——上一版给的是 `attempt=30 s > total=400 ms`，
/// 两种实现同解，天生测不出差别（复审 r3 就是据此说 `is_last` 没人守）。
///
/// 抓得住：删掉 `is_last` 那一支（或把预算写成 `attempt_deadline.min(remaining)` 一视同仁）⇒
/// `elapsed >= 3 s` 当场红；也抓得住服务端其实不发字节（`proof` 的两个断言）。
/// 抓不住：真弱网下 60 s / 420 s 够不够（要真 CDN，见报告「未验」）。
#[test]
fn the_last_candidate_gets_the_whole_remaining_budget() {
    let dir = root("deadline-last");
    let body = fake_font(b"not what the server sends");
    let (trickle_url, proof) = serve_trickle();
    let spec = FontSpec::new(vec![trickle_url], sha256(&body), body.len() as u64).expect("pin");

    let started = std::time::Instant::now();
    let error = fetch_blocking_with_limits(
        &dir,
        &spec,
        Duration::from_millis(300),
        Duration::from_secs(5),
    )
    .expect_err("爬行连接最终必须被总时限打断");
    let elapsed = started.elapsed();

    assert_eq!(error.class(), "network", "超时属于网络档：{error}");
    assert!(
        error.message().contains("超时"),
        "文案要说得清是超时：{error}"
    );
    assert!(
        elapsed >= Duration::from_secs(3),
        "最后一个候选该拿整个剩余预算（≈5 s），而不是在 300 ms 处就被切：{elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(9),
        "不该超过总时限太多：{elapsed:?}"
    );
    assert!(!cache_path(&dir, &spec).exists(), "超时不许落盘");
    assert!(
        proof.dripped.load(Ordering::SeqCst) >= 1,
        "这条用例必须掐在「有字节在动」的连接上，而不是一个不响的端口"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// pin 的 URL 口径：**https 一律可以，http 只给回环**（M168 复审次要项）。
///
/// 抓得住：把 `http://` 一律放行（注释说「必须 https」、实现说「随便」的老毛病），
/// 也抓得住收紧过头（连回环的单测服务都不让用 ⇒ 一批用例会红）。
#[test]
fn the_pin_only_accepts_https_or_loopback_http() {
    let hash = "a".repeat(64);
    assert!(FontSpec::new(vec!["https://mirror.example/a.ttf"], &hash, 1).is_ok());
    assert!(FontSpec::new(vec!["http://127.0.0.1:8080/a.ttf"], &hash, 1).is_ok());
    assert!(FontSpec::new(vec!["http://localhost:9/a.ttf"], &hash, 1).is_ok());
    assert!(
        FontSpec::new(vec!["http://127.0.0.2:9/a.ttf"], &hash, 1).is_ok(),
        "127.0.0.0/8 整个都是回环"
    );
    assert!(
        FontSpec::new(vec!["http://example.com/a.ttf"], &hash, 1).is_err(),
        "非回环的明文 http 不许过"
    );
    assert!(FontSpec::new(vec!["ftp://example.com/a.ttf"], &hash, 1).is_err());
}

/// pin 自身不合法时**一个请求都不发**（免得为一个坏 pin 白跑一趟网络）。
#[test]
fn an_invalid_pin_never_reaches_the_network() {
    let dir = root("pin");
    let served = fake_font(b"x");
    let (url, counter) = serve(served.clone(), 4);

    // 哈希形态不对
    assert!(FontSpec::new(vec![url.clone()], "not-a-hash", 10).is_err());
    // 非 http(s)
    assert!(FontSpec::new(vec!["file:///etc/passwd"], sha256(&served), 10).is_err());
    // 没有 URL
    assert!(FontSpec::new(Vec::<String>::new(), sha256(&served), 10).is_err());
    // 超过大小上限
    assert!(FontSpec::new(vec![url], sha256(&served), MAX_FONT_BYTES + 1).is_err());
    // 零字节
    assert!(FontSpec::new(vec!["https://example.invalid/f.ttf"], sha256(&served), 0).is_err());

    assert_eq!(counter.load(Ordering::SeqCst), 0, "坏 pin 不许发请求");
    std::fs::remove_dir_all(&dir).ok();
}

/// `looks_like_sfnt`：只认魔数 + 表目录，不猜内容。
#[test]
fn sfnt_sniffing_accepts_fonts_and_rejects_junk() {
    assert!(looks_like_sfnt(&fake_font(b"x")));
    // OTF（CFF 轮廓）：魔数 OTTO + 一张表
    let mut otf = b"OTTO".to_vec();
    otf.extend_from_slice(&1u16.to_be_bytes());
    otf.extend_from_slice(&[0u8; 6]);
    otf.extend_from_slice(&[0u8; 16]);
    assert!(looks_like_sfnt(&otf));
    // TTC（字体集合）：长度够就收
    assert!(looks_like_sfnt(
        b"ttcf\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
    ));
    assert!(!looks_like_sfnt(b"<html>"));
    assert!(!looks_like_sfnt(b"\x00\x01\x00\x00\x00\x00")); // 表数 0
    assert!(!looks_like_sfnt(b"\x00\x01\x00\x00\x00\x02")); // 长度不够装两张表
    assert!(!looks_like_sfnt(&[]));
}

/// 缓存文件名带 pin 的哈希段：换 pin（换字体/换子集）⇒ 换文件名 ⇒ 不会命中旧内容。
#[test]
fn the_cache_name_changes_with_the_pin() {
    let dir: &Path = std::path::Path::new("/tmp/ull-font-name");
    let a = FontSpec::new(vec!["https://x.invalid/a.ttf"], "a".repeat(64), 1).expect("pin a");
    let b = FontSpec::new(vec!["https://x.invalid/a.ttf"], "b".repeat(64), 1).expect("pin b");
    assert_ne!(cache_path(dir, &a), cache_path(dir, &b));
    assert_eq!(
        cache_path(dir, &a),
        dir.join("fonts")
            .join(format!("cjk-noto-sans-sc-{}.ttf", "a".repeat(16)))
    );
}
