//! 更新通道端到端验收（M169）：**临时数据根 + 本地更新源 + 真形状的清单与产物**，
//! 从 CLI 入口走完「发现新版本 → 下载 → 长度/SHA256 双检 → 就位」。
//!
//! 一键跑（**必须带 feature**；tower 要求验收落在 release profile 上，debug 与 release 的
//! `debug_assertions`、优化与 panic 行为都不同，用 debug 验收等于没验真实产物）：
//!
//! ```text
//! cargo test --release -p unlucky-cli --features update-test-source \
//!     --test update_e2e_test --test update_e2e_tamper_test
//! ```
//!
//! （两个测试目标各跑一半：本文件是正路 + 幂等，`update_e2e_tamper_test` 是篡改/截断。）
//!
//! 不带 `--features update-test-source` 时整个目标 `#![cfg]` 掉（**0 个用例**，不是绿）：测试通道
//! 在那种构建里根本不存在，「环境变量设满也走 GitHub」由 `crates/core` 的
//! `the_test_channel_is_a_compile_time_opt_in` 在**不带 feature 的跑法**下守着。
//!
//! 为什么需要 feature：`ULL_UPDATE_DIR` 曾经只在 `debug_assertions` 下生效，于是发布产物（release）
//! **没法**指向本地源，这条链在 release 上从来没有端到端证据。M169 把判据换成编译期 feature
//! （默认关 ⇒ 官方二进制里没有这条路径），验收构建显式打开它。
//!
//! # 现场
//!
//! 见 `tests/support/update_e2e.rs`：数据根 / 本地源 / 安装目录三份临时目录，产物是现场构建出来的
//! `ull.exe` 的**前 1 MiB**（真 PE 头、真字节；整份 ~400 MB，测材太重在 `PAYLOAD_LIMIT` 的注释里
//! 写了原因），清单与 `packaging/tests/manifest.golden.json` 同字段。**`--dest` 永远指向临时副本**
//! ——这个文件里没有一处会替换本进程的 exe。
//!
//! # 抓得住什么 / 抓不住什么
//!
//! | 本套断言 | 抓得住 | 抓不住 |
//! |---|---|---|
//! | 清单 → 选版本 → 下载 → 双检 → 就位整环跑通 | 任一环断了（字段拼错、渠道不符、没下、没校验、没搬）都会红 | 真 GitHub Release 的下载链路（本套一个字节不出网） |
//! | 清单 `sha256` 与盘上字节不符 ⇒ 拒绝 + 目标不动 | 「校验和判据被绕过/写成恒真」；篡改包体或篡改清单都能抓（见 `update_e2e_tamper_test`） | 「CDN 换了内容**而清单里的 sha 也一起换**」——这条路上没有可信根，只能靠 GitHub 侧 |
//! | 清单 `size` 与盘上长度不符 ⇒ 拒绝 | 截断/拼接（长度判据先跑，比摘要便宜） | 同上：清单自己撒谎时没有任何判据能发现 |
//! | 重复 `apply` 后目标字节稳定、暂存无残留、只留一份备份 | 「第二次把 dest 写坏 / 暂存留下垃圾 / 备份无限堆积」 | 「版本号真的前进了」——`current` 取的是**编译期**版本，不读 dest 里的 exe |
//! | 失败后目标仍是旧字节、无 `.ullold-*` | 「校验失败却已经动过目标」 | 跨卷失败路径：本文件只验了跨卷**成功**那一半（见下一行）；跨卷下的中途失败由 `install_io_cases` 注入验 |
//! | 跨卷 `apply`（数据根一卷、安装目录另一卷，M171） | 「rename 不能跨卷 ⇒ 更新必然失败」这条已发布版本上的真 bug；用例自己先证明前提（rename 报 `CrossesDevices`） | 只有一块可写卷的机器上**只有显式豁免才允许跳过**（`ULL_XDEV_TEST_WAIVE=1`，探不到又没豁免 ⇒ 红）；macOS 的跨卷行为一台没跑过 |
//! | 失败路径三例（`update_e2e_tamper_test`：篡改清单 sha / 篡改包体 / 截断） | 「校验判据被绕过」「失败却已动过目标」「失败留下 `.ullupd`」 | **回滚与备份**：摘要/长度失败发生在 `install_staged` **之前**，那一步根本走不到——回滚靠正路的备份断言 + `install_cases` 单测（r1 复审 ⑤ 的变异实测：去掉 dest 备份后失败路径三例仍全绿，只有正路/幂等变红） |
//!
//! # 不验的部分（如实划界）
//!
//! 1. **真实 GitHub Release 下载**：本套一个字节都不出网（源是本地目录）。`releases/latest` 的
//!    真实响应、镜像/gh-proxy 回退、CDN 的 ETag/断点续传都属于真网络验收。
//! 2. **GUI 的点击与重启替换**：设置页「下载并安装」、提示条、`RestartNow` 之后的进程重启不在
//!    自动化里（替换自身 exe 需要真机，且要人工确认新版本能起来）。
//! 3. **按版本号忽略**：`presenter::host::update_state` 的 `skipped` 是会话级行为，有单测
//!    （`update_session_tests.rs`），CLI 侧没有对应开关，所以这里不覆盖。
//! 4. **`--dest` 之外的自替换**（替换本进程 exe）：`tests/support/update_e2e.rs` 的文件头写了为什么
//!    这几条用例永远带 `--dest`，以及「去掉它会真的改名换掉测试二进制」这件事。
//! 5. **macOS / Linux 上的跨卷**：跨卷用例只在**本机 Windows**（C: NVMe + D: 机械盘）上真跑过；
//!    macOS 的卷（APFS 宗卷之间 `/Volumes` 挂载）与 Linux 的 tmpfs↔ext4 一次都没跑——两边的
//!    `EXDEV` 语义相同，但「同目录中转 + 改名」在同一台机器上有没有别的坑，本套没有证据。

#![cfg(feature = "update-test-source")]

#[path = "support/mod.rs"]
mod support;

use support::update_e2e::{channel_lock, Channel, Fixture, INSTALLED};

use unlucky_core::update::Target;

/// 发现新版本 → 下载 → 双检 → 就位：整环从 CLI 入口跑一遍，并核对每一步的产物。
#[test]
fn discovers_downloads_and_installs_from_the_local_channel() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-happy");
    Channel::enable(fx.dist.path());
    fx.publish_good();

    // 1) 发现新版本：源是本地目录，脚本能看出这一份结论来自测试通道。
    let (code, out, err) = fx.run(&["check", "--json"]);
    assert_eq!(code, 0, "check 应成功：{err}");
    let json = support::parse_json(&out);
    let data = support::update_e2e::payload(&json);
    assert_eq!(data["status"], "available", "{json}");
    assert_eq!(data["version"], fx.version.as_str());
    assert_eq!(data["current"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        data["size"].as_u64(),
        Some(fx.payload.len() as u64),
        "清单声明的字节数 = 产物真实字节数：{json}"
    );
    assert_eq!(data["target"], Target::current().as_str());
    assert_eq!(data["notes"], "修掉启动崩溃；实例页加载更快");
    assert_eq!(data["forced"], false);
    assert_eq!(data["test_source"], true, "测试通道必须在输出里点名");
    assert_eq!(
        data["source"],
        format!("file:{}", fx.dist.path().display()),
        "源必须是本地目录（不是 GitHub）"
    );

    // 2) 人读输出也点名测试通道：人看的那份不能比 JSON 少说一句。
    let (code, human, _) = fx.run(&["check"]);
    assert_eq!(code, 0);
    assert!(human.contains("发现新版本"), "{human}");
    assert!(
        human.contains("测试通道生效"),
        "人读输出要点名「这一份结论来自本地夹具」：{human}"
    );

    // 3) 下载 → 长度/SHA256 双检 → 就位。
    let (code, json) = fx.apply();
    assert_eq!(code, 0, "apply 应成功：{json}");
    let data = support::update_e2e::payload(&json);
    assert_eq!(data["status"], "installed", "{json}");
    assert_eq!(data["version"], fx.version.as_str());
    assert_eq!(
        data["bytes"].as_u64(),
        Some(fx.payload.len() as u64),
        "{json}"
    );
    assert_eq!(data["path"], fx.dest.display().to_string().as_str());
    assert_eq!(data["test_source"], true);
    let backup = data["backup"]
        .as_str()
        .expect("旧版本应留备份（回滚用）")
        .to_string();

    // 4) 盘上事实：目标 = 产物字节，备份 = 旧字节，暂存无残留。
    assert_eq!(
        fx.installed_bytes(),
        fx.payload,
        "就位的必须就是清单里那一份字节"
    );
    assert_eq!(
        std::fs::read(&backup).expect("读备份"),
        INSTALLED,
        "旧版本要在备份里，不能凭空消失"
    );
    assert!(
        fx.staging_leftovers().is_empty(),
        "暂存目录里不许留 .ullupd：{:?}",
        fx.staging_leftovers()
    );

    // 5) 装完再查：**仍然报有更新**——因为 current 取的是编译期版本，不读 dest 里的 exe。
    //    这不是 bug（真实场景里替换完就会重启），但脚本作者必须知道这一条。
    let (code, out, _) = fx.run(&["check", "--json"]);
    assert_eq!(code, 0);
    let json = support::parse_json(&out);
    assert_eq!(support::update_e2e::payload(&json)["status"], "available");
}

/// 没有清单的本地渠道 = 「还没有任何发布」，不是错误、也不是「已是最新」以外的胡话。
///
/// 顺带把「源真的被换成本地目录」在没有产物的情况下也验一遍（全程离线）。
#[test]
fn an_empty_local_channel_is_up_to_date_not_an_error() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-empty");
    Channel::enable(fx.dist.path());

    let (code, out, err) = fx.run(&["check", "--json"]);
    assert_eq!(code, 0, "缺清单不该是错误：{err}");
    let json = support::parse_json(&out);
    let data = support::update_e2e::payload(&json);
    assert_eq!(data["status"], "up_to_date", "{json}");
    assert_eq!(data["latest"], serde_json::Value::Null);
    assert_eq!(data["test_source"], true);
}

/// 重复 `apply` 的**终态**与一次相同：目标字节不变、暂存不留垃圾、备份只有一份。
///
/// 如实记两条：`apply` 每次都留一份备份（CLI 从不删，等下一次启动/人工清理），但**只留最近一份**
/// （`install::prune_stale_artifacts`，M171）；第二次之所以还会「装上」，是因为 `current` 是编译期
/// 版本——重复执行不是空操作，只是结果幂等。
#[test]
fn applying_twice_leaves_the_same_target_bytes() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let fx = Fixture::new("e2e-twice");
    Channel::enable(fx.dist.path());
    fx.publish_good();

    for round in 1..=2 {
        let (code, json) = fx.apply();
        assert_eq!(code, 0, "第 {round} 次 apply 应成功：{json}");
        assert_eq!(support::update_e2e::payload(&json)["status"], "installed");
        assert_eq!(
            fx.installed_bytes(),
            fx.payload,
            "第 {round} 次 apply 之后目标字节必须仍是产物"
        );
        assert!(
            fx.staging_leftovers().is_empty(),
            "第 {round} 次 apply 之后暂存目录还有 {:?}",
            fx.staging_leftovers()
        );
    }
    assert_eq!(
        fx.backups().len(),
        1,
        "只留最近一份备份（每次 apply 建一份、成功即清更早的）：{:?}",
        fx.backups()
    );
}

/// 跨卷（M171）：数据根在一卷、安装目录在另一卷时，整环从 CLI 入口仍然跑通。
///
/// 这正是已发布版本上必然失败的那个场景（便携版放 U 盘、数据根在 D 盘而程序在 C 盘）：
/// `stage()` 把包落在 `<数据根>/cache/update/staging`，`install_staged()` 要把包搬进 exe 目录，
/// 而 `rename` 不能跨卷。修法是先落到**目标同目录**的中转名（同卷改名）再顶上去。
///
/// 用例先自己证明**前提成立**（从数据根到安装目录 rename 必须报 `CrossesDevices`），否则
/// 「跨卷成功」这句话就是假的。本机只有一块可写卷时**只有显式豁免才允许跳过**
/// （`ULL_XDEV_TEST_WAIVE=1`，见 `support::cross_volume_waived`）：探不到又没豁免 ⇒ 红。
/// 跨卷逻辑本身另由 `crates/core` 的 `install_xdev_cases`（真跨卷）与 `install_io_cases`（注入）守着。
#[test]
fn installs_across_volumes_from_the_cli() {
    let _serial = channel_lock();
    let _restore = Channel::capture();
    let Some(fx) = Fixture::cross_volume("e2e-xdev") else {
        support::cross_volume_waived("cli-e2e");
        return;
    };
    Channel::enable(fx.dist.path());
    fx.publish_good();

    // 前提：数据根（暂存所在地）与安装目录**真的**不同卷。
    let install_dir = fx.dest.parent().expect("安装目录").to_path_buf();
    let probe = fx.data.path().join("volume-probe.bin");
    std::fs::write(&probe, b"probe").expect("写探针");
    let cross = std::fs::rename(&probe, install_dir.join("volume-probe.bin"))
        .expect_err("跨卷 rename 必须失败");
    assert_eq!(
        cross.kind(),
        std::io::ErrorKind::CrossesDevices,
        "前提不成立：数据根与安装目录同卷（{cross}）"
    );

    let (code, json) = fx.apply();
    assert_eq!(code, 0, "跨卷 apply 应成功：{json}");
    let data = support::update_e2e::payload(&json);
    assert_eq!(data["status"], "installed", "{json}");
    assert_eq!(data["path"], fx.dest.display().to_string().as_str());
    assert_eq!(
        fx.installed_bytes(),
        fx.payload,
        "装上去的必须是清单里那一份完整字节（不是半截）"
    );
    assert!(
        fx.staging_leftovers().is_empty(),
        "暂存目录里不许留 .ullupd：{:?}",
        fx.staging_leftovers()
    );
    let backups = fx.backups();
    assert_eq!(backups.len(), 1, "旧版本要留一份备份：{backups:?}");
    let backup = install_dir.join(&backups[0]);
    assert_eq!(
        std::fs::read(&backup).expect("读备份"),
        INSTALLED,
        "旧字节必须完整留在备份里"
    );
}
