//! 导入的**取消令牌**（M195）：用户点「取消」之后，后台真的停手并回滚，而不是跑完再假装停了。
//!
//! # 取消不是失败（这是本模块存在的唯一理由）
//!
//! | 现象 | 取消 | 失败 |
//! |------|------|------|
//! | 结果类型 | [`super::report::ImportStatus::Cancelled`] | [`super::report::ImportStatus::Failed`] |
//! | `ImportReport::error` | `None` | 必有 |
//! | CLI 退出码 | `130` | `1` |
//! | UI | 面板收回 / 「已取消」，**不弹红** | 红色结果块 |
//!
//! 管线内部因此有**两条**停止路线：[`Stopped`]。它故意不是 [`super::error::ImportError`] 的
//! 第 N 个大类——把两者并成一个变体，调用方就再也分不清"用户改主意"与"这包装坏了"，
//! 而这两件事的收尾文案、退出码、重试语义全都不同。
//!
//! # 取消到底停在哪一段（逐段点名，没有检查点的段落要说明为什么）
//!
//! | 段 | 代码 | 检查点 | 为什么是这个粒度 |
//! |----|------|--------|------------------|
//! | 读包 | `PackArchive::open` → `PackRegistry::read` → `ImportPlan::build` | 入口前、**解析后**各一次 | 一次 `open` 是**不可分割**的（zip 目录 + 四检 + 限额一次扫完），取消等它返回；解析完立刻有一次，所以"用户刚点取消时包已经读完"也不会白写盘 |
//! | 校验 / 补全 | `pipeline::load_baseline`、`complete::complete` | 补全循环**每条目**一次 | 补全是逐条网络请求（按 sha512 反查 sha1），条目之间是天然的取消点 |
//! | 解压 / 写入（暂存目录） | `phase::extract_overrides` | **每条**一次 | 只写临时目录，取消 = 临时目录整个删掉，目标目录一个字节都没动 |
//! | 远端下载（暂存目录） | `phase::fetch_remote` | **每条**一次 | 单条 `PackFetcher::fetch_file` **在飞不可中断**（fetcher 接口没有取消参数），取消在它返回后的下一次检查生效 |
//! | 落盘 | `Staging::commit` | 提交前一次；合并态**逐文件**一次 | 全新导入是一次 `rename`（单步原子，之后无可取消）；合并态是逐文件 `rename` + journal，中途取消**逆序回滚**（新文件删掉、旧备份改回原名） |
//! | 登记实例 | `game::install::register` | 落盘后、登记前一次 | 内容已进目标目录，取消必须**连内容一起回滚**，否则留下"有目录没档案"的半个实例 |
//! | 提交前清中间产物 | `Staging::purge_engine_artifacts` | 与"提交前"共用一次 | 它只是暂存目录里的一次本地遍历，与 `commit` 同一条边界 |
//!
//! **预检（[`super::scan`]）不接取消**：它只读盘、不写一个字节，取消了也不会留下任何东西；
//! 给它加令牌只是让"用户点完取消还得等预检跑完"这件事更难看。
//!
//! # 与 M186 的下载取消不是一套（不许照搬）
//!
//! M186 的口径是"解析后查一次 + 下载循环每次迭代前查一次"，因为那边是**一批独立文件**的下载
//! 队列，随时可以在文件之间停。导入是**单包解压 + 两阶段落盘**：真正的风险不是"少下几个文件"，
//! 而是"临时目录里的半包被提交进目标目录"。所以这里多了两处 M186 没有的检查点——**落盘逐文件**
//! 与**登记前**——它们对应两种只能靠回滚才能收场的状态（合并态已搬了一半 / 内容已落盘但档案
//! 还没建）。照搬 M186 的两处检查会漏掉这两段，正是"取消后仍写盘"的原始 finding 形态。
//!
//! # 目标的形态（取消后的机器位）
//!
//! 报告的收尾在 [`super::report::ImportReport::cancel`]（三档注记与 `rolled_back` 都在那里）：
//!
//! - 取消发生在**落盘之前** ⇒ 临时目录已删，`rolled_back = true`、`rollback` 为 `None`；
//! - 取消发生在**落盘之中/之后** ⇒ 回滚已做，`rollback` 里有结果，`rolled_back` 只在**完整**
//!   回滚时为 `true`（回滚本身失败时如实为 `false` 并逐项列出，与失败路径同一套口径）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::error::ImportError;

/// 取消令牌：一个可跨线程共享的布尔开关。
///
/// [`Default`] 造出的是**永不取消**的独立令牌（没挂令牌的调用方零成本，不必到处写 `Option`）。
/// 要用既有 `Arc<AtomicBool>`（presenter 的 `cancel_pack` 就是这么存的）时走
/// [`ImportCancel::from_flag`]，两边共享同一个开关。
#[derive(Debug, Clone, Default)]
pub struct ImportCancel {
    flag: Arc<AtomicBool>,
}

impl ImportCancel {
    /// 一个新的、未取消的令牌。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 接管一个既有开关（共享同一份状态：外面置位，这里立刻看得见）。
    #[must_use]
    pub fn from_flag(flag: Arc<AtomicBool>) -> Self {
        Self { flag }
    }

    /// 底层开关（要交给别的线程持有时用）。
    #[must_use]
    pub fn flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.flag)
    }

    /// 置位（幂等）。
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }

    /// 是否已被取消。
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    /// 一个检查点：已取消 ⇒ [`Stopped::Cancelled`]（由 `?` 冒泡到编排层）。
    pub(crate) fn stopped(&self) -> Result<(), Stopped> {
        if self.is_cancelled() {
            Err(Stopped::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// 管线内部的两条停止路线：**取消**与**失败**。
///
/// 分开的理由见模块文档："用户改主意"不是"这包装坏了"。`From<ImportError>` 让所有既有的
/// `?`（IO / 穿越 / 摘要不符 / 限额）继续直接冒泡成 [`Stopped::Failed`]，不用把每个调用点
/// 都改成手写映射。
#[derive(Debug)]
pub(crate) enum Stopped {
    /// 用户在检查点之前点了取消。
    Cancelled,
    /// 一次必须回滚的失败。
    Failed(ImportError),
}

impl From<ImportError> for Stopped {
    fn from(error: ImportError) -> Self {
        Self::Failed(error)
    }
}

impl From<crate::addon::pack::PackError> for Stopped {
    fn from(error: crate::addon::pack::PackError) -> Self {
        Self::Failed(error.into())
    }
}

impl From<super::fetch::FetchError> for Stopped {
    fn from(error: super::fetch::FetchError) -> Self {
        Self::Failed(error.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_shared_only_with_the_flags_it_was_built_from() {
        let token = ImportCancel::new();
        assert!(!token.is_cancelled());
        token.cancel();
        assert!(token.is_cancelled(), "自己置位自己要看得见");

        let foreign = ImportCancel::new();
        assert!(!foreign.is_cancelled(), "默认令牌之间不共享状态");

        let flag = Arc::new(AtomicBool::new(false));
        let shared = ImportCancel::from_flag(Arc::clone(&flag));
        flag.store(true, Ordering::SeqCst);
        assert!(shared.is_cancelled(), "外部置位要被看见");
    }
}
