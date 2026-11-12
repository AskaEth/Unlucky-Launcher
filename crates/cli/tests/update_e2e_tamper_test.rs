//! 更新通道端到端验收（M169）的**拒绝侧**：篡改摘要、篡改包体、截断包体。
//!
//! 正路与幂等在 `update_e2e_test.rs`（现场说明、抓得住/抓不住、不验的部分都在那边，别重复读）。
//! 这里只钉一条底线：**校验不过就一个字节都不许落到目标上**，而且失败不会把现场搞脏到
//! 「修好清单也装不上」。
//!
//! ⚠ 这三例**抓不住回滚/备份**：摘要与长度失败都发生在 `install_staged` 之前，那一步走不到
//! （r1 复审 ⑤ 的变异实测：去掉 dest 备份后这三例仍全绿）。回滚由 `update_e2e_test.rs` 的备份
//! 断言与 `core::update::tests::install_cases` 守。
//!
//! 一键跑（必须带 feature；tower 定案：验收落在 release profile 上）：
//!
//! ```text
//! cargo test --release -p unlucky-cli --features update-test-source --test update_e2e_tamper_test
//! ```
//!
//! 不带 feature 时整个目标 `#![cfg]` 掉（0 个用例）——测试通道在那种构建里不存在。

#![cfg(feature = "update-test-source")]

#[path = "support/mod.rs"]
mod support;

use support::update_e2e::{channel_lock, sha256, Channel, Fixture, INSTALLED};

/// 清单的 `sha256` 与盘上字节不符 ⇒ 拒绝，且**现有安装一个字节都不动**。
///
/// 抓得住：校验和判据被绕过（写成恒真、先搬后校、只比长度）。抓不住：清单与包体一起被换
/// （同时改 sha），那条路上没有可信根。
#[test]
fn a_tampered_digest_is_refused_and_leaves_the_install_intact() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-digest");
    Channel::enable(fx.dist.path());
    // 长度对、摘要错：先过长度判据，才能测到摘要这一关。
    fx.publish(&sha256(b"another build entirely"), fx.payload.len() as u64);

    let (code, json) = fx.apply();
    assert_ne!(code, 0, "摘要不符必须拒绝（实际退出码 0）：{json}");
    assert_eq!(json["error"]["code"], "update_apply_failed", "{json}");
    assert_eq!(json["error"]["detail"]["kind"], "digest_mismatch", "{json}");

    assert_eq!(fx.installed_bytes(), INSTALLED, "校验失败不许动目标文件");
    assert!(
        fx.backups().is_empty(),
        "还没走到替换就失败了，安装目录里不该出现 .ullold：{:?}",
        fx.backups()
    );
    assert!(
        fx.staging().exists(),
        "下载过才会建暂存目录（说明失败点在双检，不是根本没下）"
    );
    assert!(
        fx.staging_leftovers().is_empty(),
        "校验失败的 .ullupd 必须被删掉：{:?}",
        fx.staging_leftovers()
    );

    // 修好清单再装一次：失败没把现场搞脏，这条路还能走通。
    fx.publish_good();
    let (code, json) = fx.apply();
    assert_eq!(code, 0, "修好清单之后应能装上：{json}");
    let data = support::update_e2e::payload(&json);
    assert_eq!(data["status"], "installed", "{json}");
    assert_eq!(fx.installed_bytes(), fx.payload);
}

/// 产物被换（清单没动）⇒ 一样拒绝：篡改落在哪一侧都能抓到。
#[test]
fn a_tampered_artifact_is_refused_too() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-artifact");
    Channel::enable(fx.dist.path());
    fx.publish_good();
    // 盘上换成同长度的另一份字节：只有摘要能抓（长度判据看不出区别）。
    let mut evil = fx.payload.clone();
    evil[0] ^= 0xff;
    std::fs::write(fx.dist.path().join(&fx.asset), &evil).expect("篡改产物");

    let (code, json) = fx.apply();
    assert_ne!(code, 0, "篡改的包必须拒绝：{json}");
    assert_eq!(
        json["error"]["detail"]["kind"], "digest_mismatch",
        "篡改产物应当报摘要不符：{json}"
    );
    assert_eq!(fx.installed_bytes(), INSTALLED);
    assert!(fx.staging_leftovers().is_empty());
}

/// 包体被截断（清单长度按完整包算）⇒ 长度判据先拒：不读摘要、不碰目标。
#[test]
fn a_truncated_payload_is_refused_as_size_mismatch() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-truncated");
    Channel::enable(fx.dist.path());
    fx.publish_good();
    let full = fx.payload.clone();
    std::fs::write(fx.dist.path().join(&fx.asset), &full[..full.len() / 2]).expect("截断产物");

    let (code, json) = fx.apply();
    assert_ne!(code, 0, "截断的包必须拒绝：{json}");
    assert_eq!(json["error"]["detail"]["kind"], "size_mismatch", "{json}");
    assert_eq!(fx.installed_bytes(), INSTALLED);
}
