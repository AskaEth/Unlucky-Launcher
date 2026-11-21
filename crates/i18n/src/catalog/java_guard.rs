//! 「Java 与目标版本不符」那句**启动前**的提示（M331），以及「老版本原版 + 高版本 Java」那条
//! **只是建议**的提示（M349）——两者在界面上是**同一个框**，出口也是同一套，所以同住一个域。
//!
//! 场景是用户最常撞上的那种：托管里只有 17/21，点开一个 1.12.2（只能用 Java 8）。以前界面
//! 照常起、游戏必崩（`UnsupportedClassVersionError`），用户只看到「游戏崩溃 退出码 1」。
//! 现在启动受理处就拦下来，弹这个框：**要么去设置换 Java，要么明确要试这一次**。
//!
//! 三条口径：
//!
//! 1. **不许出现内部 code**（`java_too_old` / `java_too_new`）：那是 `--json` 的字段值，
//!    用户看不懂；界面按 [`Key`](crate::catalog::Key) 选句子，方向由 presenter 的
//!    `JavaGuardDirection` 给，数字由占位符填。
//! 2. **必须说清「需要什么」与「现在是什么」**：只说「Java 版本不对」等于让用户自己猜——
//!    这条提示的全部价值就在这两个数字上。
//! 3. **两个出口都要有词**：光说不行、不给路，用户只会觉得启动器坏了（D4 的同一个理由）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节；本域**追加在链尾**
//! （`pack_java_entries` 之后），于是 `Key::ALL` 顺序 = 金表顺序 = 先 main 的全部域，最后本域
//! （金表只追加不重排）。

macro_rules! java_guard_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 启动前「Java 不符」的对话框（M331）
            JavaGuardTitle => (
                "This version needs a different Java",
                "这个版本要用另一个 Java"
            ),
            JavaGuardInstance => (
                "Instance {name} · version {version}",
                "实例 {name} · 版本 {version}"
            ),
            JavaGuardTooOld => (
                "It needs Java {min} or newer",
                "它需要 Java {min} 及以上"
            ),
            JavaGuardTooNew => (
                "It only runs on Java {max} or older",
                "它只能用 Java {max} 及以下"
            ),
            JavaGuardActual => (
                "Selected right now: Java {actual}",
                "现在用的是 Java {actual}"
            ),
            JavaGuardConsequence => (
                "Launching anyway will most likely crash the game",
                "仍要启动大概率会崩"
            ),
            JavaGuardOpenSettings => (
                "Change Java in settings",
                "去设置改 Java"
            ),
            // M349 r3 / 复审 r2 P2-1：钉过实例 Java 的那一档，第三枚按钮指向**实例**的启动参数
            // （全局设置页对这条实例无效——实例层 `launch.javaPath` 优先级更高）。
            JavaGuardInstanceArgs => (
                "This instance's launch options",
                "去实例启动参数"
            ),
            JavaGuardLaunchAnyway => ("Launch anyway", "仍要启动"),
            // ── M349：老版本**原版** + 更高版本 Java 的建议（不拦，只是先问一句）
            //
            // **命令行与界面读的就是这三条 key**（M349 r2 / 复审 P2-2）：`ull launch` 的 `建议:` 行
            // 由 `cli/commands/java_launch.rs` 用同一份目录渲染，界面由 `app/dialog/model.rs` 渲染；
            // 判据、档位与**五个数字**（含下面两个证据占位符）都来自 core 的
            // `legacy_advisory_facts`。
            //
            // 证据数字**必须**是占位符（`{measured_version}` / `{measured_java}`）：写死成字面，
            // 改 core 的 `MEASURED_LEGACY_VERSION` / `MEASURED_LEGACY_JAVA` 时两侧就会漂。
            // 守门：`catalog/tests.rs::the_advisory_templates_carry_the_evidence_as_placeholders`。
            JavaAdvisoryTitle => (
                "This old version was built for an older Java",
                "这个老版本是按更老的 Java 设计的"
            ),
            // 实测过的那一档：只说「可能能启动」，并把证据强度写在句子里（到主菜单 ≠ 能玩）。
            JavaAdvisoryMeasured => (
                "Vanilla {version} was built for Java {min}; this launch uses Java {actual}. It may still start — only {measured_version} + Java {measured_java} reached the main menu, and entering a world is unverified. Forge / LiteLoader on top makes Java {min} more likely to be needed.",
                "原版 {version} 是按 Java {min} 设计的，这次用的是 Java {actual}；可能能启动——只实测过 {measured_version} + Java {measured_java} 到主菜单，进世界未验证；加装 Forge / LiteLoader 时更可能需要 Java {min}"
            ),
            // 钉过实例 Java：先说清「为什么这里没有一键装」——装进托管目录的任何一份都不会被
            // 挑中（实例层钉死那份优先），所以这个框只给「仍要启动」与「去实例启动参数」。
            JavaAdvisoryPinned => (
                "This instance pins Java {java} in its launch options; that pick beats the global default and the managed JDKs, so there is no one-click install here.",
                "这条实例在「启动参数」里钉了 Java {java}，它优先于全局默认与托管目录——所以这里没有一键安装"
            ),
            // 没测过的那一档：不给这种安慰，直接建议换 Java（M349 的「更肯定」）。
            JavaAdvisoryUnmeasured => (
                "Vanilla {version} was built for Java {min}; this launch uses Java {actual}. This pair has never been measured (only {measured_version} + Java {measured_java} was), so Java {min} is the recommendation.",
                "原版 {version} 是按 Java {min} 设计的，这次用的是 Java {actual}；这一档没实测过（只验过 {measured_version} + Java {measured_java} 到主菜单），建议直接用 Java {min}"
            ),
        } }
    };
}

pub(crate) use java_guard_entries;
