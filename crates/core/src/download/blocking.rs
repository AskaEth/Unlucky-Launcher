//! 同步桥：让**非 async 宿主**驱动异步下载引擎。
//!
//! `core::download` 的引擎是异步的（tokio + reqwest），GUI 侧自带运行时；
//! 但 headless 消费者（`crates/cli` 的 `ull`）不依赖 tokio，也不该为一次安装
//! 把整个异步运行时依赖树拉进 CLI 的 manifest（D18：CLI 依赖树保持干净）。
//!
//! 因此把「跑完整段 future」这件事收在 core 里：
//!
//! ```ignore
//! let outcome = unlucky_core::download::blocking::block_on(async { /* 规划 + 下载 */ });
//! ```
//!
//! # 一句话约定（M165 起）
//!
//! **整个进程只有一个运行时**，所有 [`block_on`] 都跑在它上面。理由两条，都是实测口径：
//!
//! 1. **连接池绑在运行时上**。`reqwest` 的连接池条目由驱动连接的运行时任务持有：运行时一销毁，
//!    池里的空闲连接就全成了尸体。M165 之前每次 `block_on` 现建一个临时运行时，于是下载引擎
//!    的连接池**跨请求根本留不住**（每次重新 TCP + TLS）；引擎复用（[`super::shared`]）与
//!    运行时常驻必须成对出现，只做一半会退化成「复用一个装着死连接的池」。
//! 2. **建运行时就是起线程**。每个请求建一次 = 每次多起两个线程，再各自销毁。
//!
//! 调用形状不变：把**一整段**流程包在一次 `block_on` 里仍然是对的（同一个 future 里少切几次
//! 上下文，也少几次许可申请/释放）；只是这几次调用现在落在同一个运行时上，不再互相孤立。
//!
//! # 仍然不能嵌套
//!
//! 在已有异步上下文里调 [`block_on`] 会 panic（tokio 的硬规则）。这条与 M165 之前一致。

use std::future::Future;
use std::sync::OnceLock;

/// 共享运行时的 worker 数。
///
/// 下载流程里有真实的同步 I/O（`PartFile` 落盘、`write_atomic` 写索引/健康度），它们跑在
/// worker 线程上；M165 之前每个请求自带 2 个 worker，现在这些请求共用同一池，所以按机器并行度
/// 给一点余量（下限 2 保证单核机器也够驱动，上限 8 免得为一个启动器起一船线程）。
fn workers() -> usize {
    std::thread::available_parallelism()
        .map_or(2, std::num::NonZeroUsize::get)
        .clamp(2, 8)
}

/// 进程内唯一的阻塞桥运行时（第一次 [`block_on`] 时创建，之后一直活着）。
///
/// 故意不实现关闭：它的生命周期就是进程的生命周期。静态量不会被 drop，因此进程退出时
/// 也不会去等它「优雅停机」（那正是短命运行时被销毁时的开销）。
fn bridge() -> &'static tokio::runtime::Runtime {
    static BRIDGE: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    BRIDGE.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(workers())
            .thread_name("ull-blocking")
            .enable_all()
            .build()
            .expect("创建 tokio 运行时失败（内存/线程资源不足）")
    })
}

/// 在进程共享的多线程运行时上跑完 `future`（阻塞当前线程）。
///
/// 多个线程可以同时调用：它们在同一个运行时上各自阻塞自己的线程，任务在同一个 worker 池里跑，
/// 于是连接池、限流、健康度这些「跟运行时/引擎绑在一起」的东西真的能跨调用复用。
///
/// # Panics
///
/// 运行时建不起来（系统资源耗尽）时 panic：此时进程已无法继续，早点暴露比静默降级好。
/// 在已有 tokio 运行时上下文里调用同样会 panic（tokio 不允许嵌套 `block_on`）。
pub fn block_on<F: Future>(future: F) -> F::Output {
    bridge().block_on(future)
}

#[cfg(test)]
mod tests {
    use super::block_on;

    #[test]
    fn drives_a_plain_future_to_completion() {
        assert_eq!(block_on(async { 1 + 1 }), 2);
    }

    #[test]
    fn drives_tokio_timers() {
        let start = std::time::Instant::now();
        block_on(async {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        });
        assert!(start.elapsed() >= std::time::Duration::from_millis(5));
    }

    /// 共享运行时能被多个线程同时驱动（宿主侧就是多线程并行发请求）。
    #[test]
    fn shared_runtime_serves_concurrent_callers() {
        let threads: Vec<_> = (0..4)
            .map(|index| {
                std::thread::spawn(move || {
                    block_on(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                        index
                    })
                })
            })
            .collect();
        let mut seen: Vec<u32> = threads
            .into_iter()
            .map(|handle| handle.join().expect("线程不该 panic"))
            .collect();
        seen.sort_unstable();
        assert_eq!(seen, vec![0, 1, 2, 3]);
    }
}
