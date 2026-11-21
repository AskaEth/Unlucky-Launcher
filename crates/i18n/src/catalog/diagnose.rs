//! 64 条诊断规则的**标题与提示**（M392 `DIAGI18N`：把 `rules.d/*.yaml` 的中文原文搬进目录），
//! 外加 10 条**引擎内置结论**的标题（M395 `BUILTIN10`：`signal.*` ×6 + `hs_err.*` ×4）。
//!
//! ## 为什么这一域存在
//!
//! 规则表 `crates/core/src/diagnose/rules.d/*.yaml` 的 `cause.summary` / `cause.detail` 是**中文原文**，
//! 而诊断卡直接把它们画在界面上 ⇒ 英文界面里会冒出整句中文（`GAPS.md` §2.6 的 N3，i18n 到 100%
//! 的最后一公里）。规则表同时**早就有**机器可读的 `cause.key`（`diagnose.cause.<rule id>`，64/64），
//! 所以缺的不是标识，是「标识 → 中英文案」这一层：就是本文件。
//!
//! 内置结论（10 条）没有 YAML：文案是 `core/src/diagnose/synthetic.rs` 用 `format!` 拼的，
//! 带动态片段（退出码摘要 / 错误行数 / hs_err 的模块名与失败帧 / 内存段）。M392 时它们只做到
//! 「中文直出 + 英文占位」，M395 把动态片段提成 core 的 `Finding::groups` 具名参数，于是这 10 条
//! 也能进目录、也走模板。它们的 zh 侧同样是**副本**（逐字等于 core 的 `message`），守门见下。
//!
//! ## 键名规则（不许另起一套）
//!
//! 沿用 `cause.key` 的命名族，只换第二段名，`<rule id>` **原样**：
//!
//! ```text
//! cause.key : diagnose.cause.<rule id>     ← core 的规则表里已有（M16 起，64/64）
//! title key : diagnose.title.<rule id>
//! hint  key : diagnose.hint.<rule id>      ← 64 条规则都有（M400 补齐了最后 21 条）
//! ```
//!
//! 例：规则 `integrity.file_changed` → [`Key::DiagnoseTitleIntegrityFileChanged`] /
//! [`Key::DiagnoseHintIntegrityFileChanged`]。枚举变体名 = `Diagnose` + `Title`/`Hint` +
//! `<rule id>` 的 PascalCase（`.` 与 `_` 都当分词边界，首段首字母大写、其余小写），
//! **机械推导、无例外**——于是「rule id ↔ 变体名」不需要任何人工对照表。
//!
//! 内置结论同一条规则：`signal.silent_failure` → `DiagnoseTitleSignalSilentFailure`，
//! `hs_err.access_violation` → `DiagnoseTitleHsErrAccessViolation`。它们**没有 hint**
//! （core 的 `format!` 只有一句，规则表里也没有 `detail`——缺的**不补编**）。
//! 另有两条**不是**规则 id 推导的键：`DiagnoseBuiltinUnknownModule` / `DiagnoseBuiltinUnknownFrame`，
//! 是参数值为空时补的「未知模块 / 未知」。
//!
//! ## 140 条 = 128 条规则文案 + 10 条内置结论 + 2 条兜底词
//!
//! 128 = **64 条** title ＋ **64 条** hint（规则表 64 条，一比一；title 是 M392 的，hint 是
//! M392 的 43 条 + M400 补的 21 条）；10 条内置结论（`signal.*` ×6 + `hs_err.*` ×4）只有 title；
//! 2 条兜底词（`DiagnoseBuiltinUnknownModule` / `DiagnoseBuiltinUnknownFrame`）是参数值为空时
//! 补的「未知模块 / 未知」。
//!
//! M392 只搬了 YAML 里**已有**的 `detail`（43 条），剩下 21 条当时留成 `None`：「规则说明是
//! 诊断结论的一部分，编一句等于让启动器对它没研究过的问题给建议」。**M400 把这 21 条补齐了**，
//! 每条都指向一个能照着做的动作，依据按可信度分三档：
//!
//! - **A · HMCL 官方源码**：`game.crash.reason.<HMCL 枚举名>` 那几句说明（本仓 `rules.d/*.yaml`
//!   的 `source:` 指的正是那些枚举）——降 Fabric Loader 到 0.11.7、Forge 36.2.26+、换 OptiFine
//!   jar、换低分辨率资源包、装 Java 8、删独立的 Shaders 模组、装 MixinBootstrap（还崩就给
//!   文件名加 `!`）。
//!   ⚠ **HMCL 的键名多数与 rule id 同名，少数不同，去 grep 以枚举名为准**，至少这 4 条不同：
//!   `fabric.loader_0_12` → `fabric_version_0_12`、`jvm.jdk_9` → `jdk_9`、
//!   `mod.install_mixinbootstrap` → `install_mixinbootstrap`、
//!   `conflict.optifine_world_load` → `optifine_causes_the_world_to_fail_to_load`。
//!   **抄的是说法，不是它的界面**：HMCL 的「自动安装」在 ULL 里不存在（
//!   `pages/download/options/model.rs` 的 `LoaderKind::OptiFine => None`，「OptiFine 不是本页
//!   选项：它没有安装管线」），所以凡涉及操作位置的句子都必须换成 ULL 里真实存在的路径。
//! - **A · 本仓规则表**：`pattern` 抓到的那个 mod / 路径 / 类名，加上 `fix_action.mod_ids` 点名的
//!   就是该卸的那个——`forge.duplicated_mod`、`forge.bootstrap_failed`、`mod.loading_crashed_fabric`、
//!   `mod.mixin_apply_failed`、`fabric.mod_resolution_*`、`integrity.illegal_access`、
//!   `fabric.warnings`、`conflict.multipart_optifine`、`conflict.performant_optifine`。
//! - **B · 社区/模组页**：OptiFine 与 NeoForge 互斥、1.20+ 用 Embeddium + Oculus 替代 OptiFine、
//!   不想换 OptiFine 就换掉 Jade（WTHIT 是同类）。这一档只用来**点名替代品**，具体版本仍由用户
//!   按自己的游戏版本选。
//!
//! 每条都写「下一步删哪个/换哪个版本/点哪里」，不复述结论，也不把规则 id 写进文案。**文案里出现的
//! 每个操作位置（屏幕名、按钮名、字段名）都必须在 ULL 里真实存在**——写之前去对应页面 grep 一次，
//! 找不到就换说法（`jvm.jdk_9` 的「启动参数 → Java 可执行文件」就是照 `InstancesLaunchTitle` /
//! `InstancesLaunchJava` 这两个真 key 写的）。
//!
//! 这 21 条的 zh 与 YAML 的 `cause.detail` **逐字相等**（同一条句子存两份，只改一边就红：
//! `app-iced/src/pages/logs/diagnostic_tests.rs` 的 `diagnose_catalog_matches_the_rule_table`），
//! en 是译文。**追加在链尾而不是插在各自 title 后面**：`Key::ALL` 的顺序就是金表顺序，插在中间
//! 会平移其后所有 key（金表整片红）——本仓既定做法是「只追加不重排」（见 `catalog.rs` 域装配）。
//!
//! ## 中文侧是**副本**，不是真相源
//!
//! 这里的 zh 逐字等于 YAML 的 `cause.summary` / `cause.detail`（替换 `{group}` 占位符前后都相等）。
//! 真相源仍然是 YAML（core 的 `message` / `message_key` 从那里出，喂 AI / CLI / 会话 JSONL）。
//! 两份会漂——守门在 `crates/app-iced/src/pages/logs/diagnostic_tests.rs` 的
//! `diagnose_catalog_matches_the_rule_table`（那里同时看得见 `unlucky_core` 与 `unlucky_i18n`；
//! `unlucky-presenter` 看不见 i18n，`unlucky-core` 的 `Cargo.toml` 不在本 mission 的 scope 里）。
//!
//! 内置结论那 10 条的真相源是 `core/src/diagnose/synthetic.rs` 的 `format!`，同一条纪律：
//! 把 core 的 `Finding::groups` 代入本文件的 zh 模板后必须与 core 的 `message` **逐字相等**，
//! 守门是同目录的 `builtin_conclusions_read_the_same_in_chinese`（M395 新增，覆盖 10/10 +
//! 「模块未知」那一支）。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。

macro_rules! diagnose_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 诊断规则标题与提示（M392 DIAGI18N）──
            // 顺序 = `rules.d/<category>.yaml` 的规则顺序（fabric / forge / graphics / integrity /
            // jvm / memory / mod_conflict / mod_runtime / other / world），每条规则先 title 后 hint。
            // ── fabric（8 条 / 6 hint）──
            DiagnoseTitleFabricModResolution => ("Fabric could not resolve mod dependencies (missing prerequisite, version conflict, or mismatched mods)", "Fabric 解析 mod 依赖失败（缺依赖/版本冲突/模组不配套）"),
            DiagnoseHintFabricModResolution => ("Work through the mod list below one by one: install missing prerequisites, drop one side of a conflict", "按下方列出的 mod 名单逐个处理：缺依赖就补装，冲突就删一个"),
            DiagnoseTitleFabricModResolutionConflict => ("Fabric reports a mod conflict: {sourcemod} and {destmod} cannot coexist", "Fabric 报告 mod 冲突：{sourcemod} 与 {destmod} 不能共存"),
            DiagnoseHintFabricModResolutionConflict => ("Keep only one of them, or upgrade both to versions that work together", "两个只能留一个（或都升级到互相兼容的版本）"),
            DiagnoseTitleFabricModResolutionMissing => ("Missing prerequisite mod: {sourcemod} needs {destmod}", "缺少前置 mod：{sourcemod} 需要 {destmod}"),
            DiagnoseHintFabricModResolutionMissing => ("Install the matching version of {destmod}; if that is impossible, remove {sourcemod}", "装上 {destmod} 的对应版本；装不上就删掉 {sourcemod}"),
            DiagnoseTitleFabricModResolutionMissingMinecraft => ("{mod} requires Minecraft {version}, which does not match this instance's version", "{mod} 要求 Minecraft {version}，与当前实例版本不一致"),
            DiagnoseTitleFabricModResolutionCollection => ("The mod set cannot be resolved: no version combination of {sourcemod} and {destmod} works", "mod 集合无法解析：{sourcemod} 与 {destmod} 的版本组合不成立"),
            DiagnoseTitleFabricModResolutionDetails => ("Fabric printed mod resolution details (Mod File / Failure message): the conclusion is in the rule above", "Fabric 打印了 mod 解析明细（Mod File / Failure message）：具体结论见上方规则"),
            DiagnoseHintFabricModResolutionDetails => ("This entry only says the log holds resolution failure details; it points the evidence at the AI or the user", "这条只表示「日志里有解析失败明细」，用于把证据片段指给 AI/用户"),
            DiagnoseTitleFabricLoader012 => ("Fabric Loader 0.12 is incompatible with old mixin classes (the loader removed deprecated classes)", "Fabric Loader 0.12 与旧 mixin 类不兼容（loader 删掉了 deprecated 类）"),
            DiagnoseTitleFabricWarnings => ("Fabric reports an incompatible mod set: {reason}", "Fabric 报告 mod 集合不兼容：{reason}"),
            // ── forge（8 条 / 6 hint）──
            DiagnoseTitleForgeMissingDependencies => ("Forge reports a missing or unsupported required dependency: some mod's prerequisite is absent or the wrong version", "Forge 报「缺少或不支持强制依赖」：有 mod 的前置没装或版本不对"),
            DiagnoseHintForgeMissingDependencies => ("Install the prerequisites named in the error; if they cannot be installed, remove the mod that needs them", "按报错里的 mod 名补齐前置；装不上就删掉依赖它的 mod"),
            DiagnoseTitleForgeDuplicateMods => ("The same mod is installed twice (duplicate jars in the mods directory)", "同一个 mod 装了两份（mods 目录里重复的 jar）"),
            DiagnoseHintForgeDuplicateMods => ("Delete the duplicate and keep a single version", "删掉重复的那一份，只留一个版本"),
            DiagnoseTitleForgeDuplicatedMod => ("Duplicate mod: {name} also appears at {path}", "mod 重复：{name} 同时出现在 {path}"),
            DiagnoseTitleForgeRepeatInstallation => ("Forge is installed twice (the same version shows up with several install entries)", "Forge 被装了两次（同一版本出现多个安装参数）"),
            DiagnoseHintForgeRepeatInstallation => ("Delete the duplicate Forge install or version JSON, then install it once more", "删掉重复的 Forge 安装/版本 JSON，再重装一次"),
            DiagnoseTitleForgeOptifineRepeatInstallation => ("The OptiFine module name is registered twice (usually two OptiFine jars in the mods directory)", "OptiFine 的模块名被注册了两次（多半是 mods 目录里有两个 OptiFine jar）"),
            DiagnoseTitleForgeTooManyMods => ("Too many mods installed: block and item IDs overflowed (the classic 1.12-and-earlier limit)", "装的 mod 太多，方块/物品 ID 超出上限（1.12 及更早的经典问题）"),
            DiagnoseHintForgeTooManyMods => ("Remove some mods, or move to 1.13+ where the ID limit no longer exists", "删掉一部分 mod，或换用 1.13+ 的版本（不再有 ID 上限）"),
            DiagnoseTitleForgeModuleNameInvalid => ("A mod jar's file name has no ASCII letters or digits, so Forge reads it as an invalid JPMS module name (e.g. a jar named only in Chinese characters)", "某个 mod 的 jar 文件名不含英文字母/数字，Forge 把它当成非法 JPMS 模块名（如「拔刀剑.jar」）"),
            DiagnoseHintForgeModuleNameInvalid => ("Rename that jar to an ASCII file name (keep the .jar suffix) and launch again", "把该 jar 改成英文文件名（保留 .jar 后缀）再启动"),
            DiagnoseTitleForgeBootstrapFailed => ("Instantiating mod {id} failed (it throws during initialisation)", "mod {id} 的实例创建失败（初始化就抛异常）"),
            // ── graphics（11 条 / 7 hint）──
            DiagnoseTitleGraphicsGlOperationFailure => ("A mod or shader pack issued an illegal OpenGL call (GL_INVALID_OPERATION)", "某个 mod/光影包做了非法的 OpenGL 操作（GL_INVALID_OPERATION）"),
            DiagnoseHintGraphicsGlOperationFailure => ("Swap out the shader or resource pack you added last, or lower the resource pack resolution", "换掉最近装的光影/资源包，或把资源包分辨率降下来"),
            DiagnoseTitleGraphicsOpenglNotSupported => ("The graphics driver does not support OpenGL (usually no driver installed, so it fell back to software rendering)", "显卡驱动不支持 OpenGL（多半是没装显卡驱动，走了软件渲染）"),
            DiagnoseHintGraphicsOpenglNotSupported => ("Install or update the vendor's graphics driver; inside a virtual machine, enable 3D acceleration", "装/更新显卡厂商驱动；虚拟机里需要开 3D 加速"),
            DiagnoseTitleGraphicsGraphicsDriver => ("Creating an OpenGL context failed on the driver side (pixel format or a driver crash)", "显卡驱动侧创建 OpenGL 上下文失败（像素格式/驱动崩溃）"),
            DiagnoseHintGraphicsGraphicsDriver => ("Update the graphics driver and reboot; on old cards use software rendering or OpenGL compatibility mode", "更新显卡驱动并重启；老显卡可用软渲染或换 OpenGL 兼容模式"),
            DiagnoseTitleGraphicsMacosDisplayPort => ("GLFW could not create a window on macOS (Cocoa found no display service port)", "macOS 上 GLFW 初始化窗口失败（Cocoa 找不到 display service port）"),
            DiagnoseHintGraphicsMacosDisplayPort => ("Usually happens in remote-desktop or headless sessions; log in locally or update macOS and try again", "多见于远程桌面/无显示器会话；换成本机登录或升级 macOS 后再试"),
            DiagnoseTitleGraphicsResolutionTooHigh => ("The resource pack resolution is too high (the game itself asks for a lower-resolution pack)", "资源包分辨率过高（游戏直接提示换低分辨率资源包）"),
            DiagnoseTitleGraphicsRtssSodium => ("RTSS (MSI Afterburner's frame-rate overlay) is incompatible with Sodium", "RTSS（微星小飞机/帧数显示）与 Sodium 不兼容"),
            DiagnoseHintGraphicsRtssSodium => ("Turn off or uninstall the RTSS OSD overlay, or switch to a Sodium-compatible frame counter", "关掉或卸载 RTSS 的 OSD 覆盖层，或改用 Sodium 兼容的帧数显示"),
            DiagnoseTitleGraphicsVulkanLoaderMissing => ("No Vulkan loader on this machine (the Vulkan backend cannot start, so the game fell back to OpenGL)", "机器上没有 Vulkan loader（Vulkan 后端起不来，游戏已回落 OpenGL）"),
            DiagnoseHintGraphicsVulkanLoaderMissing => ("Install or update the graphics driver (Windows needs vulkan-1.dll, Linux needs libvulkan.so.1); if that is impossible, use the OpenGL backend", "装/更新显卡驱动（Windows 需要 vulkan-1.dll，Linux 需要 libvulkan.so.1）；装不上就改用 OpenGL 后端"),
            DiagnoseTitleGraphicsVulkanDeviceVersionTooLow => ("The GPU and driver support a Vulkan version below 1.2 (the Vulkan backend cannot start)", "显卡/驱动支持的 Vulkan 版本低于 1.2（Vulkan 后端起不来）"),
            DiagnoseHintGraphicsVulkanDeviceVersionTooLow => ("Update the graphics driver; cards whose drivers stopped updating (10+ years old) have no Vulkan 1.2 and must stay on OpenGL", "更新显卡驱动；驱动停更的老卡（10 年以上）没有 Vulkan 1.2，只能继续用 OpenGL"),
            DiagnoseTitleGraphicsVulkanMissingExtension => ("The driver lacks required Vulkan extensions (VK_KHR_dynamic_rendering / VK_KHR_push_descriptor)", "驱动缺 Vulkan 必需扩展（VK_KHR_dynamic_rendering / VK_KHR_push_descriptor）"),
            DiagnoseHintGraphicsVulkanMissingExtension => ("The MC 26.2+ Vulkan backend needs VK 1.2 + dynamic_rendering + push_descriptor and will not run without any one of them; Mesa Dozen lacks push_descriptor too", "MC 26.2+ 的 Vulkan 后端要求 VK 1.2 + dynamic_rendering + push_descriptor，三者缺一就跑不了；Mesa Dozen 也不支持 push_descriptor"),
            DiagnoseTitleGraphicsVulkanMissingFeature => ("The driver lacks a required Vulkan feature set (VULKAN_CORE_1_2)", "驱动缺 Vulkan 必需特性集（VULKAN_CORE_1_2）"),
            DiagnoseHintGraphicsVulkanMissingFeature => ("Same as above: the driver is not capable enough. Common on old integrated GPUs and in virtual machines; use the OpenGL backend", "同上，属于驱动能力不足；老核显/虚拟机里常见，用 OpenGL 后端即可"),
            DiagnoseTitleGraphicsNoGraphicsBackend => ("Both the Vulkan and the OpenGL backend failed (26.2+ has no other path)", "Vulkan 与 OpenGL 两个后端都创建失败（26.2+ 只有这两条路）"),
            DiagnoseHintGraphicsNoGraphicsBackend => ("Neither the hardware nor the driver qualifies: update the graphics driver first, enable 3D acceleration in virtual machines, and if that fails the machine has to change", "硬件/驱动两侧都不满足：先更新显卡驱动；虚拟机需要开 3D 加速；实在不行只能换机器"),
            // ── integrity（8 条 / 4 hint）──
            DiagnoseTitleIntegrityFileChanged => ("A game file was modified (SHA1 mismatch): the jar contents do not match the manifest hash", "游戏文件被改过（SHA1 校验不一致）：jar 内容与清单哈希对不上"),
            DiagnoseHintIntegrityFileChanged => ("Download or verify that file again; do not edit anything inside the jar by hand", "重新下载/校验该文件；不要手改 jar 里的内容"),
            DiagnoseTitleIntegrityNoSuchMethod => ("NoSuchMethodError: what the loader or a coremod injects does not match the library versions in use", "NoSuchMethodError：加载器/coremod 注入的东西与当前库版本不匹配"),
            DiagnoseHintIntegrityNoSuchMethod => ("Common when a mod does not match the game or Forge version; reinstall the loader or remove the conflicting mod", "常见于 mod 与游戏版本/Forge 版本不配套；重装加载器或删掉冲突 mod"),
            DiagnoseTitleIntegrityNoClassDefFound => ("NoClassDefFoundError: missing class files (libraries not fully downloaded, a jar was extracted, or a mismatched mod)", "NoClassDefFoundError：缺少类文件（库没下全 / jar 被解压 / mod 不配套）"),
            DiagnoseHintIntegrityNoClassDefFound => ("Check that libraries is complete first, then look for extracted folders inside mods", "先检查 libraries 是否完整，再排查 mods 里的解压目录"),
            DiagnoseTitleIntegrityIllegalAccess => ("IllegalAccessError: a coremod tries to reflect into another class's internals (sealed off on newer JDKs)", "IllegalAccessError：某个 coremod 想反射访问别的类的内部成员（在新 JDK 上被封了）"),
            DiagnoseTitleIntegrityFileAlreadyExists => ("A mod expects a file or directory that should not exist there, so writing it failed", "某个 mod 要求一个「不该存在」的文件/目录，导致写入失败"),
            DiagnoseHintIntegrityFileAlreadyExists => ("Delete that file as the message says (usually an empty folder someone created or leftover from unzipping)", "按提示删掉该文件（通常是玩家手建的空目录/unzip 残留）"),
            DiagnoseTitleIntegrityModFilesDecompressed => ("A jar in mods was extracted into a folder: the loader refuses to start", "mods 目录里有被解压成文件夹的 jar：加载器会直接拒绝启动"),
            DiagnoseHintIntegrityModFilesDecompressed => ("Delete the extracted folder and keep only the .jar file (many archivers extract on double-click)", "把解压出来的目录删掉，只保留 .jar 文件（很多压缩软件双击就解压了）"),
            DiagnoseTitleIntegrityIncompleteForge => ("The Forge installation is incomplete (forge-*-client.jar / fmlcore missing)", "Forge 安装不完整（缺 forge-*-client.jar / fmlcore）"),
            DiagnoseHintIntegrityIncompleteForge => ("Reinstall Forge for that version and check that libraries downloaded completely", "重装该版本的 Forge，并检查 libraries 是否下全"),
            DiagnoseTitleIntegrityUnsatisfiedLink => ("Native library not found (natives): {name} was never extracted or an antivirus removed it", "找不到原生库（natives）：{name} 没解压出来或被杀软删了"),
            DiagnoseHintIntegrityUnsatisfiedLink => ("Reinstall the component so natives get extracted again; on Windows check the antivirus quarantine", "重装组件（重新解压 natives）；Windows 上检查杀软隔离区"),
            // ── jvm（8 条 / 6 hint）──
            DiagnoseTitleJvmOpenj9 => ("This is an OpenJ9 VM: Minecraft and nearly all mods only support HotSpot (Oracle/OpenJDK)", "当前用的是 OpenJ9 虚拟机：Minecraft 与绝大多数 mod 只支持 HotSpot（Oracle/OpenJDK）"),
            DiagnoseHintJvmOpenj9 => ("Switch to a HotSpot build of Java 8/17/21 (the hotspot builds from Adoptium, Zulu or Microsoft OpenJDK all work)", "请换成 HotSpot 的 Java 8/17/21（Adoptium、Zulu、Microsoft OpenJDK 的 hotspot 版本都可以）"),
            DiagnoseTitleJvmNeedJdk11 => ("The mod or loader needs Java 11 and the current Java is too old", "模组/加载器需要 Java 11，当前 Java 版本不够"),
            DiagnoseHintJvmNeedJdk11 => ("Point this instance at Java 11 or newer (1.17+ games usually need 17/21)", "把实例的 Java 换到 11 或更高（1.17+ 的游戏通常要 17/21）"),
            DiagnoseTitleJvmTooOldJava => ("Java is too old: it cannot load classes compiled for class file version {expected}. Use a newer Java", "Java 版本过低：无法加载用 class file version {expected} 编译的类，请换用更高版本的 Java"),
            DiagnoseHintJvmTooOldJava => ("Subtract 44 from the class file version to get the Java major version needed (52→Java 8, 61→Java 17)", "这个 class 版本号减去 44 就是所需的 Java 大版本（52→Java 8、61→Java 17）"),
            DiagnoseTitleJvm32bit => ("Heap allocation failed: usually 32-bit Java, or -Xmx is larger than the memory the system can give", "分配堆内存失败：多半是 32 位 Java 或 -Xmx 超过系统可用内存"),
            DiagnoseHintJvm32bit => ("32-bit Java can only use about 1.5 GB; switch to 64-bit Java or lower -Xmx below 2048 MB", "32 位 Java 最多只能用约 1.5 GB；换 64 位 Java 或把 -Xmx 降到 2048 MB 以下"),
            DiagnoseTitleJvmJdk9 => ("This game only runs on Java 8 (Forge and legacy core libraries do not support Java 9+), and the current Java is too new", "游戏只能在 Java 8 上跑（Forge/旧核心库不兼容 Java 9+），当前 Java 版本太高"),
            DiagnoseTitleJvmVersionTooHigh => ("Java is too new: the old Forge or core libraries use interfaces that newer JDKs removed or encapsulated", "Java 版本过高：老版本 Forge/核心库用了被新 JDK 移除或封装的接口"),
            DiagnoseHintJvmVersionTooHigh => ("Use Java 8 for 1.16 and earlier, Java 17 for 1.17–1.20.4, and Java 21 for 1.20.5+", "1.16 及更早优先用 Java 8；1.17–1.20.4 用 Java 17；1.20.5+ 用 Java 21"),
            DiagnoseTitleJvmModlauncher8 => ("ModLauncher 8 is incompatible with this Java (JDK-8273826): 1.16+ Forge crashes this way on newer JDKs", "ModLauncher 8 与当前 Java 不兼容（JDK-8273826）：1.16+ Forge 在新 JDK 上会这样崩"),
            DiagnoseTitleJvmMacJdk8u261 => ("On macOS, JDK 8u261+ built with newer Xcode crashes Forge/OptiFine (sp614x/optifine#4824)", "macOS 上 JDK 8u261+ 用新 Xcode 编译导致的 Forge/OptiFine 崩溃（sp614x/optifine#4824）"),
            DiagnoseHintJvmMacJdk8u261 => ("Use Java 8u252 or earlier, or upgrade Forge/OptiFine", "换 Java 8u252 及更早，或升级 Forge/OptiFine"),
            // ── memory（2 条 / 2 hint）──
            DiagnoseTitleMemoryOutOfMemory => ("The game ran out of memory (OutOfMemoryError): -Xmx is too small, or too many mods / too large a modpack are loaded", "游戏内存不足（OutOfMemoryError）：-Xmx 太小，或装的 mod 太多/整合包太大"),
            DiagnoseHintMemoryOutOfMemory => ("Set -Xmx to about half of physical memory (4096–8192 MB is typical), and raise it a bit with shaders or many mods", "把 -Xmx 调到物理内存的一半左右（一般 4096–8192 MB），装光影/大量 mod 时适度加大"),
            DiagnoseTitleMemoryMemoryExceeded => ("System RAM and swap are exhausted: the JVM keeps failing to allocate because the machine is out of memory", "系统物理内存/交换区耗尽：JVM 连续分配失败，整机内存不够"),
            DiagnoseHintMemoryMemoryExceeded => ("Close memory-hungry programs, lower -Xmx, or enlarge the system page file", "关掉占内存的程序、降低 -Xmx，或扩大系统虚拟内存（页面文件）"),
            // ── mod_conflict（8 条 / 5 hint）──
            DiagnoseTitleConflictOptifineForge => ("OptiFine is incompatible with Forge (or a Forge mod): the NoSuchMethodError points at a vanilla method OptiFine rewrote", "OptiFine 与 Forge（或某个 Forge mod）不兼容：NoSuchMethodError 指向被 OptiFine 改写的原版方法"),
            DiagnoseHintConflictOptifineForge => ("Remove OptiFine and use an alternative (Sodium/Iris, Embeddium, Oculus and similar), or switch to a matching OptiFine build", "删掉 OptiFine，改用同类替代（Sodium/Iris、Embeddium、Oculus 等），或换 OptiFine 的匹配版本"),
            DiagnoseTitleConflictShadersMod => ("The standalone Shaders Mod is installed, but OptiFine already ships shader support and the two cannot coexist", "装了独立的 Shaders Mod，但 OptiFine 自带光影支持，两者不能共存"),
            DiagnoseTitleConflictMultipartOptifine => ("The Forge Multipart coremod transformation failed (usually because OptiFine is present at the same time)", "Forge Multipart 的 coremod 转换失败（多为 OptiFine 同时存在导致）"),
            DiagnoseTitleConflictPerformantOptifine => ("A Performant and OptiFine mixin conflict (LivingEntityMixin failed to inject)", "Performant 与 OptiFine 的 mixin 冲突（LivingEntityMixin 注入失败）"),
            DiagnoseTitleConflictTwilightOptifine => ("Texture out of bounds (outside of image bounds): the classic Twilight Forest and OptiFine conflict on 1.16", "贴图越界（outside of image bounds）：1.16 上 Twilight Forest 与 OptiFine 的经典冲突"),
            DiagnoseTitleConflictJadeOptifine => ("Jade conflicts with OptiFine on 1.20+ (GameRendererMixin failed to inject)", "Jade 与 OptiFine 在 1.20+ 冲突（GameRendererMixin 注入失败）"),
            DiagnoseTitleConflictNeoforgeOptifine => ("NeoForge is incompatible with OptiFine (it refuses during launcher setup)", "NeoForge 与 OptiFine 不兼容（launcher setup 阶段就拒绝）"),
            DiagnoseTitleConflictOptifineWorldLoad => ("OptiFine stops the world from loading (the ChunkManager$ProxyTicketManager method was rewritten)", "OptiFine 导致世界无法加载（ChunkManager$ProxyTicketManager 方法被改写）"),
            // ── mod_runtime（8 条 / 3 hint）──
            DiagnoseTitleModLoadingCrashedForge => ("Forge caught {name} ({id}) throwing during load", "Forge 抓到 {name}（{id}）在加载时抛异常"),
            DiagnoseHintModLoadingCrashedForge => ("Disable that mod or switch to a compatible version; the same stack usually names the exact exception type", "禁用该 mod 或换它的兼容版本；同一条栈里往往还有具体异常类型"),
            DiagnoseTitleModLoadingCrashedFabric => ("Fabric failed while running mod {id} during the entrypoint phase", "Fabric 在入口点阶段执行 mod {id} 时出错"),
            DiagnoseTitleModConfigFailed => ("Mod {id} could not parse its config file {file} (usually hand-edited into an invalid state, or the format changed across versions)", "mod {id} 的配置文件 {file} 解析失败（多半是被手改坏或版本升级后格式变了）"),
            DiagnoseHintModConfigFailed => ("Delete that config file so the mod regenerates defaults", "删掉该配置文件让 mod 重新生成默认值"),
            DiagnoseTitleModMixinFailure => ("Mixin injection failed: a mod's mixin does not match the current game or another mod's code", "Mixin 注入失败：某个 mod 的 mixin 与当前游戏/其他 mod 的代码不匹配"),
            DiagnoseHintModMixinFailure => ("Common when a mod does not match the game version; the mixin target class in the log identifies the mod", "常见于 mod 与游戏版本不配套；看日志里 mixin 的 target 类名能定位到 mod"),
            DiagnoseTitleModMixinApplyFailed => ("Mod {id} failed to apply its mixin", "mod {id} 的 mixin 应用失败"),
            DiagnoseTitleModForgeError => ("Forge caught an unhandled exception and showed its error screen (Forge supplies the stack itself)", "Forge 捕获到未处理异常并已弹出错误界面（Forge 自己给了栈）"),
            DiagnoseHintModForgeError => ("Whichever mod's package the topmost at line names is the one to disable or upgrade", "栈顶的 at 行指向哪个 mod 的包名，就去禁用/升级那个 mod"),
            DiagnoseTitleModNightConfigFixes => ("A night-config config file is corrupted (common with the nightconfigfixes set of mods)", "night-config 配置文件读坏了（常见于 nightconfigfixes 相关 mod 组合）"),
            DiagnoseHintModNightConfigFixes => ("Delete the affected mod's config file, or remove nightconfigfixes", "删掉对应 mod 的配置文件，或移除 nightconfigfixes"),
            DiagnoseTitleModInstallMixinbootstrap => ("The MixinBootstrap prerequisite is missing (many mods need it on Forge 1.12.2)", "缺少 MixinBootstrap 前置（Forge 1.12.2 上很多 mod 需要它）"),
            // ── other（1 条 / 1 hint）──
            DiagnoseTitleOtherDebugCrash => ("This crash was triggered on purpose (F3+C in game), not an actual fault", "这次崩溃是手动触发的（游戏里按 F3+C），不是真的故障"),
            DiagnoseHintOtherDebugCrash => ("If someone pressed export-diagnostics while testing, ignore this entry", "如果用户点了「导出诊断包」做测试，可以忽略这条"),
            // ── world（2 条 / 2 hint）──
            DiagnoseTitleWorldEntityTick => ("Crashed during an entity tick: entity {type} at {location}", "崩在实体 tick：实体 {type}（坐标 {location}）"),
            DiagnoseHintWorldEntityTick => ("Find which mod that entity comes from (health-bar, mob-enhancement and spawn mods are the usual source), then disable it or delete the entity from the save", "看看这个实体来自哪个 mod（血条/怪物增强/刷怪类 mod 居多），禁用该 mod 或删掉存档里这个实体"),
            DiagnoseTitleWorldBlockModel => ("Crashed on a block ({type} at {location}): usually a mod that touches block models or textures", "崩在方块（{type}，坐标 {location}）：多半是方块模型/贴图相关 mod 的问题"),
            DiagnoseHintWorldBlockModel => ("Identify the mod that owns that block; a resource pack can cause it too (switch back to vanilla packs to confirm)", "定位该方块所属 mod；资源包也可能导致（先换回原版资源包验证）"),
            // ── 引擎内置结论（10 条，M395 BUILTIN10）──
            // 前 64 条的真相源是 `rules.d/*.yaml`；这 10 条的真相源是 `core/src/diagnose/synthetic.rs`
            // 的 `format!`（退出码 / hs_err 这类结构化信号塞 YAML 别扭）。中文侧仍是**副本**：
            // 逐字等于 core 的 `message`（`{{name}}` 由 core 的 `Finding::groups` 代入），
            // 守门在 `app-iced/src/pages/logs/diagnostic_tests.rs` 的
            // `builtin_conclusions_read_the_same_in_chinese`——那里同时看得见 core 与目录。
            //
            // 参数名是 core 定的稳定语义名（不是 `arg1`）：`exit_code` / `exit_type` / `uptime_ms` /
            // `error_lines` / `module_name` / `failed_frame` / `physical_ram` / `physical_free` /
            // `swap` / `swap_free`。值都是语言中立的片段（数字、slug、`16.0 GB`、`?`）——
            // `exit.summary()` 那种中文子句不能透传，否则英文界面里夹一句中文。
            DiagnoseTitleSignalAccessViolation => ("The process exited with 0xC0000005 (access violation): usually the graphics driver or render stack crashing — update the driver or switch renderer first (exit code {exit_code} ({exit_type}), uptime {uptime_ms} ms)", "进程以 0xC0000005（访问违例）退出：多半是显卡驱动/渲染栈崩溃，建议先更新显卡驱动或换渲染器（退出码 {exit_code}（{exit_type}），存活 {uptime_ms} ms）"),
            DiagnoseTitleSignalDllNotFound => ("The process exited with 0xC0000135 (DLL not found): a native library is missing or the Java install is broken — reinstall the natives / Java components", "进程以 0xC0000135（找不到 DLL）退出：缺原生库或 Java 安装损坏，重装 natives/Java 组件"),
            DiagnoseTitleSignalStackBufferOverrun => ("The process exited with 0xC0000409 (stack buffer overrun): an internal JVM / native library fault; an hs_err file or crash report is needed to go further", "进程以 0xC0000409（栈缓冲区溢出）退出：JVM/原生库内部错误，需要 hs_err 或崩溃报告进一步判断"),
            DiagnoseTitleSignalSigkill => ("The process was killed with SIGKILL: usually the system ran out of memory (OOM killer) or a cleanup process killed it — lower -Xmx or enlarge the page file", "进程被 SIGKILL 强杀：常见于系统内存耗尽（OOM killer）或外部清理进程，建议降低 -Xmx 或扩大虚拟内存"),
            DiagnoseTitleSignalJvmError => ("The JVM never started (the game never ran at all): check whether -Xmx exceeds the available memory, and whether the JVM arguments are valid", "JVM 未能启动（游戏根本没跑起来）：先查 -Xmx 是否超过可用内存、JVM 参数是否非法"),
            DiagnoseTitleSignalSilentFailure => ("Exit code 0 but it looks like a silent failure ({error_lines} error lines): the game may have started and then been killed by a mod exception — a longer log is needed to confirm", "退出码为 0 但疑似静默失败（{error_lines} 条错误行）：游戏可能刚起来就被 mod 异常终止，需要更长的日志确认"),
            DiagnoseTitleHsErrAccessViolation => ("JVM fatal error in {module_name} (access violation, failed frame {failed_frame}): a native-layer crash — the graphics driver / render stack is the prime suspect", "JVM 致命错误 {module_name}（访问违例，失败帧 {failed_frame}）：原生层崩溃，显卡驱动/渲染栈是首要嫌疑"),
            DiagnoseTitleHsErrOutOfMemory => ("The JVM died of insufficient memory: physical RAM {physical_ram} (free {physical_free}), swap {swap} (free {swap_free}) — lower -Xmx or enlarge the system page file", "JVM 因内存不足终止：物理内存 {physical_ram}（可用 {physical_free}），交换区 {swap}（可用 {swap_free}），降低 -Xmx 或扩大系统虚拟内存"),
            DiagnoseTitleHsErrStackOverflow => ("A fatal JVM stack overflow: usually unbounded recursion or a loop inside one mod — disable the most recently installed mods and try again", "JVM 栈溢出导致的致命错误：通常是某个 mod 的递归/死循环，禁用最近装的 mod 再试"),
            DiagnoseTitleHsErrInternalError => ("An internal JVM fatal error: switch to an official HotSpot Java first, then suspect a corrupted native library (natives)", "JVM 内部致命错误：优先换官方 HotSpot Java，其次怀疑原生库（natives）损坏"),
            // 内置结论的两个「未知」词：core 不知道模块名 / 失败帧时传空串，界面按语言补这两条。
            // 它们不是规则 id 推导出来的（所以不叫 `DiagnoseTitle…`），是**参数值的兜底词**。
            DiagnoseBuiltinUnknownModule => ("unknown module", "未知模块"),
            DiagnoseBuiltinUnknownFrame => ("unknown", "未知"),
            // ── M400 DIAGHINT：21 条规则的提示（补齐 M392 留空的那一半）──
            // 上一批只搬了 YAML 里**已有**的 `detail`，这 21 条规则没有 `detail`，于是 `hint: None`。
            // 这一批给每条规则写一条**可操作**的建议（告诉用户下一步点哪里/删哪个/换哪个版本），
            // 同时往 YAML 的 `cause.detail` 写同一句中文——两处逐字相等，由
            // `app-iced/src/pages/logs/diagnostic_tests.rs` 的 `diagnose_catalog_matches_the_rule_table` 钉住。
            //
            // 为什么追加在**链尾**而不是插进各自的分类块：`Key::ALL` 顺序 = 金表顺序，插在中间会平移
            // 其后所有 key（金表整片红）。本仓既定做法是「只追加不重排」（见 `catalog.rs` 域装配）。
            // 顺序 = 规则表顺序（fabric / forge / graphics / integrity / jvm / mod_conflict / mod_runtime）。
            DiagnoseHintFabricModResolutionMissingMinecraft => ("Swap this mod for a build that supports Minecraft {version}, or install a {version} instance and move it there", "把这个模组换成支持 Minecraft {version} 的版本；或者装一个 {version} 的游戏实例，把它搬过去"),
            DiagnoseHintFabricModResolutionCollection => ("Install a different build of {destmod}: {sourcemod} only accepts some of its versions, and this one is outside that range", "给 {destmod} 换一个版本：{sourcemod} 只接受它的某几个版本，当前这个不在范围内"),
            DiagnoseHintFabricLoader012 => ("Downgrade Fabric Loader to 0.11.7 — 0.12 removed the old classes these mods still call", "把 Fabric Loader 降到 0.11.7（0.12 删掉了这些模组还在调用的旧类）"),
            DiagnoseHintFabricWarnings => ("Disable the mods the warning names one at a time, get the game running, then add them back one by one", "把警告里点名的模组逐个禁用，能进游戏之后再一个个加回来"),
            DiagnoseHintForgeDuplicatedMod => ("Delete the copy at {path} — each mod can only be installed once", "删掉 {path} 那一份，同一个模组只能留一个 jar"),
            DiagnoseHintForgeOptifineRepeatInstallation => ("Keep a single OptiFine: delete the extra jar in mods/ so only one copy is left", "OptiFine 只留一份：删掉 mods 里多余的那个 jar，只保留一个"),
            DiagnoseHintForgeBootstrapFailed => ("Disable or update that mod; if you just added it, move it out of mods/ and start again", "禁用或更新这个模组；它是最新装的话，先把它移出 mods 再启动"),
            DiagnoseHintGraphicsResolutionTooHigh => ("Switch to a lower-resolution pack (the 32x build of the same pack is usually enough); if it still crashes, go back to vanilla packs to confirm", "换一个分辨率更低的资源包（同一套材质的 32x 版通常够用）；还崩就先换回原版资源包确认是它"),
            DiagnoseHintIntegrityIllegalAccess => ("Look up which mod owns the class named in the log, then update or remove it; if that leads nowhere, disable the coremods you installed last", "按日志里的类名找出是哪个模组，更新或删掉它；查不到就先禁用最近装的核心模组"),
            DiagnoseHintJvmJdk9 => ("Point this instance at Java 8 (1.8.x): install one, then set it as the Java executable under LAUNCH OPTIONS", "给这个实例换 Java 8（1.8.x）：装一个 Java 8，然后在实例的「启动参数」里把「Java 可执行文件」指到它"),
            DiagnoseHintJvmModlauncher8 => ("Update Forge to 36.2.26 or newer; if that is not an option, switch to Java 8u312 or older", "把 Forge 更新到 36.2.26 或更高；更新不了就换 Java 8u312 及更早的版本"),
            DiagnoseHintConflictShadersMod => ("Delete the Shaders mod — OptiFine already ships shader support, and keeping both blocks startup", "删掉 Shaders 模组：OptiFine 自带光影，留着它反而起不来"),
            DiagnoseHintConflictMultipartOptifine => ("Remove OptiFine first — ForgeMultipart is a prerequisite of other mods, so removing it breaks more", "先删 OptiFine：ForgeMultipart 是别的模组的前置，删它要连带拆一圈"),
            DiagnoseHintConflictPerformantOptifine => ("Remove Performant first (OptiFine already covers most of what it does); if the crash comes back, remove OptiFine instead", "先删 Performant（它的功能 OptiFine 基本都有）；还崩再删 OptiFine"),
            DiagnoseHintConflictTwilightOptifine => ("Update Twilight Forest to the latest build for this game version; if the texture error stays, drop OptiFine and use Oculus for shaders", "把 Twilight Forest 更新到该游戏版本的最新版；还是越界就删掉 OptiFine，光影改用 Oculus"),
            DiagnoseHintConflictJadeOptifine => ("Replace OptiFine with Embeddium and Oculus, or swap Jade for WTHIT", "删掉 OptiFine 换成 Embeddium + Oculus；不想换就把 Jade 换成 WTHIT"),
            DiagnoseHintConflictNeoforgeOptifine => ("OptiFine does not work on NeoForge, so remove it; for shaders use the NeoForge build of Iris or Oculus", "OptiFine 不支持 NeoForge，只能删掉它；要光影就装 Iris 或 Oculus 的 NeoForge 版本"),
            DiagnoseHintConflictOptifineWorldLoad => ("Swap the OptiFine jar: download the build for this game version from OptiFine's site and replace the one in mods/, or just remove OptiFine (the world-load failure is a bug in one of its builds)", "换一版 OptiFine：从 OptiFine 官网下载对应游戏版本的 jar 替换 mods 里那个；或者直接删掉 OptiFine（这是它特定版本的 bug）"),
            DiagnoseHintModLoadingCrashedFabric => ("Disable or update that mod; if you just added it, take it out of mods/ first", "禁用或更新这个模组；它是最新装的话，先移出 mods 再启动"),
            DiagnoseHintModMixinApplyFailed => ("Update that mod to the build made for this game version; if there is not one, remove it", "把这个模组更新到与当前游戏版本配套的那一版；没有配套版就删掉它"),
            DiagnoseHintModInstallMixinbootstrap => ("Install MixinBootstrap 0.8 or newer; if it still crashes, put a plain ! in front of its file name so it loads first", "装上 MixinBootstrap 0.8 或更高；装上还崩就在它的文件名前加一个半角感叹号让它先加载"),
        } }
    };
}

pub(crate) use diagnose_entries;
