//! 导入**根本没进管线**时的失败文案（M336 / 台账 P31 finding②）。
//!
//! ## 这一族为什么以前是英文
//!
//! 宿主层要算出「内容解到哪个目录」才谈得上导入（`presenter/.../pack/import.rs::target_of`
//! 与 `prepare_target`）。这一步失败时，宿主手上只有一句自己拼的英文
//! （`"Target instance is not usable"`）+ 一个内部码（`"import_host"`），两个字段一起交给
//! 界面，`view/report.rs` 画成 `Target instance is not usable · import_host` ——中文界面里
//! 混一句硬编码英文，尾巴上还挂着内部 key（P31 finding②）。
//!
//! ## 现在谁产文案
//!
//! 宿主层**不产散文**（D56 + `bridge` 的 R2）：它只给**稳定码**（`import_host_occupied` /
//! `import_host_unreadable` / `import_host_create_failed` / `import_host_instance_gone`）与
//! **细节**（哪个目录 / 系统给的 IO 原文）。两者各有一个落点：**启动器日志**一行
//! `[pack] import_host_occupied: <目录>`（`pack/import.rs::report_host_failure`），以及 DTO 的
//! `code`/`message` 两格（IPC 契约；`--json` 走它）。人话由本域给，界面按码取词
//! （`packui/model/host_text.rs`）——一条句子只写一遍、而且是中英各一遍。
//!
//! ## 三条口径
//!
//! 1. **细节不是散文**：`{detail}` 里放的是路径或操作系统给的 IO 原文（`Permission denied` 这类
//!    由 OS 本地化），本仓自己不往那一格塞英文句子；
//! 2. **码不上屏**：界面那一行只有大类名（[`PackHostTargetUnusable`]）与细节句，稳定码只留在
//!    日志与 DTO 里；以前它跟在句子后面当尾巴，用户看不懂、也没法照它做什么；
//! 3. **不写反引号**：路径在句中是裸的（`catalog/tests.rs` 的反引号守卫对全表生效，界面是
//!    纯文本控件，markdown 反引号会原样上屏）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**（`java_install_entries`
//! 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，最后本域（金表只追加不重排）。

macro_rules! pack_host_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 导入没进管线：目标实例不可用（M336）
            PackHostTargetUnusable => (
                "Target instance is not usable",
                "目标实例不可用"
            ),
            PackHostOccupied => (
                "That folder already exists and is not empty: {detail}",
                "那个目录已经存在且不是空的：{detail}"
            ),
            PackHostUnreadable => (
                "The launcher could not read that folder: {detail}",
                "启动器读不出那个目录：{detail}"
            ),
            PackHostCreateFailed => (
                "The launcher could not create that folder: {detail}",
                "启动器建不出那个目录：{detail}"
            ),
            PackHostInstanceGone => (
                "That instance can no longer be resolved (it may have been removed).",
                "那个实例现在解析不出来（它可能已经被删掉了）。"
            ),
        } }
    };
}

pub(crate) use pack_host_entries;
