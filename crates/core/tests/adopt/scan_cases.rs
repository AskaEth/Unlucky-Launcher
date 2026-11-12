//! `scan` / `inspect` 的判据：正常版本能不能过、坏东西是不是**逐条**说清楚。
//!
//! 每条断言下面都标了「抓得住 / 抓不住」：抓得住的是变异实现会变红的具体结论，
//! 抓不住的是只能靠人读的东西（措辞、是否还有别的静默吞掉路径）。

use unlucky_core::game::adopt::{self, AdoptCandidate};

use crate::fixture::{assert_unchanged, foreign_minecraft, snapshot, TempRoot};

/// 扫一次并按 id 取候选（`inspect` 对不合格候选返回 `Err`，扫描则逐项列出，断言更好写）。
fn candidate_of(root: &TempRoot, version_id: &str) -> AdoptCandidate {
    let scan = adopt::scan(&root.game(), &root.launcher()).expect("扫描应成功");
    scan.candidates
        .into_iter()
        .find(|candidate| candidate.version_id == version_id)
        .unwrap_or_else(|| panic!("扫描里没有 {version_id}"))
}

/// 正常 / 缺清单 / 坏 JSON / 缺父 / 成环：一次扫描把每个候选的结论都说出来。
///
/// 抓得住：`scan` 把坏东西当空列表（`candidates` 少项就红）、漏掉父链（`inherits` 长度变）、
/// 缺父被当成可登记（`adoptable` 变真）、成环没被检出（`cycle-*` 会变可登记）。
/// 抓不住：问题文案的措辞，以及 `versions/` 之外的地方有没有被读写（由下面的哈希断言覆盖）。
#[test]
fn scan_reports_every_version_with_its_reason() {
    let root = TempRoot::new("scan");
    foreign_minecraft(&root.game_root());
    let before = snapshot(root.path());

    let scan = adopt::scan(&root.game(), &root.launcher()).expect("扫描应成功");
    let after = snapshot(root.path());
    assert_unchanged(&before, &after, "scan 之后");

    let ids: Vec<&str> = scan
        .candidates
        .iter()
        .map(|candidate| candidate.version_id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec![
            "1.20.1",
            "1.20.1-broken",
            "1.20.1-fabric-0.15.0",
            "1.20.1-nomanifest",
            "1.20.1-orphan",
            "cycle-a",
            "cycle-b",
        ],
        "扫描必须列出每一个目录（含不可登记的），不能少一行"
    );

    let by_id = |id: &str| {
        scan.candidates
            .iter()
            .find(|candidate| candidate.version_id == id)
            .unwrap_or_else(|| panic!("缺候选 {id}"))
    };

    let vanilla = by_id("1.20.1");
    assert!(
        vanilla.adoptable(),
        "正常版本应可登记：{:?}",
        vanilla.problems
    );
    assert!(vanilla.jar_present);
    assert_eq!(vanilla.inherits, vec!["1.20.1"]);
    assert!(!vanilla.is_registered());

    let fabric = by_id("1.20.1-fabric-0.15.0");
    assert!(fabric.adoptable(), "{:?}", fabric.problems);
    assert!(!fabric.jar_present, "fabric 版本自己没有 jar");
    assert_eq!(
        fabric.inherits,
        vec!["1.20.1-fabric-0.15.0", "1.20.1"],
        "父链必须走到底"
    );

    let codes = |id: &str| -> Vec<&'static str> {
        by_id(id)
            .problems
            .iter()
            .map(|problem| problem.code())
            .collect()
    };
    assert_eq!(codes("1.20.1-broken"), vec!["manifest_invalid_json"]);
    assert_eq!(codes("1.20.1-nomanifest"), vec!["manifest_missing"]);
    assert_eq!(codes("1.20.1-orphan"), vec!["missing_parent"]);
    assert_eq!(codes("cycle-a"), vec!["circular_inheritance"]);
    assert_eq!(codes("cycle-b"), vec!["circular_inheritance"]);
    for id in ["1.20.1-broken", "1.20.1-orphan", "cycle-a"] {
        assert!(!by_id(id).adoptable(), "{id} 不该被判成可登记");
    }
}

/// 缺父要说清**缺哪个**、成环要说清**环长什么样**（不是一句「继承有问题」）。
///
/// 抓得住：`MissingParent` 丢了 `parent` 字段、环链只剩最后一个 id（两个都会被下面的 contains 抓到）。
/// 抓不住：错误码是否被 CLI 原样透出（那条在 cli/tests/adopt_test 里）。
#[test]
fn missing_parent_and_cycle_name_what_is_wrong() {
    let root = TempRoot::new("chain");
    foreign_minecraft(&root.game_root());
    let scan = adopt::scan(&root.game(), &root.launcher()).expect("扫描应成功");
    let problems = |id: &str| {
        scan.candidates
            .iter()
            .find(|candidate| candidate.version_id == id)
            .expect("有候选")
            .problems
            .clone()
    };

    let orphan = problems("1.20.1-orphan");
    let message = orphan.first().expect("一条问题").to_string();
    assert!(message.contains("1.20.1-not-on-disk"), "{message}");
    assert!(message.contains("1.20.1-orphan"), "{message}");

    let cycle = problems("cycle-a");
    let message = cycle.first().expect("一条问题").to_string();
    assert!(message.contains("cycle-a"), "{message}");
    assert!(message.contains("cycle-b"), "{message}");
}

/// 坏 JSON 要带**行列号**（用户才知道去改哪一行）。
///
/// 抓得住：把 serde 的错误文本换成一句「解析失败」（`line` 就没了）。
/// 抓不住：具体行列号对不对（不同 serde_json 版本的措辞会变，只断言关键字）。
#[test]
fn broken_json_message_carries_a_line_number() {
    let root = TempRoot::new("badjson");
    foreign_minecraft(&root.game_root());
    let candidate = candidate_of(&root, "1.20.1-broken");
    let message = candidate.problems.first().expect("一条问题").to_string();
    assert!(message.contains("line"), "{message}");
    assert!(message.contains("1.20.1-broken"), "{message}");
}

/// 清单 `id` 与目录名不一致 ⇒ 拒绝（启动时会按清单 id 找 jar，必然找不到）。
///
/// 抓得住：去掉 `ManifestIdMismatch` 这条判据后，这个版本会变成「可登记」。
/// 抓不住：真实启动是否真的失败（那需要真跑游戏，本 mission 不做）。
#[test]
fn manifest_id_must_match_the_directory_name() {
    let root = TempRoot::new("idmismatch");
    foreign_minecraft(&root.game_root());
    let dir = root.game_root().join("versions/1.20.1-copy");
    std::fs::create_dir_all(&dir).expect("建目录");
    std::fs::write(
        dir.join("1.20.1-copy.json"),
        br#"{"id":"1.20.1","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写清单");
    std::fs::write(dir.join("1.20.1-copy.jar"), b"jar").expect("写 jar");

    let candidate = candidate_of(&root, "1.20.1-copy");
    let codes: Vec<&str> = candidate
        .problems
        .iter()
        .map(|problem| problem.code())
        .collect();
    assert_eq!(codes, vec!["manifest_id_mismatch"]);
}

/// 版本 id 不能当路径用（`../` / 盘符 / 结尾点号）⇒ 一条结构化问题，不去碰盘。
///
/// 抓得住：删掉 id 校验后 `inspect("../escape")` 会去读游戏根之外的路径（下面断言问题码）。
/// 抓不住：Windows 大小写不敏感之类的平台细节（与判据无关）。
#[test]
fn version_ids_that_cannot_be_paths_are_rejected() {
    let root = TempRoot::new("badid");
    foreign_minecraft(&root.game_root());
    let escape = root.path().join("outside");
    std::fs::create_dir_all(escape.join("1.20.1")).expect("建外部同名目录");
    std::fs::write(
        escape.join("1.20.1/1.20.1.json"),
        br#"{"id":"1.20.1","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写外部清单");

    // `versions/../../outside/1.20.1` 这种形状：目录真的存在（能解析出去），但 id 里含分隔符。
    let error = adopt::inspect(&root.game(), &root.launcher(), "../../outside/1.20.1")
        .expect_err("非法 id 应报错，而不是去读游戏根之外的目录");
    assert_eq!(error.code(), "adopt_not_adoptable");
    let codes: Vec<&str> = error
        .problems()
        .iter()
        .map(|problem| problem.code())
        .collect();
    assert_eq!(codes, vec!["version_id_invalid"], "{error}");

    for bad in ["1.20.1.", "1.20.1 ", "C:", "a/b"] {
        let error =
            adopt::inspect(&root.game(), &root.launcher(), bad).expect_err("非法 id 应报错");
        assert!(
            error
                .problems()
                .iter()
                .any(|problem| problem.code() == "version_id_invalid"),
            "{bad:?} 应被当成非法 id：{error}"
        );
    }
}

/// 版本目录是指向游戏根**之外**的符号链接 ⇒ 拒绝不跟随（也不读它）。
///
/// 抓得住：`ensure_inside` 不比较解析后的真实路径（那样 `1.20.1-link` 会变成「可登记」）。
/// 抓不住 / 已知局限：
/// - 本机不允许建符号链接（Windows 未开开发者模式、或非管理员）时这条会打印 `[link-guard]`
///   并**跳过**——显式记录而不是静静通过；设 `ULL_ADOPT_TEST_SYMLINK_REQUIRE=1` 则建不出来**必红**；
/// - 目录联接（junction）由下面单独那条用例覆盖（不需要管理员权限，是更常见的形态）。
#[test]
fn a_symlinked_version_pointing_outside_the_game_root_is_rejected() {
    let root = TempRoot::new("symlink");
    foreign_minecraft(&root.game_root());
    let outside = root.path().join("other-launcher");
    std::fs::create_dir_all(outside.join("1.20.1")).expect("建外部版本目录");
    std::fs::write(
        outside.join("1.20.1/1.20.1.json"),
        br#"{"id":"1.20.1","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写外部清单");
    let link = root.game_root().join("versions/1.20.1-link");
    // 建不出来时：非强制模式打印一行并跳过；`ULL_ADOPT_TEST_SYMLINK_REQUIRE=1` 必红（R4）。
    if !crate::fixture::link_guard(
        crate::fixture::try_symlink_dir(&outside.join("1.20.1"), &link),
        "根外符号链接",
    ) {
        return;
    }

    let candidate = candidate_of(&root, "1.20.1-link");
    let codes: Vec<&str> = candidate
        .problems
        .iter()
        .map(|problem| problem.code())
        .collect();
    assert!(
        codes.contains(&"outside_game_root"),
        "越界链接必须被点名：{codes:?}"
    );
    assert!(!candidate.adoptable(), "越界链接的版本不许登记");
}

/// 游戏根内的符号链接（指向同一棵树里的真实版本目录）**不**拒绝：那是合法的省盘做法。
///
/// 抓得住：`check_symlink` 一刀切拒绝所有符号链接（这条会红）。
/// 抓不住：本机建不出链接时同上跳过（打印 `[link-guard]`，强制模式下必红）。
#[test]
fn a_symlink_inside_the_game_root_is_allowed() {
    let root = TempRoot::new("symlink-in");
    foreign_minecraft(&root.game_root());
    // 真实版本目录放在根内的 `hidden/` 下（清单 id 与目录名一致），再在 `versions/` 里挂一条链接。
    let real = root.game_root().join("hidden/1.20.1-thin");
    std::fs::create_dir_all(&real).expect("建真实版本目录");
    std::fs::write(
        real.join("1.20.1-thin.json"),
        br#"{"id":"1.20.1-thin","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写清单");
    let link = root.game_root().join("versions/1.20.1-thin");
    if !crate::fixture::link_guard(
        crate::fixture::try_symlink_dir(&real, &link),
        "根内符号链接",
    ) {
        return;
    }
    let candidate = candidate_of(&root, "1.20.1-thin");
    assert!(
        candidate.adoptable(),
        "根内链接不该被拒：{:?}",
        candidate.problems
    );
}

/// `versions/` 不存在 = 空结果（新数据根不是错误），但**不存在的游戏根不会被建出来**。
///
/// 抓得住：`scan` 顺手 `create_dir_all(versions)`（下面的断言会红），或把「不存在」当错误。
/// 抓不住：读目录失败（ENOTDIR/EACCES）那条分支——真造一个不可读目录在 Windows 上不可靠，
/// 那一条由代码审读把关。
#[test]
fn a_missing_versions_dir_is_empty_not_an_error() {
    let root = TempRoot::new("noversions");
    let scan = adopt::scan(&root.game(), &root.launcher()).expect("应成功");
    assert!(scan.candidates.is_empty());
    assert!(
        !root.game_root().exists(),
        "扫描不该顺手建游戏根目录：{}",
        root.game_root().display()
    );
}

/// Windows 目录联接（junction）指向根外 ⇒ **同样**拒绝，而且不去读那个目录。
///
/// 为什么要单列：junction 不是 `is_symlink`，只看链接类型的实现会放行它——而 junction
/// **不需要管理员权限**就能建，是把别人版本目录挂进 `versions/` 最常见的形态。
///
/// 抓得住：
/// - 判据退回 `is_symlink`（除 Windows junction 外全都看不到，此时候选会变成「可登记」，第一条断言红）；
/// - 判据排到**解析清单之后**：根外那份清单是合法 JSON 且 id 与目录名一致，一旦走到解析，
///   `manifest_id` 就会被填上（第二条断言红）——所以它至少能证明「没解析过根外清单」；
/// - 建不出链接时走 `fixture::link_guard`：非强制模式打印 `[link-guard]` 并跳过，
///   强制模式（`ULL_ADOPT_TEST_SYMLINK_REQUIRE=1`）**必红**（真值表见 `fixture::link_gate_truth_table`）。
///
/// 抓不住（证据边界，别把它读大）：**只 stat/只 open 不解析**的读取这里看不出来
/// （`manifest_id` 为 `None` 只说明没解析过，不说明没碰过文件）；「边界检查排在
/// `read_manifest` 之前」这条由 `inspect.rs::check` 的代码顺序保证，靠人读。
/// 也抓不住检查与读取之间的 TOCTOU（本模块不是并发/对抗入口）。
#[cfg(windows)]
#[test]
fn a_junction_pointing_outside_is_rejected_without_reading_it() {
    let root = TempRoot::new("junction");
    foreign_minecraft(&root.game_root());
    // 逐段 join：`cmd` / `mklink` 会把正斜杠当开关，拼成一串带 `/` 的路径会被判「无效参数」。
    let outside = root.path().join("other-launcher").join("1.20.1-junction");
    std::fs::create_dir_all(&outside).expect("建外部版本目录");
    std::fs::write(
        outside.join("1.20.1-junction.json"),
        br#"{"id":"1.20.1-junction","type":"release","mainClass":"x","libraries":[]}"#,
    )
    .expect("写外部清单");
    let link = root.game_root().join("versions").join("1.20.1-junction");
    if !crate::fixture::link_guard(
        crate::fixture::try_junction(&outside, &link),
        "根外目录联接",
    ) {
        return;
    }

    let candidate = candidate_of(&root, "1.20.1-junction");
    let codes: Vec<&str> = candidate
        .problems
        .iter()
        .map(|problem| problem.code())
        .collect();
    assert!(
        codes.contains(&"outside_game_root"),
        "联接越界必须被点名：{codes:?} / {:?}",
        candidate.problems
    );
    assert!(!candidate.adoptable(), "越界联接的版本不许登记");
    assert!(
        candidate.manifest_id.is_none() && candidate.inherits.is_empty(),
        "拒绝之后不许再读根外那份清单：{:?}",
        candidate.manifest_id
    );
}
