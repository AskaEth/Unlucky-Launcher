//! [`super`] 的单测：`instances list --json` 的 `loader` 字段 + 人类可读输出不变。
//!
//! 走的是真入口 [`crate::run`]（`--data-dir` 钉在临时目录里），断言的是**用户/脚本拿到的那份
//! 输出**，不是内部函数返回值——这样 JSON 信封（`ok` / `data` / `warnings`）与人类行都在射程内。
//! 解析端本身（id → 加载器）的真值表在 `core::game::loader` 的测试里，这里只验「CLI 有没有把它
//! 如实搬到输出面上」，不重复抄一份 id 真值。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

/// 一次性临时目录（`Drop` 时清理；cli 没有 dev-dependency 的 tempfile）。
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos());
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "ull-instances-test-{tag}-{}-{seq}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("建临时目录");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn arg(&self) -> String {
        self.0.display().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 写 `.ULL/config/profiles.json`（实例名 → 版本 id；id 用 UUID 形状，互不重复）。
fn write_profiles(data_root: &Path, profiles: &[(&str, &str)]) {
    let dir = data_root.join(".ULL").join("config");
    std::fs::create_dir_all(&dir).expect("建配置目录");
    let entries: Vec<Value> = profiles
        .iter()
        .enumerate()
        .map(|(index, (name, version))| {
            serde_json::json!({
                "id": format!("11111111-2222-4333-8444-{:012}", index + 1),
                "name": name,
                "version_id": version,
            })
        })
        .collect();
    let document = serde_json::json!({ "schema_version": 1, "profiles": entries });
    std::fs::write(
        dir.join("profiles.json"),
        serde_json::to_vec_pretty(&document).expect("序列化 profiles.json"),
    )
    .expect("写 profiles.json");
}

/// 跑一次 CLI，返回（退出码, stdout, stderr）。
fn run_cli(args: &[&str]) -> (i32, String, String) {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = crate::run(&owned, &mut out, &mut err);
    (
        code,
        String::from_utf8_lossy(&out).into_owned(),
        String::from_utf8_lossy(&err).into_owned(),
    )
}

/// 跑 `instances list --json` 并返回 `data.instances` 数组。
fn list_instances(dir: &TempDir) -> Vec<Value> {
    let (code, stdout, stderr) =
        run_cli(&["instances", "list", "--json", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("输出不是 JSON");
    assert_eq!(value["ok"], true, "{stdout}");
    value["data"]["instances"]
        .as_array()
        .expect("instances 数组")
        .clone()
}

/// 四家自家 id + 原版 + 两家第三方前缀：JSON 里的 `loader.kind` / `loader.version`。
///
/// 样本与 `core::game::loader` 的真值表**同源**（自家样本取自 `LoaderKind::version_id` 的
/// `<mc>-<kind>-<loader>` 口径，第三方样本取自 PCL 的真实 id 形状），但断言只钉在输出面上。
///
/// 抓得住：CLI 没调 `InstanceLoader::parse`（自己另算一遍或干脆不派生）；种类 → 稳定 id 的
/// 词表被改（`neoforge` 写成 `neo_forge`）；版本段被截断/连标记一起搬；`optifine` /
/// `liteloader` 被漏成 `vanilla`（只在「认得出但装不了」的那两家会红）。
/// 抓不住：解析规则本身退化（那是 core 的单测射程）；`version_id` 大小写/空格的边角；
/// 真实实例里 `libraries[]` 与 id 不一致的情形（本命令只看 id）。
#[test]
fn json_rows_expose_loader_kind_and_version() {
    let dir = TempDir::new("loader");
    let cases: [(&str, &str, &str); 7] = [
        ("Fabric 实例", "1.20.4-fabric-0.16.5", "fabric"),
        ("Quilt 实例", "1.20.4-quilt-0.27.0", "quilt"),
        ("Forge 实例", "1.20.1-forge-47.2.0", "forge"),
        ("NeoForge 实例", "1.21.1-neoforge-21.1.80", "neoforge"),
        ("原版实例", "1.20.1", "vanilla"),
        ("OptiFine 实例", "1.13.2-OptiFine_HD_U_E6", "optifine"),
        ("LiteLoader 实例", "1.12.2-LiteLoader", "liteloader"),
    ];
    let expected_versions = ["0.16.5", "0.27.0", "47.2.0", "21.1.80", "", "HD_U_E6", ""];
    let profiles: Vec<(&str, &str)> = cases
        .iter()
        .map(|(name, version_id, _)| (*name, *version_id))
        .collect();
    write_profiles(dir.path(), &profiles);

    let rows = list_instances(&dir);
    assert_eq!(rows.len(), cases.len(), "{rows:?}");
    for (index, (name, version_id, kind)) in cases.iter().enumerate() {
        let row = &rows[index];
        assert_eq!(row["name"], *name, "{row}");
        assert_eq!(row["version_id"], *version_id, "{row}");
        assert_eq!(
            row["loader"]["kind"], *kind,
            "`{version_id}` 的 loader.kind 应是 `{kind}`：{row}"
        );
        assert_eq!(
            row["loader"]["version"], expected_versions[index],
            "`{version_id}` 的 loader.version：{row}"
        );
    }
}

/// 反向锚：原版的 `loader` 是 `{"kind":"vanilla","version":""}`，**不缺键、不为 `null`**。
///
/// 这条钉住的是模块文档里的「约定一」：脚本可以无条件读 `row.loader.kind`。顺带把「原版 id
/// 不许被认成加载器」也验一遍——反面是 core 那边 `plain_version_ids_stay_vanilla` 的职责，
/// 这里验的是它有没有被 CLI 的翻译层改坏（例如把空版本当成「没解析出东西」而塞 `null`）。
///
/// 抓得住：有人把原版改回「省略 `loader` 键」或「给 `null`」（两种都会让 `row["loader"]["kind"]`
/// 变成 `null`）；`loader` 对象里被塞进多余的键（下面的键集合断言）。
/// 抓不住：`vanilla` 与「id 里没痕迹的整合包」的区分——**用户视角两者本来就不可区分**，
/// 见模块文档「约定三」，只能靠读 `libraries[]`。
#[test]
fn vanilla_rows_keep_a_loader_object_never_null() {
    let dir = TempDir::new("vanilla");
    write_profiles(
        dir.path(),
        &[
            ("原版", "1.20.1"),
            ("快照", "23w45a"),
            ("预发布", "1.20.4-pre1"),
        ],
    );

    let rows = list_instances(&dir);
    assert_eq!(rows.len(), 3, "{rows:?}");
    for row in &rows {
        let loader = row.get("loader").expect("原版也必须带 loader 键");
        assert!(!loader.is_null(), "loader 不许是 null：{row}");
        assert_eq!(loader["kind"], "vanilla", "{row}");
        assert_eq!(loader["version"], "", "{row}");
        let mut keys: Vec<&str> = loader
            .as_object()
            .expect("loader 是对象")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["kind", "version"], "loader 只有这两个键：{row}");
    }
}

/// 向后兼容：人类可读输出逐字不变，`--json` 的旧字段一个没少。
///
/// 抓得住：把 `loader` 顺手塞进人类行（`instances list` 的人读格式被改）；旧七个 JSON 键
/// 被删或改名（`last_played_at` 这类「看着没用」的最容易被顺手清掉）。
/// 抓不住：人类行的**空白/顺序**之外的美化（例如多打一行汇总）——下面按整段逐行比对，
/// 所以大部分改动会红，但只在两行之间加空行这类不涉及内容的变化仍可能被容忍（本测试用的是
/// 定长片段列表，多出来的行一定红；只有同位置改写才需要人看）。
#[test]
fn human_output_and_old_json_fields_are_unchanged() {
    let dir = TempDir::new("compat");
    write_profiles(dir.path(), &[("生存服", "1.13.2-OptiFine_HD_U_E6")]);

    let (code, stdout, stderr) = run_cli(&["instances", "list", "--data-dir", &dir.arg()]);
    assert_eq!(code, 0, "stderr: {stderr}");
    let launcher_home = dir.path().join(".ULL");
    let expected = format!(
        "实例 1 个（数据根 {}）\n  \
         生存服  [1.13.2-OptiFine_HD_U_E6]  载荷缺失  \
         11111111-2222-4333-8444-000000000001\n\
         已安装版本: （无）\n",
        launcher_home.display()
    );
    assert_eq!(stdout, expected, "人类可读输出应逐字不变");
    assert!(
        !stdout.contains("loader") && !stdout.contains("OptiFine 版本"),
        "人读输出里不该出现加载器派生列：{stdout}"
    );

    let rows = list_instances(&dir);
    let row = &rows[0];
    for key in [
        "id",
        "name",
        "version_id",
        "version_present",
        "game_dir",
        "java_path",
        "last_played_at",
    ] {
        assert!(row.get(key).is_some(), "旧键 `{key}` 不许删：{row}");
    }
    // 顺手钉住新键的值，避免「旧键都在、新键是空对象」这种假绿。
    assert_eq!(row["loader"]["kind"], "optifine", "{row}");
}

/// `--json` 是事实上的对外契约：字段约定必须能在 `ull help instances` 里读到。
///
/// 抓得住：`loader` 字段被加进输出、契约说明被从 help 里删掉（只留在源码注释里等于没写——
/// 用户读不到）；「识别口径 ≠ 安装口径」这条警告被顺手删掉（`optifine` / `liteloader` 只在
/// help 里说明它们装不了）。
/// 抓不住：措辞本身——下面只按键词（`loader` / `kind` / `version` / `vanilla` / `optifine` /
/// `install --loader`）检查，换个说法仍会绿；help 的排版正确性也不在射程内。
#[test]
fn help_documents_the_loader_field_contract() {
    let topic = crate::cli::help::topic("instances").expect("help instances");
    for needle in [
        "loader",
        "kind",
        "version",
        "vanilla",
        "optifine",
        "install --loader",
    ] {
        assert!(
            topic.contains(needle),
            "help instances 缺 `{needle}`：\n{topic}"
        );
    }
}
