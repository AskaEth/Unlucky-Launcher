//! M334 **真失败路径**与**安装编排唯一**的判据（从 `m334_install_tests.rs` 拆出：那边贴着
//! 400 行硬线）。
//!
//! | 用例 | 抓得住什么 | 怎么做到不联网 |
//! |---|---|---|
//! | [`the_real_step_records_one_step_per_vendor_without_network`] | `real_step` 只调一次 core（编排在 core 里）、失败时链上逐家可读 | Java 7 比四家下限都低 ⇒ core 的 `supports` 全判否，一次请求都不发 |
//! | [`the_real_failure_reaches_the_ui_with_steps_and_no_prose`] | 终态快照带逐家的链、`message` 一个字没有 | 同上 |
//! | [`the_install_orchestration_lives_in_core_only`] | presenter 里又长出一条回退链（`with_vendor` / `order(..)` / `supports(..)`）⇒ 红 | 不跑任何东西（扫源码） |
//!
//! **抓不住**：真正的网络往返（离线不可构造）⇒「哪家真命中 / 网络类原因长什么样」没有实证。

use std::time::Duration;

use crate::dto::JavaChainOutcome;

/// 一次性数据根（用完删）。
struct TempRoot(std::path::PathBuf);

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn lonely_env(label: &str) -> (crate::host::env::HostEnv, TempRoot) {
    let dir = std::env::temp_dir().join(format!(
        "ull-m334-real-{label}-{}-{}",
        std::process::id(),
        line!()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let env = crate::host::env::HostEnv::at_root(&dir);
    std::fs::create_dir_all(env.layout.config()).expect("config 目录");
    (env, TempRoot(dir))
}

/// **走真失败路径**（`real_step` 本身，没有注入任何假实现）：Java 7 比所有发行版的下限都低
/// ⇒ core 的 `supports` 把四家全判否 ⇒ 链上每家一跳、**一条网络请求都不打**。
///
/// 这条同时证明「编排只有 core 一处」：`real_step` 只调了一次 `java::install`（没有 pin 厂商），
/// 拿回来的却是**每一家各自的**原因（core 把 `ChainLog` 挂在错误上）。
///
/// 抓得住：presenter 又自己按厂商逐家调（编排重复）⇒ 链的形状就由 presenter 决定，这条对
/// `real_step` 的「一次调用」断言与 `the_install_orchestration_lives_in_core_only` 一起红。
#[test]
fn the_real_step_records_one_step_per_vendor_without_network() {
    let (env, _root) = lonely_env("chain");
    let chain = super::install::real_step(&env, 7);
    assert!(
        chain.len() >= 2,
        "core 的回退链至少有 3~4 家 ⇒ 链上该有几跳，实际：{chain:?}"
    );
    assert!(
        chain
            .iter()
            .all(|step| step.outcome == JavaChainOutcome::Skip),
        "Java 7 比四家的下限都低 ⇒ 每一家都该是「跳过」，实际：{chain:?}"
    );
    assert!(
        chain.iter().all(|step| !step.vendor.is_empty()),
        "跳过的那几跳必须指名是哪一家（界面按目录取名）：{chain:?}"
    );
    assert!(
        chain
            .iter()
            .all(|step| step.detail.contains("不支持 Java 7")),
        "每一跳的 detail 必须是 **core 自己记的那一句**（不是 presenter 拼的）：{chain:?}"
    );
    assert!(
        !chain
            .iter()
            .any(|step| step.outcome == JavaChainOutcome::Hit),
        "没人发还命中了？{chain:?}"
    );
}

/// 真失败**穿过产品那个函数**（`spawn_install_with` + `real_step`）之后，落到界面上的两份
/// 快照长什么样：`busy = true` 那份 + 终态那份；终态带着**逐家的链**、`message` 一个字没有。
///
/// 抓得住：`message` 里塞中文（英文界面同屏两语）、或失败时链是空的（名不了发行版）。
#[test]
fn the_real_failure_reaches_the_ui_with_steps_and_no_prose() {
    let (env, _root) = lonely_env("page");
    let (tx, rx) = crate::host::job::channel();
    super::install::spawn_install_with(env, tx, 7, super::install::real_step);

    let mut pages = Vec::new();
    while pages.len() < 2 {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(crate::host::job::HostTask::Java(page)) => pages.push(*page),
            Ok(other) => panic!("只该收到 Java 页快照，实际 {other:?}"),
            Err(error) => panic!("10 s 内没收齐两份快照（收到 {} 份）：{error}", pages.len()),
        }
    }
    assert!(pages[0].busy, "第一份必须是「正在装」");
    let done = &pages[1];
    assert!(!done.busy, "第二份必须是终态");
    assert!(
        done.message.is_empty(),
        "终态不许带 presenter 写的中文串（英文界面会同屏两语）：{:?}",
        done.message
    );
    assert!(
        !done.chain.is_empty(),
        "终态必须带逐家的链，界面的具名原因只能从这里来"
    );
    assert!(
        done.chain
            .iter()
            .all(|step| step.outcome != JavaChainOutcome::Hit),
        "真实地失败了一次，不该有命中：{:?}",
        done.chain
    );
}

/// **编排只有 core 一处**（复审 r2 P2-1 的判据）。
///
/// 这条不改运行期行为，扫的是**源码**：presenter 那次「按厂商逐家调」的重复一旦回来，
/// `real_step` 就必须重新出现 `with_vendor` / `order(..)` / `supports(..)` 三件套里的某一件。
/// 为什么不用运行期判据：两条路（一次调 vs 逐家调）在桌面平台上**结论等价**，跑起来分不出来；
/// 差别在 Termux（系统包 vs 归档下载）与「core 改回退链只改一处」，两者都不是离线能实跑的。
///
/// **抓不住**：把这两行注释也扫进去的假红（所以下面按「非注释行」过滤）；以及 core 自己把回退链
/// 拆出去（那要另立判据）。
#[test]
fn the_install_orchestration_lives_in_core_only() {
    let presenter = code_lines(include_str!("install.rs"));
    for needle in ["with_vendor", ".order(", ".supports("] {
        assert!(
            !presenter.contains(needle),
            "presenter 里又出现 `{needle}`：那是**回退链编排**，只许在 core（`game::java::install`）。\
             presenter 只该调一次 `java::install(InstallRequest::new(major))`。"
        );
    }
    assert_eq!(
        presenter.matches("java::install(").count(),
        1,
        "presenter 只许调一次 `java::install`：\n{presenter}"
    );
    assert!(
        presenter.contains("InstallRequest::new(major)"),
        "要装的大版本走 `InstallRequest::new(major)`（**不 pin 厂商**）——与设置页同一条路，\
         Termux 上才会同样先试系统包：\n{presenter}"
    );

    // 另一头：回退链确实在 core（免得两边都空着也算绿）。按工作区相对路径读源码——
    // 本用例只在仓库里跑（`cargo test -p unlucky-presenter`），不需要把源码编进二进制。
    let core_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../core/src/game/java/install.rs"
    );
    let core_source = std::fs::read_to_string(core_path)
        .unwrap_or_else(|error| panic!("读不到 core 的 install.rs（{core_path}）：{error}"));
    let core = code_lines(&core_source);
    assert!(
        core.contains("for candidate in &priority"),
        "core 的回退链循环不见了？"
    );
    assert!(
        core.contains("with_chain(chain)"),
        "core 失败时要挂上 ChainLog（presenter 的逐家原因就从这里来）"
    );
}

/// 源码里**非注释行**的拼接（判据只看代码，不看解释——文档里难免提到那几个名字）。
fn code_lines(source: &str) -> String {
    source
        .lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*'))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
