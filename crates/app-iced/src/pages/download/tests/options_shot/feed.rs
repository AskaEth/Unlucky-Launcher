//! 像素守门的「喂图」通道：`ULL_DL_*_SHOT` 环境变量 → PNG 路径，以及**没喂图时该有多响**。
//!
//! 四条像素断言（`chips` / `compact` / `ready` / 父模块的 wide）都从 [`feed`] 拿图。
//! 为什么单独一个文件：调用侧只有一句 `let Some(path) = feed(KEY) else { return; };`，
//! 但「没图怎么办」是同一条策略——抄四份必然跑偏。M127 之前就是抄成了一律 `SKIP`，
//! 于是 CI 里四条全在**空跑**（`.github/` 与 `packaging/` 里 grep 这些变量零命中，
//! 说明从来没喂过图）；M127 把裁决收到这里，并让 CI 里缺图直接失败。
//!
//! ## 手动出图的四条命令（CI 的 `test` 作业跑的就是这四条）
//!
//! ```text
//! # ready700：**不设** ULL_DL_DEMO（= ready，还没选版本）
//! cargo run -p unlucky-app-iced --example pages_demo -- --screen=download \
//!   --width=700 --height=760 --screenshot=ready700.png \
//!   --screenshot-theme=light --software-renderer
//!
//! # chips700 / compact736 / wide1180：都要 ULL_DL_DEMO=options-fabric
//! ULL_DL_DEMO=options-fabric cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=download --width=700 --height=760 --screenshot=chips700.png \
//!   --screenshot-theme=light --software-renderer
//! ULL_DL_DEMO=options-fabric cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=download --width=736 --height=760 --screenshot=compact736.png \
//!   --screenshot-theme=light --software-renderer
//! ULL_DL_DEMO=options-fabric cargo run -p unlucky-app-iced --example pages_demo -- \
//!   --screen=download --width=1180 --height=760 --screenshot=wide1180.png \
//!   --screenshot-theme=light --software-renderer
//!
//! ULL_DL_READY_SHOT=ready700.png ULL_DL_CHIPS_SHOT=chips700.png \
//!   ULL_DL_COMPACT_SHOT=compact736.png ULL_DL_OPTIONS_SHOT=wide1180.png \
//!   cargo test -p unlucky-app-iced --lib options_shot
//! ```
//!
//! ## 只在 Windows runner 上可信
//!
//! 断言里有几条直接量**墨迹**：`ready` 数「读得出几个版本号」的墨迹条数、`chips` 量首行
//! 底色带与分隔线上下的高度、`compact` 量图标药丸的 40×40。这些数字是按父模块 `ui()` 里
//! 那套字体栈（`Segoe UI` / `Consolas`）标定的。Linux / macOS runner 上这套字体不存在，
//! 回退字体的字高与行距一变，行高、墨迹条数、药丸位置跟着变 ⇒ **假红**。
//!
//! 所以「像素断言只在 Windows runner 上跑」这条约束钉在**三处**，不靠「碰巧只配了一处」：
//!
//! 1. `.github/workflows/ci.yml` 的 `test` 作业：出图那步带 `if: runner.os == 'Windows'`；
//! 2. 同一作业的断言那步也带同一个条件，并把「为什么」写在那里；
//! 3. 本文件的 [`feed`]：非 Windows 上**喂了图**会当场失败（CI 里），不是静静地跑出假红。
//!
//! ## 没喂图时：CI 里失败，本机跳过
//!
//! | 环境 | 变量在 | 变量不在 |
//! |---|---|---|
//! | Windows + CI | 正常断言 | **失败**：「守门未喂图」 |
//! | Windows 本机 | 正常断言 | 跳过（一句提示） |
//! | 非 Windows + CI | **失败**：这条守门的标定前提不成立 | 跳过（本仓 CI 不在 Linux/macOS 上跑像素断言） |
//! | 非 Windows 本机 | 跑，但提示「数字可能不适用」 | 跳过（一句提示） |
//!
//! 目标是：将来谁把 CI 里出图那一步重构掉，构建会**红**，而不是静静退回手动档。

use std::path::PathBuf;

/// CI 环境（GitHub Actions 在所有 runner 上都设这个）。
fn in_ci() -> bool {
    std::env::var_os("CI").is_some_and(|value| !value.is_empty())
}

/// `key` 指的截图路径；`None` = 没喂图，调用侧 `return`（跳过与失败的裁决已经在这里做完了）。
///
/// 空串当作没设：GitHub 的条件 env 很容易求出 `''`，而 `PathBuf::from("")` 会被后面那条
/// 相对路径分支解析成**工作区根目录**，读它只会得到一句莫名其妙的「读截图」失败。
pub(super) fn feed(key: &str) -> Option<PathBuf> {
    let Some(value) = std::env::var_os(key).filter(|value| !value.is_empty()) else {
        not_fed(key);
        return None;
    };
    let path = PathBuf::from(value);

    if !cfg!(windows) {
        // 非 Windows 上喂图：本机随意（只是数字可能不适用），CI 里说明标定前提被破坏了
        assert!(
            !in_ci(),
            "[options-shot] 在非 Windows runner 上喂了 {key}：这几条断言按 Windows 字体栈\
             （Segoe UI）标定，Linux/macOS 上跑出来的绿/红都不可信。\
             像素守门只在 windows-latest 上跑，见 .github/workflows/ci.yml 的 test 作业。"
        );
        eprintln!(
            "[options-shot] 注意：{key} 喂在非 Windows 上，字体栈与标定时不同，\
             数字可能对不上（这不是产品问题）。"
        );
    }

    if path.is_absolute() || path.exists() {
        return Some(path);
    }
    // `cargo test` 的 cwd 是 crate 目录，相对路径按**工作区根**解释
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    Some(root.join(path))
}

/// 没喂图时的统一裁决：CI（Windows）里**失败**，本机跳过并留一句提示。
///
/// 由 [`feed`] 在返回 `None` 之前调用——「跳过」这件事必须是响的，不能 `return` 了事。
fn not_fed(key: &str) {
    let message = format!(
        "[options-shot] 守门未喂图：环境变量 {key} 没有设置。这条是**渲染层**像素断言，\
         需要先用 pages_demo 现出真 PNG（四条命令见 tests/options_shot/feed.rs 的模块文档）。"
    );
    // 只在 Windows + CI 上硬失败：非 Windows 上本来就不该喂图（见模块文档的表），
    // 那边缺图是**预期**，硬失败会误伤 Linux 那条矩阵。
    assert!(
        !(cfg!(windows) && in_ci()),
        "{message}\n\
         当前是 CI 环境（CI 已设置）却拿到空路径 ⇒ CI 里出图那一步被重构掉了 / 条件写错了，\
         构建必须红：空跑的守门等于没有守门。\
         修法：.github/workflows/ci.yml 的 test 作业里出图与断言两步都要带这条——\
         `if: runner.os == 'Windows'`，且出图那步要把四个 ULL_DL_*_SHOT 写进 $GITHUB_ENV。"
    );
    eprintln!("{message} SKIPPED（本机，非 CI）。");
}
