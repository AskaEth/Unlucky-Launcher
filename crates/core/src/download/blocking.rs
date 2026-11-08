//! 同步桥：让**非 async 宿主**驱动异步下载引擎。
//!
//! `core::download` 的引擎是异步的（tokio + reqwest），GUI 侧自带运行时；
//! 但 headless 消费者（`crates/cli` 的 `ull`）不依赖 tokio，也不该为一次安装
//! 把整个异步运行时依赖树拉进 CLI 的 manifest（D18：CLI 依赖树保持干净）。
//!
//! 因此把「建一个短命运行时 + 跑完整段 future」这两件事收在 core 里：
//!
//! ```ignore
//! let outcome = unlucky_core::download::blocking::block_on(async { /* 规划 + 下载 */ });
//! ```
//!
//! 约定：**整段**安装流程包在一次 `block_on` 里（同一个运行时），不要每次下载都建一个——
//! reqwest 的连接池绑定在创建它的运行时上，跨运行时会退化甚至报「dispatch task is gone」。
//!
//! 这是给「单线程 CLI」用的逃生口，不是通用设施：app 层请直接用原生异步。

use std::future::Future;

/// 在一个临时 tokio 多线程运行时上跑完 `future`。
///
/// 运行时随调用结束销毁，因此适合「一条命令一次阻塞」的 CLI 生命周期。
///
/// # Panics
///
/// 运行时建不起来（系统资源耗尽）时 panic：此时进程已无法继续，早点暴露比静默降级好。
/// 在已有 tokio 运行时上下文里调用同样会 panic（tokio 不允许嵌套 `block_on`）。
pub fn block_on<F: Future>(future: F) -> F::Output {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .thread_name("ull-blocking")
        .enable_all()
        .build()
        .expect("创建 tokio 运行时失败（内存/线程资源不足）");
    runtime.block_on(future)
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
}
