//! 解析 → 计划 → 落盘 → 继承解析 → 启动参数顺序（两条父链：原版 / Forge）。

use unlucky_core::addon::loader::{LoaderInstallPlan, LoaderPlanInput};
use unlucky_core::download::blocking::block_on;
use unlucky_core::game::arguments::Environment;
use unlucky_core::game::inherit::resolve;
use unlucky_core::game::install::execute;
use unlucky_core::game::layout::GameLayout;
use unlucky_core::launch::command_builder::tokenize;

use crate::support::{
    engine, liteloader_release, prefill, put_version, read_version, TempDir, FORGE_1122,
    VANILLA_1122,
};

/// 原版父链：装完能落盘，清单指向原版，`arguments.game` 带 tweakClass。
#[test]
fn installs_over_vanilla_and_writes_the_manifest() {
    let temp = TempDir::new("vanilla");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);

    let release = liteloader_release("1.12.2");
    assert_eq!(release.version_id, "1.12.2-liteloader-1.12.2-SNAPSHOT");
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("父版本已装，计划应可组");
    assert!(!plan.vanilla_install_needed);
    assert!(plan.install.client_jar().is_none(), "jar 归原版层");

    // 三个库：launchwrapper（Mojang 库仓）+ asm-all（Central）+ 本体 jar（官方直链）。
    let names: Vec<&str> = plan.files().iter().map(|file| file.name.as_str()).collect();
    assert_eq!(names.len(), 3, "{names:?}");
    let loader_jar = plan
        .files()
        .iter()
        .find(|file| file.name == "com.mumfrey:liteloader:1.12.2-SNAPSHOT")
        .expect("缺本体 jar");
    assert!(
        loader_jar
            .url
            .ends_with("/1.12.2-SNAPSHOT/liteloader-1.12.2-20171128.144431-4-release.jar"),
        "本体直链：{}",
        loader_jar.url
    );
    let asm = plan
        .files()
        .iter()
        .find(|file| file.name.contains("asm-all"))
        .expect("缺 asm-all");
    assert!(
        asm.url.starts_with("https://repo1.maven.org/maven2/"),
        "asm-all 只能从 Central 取：{}",
        asm.url
    );

    // 全部落点先铺好占位文件（库无 sha1/size → 存在即信），执行段只写版本 JSON。
    let engine = engine(temp.path());
    let prefilled = prefill(&plan.install);
    assert_eq!(prefilled, 3);
    let report = block_on(execute(&engine, &plan.install, 4, None)).expect("执行应成功");
    assert!(report.is_complete(), "{:?}", report.failures(3));

    let written = read_version(&layout, &release.version_id);
    assert_eq!(written.inherits_from.as_deref(), Some("1.12.2"));
    assert_eq!(
        written.main_class.as_deref(),
        Some("net.minecraft.launchwrapper.Launch")
    );
    assert!(
        written.minecraft_arguments.is_none(),
        "loader 层不许写 minecraftArguments：那会顶掉父链"
    );
}

/// Forge 父链：**父链的 FMLTweaker 必须还在**，LiteLoader 的 tweakClass 追加在它后面。
///
/// 这条是 M218 SURVEY-p1 的「准入条件」：靠 `--tweakClass` 生效，就要求继承时
/// `arguments.game` 是追加、`minecraftArguments` 不被子层覆盖。启动参数顺序照
/// `core::launch::game_args` 的第 24 项（legacy 子串）→ 第 25 项（`arguments.game`）复现，
/// 真实 argv 由 `tests/launch/cmdline_snapshot.rs` 钉住。
#[test]
fn stacks_on_forge_and_keeps_the_forge_tweak_class() {
    let temp = TempDir::new("forge");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    put_version(&layout, "1.12.2-forge-14.23.5.2860", FORGE_1122);

    let release = liteloader_release("1.12.2-forge-14.23.5.2860");
    assert!(!release.parent_is_vanilla());
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("父版本已装，计划应可组");
    assert_eq!(plan.install.jar_id, "1.12.2", "jar 仍取原版那层");

    let engine = engine(temp.path());
    prefill(&plan.install);
    let report = block_on(execute(&engine, &plan.install, 4, None)).expect("执行应成功");
    assert!(report.is_complete(), "{:?}", report.failures(3));

    let installed = read_version(&layout, &release.version_id);
    let mut source = std::collections::BTreeMap::new();
    source.insert("1.12.2".to_string(), read_version(&layout, "1.12.2"));
    source.insert(
        "1.12.2-forge-14.23.5.2860".to_string(),
        read_version(&layout, "1.12.2-forge-14.23.5.2860"),
    );
    source.insert(release.version_id.clone(), installed);
    let resolved = resolve(&source, &source[&release.version_id]).expect("三层继承应可解析");

    // ① 父链的 legacy 参数整串还在（子层没写 minecraftArguments）。
    let legacy = resolved
        .minecraft_arguments
        .as_deref()
        .expect("Forge 的 minecraftArguments 不许被子层顶掉");
    assert!(
        legacy.contains("net.minecraftforge.fml.common.launcher.FMLTweaker"),
        "Forge 的 tweakClass 丢了：{legacy}"
    );

    // ② 启动顺序：legacy（第 24 项）在前，arguments.game（第 25 项）在后。
    let mut game: Vec<String> = tokenize(legacy);
    game.extend(
        resolved
            .arguments
            .as_ref()
            .expect("arguments 应保留")
            .expand(&Environment::host())
            .game,
    );
    let forge_at = game
        .iter()
        .position(|token| token.contains("FMLTweaker"))
        .expect("Forge tweakClass 不在游戏参数里");
    let liteloader_at = game
        .iter()
        .position(|token| token == "com.mumfrey.liteloader.launch.LiteLoaderTweaker")
        .expect("LiteLoader tweakClass 不在游戏参数里");
    assert!(
        forge_at < liteloader_at,
        "launchwrapper 按出现顺序应用 tweak：Forge 必须在前，实际 {game:?}"
    );
    assert_eq!(game[liteloader_at - 1], "--tweakClass");

    // ③ 主类与库顺序：Forge 的库在前（classpath 先到先得），LiteLoader 的在后。
    assert_eq!(
        resolved.main_class.as_deref(),
        Some("net.minecraft.launchwrapper.Launch")
    );
    let libraries: Vec<&str> = resolved
        .libraries
        .as_deref()
        .expect("libraries")
        .iter()
        .map(|library| library.name.as_str())
        .collect();
    let forge_lib = libraries
        .iter()
        .position(|name| name.starts_with("net.minecraftforge:forge:"))
        .expect("缺 Forge 本体库");
    let liteloader_lib = libraries
        .iter()
        .position(|name| name.starts_with("com.mumfrey:liteloader:"))
        .expect("缺 LiteLoader 本体库");
    assert!(forge_lib < liteloader_lib, "{libraries:?}");
}

/// **新式 args 的父链**：父版本用 `arguments.game`（没有 `minecraftArguments`）。
///
/// 现实里 LiteLoader 只到 1.12.2，碰不到这种父版本；但「追加语义」是这套路线的准入条件，
/// 两种父版本形状都得钉住——否则哪天上游把老版本清单改成新式，我们会静默丢参数。
/// 这里用一份**手工夹具**（`--launchTarget forgeclient` 是 Forge 1.13+ 的真实形态）说明：
/// 父层参数在前、LiteLoader 的 tweakClass 在后，中间不互相覆盖。
#[test]
fn new_style_parent_args_are_appended_not_replaced() {
    const MODERN_PARENT: &str = r#"{
      "id": "1.12.2-modern-1.0",
      "inheritsFrom": "1.12.2",
      "mainClass": "net.minecraft.launchwrapper.Launch",
      "arguments": {"game": ["--launchTarget", "forgeclient"], "jvm": ["-Dmodern=1"]},
      "libraries": [{"name": "example:modern:1.0", "url": "https://example.invalid/"}]
    }"#;

    let temp = TempDir::new("modern-args");
    let layout = GameLayout::new(temp.path());
    put_version(&layout, "1.12.2", VANILLA_1122);
    put_version(&layout, "1.12.2-modern-1.0", MODERN_PARENT);

    let release = liteloader_release("1.12.2-modern-1.0");
    let plan = LoaderInstallPlan::build(LoaderPlanInput {
        layout: &layout,
        release: &release,
        environment: &Environment::host(),
        parent_installed: true,
    })
    .expect("计划应可组");
    let engine = engine(temp.path());
    prefill(&plan.install);
    let report = block_on(execute(&engine, &plan.install, 4, None)).expect("执行应成功");
    assert!(report.is_complete(), "{:?}", report.failures(3));

    let mut source = std::collections::BTreeMap::new();
    source.insert("1.12.2".to_string(), read_version(&layout, "1.12.2"));
    source.insert(
        "1.12.2-modern-1.0".to_string(),
        read_version(&layout, "1.12.2-modern-1.0"),
    );
    source.insert(
        release.version_id.clone(),
        read_version(&layout, &release.version_id),
    );
    let resolved = resolve(&source, &source[&release.version_id]).expect("三层继承应可解析");

    // 子层没写 legacy 串：继承结果里的 `minecraftArguments` 只能是祖父层（原版）那一份，
    // 里面绝不能出现 LiteLoader 的 tweakClass（写了就说明我们用了覆盖语义）。
    let legacy = resolved
        .minecraft_arguments
        .as_deref()
        .expect("原版那份 legacy 串应沿链继承下来");
    assert!(
        !legacy.contains("LiteLoaderTweaker"),
        "loader 层不许往 minecraftArguments 里写：{legacy}"
    );
    let game = resolved
        .arguments
        .as_ref()
        .expect("arguments")
        .expand(&Environment::host())
        .game;
    let assert_order = |needle: &str| {
        game.iter()
            .position(|token| token == needle)
            .unwrap_or_else(|| panic!("缺 `{needle}`：{game:?}"))
    };
    assert!(
        assert_order("forgeclient")
            < assert_order("com.mumfrey.liteloader.launch.LiteLoaderTweaker"),
        "父层参数必须在前：{game:?}"
    );
}

/// LiteLoader 生效所依赖的**两条继承合并律**（M222 第一步核实，直接钉在合并函数上）。
///
/// 1. `arguments.game`：父在前、子在后**追加**（所以 tweakClass 能叠在父链参数之后）；
/// 2. `minecraftArguments`：**子覆盖父**（所以 loader 层绝不能写它，否则父链的 tweakClass
///    整串消失）。
///
/// 抓得住：有人把 `minecraftArguments` 改成追加（HMCL/原版语义不是这样），或把
/// `arguments.game` 改成整体替换（LiteLoader/其它叠层加载器当场失效）。
/// 抓不住：真实启动 argv 的顺序（那是 `launch::game_args` 的第 24/25 项，
/// 由 `tests/launch/cmdline_snapshot.rs` 钉住；本 mission 没有改那一层）。
#[test]
fn merge_laws_liteloader_depends_on() {
    use unlucky_core::game::inherit::merge;
    use unlucky_core::game::manifest::VersionJson;

    let parse = |json: &str| VersionJson::from_json_str(json).expect("夹具应可解析");

    let parent = parse(
        r#"{"id":"p","minecraftArguments":"--tweakClass FMLTweaker","arguments":{"game":["--parent"]}}"#,
    );
    let child = parse(r#"{"id":"c","arguments":{"game":["--child"]}}"#);
    let merged = merge(&child, &parent);
    assert_eq!(
        merged.minecraft_arguments.as_deref(),
        Some("--tweakClass FMLTweaker"),
        "子没写就继承父（这条是「叠在 Forge 上」的前提）"
    );
    let game: Vec<String> = merged
        .arguments
        .as_ref()
        .expect("arguments")
        .expand(&Environment::host())
        .game;
    assert_eq!(
        game,
        ["--parent", "--child"],
        "arguments.game 是父在前、子在后追加"
    );

    // 子写了 minecraftArguments：整串替换（不是追加）。
    let loud_child = parse(r#"{"id":"c2","minecraftArguments":"--tweakClass LiteLoaderTweaker"}"#);
    let merged = merge(&loud_child, &parent);
    assert_eq!(
        merged.minecraft_arguments.as_deref(),
        Some("--tweakClass LiteLoaderTweaker"),
        "子覆盖父：这正是 loader 层不许写 minecraftArguments 的原因"
    );
}
