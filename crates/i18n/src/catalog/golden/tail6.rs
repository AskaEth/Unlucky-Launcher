//! 金表的第 六 段（tail5.rs 之后，M400 `DIAGHINT`）。
//!
//! 为什么新开一段：`tail5.rs` 收在 338 行，21 条 hint 要 105 行，追加进去就是 443——撞 400 行硬线。
//! 口径与前面五段完全一致——**只追加、不改既有行**，顺序即 `Key::ALL` 的续段；拼装见
//! `golden.rs` 的 `rows()`（少 chain 一段，`catalog/tests.rs` 会当场红）。
//!
//! 本段内容：M400 补齐的 **21 条规则提示**（`catalog/diagnose.rs` 链尾那一块）。它们与前五段的
//! 43 条 hint 不同源：那 43 条是 M392 从 YAML 已有 `detail` 搬过来的，这 21 条是这一轮新写的，
//! 同一句话同时写进 YAML 的 `cause.detail`（两处中文逐字相等，`app-iced/.../diagnostic_tests.rs`
//! 的 `diagnose_catalog_matches_the_rule_table` 钉住）。
//!
//! 这些行**不是手抄**：由 `catalog/diagnose.rs` 的条目按声明顺序机械导出（解析
//! `Variant => (en, zh)` 三元组）。金表的判据是逐字节相等，抄漏一个全角括号就会红。

pub(super) const ROWS: &[(&str, &str, &str)] = &[
    (
        "DiagnoseHintFabricModResolutionMissingMinecraft",
        "Swap this mod for a build that supports Minecraft {version}, or install a {version} instance and move it there",
        "把这个模组换成支持 Minecraft {version} 的版本；或者装一个 {version} 的游戏实例，把它搬过去",
    ),
    (
        "DiagnoseHintFabricModResolutionCollection",
        "Install a different build of {destmod}: {sourcemod} only accepts some of its versions, and this one is outside that range",
        "给 {destmod} 换一个版本：{sourcemod} 只接受它的某几个版本，当前这个不在范围内",
    ),
    (
        "DiagnoseHintFabricLoader012",
        "Downgrade Fabric Loader to 0.11.7 — 0.12 removed the old classes these mods still call",
        "把 Fabric Loader 降到 0.11.7（0.12 删掉了这些模组还在调用的旧类）",
    ),
    (
        "DiagnoseHintFabricWarnings",
        "Disable the mods the warning names one at a time, get the game running, then add them back one by one",
        "把警告里点名的模组逐个禁用，能进游戏之后再一个个加回来",
    ),
    (
        "DiagnoseHintForgeDuplicatedMod",
        "Delete the copy at {path} — each mod can only be installed once",
        "删掉 {path} 那一份，同一个模组只能留一个 jar",
    ),
    (
        "DiagnoseHintForgeOptifineRepeatInstallation",
        "Keep a single OptiFine: delete the extra jar in mods/ so only one copy is left",
        "OptiFine 只留一份：删掉 mods 里多余的那个 jar，只保留一个",
    ),
    (
        "DiagnoseHintForgeBootstrapFailed",
        "Disable or update that mod; if you just added it, move it out of mods/ and start again",
        "禁用或更新这个模组；它是最新装的话，先把它移出 mods 再启动",
    ),
    (
        "DiagnoseHintGraphicsResolutionTooHigh",
        "Switch to a lower-resolution pack (the 32x build of the same pack is usually enough); if it still crashes, go back to vanilla packs to confirm",
        "换一个分辨率更低的资源包（同一套材质的 32x 版通常够用）；还崩就先换回原版资源包确认是它",
    ),
    (
        "DiagnoseHintIntegrityIllegalAccess",
        "Look up which mod owns the class named in the log, then update or remove it; if that leads nowhere, disable the coremods you installed last",
        "按日志里的类名找出是哪个模组，更新或删掉它；查不到就先禁用最近装的核心模组",
    ),
    (
        "DiagnoseHintJvmJdk9",
        "Point this instance at Java 8 (1.8.x): install one, then set it as the Java executable under LAUNCH OPTIONS",
        "给这个实例换 Java 8（1.8.x）：装一个 Java 8，然后在实例的「启动参数」里把「Java 可执行文件」指到它",
    ),
    (
        "DiagnoseHintJvmModlauncher8",
        "Update Forge to 36.2.26 or newer; if that is not an option, switch to Java 8u312 or older",
        "把 Forge 更新到 36.2.26 或更高；更新不了就换 Java 8u312 及更早的版本",
    ),
    (
        "DiagnoseHintConflictShadersMod",
        "Delete the Shaders mod — OptiFine already ships shader support, and keeping both blocks startup",
        "删掉 Shaders 模组：OptiFine 自带光影，留着它反而起不来",
    ),
    (
        "DiagnoseHintConflictMultipartOptifine",
        "Remove OptiFine first — ForgeMultipart is a prerequisite of other mods, so removing it breaks more",
        "先删 OptiFine：ForgeMultipart 是别的模组的前置，删它要连带拆一圈",
    ),
    (
        "DiagnoseHintConflictPerformantOptifine",
        "Remove Performant first (OptiFine already covers most of what it does); if the crash comes back, remove OptiFine instead",
        "先删 Performant（它的功能 OptiFine 基本都有）；还崩再删 OptiFine",
    ),
    (
        "DiagnoseHintConflictTwilightOptifine",
        "Update Twilight Forest to the latest build for this game version; if the texture error stays, drop OptiFine and use Oculus for shaders",
        "把 Twilight Forest 更新到该游戏版本的最新版；还是越界就删掉 OptiFine，光影改用 Oculus",
    ),
    (
        "DiagnoseHintConflictJadeOptifine",
        "Replace OptiFine with Embeddium and Oculus, or swap Jade for WTHIT",
        "删掉 OptiFine 换成 Embeddium + Oculus；不想换就把 Jade 换成 WTHIT",
    ),
    (
        "DiagnoseHintConflictNeoforgeOptifine",
        "OptiFine does not work on NeoForge, so remove it; for shaders use the NeoForge build of Iris or Oculus",
        "OptiFine 不支持 NeoForge，只能删掉它；要光影就装 Iris 或 Oculus 的 NeoForge 版本",
    ),
    (
        "DiagnoseHintConflictOptifineWorldLoad",
        "Swap the OptiFine jar: download the build for this game version from OptiFine's site and replace the one in mods/, or just remove OptiFine (the world-load failure is a bug in one of its builds)",
        "换一版 OptiFine：从 OptiFine 官网下载对应游戏版本的 jar 替换 mods 里那个；或者直接删掉 OptiFine（这是它特定版本的 bug）",
    ),
    (
        "DiagnoseHintModLoadingCrashedFabric",
        "Disable or update that mod; if you just added it, take it out of mods/ first",
        "禁用或更新这个模组；它是最新装的话，先移出 mods 再启动",
    ),
    (
        "DiagnoseHintModMixinApplyFailed",
        "Update that mod to the build made for this game version; if there is not one, remove it",
        "把这个模组更新到与当前游戏版本配套的那一版；没有配套版就删掉它",
    ),
    (
        "DiagnoseHintModInstallMixinbootstrap",
        "Install MixinBootstrap 0.8 or newer; if it still crashes, put a plain ! in front of its file name so it loads first",
        "装上 MixinBootstrap 0.8 或更高；装上还崩就在它的文件名前加一个半角感叹号让它先加载",
    ),
];
