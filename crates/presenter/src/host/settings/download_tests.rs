//! 下载设置组的「**生效值**」守门（P3-5 / P3-3）。
//!
//! 这一组画的是引擎真的会用的那个数，不是文件里那串字节。两者的判定在 core
//! （`DownloadTuning::from_settings`：越界/坏值回落出厂，`0` 落成「只试一次」），这里守的是
//! 「宿主把哪个数交给页面」以及「文件原值与生效值不一致时那句话有没有出现」。
//!
//! 构造真文件：手改坏值绕不过 `store.set` 的 schema 校验，所以测试**直接写 `launcher.json`**
//! ——这正是用户手改文件的现场（`document::load` 对坏值只记 `LoadProblem`、原样保留）。

use super::*;
use crate::dto::find_row;
use std::path::PathBuf;

/// 一个装好「手写 `launcher.json`」的数据根。`json` 是整个文件的顶层对象。
fn env_with(tag: &str, json: serde_json::Value) -> (PathBuf, HostEnv) {
    let dir = std::env::temp_dir().join(format!("ull-download-group-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".ULL/config")).expect("config 目录");
    std::fs::write(
        dir.join(".ULL/config/launcher.json"),
        serde_json::to_string_pretty(&json).expect("序列化"),
    )
    .expect("写 launcher.json");
    let env = HostEnv::at_root(&dir);
    (dir, env)
}

/// 某一行（走页面同一条 `find_row`）。
fn row<'a>(group: &'a SettingGroupView, key: &str) -> &'a SettingRowView {
    find_row(std::slice::from_ref(group), key).unwrap_or_else(|| panic!("{key} 这一行不在组里"))
}

/// 选项表里被选中的那一项（页面正是按 `selected` 下标画选中的 chip）。
fn chosen(row: &SettingRowView) -> &str {
    row.options
        .get(usize::try_from(row.selected).expect("下标非负"))
        .map_or("<越界下标>", String::as_str)
}

/// **P3-5 主用例**：文件里是坏值 9999，行上画的是引擎真在用的 8，并把原值写明白。
///
/// 抓得住：① 宿主把文件值当生效值画（改回 `int(env, ..)`）⇒ 选中的 chip 变成 `9999`，红；
/// ② 那句「原值 9999 已按出厂 8 生效」被删掉或写成常量 ⇒ 文案断言红；
/// ③ core 的回落口径被改成「夹到边界 64」⇒ 选中项变成 64，红。
/// 抓不住：界面上这行是否真的画出来（那是 `pages/settings/download.rs` 的 body 与截图的事），
/// 也抓不住用户是否看得懂那句话。
#[test]
fn an_out_of_range_file_value_shows_the_engine_value_and_names_the_raw_one() {
    let (dir, env) = env_with(
        "concurrency",
        serde_json::json!({ "download": { "concurrency": 9999 } }),
    );

    let group = group(&env);
    let concurrency = row(&group, unlucky_core::download::config::KEY_CONCURRENCY);

    assert_eq!(
        group.id, "download",
        "设置页按这个 id 找组（app-iced `pages/settings/download.rs` 的 GROUP_ID）；改名会静默退回演示行"
    );
    assert_eq!(
        chosen(concurrency),
        "8",
        "行上画的必须是引擎生效值（出厂 8），不是文件里的 9999"
    );
    assert!(
        concurrency
            .description
            .contains("原值 9999 已按出厂 8 生效"),
        "文件值与生效值不一致时必须写明原值：{}",
        concurrency.description
    );
    // r1 视觉复审 P2-2：那句结论必须**独占一行**（接在说明末尾时，最后一个「效」会被挤成孤字）。
    assert!(
        concurrency
            .description
            .ends_with("\n原值 9999 已按出厂 8 生效"),
        "结论必须自成一行（`\\n` 分隔），否则 424 列的说明会把它折出单字孤行：{:?}",
        concurrency.description
    );
    assert_eq!(
        tuning_of(&env).max_concurrency,
        8,
        "同一份文件经 core 的翻译层也必须是 8（两处口径不许漂）"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// 文件里写的就是生效值 ⇒ **不许**冒出「原值 …」那句话（否则它是常量而不是判决）。
///
/// 抓得住：`applied_note` 不管三七二十一都返回 `Some`（把那句话变成装饰）。
/// 抓不住：句子出现的**位置**（埋在哪一段文案里）。
#[test]
fn a_file_value_that_matches_the_engine_value_gets_no_note() {
    let (dir, env) = env_with(
        "in-range",
        serde_json::json!({ "download": { "concurrency": 2, "retry_limit": 5 } }),
    );

    let group = group(&env);
    let concurrency = row(&group, unlucky_core::download::config::KEY_CONCURRENCY);
    let retry = row(&group, unlucky_core::download::config::KEY_RETRY_LIMIT);

    assert_eq!(chosen(concurrency), "2");
    assert_eq!(chosen(retry), "5");
    for row in [concurrency, retry] {
        assert!(
            !row.description.contains("原值"),
            "没有差异就不该有那句话：{}",
            row.description
        );
        // 没有结论行 ⇒ 说明里也不该有换行（那会让页面多出一行空白）。
        assert!(
            !row.description.contains('\n'),
            "没有差异就不该有第二段：{:?}",
            row.description
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

/// **每一种坏值形态都要说清原值**（r1 代码复审 p2：非整数坏值曾被静默丢掉）。
///
/// 任务书 P3-5 是无条件的「文件里的值与生效值不一致时，行上要写明原值 X 已按出厂 N 生效」，
/// 而「手改坏值」不止越界整数这一种。四种形态各验一遍：
///
/// | 文件里写的 | 生效值 | 行上必须出现 |
/// |---|---|---|
/// | `9999`（越界整数） | 8 | `原值 9999 已按出厂 8 生效`（上面那条用例） |
/// | `-4`（负整数） | 8 | `原值 -4 已按出厂 8 生效` |
/// | `"auto"`（类型不对） | 8 | `原值 auto 已按出厂 8 生效` ← **本用例补的洞** |
/// | `8.5` / `true`（类型不对） | 3 | `原值 8.5 / true 已按出厂 3 生效` |
/// | `0`（retry 特例） | 1 | `原值 0 已按 1 生效`（上面那条用例） |
///
/// 抓得住：① 原值退回 `Value::as_i64`（非整数 → `None` → `let raw = raw?` 短路 ⇒ 一个字都不说）；
/// ② 字符串原值被 serde 加上引号（上屏成 `"auto"`）；③ `null` 被当成「写过」（会冒出
/// `原值 null …`）。
/// 抓不住：别的键（region / mirror / proxy_mode）同族缺口——那三条不在 P3-5 点名范围，
/// 已由 tower 登记为另一个 follow-up。
#[test]
fn every_bad_value_shape_says_what_the_raw_value_was() {
    // 负整数
    let (dir, env) = env_with(
        "bad-negative",
        serde_json::json!({ "download": { "concurrency": -4 } }),
    );
    let group_view = group(&env);
    let concurrency = row(&group_view, unlucky_core::download::config::KEY_CONCURRENCY);
    assert_eq!(chosen(concurrency), "8", "-4 越界 ⇒ 生效值是出厂 8");
    assert!(
        concurrency.description.contains("原值 -4 已按出厂 8 生效"),
        "负整数也要说清原值：{}",
        concurrency.description
    );
    std::fs::remove_dir_all(&dir).ok();

    // 类型不对：字符串 + 浮点（一次覆盖两个）
    let (dir, env) = env_with(
        "bad-types",
        serde_json::json!({ "download": { "concurrency": "auto", "retry_limit": 8.5 } }),
    );
    let group_view = group(&env);
    let concurrency = row(&group_view, unlucky_core::download::config::KEY_CONCURRENCY);
    let retry = row(&group_view, unlucky_core::download::config::KEY_RETRY_LIMIT);
    assert_eq!(
        chosen(concurrency),
        "8",
        "\"auto\" 不是整数 ⇒ 生效值是出厂 8"
    );
    assert!(
        concurrency
            .description
            .contains("原值 auto 已按出厂 8 生效"),
        "字符串坏值必须原样上屏（不带引号）并说清它被忽略了：{}",
        concurrency.description
    );
    assert!(
        !concurrency.description.contains("\"auto\""),
        "上屏的是用户写的那个词，不是 serde 的转义形式：{:?}",
        concurrency.description
    );
    assert_eq!(chosen(retry), "3", "8.5 不是整数 ⇒ 生效值是出厂 3");
    assert!(
        retry.description.contains("原值 8.5 已按出厂 3 生效"),
        "浮点坏值也要说清：{}",
        retry.description
    );
    std::fs::remove_dir_all(&dir).ok();

    // 布尔 / null：`true` 要说；`null` 等于没写（不该冒出「原值 null …」）
    let (dir, env) = env_with(
        "bad-bool-null",
        serde_json::json!({ "download": { "retry_limit": true, "concurrency": null } }),
    );
    let group_view = group(&env);
    let retry = row(&group_view, unlucky_core::download::config::KEY_RETRY_LIMIT);
    let concurrency = row(&group_view, unlucky_core::download::config::KEY_CONCURRENCY);
    assert!(
        retry.description.contains("原值 true 已按出厂 3 生效"),
        "布尔坏值也要说清：{}",
        retry.description
    );
    assert!(
        !concurrency.description.contains("原值"),
        "`null` 等于没写，不该有提示：{}",
        concurrency.description
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// `retry_limit = 0`（旧版档位）落成「只试一次」：行上画 1，并说清原值是 0。
///
/// 抓得住：把 `0` 原样画上屏（那时引擎按 1 跑，界面说 0 —— 又一个「设置项说假话」）；
/// 或者把这句话写成「按出厂 3 生效」（实际生效值是 1，不是出厂 3）。
/// 抓不住：用户是否理解 0/1 同义（那是文案的事，本行 base 段已有说明）。
#[test]
fn a_zero_retry_in_the_file_is_shown_as_one_and_says_so() {
    let (dir, env) = env_with(
        "retry-zero",
        serde_json::json!({ "download": { "retry_limit": 0 } }),
    );

    let group = group(&env);
    let retry = row(&group, unlucky_core::download::config::KEY_RETRY_LIMIT);

    assert_eq!(chosen(retry), "1", "0 生效为 1（只试一次）");
    assert!(
        retry.description.contains("原值 0 已按 1 生效"),
        "必须写明 0 落成了 1（不是出厂 3）：{}",
        retry.description
    );
    assert!(
        !retry.description.contains("原值 0 已按出厂"),
        "1 不是出厂值（出厂 3），措辞不许混"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// **P3-3（选了「在下载设置页补一条 gh_proxy 端到端链」这一支）**：
/// 文件里的 `download.gh_proxy` → `store` → [`tuning_of`] → core 的三态 → 行上的值与那句
/// 「当前生效：…」。三档各走一遍。
///
/// 抓得住：① 键名读错（例如 `download.gh-proxy`）⇒ 三档全部退回 `Builtin`，`off`/`prefix` 两档红；
/// ② 三态翻译被拍平（`off` 也当 `Builtin`）⇒ 第二档红；③ 行上只画文件原文、不画生效档位
/// ⇒ 那一句文案红（页面上「填了 off 却没关」的那种谎正是它兜的）。
/// 抓不住：候选链里到底生成了什么（那是 `shared.rs` 的 `expand()`，core 侧已有用例），
/// 也抓不住真下载时是否走那条链（要网络）。
///
/// 为什么选这一支而不是拆 core 的 `gh_proxy_override_reaches_the_profile_and_the_chain`：
/// 那个文件不在本 mission 的 scope（`crates/core/**`），拆它得由 core 线另立 mission；
/// 而「设置文件 → 宿主 → 三态」这一段的缺口正是本 mission 要接的路，补在这里能证明整条链
/// 从用户手写的那串字节开始就是通的。
#[test]
fn the_gh_proxy_key_travels_from_the_file_to_the_core_tri_state() {
    use unlucky_core::download::config::KEY_GH_PROXY;

    // ① 留空 = 跟随内置默认（空串不是「关」）。
    let (empty_dir, empty_env) = env_with(
        "gh-empty",
        serde_json::json!({ "download": { "gh_proxy": "" } }),
    );
    let empty_group = group(&empty_env);
    let empty_row = row(&empty_group, KEY_GH_PROXY);
    assert_eq!(tuning_of(&empty_env).gh_proxy, GhProxyOverride::Builtin);
    assert_eq!(empty_row.value_text, "", "文件里的原文照旧回填");
    assert!(
        empty_row.description.contains("当前生效：跟随内置默认"),
        "空串必须说成「跟随内置默认」而不是「关闭」：{}",
        empty_row.description
    );
    std::fs::remove_dir_all(&empty_dir).ok();

    // ② 填 off = 关（不生成任何 gh-proxy 候选）。
    let (off_dir, off_env) = env_with(
        "gh-off",
        serde_json::json!({ "download": { "gh_proxy": "off" } }),
    );
    let off_group = group(&off_env);
    let off_row = row(&off_group, KEY_GH_PROXY);
    assert_eq!(tuning_of(&off_env).gh_proxy, GhProxyOverride::Off);
    assert_eq!(off_row.value_text, "off");
    assert!(
        off_row.description.contains("当前生效：已关闭（off）"),
        "填了 off 就必须说「已关闭」：{}",
        off_row.description
    );
    std::fs::remove_dir_all(&off_dir).ok();

    // ③ 填 URL 前缀 = 覆盖它。
    let (prefix_dir, prefix_env) = env_with(
        "gh-prefix",
        serde_json::json!({ "download": { "gh_proxy": "https://gh.example/" } }),
    );
    let prefix_group = group(&prefix_env);
    let prefix_row = row(&prefix_group, KEY_GH_PROXY);
    assert_eq!(
        tuning_of(&prefix_env).gh_proxy,
        GhProxyOverride::Prefix("https://gh.example/".to_string())
    );
    assert_eq!(prefix_row.value_text, "https://gh.example/");
    assert!(
        prefix_row
            .description
            .contains("当前生效：覆盖为 https://gh.example/"),
        "覆盖档必须把生效的前缀写出来：{}",
        prefix_row.description
    );
    std::fs::remove_dir_all(&prefix_dir).ok();
}
