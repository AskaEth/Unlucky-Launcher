//! 真包叠层用例（M337 / 台账「覆盖目录叠层」缺口）：一个**真**带 `overrides/` + `client-overrides/`
//! 的 Modrinth 包跑一遍叠层。
//!
//! 为什么单独一截：`layer.rs` 到了 400 行线（本仓的既有做法就是按行数拆，见 `demo/processors.rs`）。
//! 拆出来的只是夹具与这一条用例，判据仍全在 `super`（`ordered` / `resolve` / `plan_overrides`）。
//!
//! ## 真包夹具的出处（provenance）
//!
//! 下面两组 `(path, sha1, size)` 是从那个 `.mrpack` 里**直读抄下来**的，二进制不入库（仓里只留清单）。
//! 复核就照这几步，走完应得到同一个文件、同一批条目。
//!
//! | 项 | 值 |
//! |---|---|
//! | 包 | **SkyBlock Enhanced 5.4.5**，Modrinth 项目 `skyblock-enhanced`（project id `e0oMrxjp`） |
//! | 版本 id | `2r8DabtP`（`date_published` 2026-11-09T11:38:17Z） |
//! | 文件 | `SkyBlock Enhanced-5.4.5.mrpack`，**590 370 B** |
//! | SHA-1 | `c706e8194a71b68bacf46be6fe28843aeec4c570`（= Modrinth `files[].hashes.sha1`） |
//! | SHA-256 | `52a4467b81b078cbfa12c4292016f0b2e0b97dd7637577fd567953b35f10c999` |
//! | 包内 | `overrides/` **46** 条文件、`client-overrides/` **2** 条、**无** `server-overrides/`；46 ∩ 2 = ∅ |
//!
//! ```text
//! # ① 拿版本元数据（文件名 / 字节数 / sha1 / 下载地址都在 files[] 里）
//! curl -s https://api.modrinth.com/v2/version/2r8DabtP
//! # ② 下那个文件（地址就是上一步 files[0].url；这里写死形态，省一次查）
//! curl -sL -o skyblock.mrpack \
//!   "https://cdn.modrinth.com/data/e0oMrxjp/versions/2r8DabtP/SkyBlock%20Enhanced-5.4.5.mrpack"
//! # ③ 三个数对上：590370 / c706e819…（sha1）/ 52a4467b…（sha256）
//! wc -c skyblock.mrpack && sha1sum skyblock.mrpack && sha256sum skyblock.mrpack
//! # ④ 条目数与下面两个常量对账（-Z1 只印名字；`grep -v '/$'` 去掉目录条目）
//! unzip -Z1 skyblock.mrpack | grep '^overrides/'        | grep -v '/$' | wc -l   # 46
//! unzip -Z1 skyblock.mrpack | grep '^client-overrides/' | grep -v '/$' | wc -l   # 2
//! unzip -Z1 skyblock.mrpack | grep '^server-overrides/' | grep -v '/$' | wc -l   # 0
//! ```
//!
//! 逐条 `(path, sha1, size)` 与常量比时按**集合**比即可：常量按路径排序，zip 条目顺序是另一回事，
//! 用例不读 zip。

use crate::addon::import::report::FileSource;
use crate::addon::pack::{OverrideFile, PackManifest, PackOverrides, PackSide};

use super::plan_overrides;

/// 真包的 `overrides/`（**全部 46 条**，`fabric-loader 0.20.1` / Minecraft 26.2）。
///
/// 出处见模块文档的 provenance 表；`#[rustfmt::skip]` 只为一行一条。
#[rustfmt::skip]
const SKYBLOCK_BOTH: [(&str, &str, u64); 46] = [
    ("icon.png", "ee51624899235196ffce9c5d25a3c7e87ad9c224", 21326),
    ("packcore/configs/1080p_default_configs.zip", "2815cd4eed788e3b719056b9b441631a6d1b9eff", 88560),
    ("packcore/configs/1080p_default_configs_2x_.zip", "8e92438306683e0ac0274a7ca001084799c61d61", 88519),
    ("packcore/configs/1440p_default_configs.zip", "86483d2398d230b69c25ca900117e29a5776fae5", 88647),
    ("packcore/configs/1440p_default_configs_ultrawide_.zip", "e96cf79cdaa34a977c9950797e5089616b6d5bab", 88521),
    ("packcore/configs/4k_default_configs.zip", "4122cd0742324d7437f447662826456fbf4e773a", 88503),
    ("packcore/configs/4k_default_configs_ultrawide_.zip", "6317fa863768dfab1cd1f2d253c6feb4c182c809", 90090),
    ("packcore/markdown/CHANGELOG-v5.3.0-beta.1.md", "7f865913c9d6216c39f21af58f85430c529da071", 4199),
    ("packcore/markdown/CHANGELOG-v5.3.0-beta.2.md", "78317581dda2ea60ebadba29e51fa46c63d3a188", 909),
    ("packcore/markdown/CHANGELOG-v5.3.0-beta.3.md", "df8e40d5e2c7b74a68a70661b854e253619484c0", 1122),
    ("packcore/markdown/CHANGELOG-v5.3.0.md", "f8269a0e728059eafba6417dfd5c5b3c69483a26", 3917),
    ("packcore/markdown/CHANGELOG-v5.3.1.md", "e30ad3cbb47a3fd56538bc1628fa33d35c3f0e80", 700),
    ("packcore/markdown/CHANGELOG-v5.3.2.md", "946a488fd432fd485293a0cc03969e75afbb081b", 1188),
    ("packcore/markdown/CHANGELOG-v5.3.3.md", "c240b02871c81537751284342731e24e88b15542", 747),
    ("packcore/markdown/CHANGELOG-v5.3.4.md", "7bc42d571684a0a7df58465905a80aabc8443805", 1218),
    ("packcore/markdown/CHANGELOG-v5.3.5.md", "ed389ab83084592c735c5a69bcbd10c9048454ec", 1041),
    ("packcore/markdown/CHANGELOG-v5.3.6.md", "1189e5f8b683dca88b13179e2d2158c9ef674fae", 784),
    ("packcore/markdown/CHANGELOG-v5.4.0.md", "314ae3d8581a1e68717a168149c779e0c10eee7c", 1182),
    ("packcore/markdown/CHANGELOG-v5.4.1.md", "d8683d73bd4362fae0943d93951fa18ad0165f2e", 1210),
    ("packcore/markdown/CHANGELOG-v5.4.2.md", "db534b88109490b76d3b35e38c34cbcd9ff08e95", 976),
    ("packcore/markdown/CHANGELOG-v5.4.3.md", "df0d7a44691a2f0b3614a6158174056c1a34a90b", 843),
    ("packcore/markdown/CHANGELOG-v5.4.4.md", "cb9fa5b48a8991cf73e6287e8387748d92212942", 1098),
    ("packcore/markdown/CHANGELOG-v5.4.5-hotfix-1.md", "5a115233f7693e49d126ff08efbe86a9a49284fc", 866),
    ("packcore/markdown/CHANGELOG-v5.4.5.md", "c51d8188225706abb8d6ffb136b3207ab5e728bb", 798),
    ("packcore/markdown/config_packs.md", "c58bdd9c24d33678249e3caa39803616fba2c05c", 592),
    ("packcore/markdown/performance.md", "50536aed351ed31f4f8dbd048651266546d76da3", 2452),
    ("packcore/markdown/release-body-v5.3.0-beta.1.md", "7f865913c9d6216c39f21af58f85430c529da071", 4199),
    ("packcore/markdown/release-body-v5.3.0-beta.2.md", "78317581dda2ea60ebadba29e51fa46c63d3a188", 909),
    ("packcore/markdown/release-body-v5.3.0-beta.3.md", "df8e40d5e2c7b74a68a70661b854e253619484c0", 1122),
    ("packcore/markdown/release-body-v5.3.0.md", "f8269a0e728059eafba6417dfd5c5b3c69483a26", 3917),
    ("packcore/markdown/release-body-v5.3.1.md", "e30ad3cbb47a3fd56538bc1628fa33d35c3f0e80", 700),
    ("packcore/markdown/release-body-v5.3.2.md", "946a488fd432fd485293a0cc03969e75afbb081b", 1188),
    ("packcore/markdown/release-body-v5.3.3.md", "c240b02871c81537751284342731e24e88b15542", 747),
    ("packcore/markdown/release-body-v5.3.4.md", "7bc42d571684a0a7df58465905a80aabc8443805", 1218),
    ("packcore/markdown/release-body-v5.3.5.md", "ed389ab83084592c735c5a69bcbd10c9048454ec", 1041),
    ("packcore/markdown/release-body-v5.3.6.md", "1189e5f8b683dca88b13179e2d2158c9ef674fae", 784),
    ("packcore/markdown/release-body-v5.4.0.md", "314ae3d8581a1e68717a168149c779e0c10eee7c", 1182),
    ("packcore/markdown/release-body-v5.4.1.md", "d8683d73bd4362fae0943d93951fa18ad0165f2e", 1210),
    ("packcore/markdown/release-body-v5.4.2.md", "db534b88109490b76d3b35e38c34cbcd9ff08e95", 976),
    ("packcore/markdown/release-body-v5.4.3.md", "df0d7a44691a2f0b3614a6158174056c1a34a90b", 843),
    ("packcore/markdown/release-body-v5.4.4.md", "cb9fa5b48a8991cf73e6287e8387748d92212942", 1098),
    ("packcore/markdown/release-body-v5.4.5-hotfix-1.md", "5a115233f7693e49d126ff08efbe86a9a49284fc", 866),
    ("packcore/markdown/release-body-v5.4.5.md", "c51d8188225706abb8d6ffb136b3207ab5e728bb", 798),
    ("packcore/markdown/resource_packs.md", "860ce11ce0223bab5ef53040b6c7542c24a8b132", 2818),
    ("packcore/markdown/welcome.md", "5847280b92b47b2926785c6b92cf929d91dc815b", 612),
    ("packcore/modpack.json", "28334a83d27075ca09ec2ff2de22944f66617476", 669),
];

/// 同一个真包的 `client-overrides/`（全部 2 条）。
#[rustfmt::skip]
const SKYBLOCK_CLIENT: [(&str, &str, u64); 2] = [
    ("config/crash_assistant/config.toml", "638d3ef805660df14043c5075066c2622a9ac0a2", 23848),
    ("config/crash_assistant/modlist.json", "afb5d4dae2b42a996d39eeccdcc178e9660ecf54", 19675),
];

/// 真包叠层（M337）：一个**真**带 `overrides/` + `client-overrides/` 的包跑一遍叠层。
///
/// 为什么值得：这两条目录的叠层以前只有合成夹具跑过（`tests::the_later_layer_wins_*` 那四条），
/// 真包里长什么样、两个目录是不是真的都进计划、顺序对不对，一次都没验过。
///
/// 真包的事实（本用例钉住的就是这几条）：
///
/// 1. 两个目录**都在**：`overrides/` 46 条、`client-overrides/` 2 条（`config/crash_assistant/`）；
/// 2. 两份目录**没有同路径重复**（46 ∩ 2 = ∅）⇒ 真包上「后者胜」那一支**触发不到**，
///    遮蔽判据仍由 `tests` 里那四条合成用例守（别把这条读成「真包验过遮蔽」）；
/// 3. 模型里**故意把 `client-overrides/` 放在前面**（真包 zip 里它在后面）：叠层顺序是
///    `ordered` 按**侧**算的，不许吃模型/条目顺序——顺序吃错了这里立刻红。
///
/// **走的是产品那一步**（M337 r1 / 复审 P2-2）：`super::plan_overrides` 就是 `ImportPlan::build`
/// 调的那个函数，本用例不再自带一份展平循环。所以把 `ordered` 的顺序改掉、或者不跳 `Server` 侧，
/// 这里就红。
///
/// 抓不住：`PackArchive`/`Modrinth` 读真 zip 的行为（那一段要真二进制，只能照 provenance 那几步
/// 一次性复核）；`plan.rs` 里「跳过 N 个 server 文件就落一条注记」那 3 行。
#[test]
fn a_real_pack_with_both_override_dirs_layers_client_last() {
    let manifest = skyblock_manifest();
    let mut notes = Vec::new();
    let (overrides, skipped_server) = plan_overrides(&manifest, &mut notes);

    assert_eq!(overrides.len(), 48, "真包的 46 + 2 条都要进计划");
    assert_eq!(skipped_server, 0, "这个包没有 server-overrides/");
    assert!(
        notes.is_empty(),
        "真包两份目录没有同路径重复 ⇒ 一条遮蔽注记都不该有：{notes:?}"
    );
    assert!(overrides.iter().all(|entry| entry.shadowed.is_none()));

    // 顺序：`overrides/`（Both）在前、`client-overrides/`（Client）在后。
    assert!(
        overrides[..46]
            .iter()
            .all(|entry| entry.source == FileSource::Overrides && entry.dir == "overrides"),
        "客户端那份不按侧排就会混进前 46 条"
    );
    assert_eq!(overrides[46].source, FileSource::ClientOverrides);
    assert_eq!(overrides[46].dir, "client-overrides");
    assert_eq!(overrides[46].path, "config/crash_assistant/config.toml");
    assert_eq!(overrides[46].size, 23848);
    assert_eq!(overrides[47].path, "config/crash_assistant/modlist.json");

    // 逐条对账：真包里的路径与哈希（照抄模块文档 provenance 那一份，不编）。
    for (index, (path, sha1, size)) in SKYBLOCK_BOTH.iter().enumerate() {
        assert_eq!(overrides[index].path, *path, "第 {index} 条路径");
        assert_eq!(overrides[index].sha1, *sha1, "{path} 的哈希");
        assert_eq!(overrides[index].size, *size, "{path} 的字节数");
    }
    for (offset, (path, sha1, size)) in SKYBLOCK_CLIENT.iter().enumerate() {
        let entry = &overrides[46 + offset];
        assert_eq!(entry.path, *path);
        assert_eq!(entry.sha1, *sha1);
        assert_eq!(entry.size, *size);
    }
}

/// 真包的清单：**故意**把 `client-overrides/` 排在 `overrides/` 之前（见用例文档第 3 条）。
fn skyblock_manifest() -> PackManifest {
    PackManifest {
        overrides: vec![
            group("client-overrides", PackSide::Client, &SKYBLOCK_CLIENT),
            group("overrides", PackSide::Both, &SKYBLOCK_BOTH),
        ],
        ..PackManifest::new("SkyBlock Enhanced")
    }
}

fn group(dir: &str, side: PackSide, files: &[(&str, &str, u64)]) -> PackOverrides {
    PackOverrides {
        dir: dir.to_string(),
        side,
        files: files
            .iter()
            .map(|(path, sha1, size)| OverrideFile {
                path: (*path).to_string(),
                sha1: (*sha1).to_string(),
                size: *size,
            })
            .collect(),
    }
}
