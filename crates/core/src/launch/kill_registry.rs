//! 「这个 pid 是我们主动结束的」的进程内登记簿（D10）。
//!
//! M208 实机：挂死的 1.12.2 被强杀后记成「退出码 1（application_error）」，而那一局
//! `error_lines 0`。问题不在判定表，在**信息丢失**：谁发的结束请求只有调用方知道，
//! 进程自己只会回一个 1。
//!
//! 所以把「我们杀过它」变成一条有据可查的事实：
//!
//! - 发送侧（[`crate::launch::tree_kill`]）在发信号前登记 pid，**所有**结束路径都经过那里
//!   （`RunningProcess::stop` / GUI 的 `host::launch::stop` / 看门狗兜底），不需要每个调用点
//!   自己记得标记；
//! - 判定侧（[`crate::launch::launch_session`]）在收尾时取走登记，据此判
//!   [`ExitType::Killed`](crate::launch::exit_analysis::ExitType::Killed) 而不是应用错误。
//!
//! 两条纪律：
//!
//! 1. [`take`] 是**取走**语义（判完就忘）：pid 会被复用，留着旧登记会把别的进程误判成
//!    「我们杀的」；
//! 2. 登记表有上限（[`MAX_TRACKED`]），登记了却没人消费（进程自己先退了）的条目按 FIFO 淘汰，
//!    不随运行时长无限增长。

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

/// 最多同时记住多少个 pid。
pub const MAX_TRACKED: usize = 64;

fn registry() -> &'static Mutex<VecDeque<u32>> {
    static REGISTRY: OnceLock<Mutex<VecDeque<u32>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// 登记「我们正在结束 `pid`」（重复登记不叠加）。
pub fn note(pid: u32) {
    if pid == 0 {
        return;
    }
    let Ok(mut guard) = registry().lock() else {
        return;
    };
    if guard.contains(&pid) {
        return;
    }
    while guard.len() >= MAX_TRACKED {
        guard.pop_front();
    }
    guard.push_back(pid);
}

/// 取走登记，返回此前是否登记过。
pub fn take(pid: u32) -> bool {
    let Ok(mut guard) = registry().lock() else {
        return false;
    };
    match guard.iter().position(|tracked| *tracked == pid) {
        Some(index) => {
            guard.remove(index);
            true
        }
        None => false,
    }
}

/// 撤销登记（结束动作没送达、进程其实还在时用，避免下一次误判）。
pub fn forget(pid: u32) {
    let Ok(mut guard) = registry().lock() else {
        return;
    };
    if let Some(index) = guard.iter().position(|tracked| *tracked == pid) {
        guard.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::{forget, note, take, MAX_TRACKED};

    #[test]
    fn noted_pid_is_taken_exactly_once() {
        note(4242);
        assert!(take(4242), "登记过的 pid 应判成我们杀的");
        assert!(!take(4242), "取走语义：判完就忘，避免 pid 复用误伤");
    }

    #[test]
    fn forgetting_a_pid_clears_the_note() {
        note(4243);
        forget(4243);
        assert!(!take(4243));
    }

    #[test]
    fn pid_zero_is_never_tracked() {
        // 0 在 POSIX 上是「当前进程组」，绝不能出现在登记表里。
        note(0);
        assert!(!take(0));
    }

    #[test]
    fn registry_is_bounded_and_drops_the_oldest() {
        let base = 900_000;
        for offset in 0..u32::try_from(MAX_TRACKED).unwrap() + 5 {
            note(base + offset);
        }
        assert!(!take(base), "最早的一条应被 FIFO 淘汰");
        assert!(take(base + 5), "上限之后的条目仍在表里");
    }
}
