//! M334 r1 P1：**装失败之后再点一次下载**（复审实测它是坏的：点了没反应、后台却反复起安装）。
//!
//! 从 `m334.rs` 拆出（那边贴着 400 行硬线）。
//!
//! | 用例 | 抓得住 |
//! |---|---|
//! | [`a_failed_install_can_be_retried_without_the_stale_snapshot_winning`] | 删掉 `JavaGuardCard::consumed` 那道代次闸 ⇒ 第二次点击同一拍就被**旧失败快照**判回 `Failed`（用户点了没反应），而后台真的又起一条安装 |
//! | [`the_other_two_failure_shapes_have_their_own_sentences`] | 三种失败形状（某家失败 / 全跳过 / 本地问题）被压成同一句，或「全跳过」时去点名一家发行版 |

use unlucky_i18n::catalog::Key;
use unlucky_presenter::dto::{JavaChainOutcome, JavaChainStep};

use crate::app::dialog::{GuardInstall, GuardInstallFailure};
use crate::app::state::App;

// 夹具与断言助手复用 `m334.rs`（同一条产品路径，拆两个文件只为守 400 行）。
use super::m334::{blocked, body, deliver, failed, hit, java_page, recording, tertiary};

// ------------------------------------------------- P1：失败之后再点一次（复审 r1）

fn click(app: &mut App) {
    let _ = app.update(crate::app::Message::Dialog(
        crate::app::dialog::Message::Tertiary,
    ));
}

fn install_state(app: &App) -> GuardInstall {
    app.java_guard.as_ref().expect("卡片还在").install.clone()
}

fn skip(vendor: &str) -> JavaChainStep {
    JavaChainStep {
        vendor: vendor.to_string(),
        outcome: JavaChainOutcome::Skip,
        detail: String::new(),
    }
}

/// **P1（复审 r1）**：装失败之后再点一次下载，必须真的「重新开始」。
///
/// 三条一起断（缺一条这条 bug 就能回来）：
/// ① 卡片回到「下载中」——不是停在失败；
/// ② 上一次失败留下的**旧快照**不许把它打回失败（代次没变就丢弃）；
/// ③ 同一时刻只有一条安装在跑：下载中/已请求时再点几下都不派新命令。
///
/// 抓得住：删掉 `JavaGuardCard::consumed` 那道闸（`fold_guard_install` 的「旧代次丢弃」）——
/// ②③ 立刻红：第二次点击同一拍就被旧快照判回 `Failed`，而命令已经派出去了，用户反复点
/// 就反复起安装（复审实测：`dispatched: [InstallJavaFor, InstallJavaFor]` 而界面还写着旧失败）。
#[test]
fn a_failed_install_can_be_retried_without_the_stale_snapshot_winning() {
    let (mut app, recorded) = recording();
    deliver(&mut app, &blocked());

    // 第一次：点 → 下载中 → 某一家失败（链上是真实形状）。
    click(&mut app);
    java_page(&mut app, true, &[17], Vec::new(), "");
    java_page(
        &mut app,
        false,
        &[17],
        vec![failed("temurin", "Adoptium API 不可达（HTTP 503）")],
        "",
    );
    assert!(
        matches!(install_state(&app), GuardInstall::Failed { .. }),
        "前置：第一次要失败"
    );
    let lang = app.lang();
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog()
            .render(Key::JavaGuardDownload, &[("major", "8")])
    );

    // 第二次：点 ⇒ 立刻回到「下载中」；脚本里那一拍读到的仍然是上一份失败快照。
    click(&mut app);
    assert_eq!(
        install_state(&app),
        GuardInstall::Requested,
        "再点一次必须真的重新开始（而不是被旧快照判回失败）"
    );
    assert_eq!(
        tertiary(&mut app, lang),
        lang.catalog()
            .render(Key::JavaGuardDownloading, &[("major", "8")])
    );
    // 再刷一帧（旧快照原样还在 `state.java` 里）：仍然不许落回 Failed。
    app.refresh_view();
    assert_eq!(
        install_state(&app),
        GuardInstall::Requested,
        "旧代次的快照不许冒充本次的结果"
    );
    assert_eq!(
        recorded.borrow().len(),
        2,
        "第一次 + 这一次 = 两条（每次真点击各一条）：{:?}",
        recorded.borrow()
    );

    // 第三次起：还在下载中 ⇒ 点几下都不派新命令（同一时刻只有一条在跑）。
    click(&mut app);
    click(&mut app);
    assert_eq!(
        recorded.borrow().len(),
        2,
        "下载中再点不许再派安装（否则会叠着跑）：{:?}",
        recorded.borrow()
    );

    // 新一代快照到了：先「运行中」，再「装好了」。
    java_page(&mut app, true, &[17], Vec::new(), "");
    assert_eq!(install_state(&app), GuardInstall::Running);
    click(&mut app);
    assert_eq!(recorded.borrow().len(), 2, "运行中再点也不派");
    java_page(&mut app, false, &[8, 17], vec![hit("temurin")], "");
    assert_eq!(
        install_state(&app),
        GuardInstall::Done,
        "第二次真装成了 ⇒ 卡片必须跟着说「已装好」"
    );
}

/// 另外两种失败形状各有自己的句子（复审 r1 P2-1 的三种形状都要有话说）。
#[test]
fn the_other_two_failure_shapes_have_their_own_sentences() {
    // ① 链上全是「跳过」：没有一家提供这个版本。
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    click(&mut app);
    java_page(
        &mut app,
        false,
        &[17],
        vec![skip("microsoft"), skip("zulu")],
        "",
    );
    assert_eq!(
        install_state(&app),
        GuardInstall::Failed {
            failure: GuardInstallFailure::NoVendor
        }
    );
    let lang = app.lang();
    assert!(
        body(&mut app, lang).contains(
            &lang
                .catalog()
                .render(Key::JavaGuardDownloadNoVendor, &[("major", "8")])
        ),
        "全跳过要说「没有一家发行版提供 Java 8」：{}",
        body(&mut app, lang)
    );

    // ② 连落点都没解析出来（链上那一跳的 vendor 是空串）：说「没能开始」+ 原文。
    let (mut app, _recorded) = recording();
    deliver(&mut app, &blocked());
    click(&mut app);
    java_page(
        &mut app,
        false,
        &[17],
        vec![JavaChainStep {
            vendor: String::new(),
            outcome: JavaChainOutcome::Failed,
            detail: "Standard 模式下拿不到系统标准目录".to_string(),
        }],
        "",
    );
    assert_eq!(
        install_state(&app),
        GuardInstall::Failed {
            failure: GuardInstallFailure::Local {
                detail: "Standard 模式下拿不到系统标准目录".to_string()
            }
        }
    );
    let lang = app.lang();
    assert!(
        body(&mut app, lang).contains(&lang.catalog().render(
            Key::JavaGuardDownloadFailedLocal,
            &[("reason", "Standard 模式下拿不到系统标准目录")]
        )),
        "本地失败要说「这次下载没能开始（原文）」：{}",
        body(&mut app, lang)
    );
}
