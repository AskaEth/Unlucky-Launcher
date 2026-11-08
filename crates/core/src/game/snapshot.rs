//! 不可变快照 + 单写者草稿（Snapshot / Draft）。
//!
//! 依据：`DevDocs/analyse/01-architecture-overview.md` §3.3（HMCL 最值得抄的一处设计）、
//! `DevDocs/PlanDoc/01-tech-stack-and-architecture.md` 公理 2。
//!
//! 收益：**读端永远看到一整份自洽的数据**，不会出现"遍历到一半列表被改"
//! （HMCL 老式 `synchronized` 实现的典型 bug）。写端同一时刻只允许一个草稿，
//! 发布 = 指针交换，读端持有的旧快照继续有效直到自己放手。
//!
//! **实现说明**：读路径持一把共享锁，只为把 `Arc` 克隆出来（纳秒级），随后所有遍历都在锁外；
//! 真正的无锁读需要 `arc-swap` 这类依赖，P0 不引入（core 依赖面越窄越好）。
//! 与 HMCL 的另一处差异：**发布不需要 UI 线程桥接**——发布是纯指针交换，
//! 订阅通知属于 app 层（`crates/app`），core 不做线程模型假设。

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use super::error::GameError;

/// 一份不可变快照：克隆只是 `Arc` 计数，内容只读。
pub struct Snapshot<T> {
    data: Arc<T>,
}

impl<T> Snapshot<T> {
    /// 用给定内容构造快照。
    pub fn new(data: T) -> Self {
        Self {
            data: Arc::new(data),
        }
    }

    /// 读内容。
    pub fn get(&self) -> &T {
        &self.data
    }

    /// 共享同一份内容的强引用。
    pub fn shared(&self) -> Arc<T> {
        Arc::clone(&self.data)
    }
}

impl<T> Clone for Snapshot<T> {
    fn clone(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Snapshot<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Snapshot").field(&self.data).finish()
    }
}

impl<T: PartialEq> PartialEq for Snapshot<T> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

/// 写权凭证：唯一的写权持有者，`drop` 即释放（发布与放弃都走它）。
struct WriteToken<'a> {
    store: &'a AtomicBool,
}

impl<'a> WriteToken<'a> {
    fn acquire(flag: &'a AtomicBool) -> Result<Self, GameError> {
        if flag
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(GameError::DraftBusy);
        }
        Ok(Self { store: flag })
    }
}

impl Drop for WriteToken<'_> {
    fn drop(&mut self) {
        self.store.store(false, Ordering::Release);
    }
}

/// 快照仓库：持有当前快照，串行化写入。
pub struct Store<T> {
    current: RwLock<Arc<T>>,
    writing: AtomicBool,
}

impl<T: Clone> Store<T> {
    /// 以初始内容建立仓库。
    pub fn new(initial: T) -> Self {
        Self {
            current: RwLock::new(Arc::new(initial)),
            writing: AtomicBool::new(false),
        }
    }

    /// 取当前快照（读路径：一次共享锁 + `Arc` 克隆，锁内不做别的）。
    pub fn snapshot(&self) -> Snapshot<T> {
        let guard = self.current.read().unwrap_or_else(|e| e.into_inner());
        Snapshot {
            data: Arc::clone(&guard),
        }
    }

    /// 打开草稿（写会话）。同一时刻只允许一个，违反单写者约束返回
    /// [`GameError::DraftBusy`]。
    pub fn open_draft(&self) -> Result<Draft<'_, T>, GameError> {
        let token = WriteToken::acquire(&self.writing)?;
        let working = self.snapshot().get().clone();
        Ok(Draft {
            store: self,
            token,
            working,
        })
    }

    /// 整表替换（加载完成后一次性发布，仍受单写者约束）。
    pub fn replace(&self, value: T) -> Result<Snapshot<T>, GameError> {
        let mut draft = self.open_draft()?;
        *draft.edit() = value;
        Ok(draft.commit())
    }
}

/// 草稿：持有写权的可变副本。
///
/// 未 [`commit`](Draft::commit) 就 `drop` 会**放弃改动并释放写权**（不会污染快照）。
pub struct Draft<'a, T> {
    store: &'a Store<T>,
    token: WriteToken<'a>,
    working: T,
}

impl<T> Draft<'_, T> {
    /// 读草稿内容。
    pub fn view(&self) -> &T {
        &self.working
    }

    /// 改草稿内容。
    pub fn edit(&mut self) -> &mut T {
        &mut self.working
    }

    /// 发布：把草稿内容换成仓库当前快照，并释放写权。
    pub fn commit(self) -> Snapshot<T> {
        let Draft {
            store,
            token,
            working,
        } = self;
        let snapshot = Snapshot::new(working);
        {
            let mut guard = store.current.write().unwrap_or_else(|e| e.into_inner());
            *guard = snapshot.shared();
        }
        drop(token);
        snapshot
    }
}

impl<T: fmt::Debug> fmt::Debug for Draft<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Draft")
            .field("working", &self.working)
            .finish()
    }
}
