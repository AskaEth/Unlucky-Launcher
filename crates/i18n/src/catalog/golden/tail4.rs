//! 金表的第 四 段（tail3.rs 之后，M392）。
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
        "DiagnoseTitleFabricModResolution",
        "Fabric could not resolve mod dependencies (missing prerequisite, version conflict, or mismatched mods)",
        "Fabric 解析 mod 依赖失败（缺依赖/版本冲突/模组不配套）",
    ),
    (
        "DiagnoseHintFabricModResolution",
        "Work through the mod list below one by one: install missing prerequisites, drop one side of a conflict",
        "按下方列出的 mod 名单逐个处理：缺依赖就补装，冲突就删一个",
    ),
    (
        "DiagnoseTitleFabricModResolutionConflict",
        "Fabric reports a mod conflict: {sourcemod} and {destmod} cannot coexist",
        "Fabric 报告 mod 冲突：{sourcemod} 与 {destmod} 不能共存",
    ),
    (
        "DiagnoseHintFabricModResolutionConflict",
        "Keep only one of them, or upgrade both to versions that work together",
        "两个只能留一个（或都升级到互相兼容的版本）",
    ),
    (
        "DiagnoseTitleFabricModResolutionMissing",
        "Missing prerequisite mod: {sourcemod} needs {destmod}",
        "缺少前置 mod：{sourcemod} 需要 {destmod}",
    ),
    (
        "DiagnoseHintFabricModResolutionMissing",
        "Install the matching version of {destmod}; if that is impossible, remove {sourcemod}",
        "装上 {destmod} 的对应版本；装不上就删掉 {sourcemod}",
    ),
    (
        "DiagnoseTitleFabricModResolutionMissingMinecraft",
        "{mod} requires Minecraft {version}, which does not match this instance's version",
        "{mod} 要求 Minecraft {version}，与当前实例版本不一致",
    ),
    (
        "DiagnoseTitleFabricModResolutionCollection",
        "The mod set cannot be resolved: no version combination of {sourcemod} and {destmod} works",
        "mod 集合无法解析：{sourcemod} 与 {destmod} 的版本组合不成立",
    ),
    (
        "DiagnoseTitleFabricModResolutionDetails",
        "Fabric printed mod resolution details (Mod File / Failure message): the conclusion is in the rule above",
        "Fabric 打印了 mod 解析明细（Mod File / Failure message）：具体结论见上方规则",
    ),
    (
        "DiagnoseHintFabricModResolutionDetails",
        "This entry only says the log holds resolution failure details; it points the evidence at the AI or the user",
        "这条只表示「日志里有解析失败明细」，用于把证据片段指给 AI/用户",
    ),
    (
        "DiagnoseTitleFabricLoader012",
        "Fabric Loader 0.12 is incompatible with old mixin classes (the loader removed deprecated classes)",
        "Fabric Loader 0.12 与旧 mixin 类不兼容（loader 删掉了 deprecated 类）",
    ),
    (
        "DiagnoseTitleFabricWarnings",
        "Fabric reports an incompatible mod set: {reason}",
        "Fabric 报告 mod 集合不兼容：{reason}",
    ),
    (
        "DiagnoseTitleForgeMissingDependencies",
        "Forge reports a missing or unsupported required dependency: some mod's prerequisite is absent or the wrong version",
        "Forge 报「缺少或不支持强制依赖」：有 mod 的前置没装或版本不对",
    ),
    (
        "DiagnoseHintForgeMissingDependencies",
        "Install the prerequisites named in the error; if they cannot be installed, remove the mod that needs them",
        "按报错里的 mod 名补齐前置；装不上就删掉依赖它的 mod",
    ),
    (
        "DiagnoseTitleForgeDuplicateMods",
        "The same mod is installed twice (duplicate jars in the mods directory)",
        "同一个 mod 装了两份（mods 目录里重复的 jar）",
    ),
    (
        "DiagnoseHintForgeDuplicateMods",
        "Delete the duplicate and keep a single version",
        "删掉重复的那一份，只留一个版本",
    ),
    (
        "DiagnoseTitleForgeDuplicatedMod",
        "Duplicate mod: {name} also appears at {path}",
        "mod 重复：{name} 同时出现在 {path}",
    ),
    (
        "DiagnoseTitleForgeRepeatInstallation",
        "Forge is installed twice (the same version shows up with several install entries)",
        "Forge 被装了两次（同一版本出现多个安装参数）",
    ),
    (
        "DiagnoseHintForgeRepeatInstallation",
        "Delete the duplicate Forge install or version JSON, then install it once more",
        "删掉重复的 Forge 安装/版本 JSON，再重装一次",
    ),
    (
        "DiagnoseTitleForgeOptifineRepeatInstallation",
        "The OptiFine module name is registered twice (usually two OptiFine jars in the mods directory)",
        "OptiFine 的模块名被注册了两次（多半是 mods 目录里有两个 OptiFine jar）",
    ),
    (
        "DiagnoseTitleForgeTooManyMods",
        "Too many mods installed: block and item IDs overflowed (the classic 1.12-and-earlier limit)",
        "装的 mod 太多，方块/物品 ID 超出上限（1.12 及更早的经典问题）",
    ),
    (
        "DiagnoseHintForgeTooManyMods",
        "Remove some mods, or move to 1.13+ where the ID limit no longer exists",
        "删掉一部分 mod，或换用 1.13+ 的版本（不再有 ID 上限）",
    ),
    (
        "DiagnoseTitleForgeModuleNameInvalid",
        "A mod jar's file name has no ASCII letters or digits, so Forge reads it as an invalid JPMS module name (e.g. a jar named only in Chinese characters)",
        "某个 mod 的 jar 文件名不含英文字母/数字，Forge 把它当成非法 JPMS 模块名（如「拔刀剑.jar」）",
    ),
    (
        "DiagnoseHintForgeModuleNameInvalid",
        "Rename that jar to an ASCII file name (keep the .jar suffix) and launch again",
        "把该 jar 改成英文文件名（保留 .jar 后缀）再启动",
    ),
    (
        "DiagnoseTitleForgeBootstrapFailed",
        "Instantiating mod {id} failed (it throws during initialisation)",
        "mod {id} 的实例创建失败（初始化就抛异常）",
    ),
    (
        "DiagnoseTitleGraphicsGlOperationFailure",
        "A mod or shader pack issued an illegal OpenGL call (GL_INVALID_OPERATION)",
        "某个 mod/光影包做了非法的 OpenGL 操作（GL_INVALID_OPERATION）",
    ),
    (
        "DiagnoseHintGraphicsGlOperationFailure",
        "Swap out the shader or resource pack you added last, or lower the resource pack resolution",
        "换掉最近装的光影/资源包，或把资源包分辨率降下来",
    ),
    (
        "DiagnoseTitleGraphicsOpenglNotSupported",
        "The graphics driver does not support OpenGL (usually no driver installed, so it fell back to software rendering)",
        "显卡驱动不支持 OpenGL（多半是没装显卡驱动，走了软件渲染）",
    ),
    (
        "DiagnoseHintGraphicsOpenglNotSupported",
        "Install or update the vendor's graphics driver; inside a virtual machine, enable 3D acceleration",
        "装/更新显卡厂商驱动；虚拟机里需要开 3D 加速",
    ),
    (
        "DiagnoseTitleGraphicsGraphicsDriver",
        "Creating an OpenGL context failed on the driver side (pixel format or a driver crash)",
        "显卡驱动侧创建 OpenGL 上下文失败（像素格式/驱动崩溃）",
    ),
    (
        "DiagnoseHintGraphicsGraphicsDriver",
        "Update the graphics driver and reboot; on old cards use software rendering or OpenGL compatibility mode",
        "更新显卡驱动并重启；老显卡可用软渲染或换 OpenGL 兼容模式",
    ),
    (
        "DiagnoseTitleGraphicsMacosDisplayPort",
        "GLFW could not create a window on macOS (Cocoa found no display service port)",
        "macOS 上 GLFW 初始化窗口失败（Cocoa 找不到 display service port）",
    ),
    (
        "DiagnoseHintGraphicsMacosDisplayPort",
        "Usually happens in remote-desktop or headless sessions; log in locally or update macOS and try again",
        "多见于远程桌面/无显示器会话；换成本机登录或升级 macOS 后再试",
    ),
    (
        "DiagnoseTitleGraphicsResolutionTooHigh",
        "The resource pack resolution is too high (the game itself asks for a lower-resolution pack)",
        "资源包分辨率过高（游戏直接提示换低分辨率资源包）",
    ),
    (
        "DiagnoseTitleGraphicsRtssSodium",
        "RTSS (MSI Afterburner's frame-rate overlay) is incompatible with Sodium",
        "RTSS（微星小飞机/帧数显示）与 Sodium 不兼容",
    ),
    (
        "DiagnoseHintGraphicsRtssSodium",
        "Turn off or uninstall the RTSS OSD overlay, or switch to a Sodium-compatible frame counter",
        "关掉或卸载 RTSS 的 OSD 覆盖层，或改用 Sodium 兼容的帧数显示",
    ),
    (
        "DiagnoseTitleGraphicsVulkanLoaderMissing",
        "No Vulkan loader on this machine (the Vulkan backend cannot start, so the game fell back to OpenGL)",
        "机器上没有 Vulkan loader（Vulkan 后端起不来，游戏已回落 OpenGL）",
    ),
    (
        "DiagnoseHintGraphicsVulkanLoaderMissing",
        "Install or update the graphics driver (Windows needs vulkan-1.dll, Linux needs libvulkan.so.1); if that is impossible, use the OpenGL backend",
        "装/更新显卡驱动（Windows 需要 vulkan-1.dll，Linux 需要 libvulkan.so.1）；装不上就改用 OpenGL 后端",
    ),
    (
        "DiagnoseTitleGraphicsVulkanDeviceVersionTooLow",
        "The GPU and driver support a Vulkan version below 1.2 (the Vulkan backend cannot start)",
        "显卡/驱动支持的 Vulkan 版本低于 1.2（Vulkan 后端起不来）",
    ),
    (
        "DiagnoseHintGraphicsVulkanDeviceVersionTooLow",
        "Update the graphics driver; cards whose drivers stopped updating (10+ years old) have no Vulkan 1.2 and must stay on OpenGL",
        "更新显卡驱动；驱动停更的老卡（10 年以上）没有 Vulkan 1.2，只能继续用 OpenGL",
    ),
    (
        "DiagnoseTitleGraphicsVulkanMissingExtension",
        "The driver lacks required Vulkan extensions (VK_KHR_dynamic_rendering / VK_KHR_push_descriptor)",
        "驱动缺 Vulkan 必需扩展（VK_KHR_dynamic_rendering / VK_KHR_push_descriptor）",
    ),
    (
        "DiagnoseHintGraphicsVulkanMissingExtension",
        "The MC 26.2+ Vulkan backend needs VK 1.2 + dynamic_rendering + push_descriptor and will not run without any one of them; Mesa Dozen lacks push_descriptor too",
        "MC 26.2+ 的 Vulkan 后端要求 VK 1.2 + dynamic_rendering + push_descriptor，三者缺一就跑不了；Mesa Dozen 也不支持 push_descriptor",
    ),
    (
        "DiagnoseTitleGraphicsVulkanMissingFeature",
        "The driver lacks a required Vulkan feature set (VULKAN_CORE_1_2)",
        "驱动缺 Vulkan 必需特性集（VULKAN_CORE_1_2）",
    ),
    (
        "DiagnoseHintGraphicsVulkanMissingFeature",
        "Same as above: the driver is not capable enough. Common on old integrated GPUs and in virtual machines; use the OpenGL backend",
        "同上，属于驱动能力不足；老核显/虚拟机里常见，用 OpenGL 后端即可",
    ),
    (
        "DiagnoseTitleGraphicsNoGraphicsBackend",
        "Both the Vulkan and the OpenGL backend failed (26.2+ has no other path)",
        "Vulkan 与 OpenGL 两个后端都创建失败（26.2+ 只有这两条路）",
    ),
    (
        "DiagnoseHintGraphicsNoGraphicsBackend",
        "Neither the hardware nor the driver qualifies: update the graphics driver first, enable 3D acceleration in virtual machines, and if that fails the machine has to change",
        "硬件/驱动两侧都不满足：先更新显卡驱动；虚拟机需要开 3D 加速；实在不行只能换机器",
    ),
    (
        "DiagnoseTitleIntegrityFileChanged",
        "A game file was modified (SHA1 mismatch): the jar contents do not match the manifest hash",
        "游戏文件被改过（SHA1 校验不一致）：jar 内容与清单哈希对不上",
    ),
    (
        "DiagnoseHintIntegrityFileChanged",
        "Download or verify that file again; do not edit anything inside the jar by hand",
        "重新下载/校验该文件；不要手改 jar 里的内容",
    ),
    (
        "DiagnoseTitleIntegrityNoSuchMethod",
        "NoSuchMethodError: what the loader or a coremod injects does not match the library versions in use",
        "NoSuchMethodError：加载器/coremod 注入的东西与当前库版本不匹配",
    ),
    (
        "DiagnoseHintIntegrityNoSuchMethod",
        "Common when a mod does not match the game or Forge version; reinstall the loader or remove the conflicting mod",
        "常见于 mod 与游戏版本/Forge 版本不配套；重装加载器或删掉冲突 mod",
    ),
    (
        "DiagnoseTitleIntegrityNoClassDefFound",
        "NoClassDefFoundError: missing class files (libraries not fully downloaded, a jar was extracted, or a mismatched mod)",
        "NoClassDefFoundError：缺少类文件（库没下全 / jar 被解压 / mod 不配套）",
    ),
    (
        "DiagnoseHintIntegrityNoClassDefFound",
        "Check that libraries is complete first, then look for extracted folders inside mods",
        "先检查 libraries 是否完整，再排查 mods 里的解压目录",
    ),
    (
        "DiagnoseTitleIntegrityIllegalAccess",
        "IllegalAccessError: a coremod tries to reflect into another class's internals (sealed off on newer JDKs)",
        "IllegalAccessError：某个 coremod 想反射访问别的类的内部成员（在新 JDK 上被封了）",
    ),
    (
        "DiagnoseTitleIntegrityFileAlreadyExists",
        "A mod expects a file or directory that should not exist there, so writing it failed",
        "某个 mod 要求一个「不该存在」的文件/目录，导致写入失败",
    ),
];
