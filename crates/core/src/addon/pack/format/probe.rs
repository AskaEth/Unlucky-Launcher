//! **只在测试构建里存在**的限额记录钩子（p2-1，复审 r1）。
//!
//! # 为什么需要它
//!
//! M166 把夹具从 GiB 缩到 KiB 之后，「生产路径 + 真超限内容」那种顺带守住生产限额的证法没了。
//! 于是「[`super::PackFormat::write`] 到底把哪份限额交给 [`super::write_with_limits`]」
//! **没有任何测试能观测**：把它换成 `WriteLimits::new(u64::MAX, u64::MAX, …)` 会全绿
//! （复审 r1 实测 606 passed）。`the_production_limits_are_the_shared_constants` 只钉了
//! `production()` 的**返回值**，没钉 `write()` 的**调用点**。
//!
//! 所以这里加一条捕获缝：`write_with_limits` 进门时记一笔，测试走**生产**的
//! `CURSEFORGE.write(…)` 之后断言记到的就是 [`super::write::WriteLimits::production`]。
//!
//! # 三条纪律
//!
//! 1. **只在 `cfg(test)` 里编译**：发布构建里这个模块不存在，`write_with_limits` 的签名也没动；
//! 2. 记录是**线程局部**的：`cargo test` 每个用例一个线程，并行跑也不会串味；
//! 3. 这是**测试设施**，不是给生产留的"放宽限额"开关——生产路径只有
//!    [`super::write::WriteLimits::production`] 一个来源。

use std::cell::Cell;

use super::write::WriteLimits;

thread_local! {
    /// 本线程最近一次 [`super::write_with_limits`] 收到的限额。
    ///
    /// `#[allow]` 贴在**这个 `static`** 上（M170）：clippy 1.99 的
    /// `missing_const_for_thread_local` 在初始化**已经是** `const { … }` 的写法上仍然报误报
    /// （最小复现 `static A: Cell<Option<u64>> = const { Cell::new(None) };` 一样红），
    /// 所以要压掉。放在这里是最窄的作用域；反过来，外挂在 `thread_local!` **本身**上的
    /// `#[allow]` 才是无效的（被宏展开吞掉，报成 unused attribute）。
    #[allow(clippy::missing_const_for_thread_local)]
    static LAST_LIMITS: Cell<Option<WriteLimits>> = const { Cell::new(None) };
}

/// 记一笔（由 [`super::write_with_limits`] 进门时调用）。
pub(super) fn record(limits: WriteLimits) {
    LAST_LIMITS.with(|slot| slot.set(Some(limits)));
}

/// 本线程最近一次记到的限额（`None` = 还没走过 `write_with_limits`）。
pub(in crate::addon::pack) fn last_limits() -> Option<WriteLimits> {
    LAST_LIMITS.with(Cell::get)
}
