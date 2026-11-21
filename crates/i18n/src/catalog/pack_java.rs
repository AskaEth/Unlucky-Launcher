//! 整合包导入「处理器汇总行」里那句 **Java 来源**（M325/P30 第 1 条）。
//!
//! 以前那行印的是 core 的内部 slug（`来源 managed` / `来源 JAVA_HOME`）：普通玩家看不出
//! 「该去哪个地方找这份 Java」。这一域把五个来源各翻一句人话，句子的**主语是「哪份 Java」**
//! （启动器装的 / 实例自己指定的 / 环境变量指的 / 系统路径上找到的 / 系统默认装的），
//! 于是用户读完知道该动哪里。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**（`settings_demo_label_entries`
//! 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，最后本域（金表只追加不重排）。
//! 为什么不写进 [`super::content`] 那张整合包表：那个域在链中间，往那里追加会平移其后所有 key。
//!
//! 三条口径：
//!
//! 1. **五条不许漏进 slug 原文**（`instance` / `managed` / `JAVA_HOME` / `PATH` / `system`）：
//!    那是内部标识，用户看不懂；`--json` 的 `java_origin` 仍然给这几个稳定 id（脚本要它）；
//! 2. **五条各说一件不同的事**：`PATH` 与「系统默认安装位置」是两个排查方向（前者由用户在
//!    环境变量里排过序，后者是 MSI / 官方启动器留下的），不许合并成一句「系统里的」；
//! 3. 条数就是 core 侧 `JavaOrigin` 的档数——少一条那条来源就没词可用了。

macro_rules! pack_java_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 处理器汇总行「来源 {…}」（M325）
            PackProcessorJavaFromInstance => (
                "set for this instance",
                "这个实例设置里指定的"
            ),
            PackProcessorJavaFromLauncher => (
                "installed by the launcher",
                "启动器自己装的"
            ),
            PackProcessorJavaFromEnvVar => (
                "from the Java environment variable",
                "系统的 Java 环境变量指向的"
            ),
            PackProcessorJavaFromPath => (
                "found on the system path",
                "系统路径里找到的"
            ),
            PackProcessorJavaFromSystem => (
                "from the usual install location",
                "系统默认安装位置里的"
            ),
        } }
    };
}

pub(crate) use pack_java_entries;
