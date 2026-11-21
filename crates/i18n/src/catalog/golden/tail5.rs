//! 金表的第 五 段（tail4.rs 之后，M392）。
//!
//! 为什么新开一段：既有四段分别 215 / 287 / 380 / 88 行，`tail2.rs` 已贴 400 行硬线；本轮 107 行
//! （64 title + 43 hint）一段就 547 行，所以拆成 `tail4.rs`（前 54 条）与 `tail5.rs`（其余 53 条）。
//! 口径与前面四段完全一致——**只追加、不改既有行**，顺序即 `Key::ALL` 的续段；拼装见
//! `golden.rs` 的 `rows()`（少 chain 一段，`catalog/tests.rs` 会当场红）。
//!
//! 本段内容：M392 `DIAGI18N` 把 64 条诊断规则的标题/提示搬进目录（`catalog/diagnose.rs`）。
//! 这些行**不是手抄**：由 `catalog/diagnose.rs` 的条目按声明顺序机械导出（解析
//! `Variant => (en, zh)` 三元组）。金表的判据是逐字节相等，抄漏一个全角括号就会红。

pub(super) const ROWS: &[(&str, &str, &str)] = &[
    (
        "DiagnoseHintIntegrityFileAlreadyExists",
        "Delete that file as the message says (usually an empty folder someone created or leftover from unzipping)",
        "按提示删掉该文件（通常是玩家手建的空目录/unzip 残留）",
    ),
    (
        "DiagnoseTitleIntegrityModFilesDecompressed",
        "A jar in mods was extracted into a folder: the loader refuses to start",
        "mods 目录里有被解压成文件夹的 jar：加载器会直接拒绝启动",
    ),
    (
        "DiagnoseHintIntegrityModFilesDecompressed",
        "Delete the extracted folder and keep only the .jar file (many archivers extract on double-click)",
        "把解压出来的目录删掉，只保留 .jar 文件（很多压缩软件双击就解压了）",
    ),
    (
        "DiagnoseTitleIntegrityIncompleteForge",
        "The Forge installation is incomplete (forge-*-client.jar / fmlcore missing)",
        "Forge 安装不完整（缺 forge-*-client.jar / fmlcore）",
    ),
    (
        "DiagnoseHintIntegrityIncompleteForge",
        "Reinstall Forge for that version and check that libraries downloaded completely",
        "重装该版本的 Forge，并检查 libraries 是否下全",
    ),
    (
        "DiagnoseTitleIntegrityUnsatisfiedLink",
        "Native library not found (natives): {name} was never extracted or an antivirus removed it",
        "找不到原生库（natives）：{name} 没解压出来或被杀软删了",
    ),
    (
        "DiagnoseHintIntegrityUnsatisfiedLink",
        "Reinstall the component so natives get extracted again; on Windows check the antivirus quarantine",
        "重装组件（重新解压 natives）；Windows 上检查杀软隔离区",
    ),
    (
        "DiagnoseTitleJvmOpenj9",
        "This is an OpenJ9 VM: Minecraft and nearly all mods only support HotSpot (Oracle/OpenJDK)",
        "当前用的是 OpenJ9 虚拟机：Minecraft 与绝大多数 mod 只支持 HotSpot（Oracle/OpenJDK）",
    ),
    (
        "DiagnoseHintJvmOpenj9",
        "Switch to a HotSpot build of Java 8/17/21 (the hotspot builds from Adoptium, Zulu or Microsoft OpenJDK all work)",
        "请换成 HotSpot 的 Java 8/17/21（Adoptium、Zulu、Microsoft OpenJDK 的 hotspot 版本都可以）",
    ),
    (
        "DiagnoseTitleJvmNeedJdk11",
        "The mod or loader needs Java 11 and the current Java is too old",
        "模组/加载器需要 Java 11，当前 Java 版本不够",
    ),
    (
        "DiagnoseHintJvmNeedJdk11",
        "Point this instance at Java 11 or newer (1.17+ games usually need 17/21)",
        "把实例的 Java 换到 11 或更高（1.17+ 的游戏通常要 17/21）",
    ),
    (
        "DiagnoseTitleJvmTooOldJava",
        "Java is too old: it cannot load classes compiled for class file version {expected}. Use a newer Java",
        "Java 版本过低：无法加载用 class file version {expected} 编译的类，请换用更高版本的 Java",
    ),
    (
        "DiagnoseHintJvmTooOldJava",
        "Subtract 44 from the class file version to get the Java major version needed (52→Java 8, 61→Java 17)",
        "这个 class 版本号减去 44 就是所需的 Java 大版本（52→Java 8、61→Java 17）",
    ),
    (
        "DiagnoseTitleJvm32bit",
        "Heap allocation failed: usually 32-bit Java, or -Xmx is larger than the memory the system can give",
        "分配堆内存失败：多半是 32 位 Java 或 -Xmx 超过系统可用内存",
    ),
    (
        "DiagnoseHintJvm32bit",
        "32-bit Java can only use about 1.5 GB; switch to 64-bit Java or lower -Xmx below 2048 MB",
        "32 位 Java 最多只能用约 1.5 GB；换 64 位 Java 或把 -Xmx 降到 2048 MB 以下",
    ),
    (
        "DiagnoseTitleJvmJdk9",
        "This game only runs on Java 8 (Forge and legacy core libraries do not support Java 9+), and the current Java is too new",
        "游戏只能在 Java 8 上跑（Forge/旧核心库不兼容 Java 9+），当前 Java 版本太高",
    ),
    (
        "DiagnoseTitleJvmVersionTooHigh",
        "Java is too new: the old Forge or core libraries use interfaces that newer JDKs removed or encapsulated",
        "Java 版本过高：老版本 Forge/核心库用了被新 JDK 移除或封装的接口",
    ),
    (
        "DiagnoseHintJvmVersionTooHigh",
        "Use Java 8 for 1.16 and earlier, Java 17 for 1.17–1.20.4, and Java 21 for 1.20.5+",
        "1.16 及更早优先用 Java 8；1.17–1.20.4 用 Java 17；1.20.5+ 用 Java 21",
    ),
    (
        "DiagnoseTitleJvmModlauncher8",
        "ModLauncher 8 is incompatible with this Java (JDK-8273826): 1.16+ Forge crashes this way on newer JDKs",
        "ModLauncher 8 与当前 Java 不兼容（JDK-8273826）：1.16+ Forge 在新 JDK 上会这样崩",
    ),
    (
        "DiagnoseTitleJvmMacJdk8u261",
        "On macOS, JDK 8u261+ built with newer Xcode crashes Forge/OptiFine (sp614x/optifine#4824)",
        "macOS 上 JDK 8u261+ 用新 Xcode 编译导致的 Forge/OptiFine 崩溃（sp614x/optifine#4824）",
    ),
    (
        "DiagnoseHintJvmMacJdk8u261",
        "Use Java 8u252 or earlier, or upgrade Forge/OptiFine",
        "换 Java 8u252 及更早，或升级 Forge/OptiFine",
    ),
    (
        "DiagnoseTitleMemoryOutOfMemory",
        "The game ran out of memory (OutOfMemoryError): -Xmx is too small, or too many mods / too large a modpack are loaded",
        "游戏内存不足（OutOfMemoryError）：-Xmx 太小，或装的 mod 太多/整合包太大",
    ),
    (
        "DiagnoseHintMemoryOutOfMemory",
        "Set -Xmx to about half of physical memory (4096–8192 MB is typical), and raise it a bit with shaders or many mods",
        "把 -Xmx 调到物理内存的一半左右（一般 4096–8192 MB），装光影/大量 mod 时适度加大",
    ),
    (
        "DiagnoseTitleMemoryMemoryExceeded",
        "System RAM and swap are exhausted: the JVM keeps failing to allocate because the machine is out of memory",
        "系统物理内存/交换区耗尽：JVM 连续分配失败，整机内存不够",
    ),
    (
        "DiagnoseHintMemoryMemoryExceeded",
        "Close memory-hungry programs, lower -Xmx, or enlarge the system page file",
        "关掉占内存的程序、降低 -Xmx，或扩大系统虚拟内存（页面文件）",
    ),
    (
        "DiagnoseTitleConflictOptifineForge",
        "OptiFine is incompatible with Forge (or a Forge mod): the NoSuchMethodError points at a vanilla method OptiFine rewrote",
        "OptiFine 与 Forge（或某个 Forge mod）不兼容：NoSuchMethodError 指向被 OptiFine 改写的原版方法",
    ),
    (
        "DiagnoseHintConflictOptifineForge",
        "Remove OptiFine and use an alternative (Sodium/Iris, Embeddium, Oculus and similar), or switch to a matching OptiFine build",
        "删掉 OptiFine，改用同类替代（Sodium/Iris、Embeddium、Oculus 等），或换 OptiFine 的匹配版本",
    ),
    (
        "DiagnoseTitleConflictShadersMod",
        "The standalone Shaders Mod is installed, but OptiFine already ships shader support and the two cannot coexist",
        "装了独立的 Shaders Mod，但 OptiFine 自带光影支持，两者不能共存",
    ),
    (
        "DiagnoseTitleConflictMultipartOptifine",
        "The Forge Multipart coremod transformation failed (usually because OptiFine is present at the same time)",
        "Forge Multipart 的 coremod 转换失败（多为 OptiFine 同时存在导致）",
    ),
    (
        "DiagnoseTitleConflictPerformantOptifine",
        "A Performant and OptiFine mixin conflict (LivingEntityMixin failed to inject)",
        "Performant 与 OptiFine 的 mixin 冲突（LivingEntityMixin 注入失败）",
    ),
    (
        "DiagnoseTitleConflictTwilightOptifine",
        "Texture out of bounds (outside of image bounds): the classic Twilight Forest and OptiFine conflict on 1.16",
        "贴图越界（outside of image bounds）：1.16 上 Twilight Forest 与 OptiFine 的经典冲突",
    ),
    (
        "DiagnoseTitleConflictJadeOptifine",
        "Jade conflicts with OptiFine on 1.20+ (GameRendererMixin failed to inject)",
        "Jade 与 OptiFine 在 1.20+ 冲突（GameRendererMixin 注入失败）",
    ),
    (
        "DiagnoseTitleConflictNeoforgeOptifine",
        "NeoForge is incompatible with OptiFine (it refuses during launcher setup)",
        "NeoForge 与 OptiFine 不兼容（launcher setup 阶段就拒绝）",
    ),
    (
        "DiagnoseTitleConflictOptifineWorldLoad",
        "OptiFine stops the world from loading (the ChunkManager$ProxyTicketManager method was rewritten)",
        "OptiFine 导致世界无法加载（ChunkManager$ProxyTicketManager 方法被改写）",
    ),
    (
        "DiagnoseTitleModLoadingCrashedForge",
        "Forge caught {name} ({id}) throwing during load",
        "Forge 抓到 {name}（{id}）在加载时抛异常",
    ),
    (
        "DiagnoseHintModLoadingCrashedForge",
        "Disable that mod or switch to a compatible version; the same stack usually names the exact exception type",
        "禁用该 mod 或换它的兼容版本；同一条栈里往往还有具体异常类型",
    ),
    (
        "DiagnoseTitleModLoadingCrashedFabric",
        "Fabric failed while running mod {id} during the entrypoint phase",
        "Fabric 在入口点阶段执行 mod {id} 时出错",
    ),
    (
        "DiagnoseTitleModConfigFailed",
        "Mod {id} could not parse its config file {file} (usually hand-edited into an invalid state, or the format changed across versions)",
        "mod {id} 的配置文件 {file} 解析失败（多半是被手改坏或版本升级后格式变了）",
    ),
    (
        "DiagnoseHintModConfigFailed",
        "Delete that config file so the mod regenerates defaults",
        "删掉该配置文件让 mod 重新生成默认值",
    ),
    (
        "DiagnoseTitleModMixinFailure",
        "Mixin injection failed: a mod's mixin does not match the current game or another mod's code",
        "Mixin 注入失败：某个 mod 的 mixin 与当前游戏/其他 mod 的代码不匹配",
    ),
    (
        "DiagnoseHintModMixinFailure",
        "Common when a mod does not match the game version; the mixin target class in the log identifies the mod",
        "常见于 mod 与游戏版本不配套；看日志里 mixin 的 target 类名能定位到 mod",
    ),
    (
        "DiagnoseTitleModMixinApplyFailed",
        "Mod {id} failed to apply its mixin",
        "mod {id} 的 mixin 应用失败",
    ),
    (
        "DiagnoseTitleModForgeError",
        "Forge caught an unhandled exception and showed its error screen (Forge supplies the stack itself)",
        "Forge 捕获到未处理异常并已弹出错误界面（Forge 自己给了栈）",
    ),
    (
        "DiagnoseHintModForgeError",
        "Whichever mod's package the topmost at line names is the one to disable or upgrade",
        "栈顶的 at 行指向哪个 mod 的包名，就去禁用/升级那个 mod",
    ),
    (
        "DiagnoseTitleModNightConfigFixes",
        "A night-config config file is corrupted (common with the nightconfigfixes set of mods)",
        "night-config 配置文件读坏了（常见于 nightconfigfixes 相关 mod 组合）",
    ),
    (
        "DiagnoseHintModNightConfigFixes",
        "Delete the affected mod's config file, or remove nightconfigfixes",
        "删掉对应 mod 的配置文件，或移除 nightconfigfixes",
    ),
    (
        "DiagnoseTitleModInstallMixinbootstrap",
        "The MixinBootstrap prerequisite is missing (many mods need it on Forge 1.12.2)",
        "缺少 MixinBootstrap 前置（Forge 1.12.2 上很多 mod 需要它）",
    ),
    (
        "DiagnoseTitleOtherDebugCrash",
        "This crash was triggered on purpose (F3+C in game), not an actual fault",
        "这次崩溃是手动触发的（游戏里按 F3+C），不是真的故障",
    ),
    (
        "DiagnoseHintOtherDebugCrash",
        "If someone pressed export-diagnostics while testing, ignore this entry",
        "如果用户点了「导出诊断包」做测试，可以忽略这条",
    ),
    (
        "DiagnoseTitleWorldEntityTick",
        "Crashed during an entity tick: entity {type} at {location}",
        "崩在实体 tick：实体 {type}（坐标 {location}）",
    ),
    (
        "DiagnoseHintWorldEntityTick",
        "Find which mod that entity comes from (health-bar, mob-enhancement and spawn mods are the usual source), then disable it or delete the entity from the save",
        "看看这个实体来自哪个 mod（血条/怪物增强/刷怪类 mod 居多），禁用该 mod 或删掉存档里这个实体",
    ),
    (
        "DiagnoseTitleWorldBlockModel",
        "Crashed on a block ({type} at {location}): usually a mod that touches block models or textures",
        "崩在方块（{type}，坐标 {location}）：多半是方块模型/贴图相关 mod 的问题",
    ),
    (
        "DiagnoseHintWorldBlockModel",
        "Identify the mod that owns that block; a resource pack can cause it too (switch back to vanilla packs to confirm)",
        "定位该方块所属 mod；资源包也可能导致（先换回原版资源包验证）",
    ),
    (
        "DiagnoseTitleSignalAccessViolation",
        "The process exited with 0xC0000005 (access violation): usually the graphics driver or render stack crashing — update the driver or switch renderer first (exit code {exit_code} ({exit_type}), uptime {uptime_ms} ms)",
        "进程以 0xC0000005（访问违例）退出：多半是显卡驱动/渲染栈崩溃，建议先更新显卡驱动或换渲染器（退出码 {exit_code}（{exit_type}），存活 {uptime_ms} ms）",
    ),
    (
        "DiagnoseTitleSignalDllNotFound",
        "The process exited with 0xC0000135 (DLL not found): a native library is missing or the Java install is broken — reinstall the natives / Java components",
        "进程以 0xC0000135（找不到 DLL）退出：缺原生库或 Java 安装损坏，重装 natives/Java 组件",
    ),
    (
        "DiagnoseTitleSignalStackBufferOverrun",
        "The process exited with 0xC0000409 (stack buffer overrun): an internal JVM / native library fault; an hs_err file or crash report is needed to go further",
        "进程以 0xC0000409（栈缓冲区溢出）退出：JVM/原生库内部错误，需要 hs_err 或崩溃报告进一步判断",
    ),
    (
        "DiagnoseTitleSignalSigkill",
        "The process was killed with SIGKILL: usually the system ran out of memory (OOM killer) or a cleanup process killed it — lower -Xmx or enlarge the page file",
        "进程被 SIGKILL 强杀：常见于系统内存耗尽（OOM killer）或外部清理进程，建议降低 -Xmx 或扩大虚拟内存",
    ),
    (
        "DiagnoseTitleSignalJvmError",
        "The JVM never started (the game never ran at all): check whether -Xmx exceeds the available memory, and whether the JVM arguments are valid",
        "JVM 未能启动（游戏根本没跑起来）：先查 -Xmx 是否超过可用内存、JVM 参数是否非法",
    ),
    (
        "DiagnoseTitleSignalSilentFailure",
        "Exit code 0 but it looks like a silent failure ({error_lines} error lines): the game may have started and then been killed by a mod exception — a longer log is needed to confirm",
        "退出码为 0 但疑似静默失败（{error_lines} 条错误行）：游戏可能刚起来就被 mod 异常终止，需要更长的日志确认",
    ),
    (
        "DiagnoseTitleHsErrAccessViolation",
        "JVM fatal error in {module_name} (access violation, failed frame {failed_frame}): a native-layer crash — the graphics driver / render stack is the prime suspect",
        "JVM 致命错误 {module_name}（访问违例，失败帧 {failed_frame}）：原生层崩溃，显卡驱动/渲染栈是首要嫌疑",
    ),
    (
        "DiagnoseTitleHsErrOutOfMemory",
        "The JVM died of insufficient memory: physical RAM {physical_ram} (free {physical_free}), swap {swap} (free {swap_free}) — lower -Xmx or enlarge the system page file",
        "JVM 因内存不足终止：物理内存 {physical_ram}（可用 {physical_free}），交换区 {swap}（可用 {swap_free}），降低 -Xmx 或扩大系统虚拟内存",
    ),
    (
        "DiagnoseTitleHsErrStackOverflow",
        "A fatal JVM stack overflow: usually unbounded recursion or a loop inside one mod — disable the most recently installed mods and try again",
        "JVM 栈溢出导致的致命错误：通常是某个 mod 的递归/死循环，禁用最近装的 mod 再试",
    ),
    (
        "DiagnoseTitleHsErrInternalError",
        "An internal JVM fatal error: switch to an official HotSpot Java first, then suspect a corrupted native library (natives)",
        "JVM 内部致命错误：优先换官方 HotSpot Java，其次怀疑原生库（natives）损坏",
    ),
    (
        "DiagnoseBuiltinUnknownModule",
        "unknown module",
        "未知模块",
    ),
    (
        "DiagnoseBuiltinUnknownFrame",
        "unknown",
        "未知",
    ),
];
