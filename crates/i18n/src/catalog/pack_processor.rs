//! 整合包导入「加载器安装步骤」那一段的六句（M328/P30 第 2 条）。
//!
//! core 的 `ImportReport.processors` 一直有逐步骤结局（谁跑的、跑了几步、每步什么结局），
//! 但界面侧一个像素都没印——用户在「已导入 / 部分成功」之后看不到加载器安装器到底跑过哪几步、
//! 哪一步是跳过的。这一域供那段用：段头、汇总、Java 来源行、单步两档、空态。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**（`pack_java_entries`
//! 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，再 pack_java，最后本域
//! （金表只追加不重排）。
//!
//! 三条口径：
//!
//! 1. **来源不是 slug**：`{origin}` 那一位由 [`super::pack_java`] 的五条人话填（M325），
//!    本域只负责「用的是哪个 Java」这句外壳；内部标识（`managed` / `JAVA_HOME` / `PATH`）不上屏；
//! 2. **单步两档两个壳同源**（M346）：CLI 的处理器段落（`cli/commands/pack/wording.rs` 的
//!    `step_body`）读的就是 [`PackProcessorStepRan`] / [`PackProcessorStepSkipped`]——以前 CLI
//!    各处手写中文，改本域文案时命令行一个字不动。**谁在印**：`pack import` 与
//!    `install --loader`（实时行 + 收尾清单，共四个调用点）；不带 `--loader` 的 `install` 走
//!    `vanilla::run` → `install/report.rs`，那条链没有 processors 段，压根不印这两句。
//!    剩下仍各排各版的只有**汇总行外壳**与逐步骤的序号/名字排版（`PackProcessorSummary` 只负责
//!    "执行 N 步 · 跳过 N 步"那截，CLI 的汇总行是 `处理器：…` 另一种排法，还没并）；
//! 3. **单步两档各说一件不同的事**：`ran` 带上产物数与耗时（真起过 java，用户想知道花了多久），
//!    `skipped` 不带耗时也不带产物数（跳过时耗时恒为 0，印一个 0 只会误导；产物个数在
//!    `--json` 的 `processors.steps[].outputs` 里给脚本）。

macro_rules! pack_processor_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 加载器安装步骤段（M328）
            PackProcessorTitle => (
                "Loader install steps",
                "加载器安装步骤"
            ),
            PackProcessorSummary => (
                "{ran} ran · {skipped} skipped",
                "执行 {ran} 步 · 跳过 {skipped} 步"
            ),
            PackProcessorJava => (
                "ran on Java {java} ({origin})",
                "用的是 Java {java}（{origin}）"
            ),
            PackProcessorStepRan => (
                "ran · {outputs} outputs · {elapsed} ms",
                "执行完成 · 产物 {outputs} 个 · {elapsed} ms"
            ),
            PackProcessorStepSkipped => (
                "skipped · output already there",
                "跳过 · 产物已就绪"
            ),
            PackProcessorEmpty => (
                "The loader installer listed no steps.",
                "加载器安装器没有列出任何步骤。"
            ),
        } }
    };
}

pub(crate) use pack_processor_entries;
