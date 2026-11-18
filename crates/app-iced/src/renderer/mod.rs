//! 渲染后端的**判据表**（M97）+ 三段可观测性基建。
//!
//! 本模块回答一个问题：**这个进程到底跑在哪个渲染器上，凭什么这么断定。**
//!
//! ## 判据表（验收直接对照这张表）
//!
//! `ICED_BACKEND` **永远是单项**（不是候选列表）。理由见下节「为什么不再用候选列表」：
//! 单项 ⇒ 「跑起来了 ⇒ 就是它」，不存在需要事后猜的空间。`兜底` 列是**本 crate 自己的重试**，
//! 不是 iced 的回退，所以它一定伴随一行具名错误日志。
//!
//! | # | 条件 | 选哪个 | 日志该打印什么 | 谁能验证 |
//! |---|---|---|---|---|
//! | 1 | `--renderer=cpu` / `--software-renderer` | `tiny-skia` | `renderer = tiny-skia reason=forced-cpu` | 三平台（CI 只用这条） |
//! | 2 | `--renderer=gpu` 但本构建没编 wgpu | `tiny-skia` | `reason=gpu-not-compiled` + `；本构建未编入 wgpu` | 三平台（`--no-default-features`） |
//! | 3 | `--renderer=gpu` 且编了 wgpu | `wgpu`，兜底 `tiny-skia` | `renderer = wgpu reason=forced-gpu 兜底=tiny-skia` | Windows / Linux 本机；macOS 需真机 |
//! | 4 | `--renderer=auto` 但没编 wgpu | `tiny-skia` | `reason=auto-gpu-not-compiled` | 三平台（`--no-default-features`） |
//! | 5 | `auto` + 枚举不到硬件适配器（只有 WARP / llvmpipe） | `tiny-skia`，兜底 `wgpu` | `reason=auto-no-hardware-adapter` | Windows 本机（软渲染 VM） |
//! | 6 | `auto` + 有硬件适配器 | `wgpu`，兜底 `tiny-skia` | `reason=auto-hardware-adapter` | Windows 本机 **√**；macOS 需真机 |
//! | 7 | 同上但 `ULL_PREFLIGHT=1` 预检失败 | `tiny-skia`，兜底 `wgpu` | `[gpu-preflight] FAILED stage=… error=…` + `reason=preflight-failed` | 三平台（可用 `WGPU_BACKEND=gl` 造） |
//! | 8 | 任何一行选了 wgpu，但 iced 真初始化失败，且**交互启动**（无 `--screenshot`） | 具名错误 → **换一个进程**重开 `tiny-skia` | `[gpu] wgpu 初始化失败（具名）：…` → `[gpu] 降级 → tiny-skia（…换一个进程重开…）` → 子进程 `[probe] 本次是降级重开（ULL_GPU_FALLBACK=tiny-skia）` | **Windows 已验**（`WGPU_BACKEND=metal` 造）；macOS 需真机 |
//! | 9 | 同上行，但**取证启动**（带 `--screenshot`，含 `--screen=gallery`） | 具名错误 → **不出图**，退出码 1 | `[gpu] wgpu 初始化失败（具名）：…` → `[gpu] 拒绝降级出图：本次是取证启动（--screenshot …）` | **Windows 已验**（同造法）；macOS 需真机 |
//!
//! **第 8/9 行的硬规则（M97 定）**：
//!
//! - 取证工具（`--screenshot`）宁可红着退出，也不能悄悄换一种后端继续出图。以前 gallery 在
//!   wgpu 挂掉时会静默降级到 tiny-skia 照样落 PNG，而报告上写的是「GPU 渲染截图」——所有视觉
//!   结论的可信度都被悄悄打折，而且从输出上看不出来。现在每张图的日志里都带
//!   **实际**后端名（`[shot] … renderer=wgpu`），图能自证来源。
//! - 交互启动保留安全网：真无 GPU / wgpu 起不来时降级到 tiny-skia，不崩。
//!
//! **降级为什么是换进程**：第一版写的是「打印错误 → 在本进程里再 `run()` 一次」，Windows 实测
//! 直接 panic（`iced_winit-0.14.1/src/lib.rs:81` → `Create event loop: RecreationAttempt`）——
//! winit 不允许一个进程建两次事件循环。所以安全网是 `respawn`：起一个子进程、只换
//! `ICED_BACKEND` + `ULL_GPU_FALLBACK`，然后带着子进程的退出码退出。子进程看到
//! `ULL_GPU_FALLBACK` 就跳过探测（否则会得到同一个结论、无限重开）。
//!
//! `ULL_PREFLIGHT` 开关的代价（**实测**，不是估计）：Windows RX 6600 XT / Vulkan 上
//! `took=717ms`（首次要扫一遍 Vulkan loader），不可用时 `took=0ms`。打开它等于启动多 ~0.7 s，
//! 所以默认关。它测不了 `create_surface`（那时还没有窗口），代替不了真跑一次，只用来「开窗前拿原因」。
//! 注意：`ULL_PREFLIGHT=1` + 预检失败 + `--screenshot` 时**仍会**降级出图（`reason=preflight-failed`，
//! 图上是 `renderer=tiny-skia`）——那条路径不是静默的，但图确实是 CPU 的，读图前先读日志。
//!
//! ## 怎么从日志判断「有没有静默降级」
//!
//! | 你看到 | 结论 |
//! |---|---|
//! | `[probe] renderer = wgpu` 且之后有 `[gpu] iced_wgpu: …Selected: AdapterInfo…` | **真在走 GPU**（这行是 iced_wgpu 自己在创建设备时打的，不是我们的猜测） |
//! | `[gpu] wgpu 初始化失败（具名）：…` | wgpu 起不来，**原因就在这行里**；交互启动已换进程降级 CPU，取证启动已拒绝出图 |
//! | `[gpu] 拒绝降级出图：…` | 这张图**没有**产出（取证启动 + wgpu 起不来）。想用 CPU 出图请显式 `--renderer=cpu` |
//! | `[gpu] 降级 → tiny-skia（…换一个进程重开…）` | 交互启动降级了；后面必然跟一行 `[probe] 本次是降级重开` |
//! | `[probe] renderer = tiny-skia reason=…`（`reason` 不是 `forced-cpu`） | 探测判定不该上 GPU，**原因在 `reason`** |
//! | `[shot] … renderer=wgpu` / `renderer=tiny-skia` | 这张图**实际**由哪个后端产出（M97 之前这里打的是请求值 `auto`，等于没打） |
//! | 只有 `[probe] renderer = …` 一行、没有 `[gpu] iced_wgpu:` 也没有取证行 | 那是 M97 之前的旧日志——**它从来没观测过实际后端**，不可采信 |
//!
//! ## 为什么不再用候选列表（M97 的根因订正）
//!
//! M97 之前：`auto` + 有硬件 → **不设** `ICED_BACKEND`，把决策交给 iced 的
//! `fallback::Compositor`；那条路径里 wgpu 的失败被 `errors` 向量装起来、tiny-skia 一成功就整体丢弃
//! （`iced_renderer-0.14.0/src/fallback.rs`，我们改不了）。日志只剩 `renderer = auto (wgpu -> tiny-skia)`
//! ——而这行是 `probe.rs` 里按 `env == None` 打出来的**常量字符串**，跟实际后端无关。
//! 于是 macOS M1 上「枚举到 Apple M1」与「最终是不是真在用 GPU」彻底脱钩：**看起来正常但可能在跑 CPU**。
//!
//! 现在的唯一判据：`ICED_BACKEND` 只写一项 ⇒ 能跑起来就必定是它；起不来就必然是
//! `iced::Error::GraphicsCreationFailed` 里那串具名错误（哪个 backend、reason 是什么），
//! 由 `app/boot.rs` 打印后决定「换进程降级」还是「拒绝出图」。
//!
//! ## 四段基建
//!
//! | 模块 | 作用 | 默认 |
//! |---|---|---|
//! | [`preflight`] | 在 iced 之前自己跑一遍 wgpu 的「实例→适配器→设备」，把失败原因拿在手里 | **关**（`ULL_PREFLIGHT=1` 打开；会重复创建实例/设备，别放默认路径） |
//! | [`log_bridge`] | 把 `iced_wgpu` / `wgpu` 的 `log` 输出转到 stderr（`Selected: AdapterInfo{…}` 这行就是 GPU 生效的实证） | 开（`ULL_GPU_LOG=off` 可关） |
//! | [`acceptance`] | **验收诊断（M98）**：`[env]` `[backend]` `[window]` `[font] chain` `[theme]` `[perf]` `[i18n]` `[caps]` 八类里本 crate 能观测的那部分，固定前缀、单行可 grep | 便宜的行恒开；贵的（适配器枚举）由 `ULL_ACCEPTANCE=1` 打开 |
//!
//! ### 「单项 ⇒ 没有静默回退」这条推理的依据（M98 r1 复审挖到源码层）
//!
//! `iced_renderer` 的 `fallback::Compositor` 拿到 `ICED_BACKEND` 后**仍会把每个候选依次拿去试
//! wgpu 和 tiny-skia**，两个子渲染器收同一个 backend 字符串，所以「单项」本身并不阻止它去试
//! tiny-skia。真正挡住它的是 `iced_tiny_skia` 的 `with_backend`：它只接受
//! `None | "tiny-skia" | "tiny_skia"`，`Some("wgpu")` 直接返回 `GraphicsAdapterNotFound`。
//! 于是 `ICED_BACKEND=wgpu` 这条路**不可能**落到 tiny-skia：起不来就两项都失败、返回
//! `GraphicsCreationFailed`（判据表第 8/9 行接住它）。
//!
//! 这条依赖 iced 的实现细节，所以：① `iced = "=0.14.0"` 精确钉版本；② 验收脚本另有
//! `[gpu] …Selected: AdapterInfo` 这条**不依赖本 crate 推理**的实证断言。升级 iced 时这两条都要重看。
//!
//! **本模块自己只回答「判据」**；「结果」由三处分别取证，别混：① `[backend] final=`（本 crate 的
//! 起步结果，判据是单项 `ICED_BACKEND`）；② `[gpu] iced_wgpu: …Selected:`（iced 自己打的，
//! GPU 真生效的实证）；③ `[shot] … renderer=`（这张图是哪个后端出的）。
//!
//! ## 真机验收（macOS Metal 路径 —— **需真机复核**，Windows 验不了这一段）
//!
//! 判据表第 3/6/8/9 行的 macOS 分支只能真机验。下面每段都可直接粘贴；`# 期望` 后面是**必须能在
//! stderr 里 grep 到的原文**（M98 的验收包会直接把这些字符串编成断言）。
//!
//! ```bash
//! # 0) 一次
//! cargo build -p unlucky-app-iced
//!
//! # 1) 交互启动：这才是判「M1 到底有没有在用 GPU」的那次。看完关窗即可。
//! ./target/debug/unlucky-launcher-iced --renderer=auto
//! # 期望（这两行同一次运行里都要有）：
//! #   [probe] renderer = wgpu reason=auto-hardware-adapter 兜底=tiny-skia
//! #   [gpu] iced_wgpu::window::compositor: Selected: AdapterInfo {     name: "Apple M1",
//! # 上面第二行里的 backend 必须是 Metal（同一行靠后）。
//! # 若出现下面任一行 ⇒ 这次**不是** GPU，把那段原文（含上一行）发回来：
//! #   [gpu] wgpu 初始化失败（具名）：
//! #   [gpu] 降级 → tiny-skia
//! #   [probe] 本次是降级重开（ULL_GPU_FALLBACK=tiny-skia）
//!
//! # 2) 开窗前拿原因（不建窗；多花 ~0.7 s）
//! ULL_PREFLIGHT=1 ./target/debug/unlucky-launcher-iced --renderer=auto
//! # 期望：[gpu-preflight] ok adapter="Apple M1" type=IntegratedGpu backend=Metal took=
//!
//! # 3) 取证出图（D30 通道；这一张图自证来源）
//! ./target/debug/unlucky-launcher-iced --screen=gallery --screenshot /tmp/gallery.png
//! # 期望（三行都该有）：
//! #   [gpu] iced_wgpu::window::compositor: Selected: AdapterInfo {     name: "Apple M1",
//! #   [shot] 1180x760 scale=1 -> /tmp/gallery.png
//! #   [shot] screen=gallery theme=light renderer=wgpu
//! # 若 wgpu 起不来 ⇒ 没有 /tmp/gallery.png、退出码 1、且打印 [gpu] 拒绝降级出图：
//!
//! # 4) 强制 CPU 对照（必须与 M97 之前逐字一致）
//! ./target/debug/unlucky-launcher-iced --renderer=cpu --screenshot /tmp/cpu.png
//! # 期望：
//! #   [probe] adapters=0 hardware=false :: request=cpu -> ICED_BACKEND=tiny-skia
//! #   [probe] renderer = tiny-skia reason=forced-cpu 兜底=none（候选表唯一项）
//! #   [shot] screen=shell theme=light renderer=tiny-skia
//!
//! # 5) 负例：取证启动遇到 wgpu 不可用时必须红着退出（Mac 上用一个不存在的后端来造）
//! WGPU_BACKEND=dx12 ./target/debug/unlucky-launcher-iced --screen=gallery --screenshot /tmp/bad.png
//! # 期望：**没有** /tmp/bad.png、退出码 1、并且打印：
//! #   [gpu] wgpu 初始化失败（具名）：multiple errors occurred: [GraphicsAdapterNotFound { backend: "wgpu", reason: RequestFailed("no adapter was found for the options requested:
//! #   [gpu] 拒绝降级出图：本次是取证启动（--screenshot /tmp/bad.png），换后端出的图会撒谎。
//! ```
//!
//! 第 5 段在 Windows 上已用同构的 `WGPU_BACKEND=metal` 验过（`metal support not compiled in`
//! → 具名错误 → 拒绝出图 → exit 1）；macOS 上要确认打印的是 `dx12 support not compiled in` 那类
//! wgpu 原话，而不是我们自己的话。

pub mod acceptance;
pub mod decision;
pub mod log_bridge;
pub mod preflight;

pub use decision::{decide, demote_after_preflight, fallback_round, strict, Decision, Kind};

/// 把 `iced::Error` 里的**具名**图形错误掏出来（`Display` 那句笼统的话不算原因）。
///
/// `iced::Error::GraphicsCreationFailed(inner)` 的 `Display` 只有
/// 「the application graphics context could not be created」，**没有** `{0}`；
/// 真正的 backend 名与 `Reason` 在内层 `graphics::Error` 里。所以这里同时给 Display 与 Debug。
#[must_use]
pub fn named_graphics_error(error: &iced::Error) -> Option<String> {
    match error {
        iced::Error::GraphicsCreationFailed(inner) => Some(format!("{inner}｜详情 {inner:?}")),
        _ => None,
    }
}

/// 跑一次 iced；起步后端起不来时把**具名**原因交给调用方（不在这里重试）。
///
/// ## 为什么不能在本进程里重试（M97 实测）
///
/// 第一版这里是「打印错误 + 再 `run()` 一次」。Windows 实测直接 panic：
///
/// ```text
/// thread 'main' panicked at iced_winit-0.14.1/src/lib.rs:81:
/// Create event loop: RecreationAttempt
/// ```
///
/// winit 不允许一个进程建两次事件循环，所以**进程内重试在架构上就不成立**。
/// 安全网因此改成**换一个进程重开**（`app/boot.rs` 的 `respawn`），
/// 猜测的写法别再加回来。
pub fn run_once(decision: Decision, attempt: impl FnOnce(Kind) -> iced::Result) -> iced::Result {
    attempt(decision.primary)
}

/// 降级重开用的环境变量：子进程看到它就跳过探测、直接用它指向的后端（见 `app::boot::boot_probe`）。
///
/// ⚠️ 这是**防无限重开**的唯一闸门：子进程若不跳过探测，会算出同一个结论（auto + 有硬件 → wgpu）、
/// 再失败、再 respawn，变成自杀式递归。子进程侧的判据在 `boot_probe` 的第一个分支。
///
/// 它也是**进程间**的交接约定，不是给用户设的开关（用户入口仍是 `--renderer`）。
pub const FALLBACK_ENV: &str = "ULL_GPU_FALLBACK";

/// 读 [`FALLBACK_ENV`]：本进程是不是「上一步失败后重开」的那一次。
#[must_use]
pub fn forced_fallback() -> Option<Kind> {
    match std::env::var(FALLBACK_ENV).ok()?.trim() {
        "tiny-skia" => Some(Kind::TinySkia),
        "wgpu" => Some(Kind::Wgpu),
        _ => None,
    }
}

/// 打印**具名**失败原因（三行以内，`stage`/`backend`/`reason` 都可 grep）。
///
/// `iced::Error::GraphicsCreationFailed` 的 `Display` **不包含**内部错误
/// （`#[error("the application graphics context could not be created")]`，没有 `{0}`），
/// 所以这里必须显式把内层错误打出来，否则又变回「只知道失败，不知道为什么」。
///
/// **M188 复审 P2-1**：这两行是**事故本体**，必须同时进 `<.ULL>/logs/launcher.log`——
/// 双击启动的 GUI 进程没有 stderr，只打 `eprintln!` 等于「最该有日志的那条路径上没有日志」。
pub fn report_failure(backend: Kind, error: &(impl std::fmt::Display + std::fmt::Debug)) {
    crate::app::diag::diag!("[gpu] {} 初始化失败（具名）：{error}", backend.name());
    crate::app::diag::diag!("[gpu] 失败详情（具名）：{error:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Backend, Probe};

    /// `run_once` **必须**把决策里的起步后端传下去。
    ///
    /// 抓得住：`run_once` 被改成传死值（例如恒 `Kind::Wgpu`）——那会让「强制 CPU」也去试 GPU。
    /// 抓不住：真 iced 启动成功/失败（那条只能真跑，见判据表第 6/8/9 行的实测记录）。
    #[test]
    fn run_once_passes_the_decision_primary() {
        for request in [Backend::Cpu, Backend::Gpu] {
            let decision = decide(request, &Probe::default(), true);
            let mut seen = None;
            let result = run_once(decision, |kind| {
                seen = Some(kind);
                Ok(())
            });
            assert!(result.is_ok(), "闭包返回 Ok 时 run_once 必须原样返回");
            assert_eq!(
                seen,
                Some(decision.primary),
                "run_once 传给 iced 的后端必须是决策里的起步项"
            );
        }
    }

    /// `run_once` 只跑一次：失败也**不**在进程内重试（winit 只允许建一次事件循环）。
    ///
    /// 抓得住：谁把「进程内重试」加回 `run_once`（会 panic：`RecreationAttempt`）。
    /// 抓不住：`respawn` 那条换进程的路（见 `app::boot::tests`）。
    #[test]
    fn run_once_never_retries_in_process() {
        let mut calls = 0;
        let _ = run_once(decide(Backend::Gpu, &Probe::default(), true), |_| {
            calls += 1;
            Err(iced::Error::WindowCreationFailed(Box::new(
                std::io::Error::other("测试用"),
            )))
        });
        assert_eq!(calls, 1, "进程内只许跑一次；重试只能换进程");
    }

    /// **M188 复审 P2-1 的守门**：后端起不来那两行必须进 `<.ULL>/logs/launcher.log`，不只是 stderr。
    ///
    /// 为什么这条最要紧：双击启动的 GUI 进程**没有 stderr 去向**，而「GPU / 渲染后端起不来」
    /// 正是最需要事后复盘的一类事故。只打 `eprintln!` 等于在这条路径上什么都没留下——
    /// 用户那句「崩了但只有一个安装日志」会在最该有日志的地方继续成立。
    ///
    /// 抓得住：`report_failure` 退回纯 `eprintln!`（变异证明：改回去本条立刻红）。
    /// 抓不住：真机上 iced 起不来的样子（那要真 GPU/驱动，见报告「没验的部分」）。
    #[test]
    fn a_backend_startup_failure_lands_in_the_launcher_log() {
        use crate::app::diag::LOG_STATE_LOCK;
        use unlucky_core::launch::launcher_log;

        // `launcher_log` 是进程级单例：与 `app/diag.rs` 的滚动闸、`app/dialog/tests.rs` 的启动事实
        // 用例串行（同一个 `LOG_STATE_LOCK`）。
        let _guard = LOG_STATE_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let dir = std::env::temp_dir().join(format!("ull-m188-gpu-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("建夹具目录");
        launcher_log::set_dir(&dir);

        report_failure(Kind::Wgpu, &"no suitable adapter (host: test)".to_string());

        let path = dir.join(launcher_log::FILE_NAME);
        let text = std::fs::read_to_string(&path).expect("launcher.log");
        assert!(
            text.contains("[gpu] wgpu 初始化失败（具名）：no suitable adapter"),
            "具名原因没进日志：{text}"
        );
        assert!(
            text.contains("[gpu] 失败详情（具名）："),
            "失败详情没进日志：{text}"
        );
        // 取证：`cargo test … -- --nocapture` 时把真实落盘内容打出来（报告/复审可复核）。
        println!("launcher.log:\n{text}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
