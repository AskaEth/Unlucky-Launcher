//! 跨卷替换：暂存包在 A 卷、目标 exe 在 B 卷时，替换必须成功且**不留半截文件**，
//! 并且「凑不出跨卷现场」这件事**必须是响的**。
//!
//! # 这条用例为什么必须存在
//!
//! `stage()` 把包落在 `<数据根>/cache/update/staging`，`install_staged()` 把包搬进 exe 所在目录。
//! 两者不在同一卷时（便携版放 U 盘、数据根设到 D 盘而程序在 C 盘、部分 macOS 配置），
//! `fs::rename` 直接返回 `EXDEV`/`ERROR_NOT_SAME_DEVICE`（`CrossesDevices`）——M169 复审
//! 在已发布版本上判定「更新必然失败」。修法是**先把包落到目标同目录的临时名，再同卷改名**：
//! 目标路径上的新字节永远来自一次原子命名，不存在「拷到一半」的中间态。
//!
//! # 跨卷是探出来的，探不到**不许静默变绿**（M171 r2 P2-1）
//!
//! [`XdevRoot::probe`] 用一次真实 `rename` 验证候选目录**真的**在另一块卷上。问题是「探不到」
//! 以前写成 `eprintln! + return`：libtest 默认捕获 stdout/stderr，于是这台机器只有一块卷时
//! 用例显示 `... ok`，跨卷守卫**空跑了却看不出来**（复审 M3 变异实测）。本仓已确立的口径是
//! 「跳过这件事必须是响的」（同 `crates/app-iced/.../options_shot/feed.rs::not_fed`），
//! 所以裁决被收进 [`xdev_guard`]，三档：
//!
//! | 探到第二块卷 | `ULL_XDEV_TEST_WAIVE=1` | 结果 |
//! |---|---|---|
//! | 是 | 无所谓 | **绿**，真跨卷用例真跑（豁免只在探测失败时被咨询） |
//! | 否 | 给了 | **绿**，并打印「已显式豁免」——这是**记录在案的**降级，不是意外 |
//! | 否 | 没给 | **红**：本机/本作业的跨卷守卫没有生效 |
//!
//! 另有 `ULL_XDEV_TEST_REQUIRE=1`（声明「本机必须有第二块卷」）：与豁免同时给时以它为准
//! （声明了必须有、实际没有 ⇒ 红），一句话里不许自相矛盾。`ULL_XDEV_TEST_ROOT` 可以直接
//! 指定一块异卷目录（Windows 举例：`ULL_XDEV_TEST_ROOT=D:/Temp`）。
//!
//! # 谁该给哪个 env
//!
//! * **本仓开发机**（`C:` NVMe + `D:` 机械盘，见 [`volume_candidates`]）：什么都不用给，探得到。
//! * **CI `test (ubuntu-latest)`**：`/tmp` 与会话临时目录同卷，但 `/dev/shm` 是另一块 tmpfs ⇒
//!   通常直接探得到（本机 docker 里实测 `/tmp` 设备号 119、`/dev/shm` 158）。探不到就会红。
//! * **CI `test (windows-latest)` / `update-e2e (windows-latest)`**：runner 是只有一块系统卷、
//!   还是额外挂了一块临时盘（`D:`），**本仓没有核实手段**（没有 runner 权限，文档也没写死盘符）。
//!   所以策略**不赌它**：探到就跑真跨卷；探不到就必须在作业里显式写
//!   `ULL_XDEV_TEST_WAIVE: "1"`——这一行属 `.github/workflows/ci.yml`（不在 M171 的改动范围）。
//!
//! 只有注入用例（`install_io_cases`）永远在跑：它把「跨卷被拒」「复制到一半失败」直接喂给
//! `install_with`，不依赖盘的分区布局。
//!
//! # 探不到时怎么办（三选一）
//!
//! 1. 给一块异卷目录：`ULL_XDEV_TEST_ROOT=<另一块卷上的可写目录>`；
//! 2. 明确豁免：`ULL_XDEV_TEST_WAIVE=1`；
//! 3. 认为自己本该有第二块卷 ⇒ `ULL_XDEV_TEST_REQUIRE=1`，让它在配置坏掉时就地红。

use std::fs;

use crate::update::manifest::ReleaseManifest;
use crate::update::{install_staged, stage, DirSource, ReplacePolicy, StagedPackage};

use super::support::{
    artifact, manifest, names_in, release, TempDir, XdevRoot, XDEV_REQUIRE_ENV, XDEV_WAIVE_ENV,
};

const UPDATE: &[u8] = b"new build bytes, cross volume";
const OLD: &[u8] = b"old build bytes";

fn policy() -> ReplacePolicy {
    ReplacePolicy::new(2, std::time::Duration::from_millis(1))
}

/// 跨卷守卫的裁决结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum XdevGuard {
    /// 探到了第二块卷：真跨卷用例会真跑。
    Live,
    /// 没有第二块卷，但调用方**显式**豁免了。
    Waived,
}

/// 裁决（纯函数：只吃「探到了吗」与两档环境变量，不读进程环境 ⇒ 并行跑不会互相干扰）。
///
/// 规则见模块文档的表。`require` 优先于 `waiver`：声明「必须有」又同时说「可以没有」是自相矛盾，
/// 这时按更严的那条办——否则一句手误就把守卫关掉了。
fn xdev_guard(
    found: bool,
    waiver: Option<&str>,
    require: Option<&str>,
) -> Result<XdevGuard, String> {
    if found {
        return Ok(XdevGuard::Live);
    }
    // 只认 `1`（`true`/`yes`/空串一律不算：手误不该开闸），两侧空白忽略。
    let required = require.map(str::trim) == Some("1");
    let waived = waiver.map(str::trim) == Some("1");
    if waived && !required {
        return Ok(XdevGuard::Waived);
    }
    Err(xdev_failure(required, waived))
}

/// 探不到第二块卷、又不许豁免时的失败正文（把「有哪三条出路」写全，别让人去猜）。
fn xdev_failure(required: bool, waived: bool) -> String {
    let lead = match (required, waived) {
        (true, true) => concat!(
            "同时给了 ULL_XDEV_TEST_REQUIRE=1 与 ULL_XDEV_TEST_WAIVE=1：自相矛盾。",
            "以 REQUIRE 为准（声明了必须有，就没有豁免）。"
        ),
        (true, _) => {
            "给了 ULL_XDEV_TEST_REQUIRE=1（声明本机必须有第二块卷），但这台机器上探不到——那是配置坏了，不是可以跳过。"
        }
        (false, _) => {
            "没有第二块可写卷，也没有显式豁免：跨卷守卫在这台机器上是空跑。"
        }
    };
    format!(
        "[xdev-guard] {lead}\n\
         这条守卫守的是已发布版本上的真 bug（数据根与程序不同卷 ⇒ 更新必然失败），\
         空跑的守卫等于没有，所以默认判红。三种合规做法任选其一：\n\
         1. 给它一块异卷目录：ULL_XDEV_TEST_ROOT=<另一块卷上的可写目录>（本机举例：D:/Temp）；\n\
         2. 明确豁免：ULL_XDEV_TEST_WAIVE=1（值必须是 `1`）；\n\
         3. 认为本机本该有第二块卷 ⇒ 用 ULL_XDEV_TEST_REQUIRE=1 声明，坏掉时就地红。\n\
         CI：ubuntu runner 的 /dev/shm 与 /tmp 是不同设备（通常直接满足第 1 条）；\
         windows runner 只有一块卷时，作业里要显式给第 2 条（.github/workflows/ci.yml 的 \
         test 作业与 update-e2e 作业）。"
    )
}

/// 要一块异卷现场来做真跨卷用例：`Some` = 探到了；`None` = **显式豁免**（已打印记录）。
///
/// 探不到又没豁免就 panic——这是给逐条用例用的入口，保证没有哪一条能靠 `return` 静静跳过。
fn cross_volume_scene(tag: &str) -> Option<XdevRoot> {
    let root = XdevRoot::probe(tag);
    let verdict = xdev_guard(
        root.is_some(),
        std::env::var(XDEV_WAIVE_ENV).ok().as_deref(),
        std::env::var(XDEV_REQUIRE_ENV).ok().as_deref(),
    );
    match verdict {
        Ok(XdevGuard::Live) => root,
        Ok(XdevGuard::Waived) => {
            eprintln!(
                "[xdev-guard] {XDEV_WAIVE_ENV}=1：本机没有第二块可写卷，真跨卷用例（{tag}）\
                 按显式豁免跳过——跨卷逻辑此刻只有 install_io_cases 的注入用例在守。"
            );
            None
        }
        Err(message) => panic!("{message}"),
    }
}

/// **常跑的裁决用例**：跨卷守卫要么在真跑，要么有记录在案的豁免，要么红。
///
/// 它自己不需要第二块卷也能跑，所以永远被执行；`--nocapture` 下能看到走的是哪一档。
#[test]
fn the_cross_volume_guard_is_live_or_explicitly_waived() {
    let root = XdevRoot::probe("verdict");
    let waiver = std::env::var(XDEV_WAIVE_ENV).ok();
    let require = std::env::var(XDEV_REQUIRE_ENV).ok();
    match xdev_guard(root.is_some(), waiver.as_deref(), require.as_deref()) {
        Ok(XdevGuard::Live) => {
            let at = root.as_ref().map(|root| root.path().display().to_string());
            eprintln!(
                "[xdev-guard] 探到第二块可写卷（{}）：真跨卷用例会真跑。",
                at.unwrap_or_default()
            );
        }
        Ok(XdevGuard::Waived) => {
            eprintln!(
                "[xdev-guard] 本机没有第二块可写卷，但已显式豁免（{XDEV_WAIVE_ENV}=1）：\
                 真跨卷用例跳过，跨卷逻辑由 install_io_cases 的注入用例守着。"
            );
        }
        Err(message) => panic!("{message}"),
    }
}

/// 裁决表的每一格都得钉住——尤其「既没探到也没豁免 ⇒ 红」这一格（P2-1 的回归点）。
#[test]
fn the_verdict_table_has_exactly_three_ways() {
    // 探到了：永远是 Live（豁免给了也不影响，真跨卷用例照跑）。
    for (waiver, require) in [(None, None), (Some("1"), None), (None, Some("1"))] {
        assert_eq!(
            xdev_guard(true, waiver, require),
            Ok(XdevGuard::Live),
            "探到了就该真跑：{waiver:?}/{require:?}"
        );
    }
    // 探不到 + 显式豁免 ⇒ Waived（记录在案的降级）。
    assert_eq!(xdev_guard(false, Some("1"), None), Ok(XdevGuard::Waived));
    assert_eq!(
        xdev_guard(false, Some(" 1 "), None),
        Ok(XdevGuard::Waived),
        "两侧空白应忽略"
    );
    // 探不到 + 没豁免 ⇒ 红（这条就是 P2-1 要的那个「响」）。
    let error = xdev_guard(false, None, None).expect_err("没豁免就必须红");
    assert!(error.contains("空跑"), "失败正文要说清是空跑：{error}");
    // 只认 `1`。
    for bogus in [Some("true"), Some("yes"), Some(""), Some("0")] {
        assert!(
            xdev_guard(false, bogus, None).is_err(),
            "只有 `1` 算豁免，{bogus:?} 不算"
        );
    }
    // 声明「必须有」⇒ 探不到就红，且 REQUIRE 压过 WAIVE（不许自相矛盾地放行）。
    let error = xdev_guard(false, None, Some("1")).expect_err("声明了必须有就必须红");
    assert!(error.contains("REQUIRE"), "{error}");
    let error = xdev_guard(false, Some("1"), Some("1")).expect_err("矛盾时按更严的办");
    assert!(error.contains("自相矛盾"), "{error}");
}

/// 在 `tmp`（会话临时目录 = A 卷）里造一份已校验的暂存包。
fn staged_on_a_volume(tmp: &TempDir) -> StagedPackage {
    tmp.write("ull-test.zip", UPDATE);
    let json = manifest(
        "stable",
        &[release(
            "0.3.0",
            "",
            &[artifact("win-x64", "portable-exe", "ull-test.zip", UPDATE)],
        )],
    );
    let parsed = ReleaseManifest::from_json(&json).expect("清单应可解析");
    stage(
        &DirSource::new(tmp.path()),
        &parsed.releases[0].artifacts[0],
        &tmp.join("staging"),
    )
    .expect("取包与校验应通过")
}

/// ① 跨卷安装必须成功，且目标字节 = 包体字节（不是半截、不是旧字节）。
#[test]
fn installs_across_volumes_and_lands_the_exact_bytes() {
    let Some(xdev) = cross_volume_scene("install") else {
        return;
    };
    let tmp = TempDir::new("xdev-install");
    let staged = staged_on_a_volume(&tmp);
    let app = xdev.dir("app");
    let dest = app.join("ull.exe");
    fs::write(&dest, OLD).expect("写旧版本");

    // 前提断言：暂存与目标**真的**不在同一卷（否则这条用例只是在同卷上重跑一遍）。
    let cross =
        fs::rename(staged.path(), app.join("probe-move")).expect_err("跨卷 rename 必须失败");
    assert_eq!(
        cross.kind(),
        std::io::ErrorKind::CrossesDevices,
        "前提不成立：暂存与目标同卷（{cross}）"
    );

    let receipt = install_staged(&staged, &dest, policy()).expect("跨卷替换必须成功");

    assert_eq!(
        fs::read(&dest).expect("目标应存在"),
        UPDATE,
        "目标必须就是包体字节"
    );
    assert_eq!(receipt.bytes, UPDATE.len() as u64);
    assert_eq!(receipt.dest, dest);
    let backup = receipt.backup.clone().expect("旧版本应留备份");
    assert_eq!(backup.parent(), dest.parent(), "备份必须与目标同目录");
    assert_eq!(
        fs::read(&backup).expect("备份应存在"),
        OLD,
        "旧字节要完整留在备份里"
    );
    let names = names_in(&app);
    assert_eq!(
        names.len(),
        2,
        "目标目录里只该有目标与一份备份（跨卷中转的临时名必须被改掉、不许残留）：{names:?}"
    );
    assert_eq!(names[1], "ull.exe", "排序后目标在后：{names:?}");
    assert!(
        names[0].starts_with(".ull.exe.ullold-"),
        "另一个必须是备份：{names:?}"
    );
}
