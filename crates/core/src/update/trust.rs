//! 钉进二进制的清单签名信任锚（keyring）：**多把**公钥，`key_id` 只是查表用的标签。
//!
//! # 为什么是一张编译期常量表
//!
//! 自更新的信任根必须不可被「用户机器上的一枚文件」替换：清单与签名都能被能改 Release 的人一起
//! 换掉，唯一挡得住的是「客户端**编进二进制**的那几把公钥」。所以这张表是 `const`：
//!
//! * 改仓库里的本文件只会改变**下一次构建**的行为，不会改变任何**已发布二进制**的验签结果；
//! * 没有环境变量、没有设置键、没有「旁边的 keyring.json」这类运行期覆盖口子
//!   （`tests/signature_cases.rs::no_runtime_keyring_override` 守这条）。
//!
//! # 为什么是「一组」而不是「一把」
//!
//! 换签名方（例如日后接 SignPath 的托管签名，或旧钥泄露后轮换）时，用户机器上装着的是**旧**二进制，
//! 它只认旧钥。做成数组才有两条路同时成立：新签出的清单可以同时带新旧两把钥的签名（旧二进制认旧的、
//! 新二进制认新的），以及新二进制可以一把一把地收紧。步骤见 `packaging/docs/signing.md`。
//!
//! # 唯一的加钥匙纪律：**旧钥签新钥**
//!
//! 往本表里加一把公钥，必须同时有「旧钥对新钥的背书」——即那次发布的清单同时带旧钥签名（见上），
//! 以及 changelog 里点名新钥的 `key_id` 与指纹。**不允许**在没有旧钥签名的情况下替换这张表：
//! 那等于让「能改仓库的人」变成「能改信任根的人」。
//!
//! # 正式构建的表当前是**空的**（fail-closed）
//!
//! 私钥只能来自仓库 Secret，本轮没有（也不该由代码仓库产生）私钥，所以正式表里一个条目都没有：
//! 任何正式构建拿到清单都会以 [`super::error::UpdateErrorKind::SignatureMissing`] 之类的分类拒绝更新，
//! 而 `packaging/sign_manifest.py --check-pinned-keyring` 会让发布流水线在「Secret 没配 / 公钥没钉」
//! 时**当场红**（宁可不出 Release，也不出未签名的 Release）。
//!
//! 录一把真钥的两步（细节见 `packaging/docs/signing.md`）：
//!
//! ```text
//! python packaging/sign_manifest.py --generate-key --out-key release.key   # ① 生成，私钥写进 Secret
//! # ② 把打印出来的 `PinnedKey { … }` 一行贴到下面 PINNED-KEYS 标记之间
//! ```
//!
//! 测试/验收构建另有一把**只在** `cfg(test)` / `update-test-source` / `update-test-key` 三档里存在的
//! 测试钥（三档的口径见下面 `keyring()`）：它的私钥种子就写在测试夹具里、公开可见，因此它可以签出
//! 「本仓库自己认得」的清单，但对任何正式构建毫无意义（正式表里没有它，`keyring()` 也不会把它并进
//! 信任表，而且那条目根本不进编译 —— 三档都不开的产物里连它的字节都没有，读数见 `keyring()`）。
//! 后两档（那两个 feature）各由 `packaging/check_test_source_off.py` /
//! `check_test_key_off.py` 守着 ——正式发布流水线与打包脚本都不许打开它们。
//! （`cfg(test)` 那档不需要守门：发布构建里根本没有 `test` cfg。）

use std::sync::OnceLock;

/// 一把钉死的公钥。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PinnedKey {
    /// 标签（清单签名里的 `key_id` 靠它查表；**改名就是换钥**）。
    pub(crate) key_id: &'static str,
    /// Ed25519 公钥的 64 位十六进制小写形态（32 字节）。
    pub(crate) public_key_hex: &'static str,
}

/// 正式构建的信任锚（发布二进制里就是这一份）。
///
/// ── 加钥匙只改下面 `PINNED-KEYS` 标记之间的内容，别改标记本身 ──
/// （`packaging/sign_manifest.py --check-pinned-keyring` 按标记解析这一段，标记变了它就找不到表）。
pub(crate) const PRODUCTION: &[PinnedKey] = &[
    // >>> PINNED-KEYS >>>
    PinnedKey {
        key_id: "ull-release",
        public_key_hex: "1c3687ecd656a29fda87a7afb56e07d2b9e223b62cf3a6f1a901fb499a97b122",
    },
    // <<< PINNED-KEYS <<<
];

/// 测试钥：**只**在 `cfg(test)` / `update-test-source` / `update-test-key` 三档构建里存在（正式表里没有它；
/// 三档的口径见下面 `keyring()`）。
///
/// 公钥由 `tests/support.rs` 里那串公开可见的 32 字节种子推出来（种子 = `ull-m190-test-key-seed-000000001`），
/// 两者一旦漂开，`tests/signature_cases.rs::pinned_test_key_matches_the_test_seed` 会当场红。
///
/// ⚠ 这个 `#[cfg]` 是**编译期剔除**，不是 `cfg!()` 那种「运行期值为假的分支」：不在这三档里时，
/// 下面这条条目根本不进编译（只剩再下面那条同名空表），所以那串十六进制与它的 `key_id`
/// 在产物里连字节都没有 —— 读数是实测的，见 `keyring()` 文档末尾那段。
#[cfg(any(test, feature = "update-test-source", feature = "update-test-key"))]
pub(crate) const TEST_ONLY: &[PinnedKey] = &[PinnedKey {
    key_id: "test-only-key",
    public_key_hex: "f3ea49a34ebda73d6500c9416f425e0358baf094b957baf3ea7daa6df63e876b",
}];

/// 三档之外的 `TEST_ONLY`：空表。
///
/// 它存在的唯一目的，是让 [`keyring()`] 里那段「把测试钥并进来」的代码在**两档里写法一致**
/// （`keys` 的 `mut` 在两档里都有用处，不至于为 `unused_mut` 单独开 lint 口子）。空表本身就是
/// 「这段字节不在产物里」的全部内容 —— 真条目在上一档里。
#[cfg(not(any(test, feature = "update-test-source", feature = "update-test-key")))]
pub(crate) const TEST_ONLY: &[PinnedKey] = &[];

/// 本构建实际的信任锚（正式表 + 仅测试构建才有的那把测试钥）。
///
/// 三档「仅测试」的口径，逐档说明为什么需要它：
///
/// | 档 | 谁在用 | 为什么不能只靠 `cfg(test)` |
/// |---|---|---|
/// | `cfg(test)` | core 自己的单测 | 够用 |
/// | `update-test-source` | CLI 的端到端验收（`--features update-test-source`） | 那是**另一个 target**，`cfg(test)` 不生效 |
/// | `update-test-key` | `crates/presenter` 的 `[dev-dependencies]` | presenter 的更新链路用例编译 core 时既无 `cfg(test)`，也不该打开 `update-test-source`（那条会同时给出「本地目录当更新源」这条路）。这一档**只**加一把公开的测试公钥，不开任何数据源开关 |
///
/// 三档都不开的构建里，这把测试钥**既不被并进信任表，也不会被编进产物**（官方构建三个 feature 都不开，
/// 且 `check_test_source_off.py` / `check_test_key_off.py` 守着）。
///
/// 这是**编译期**结论，不是运行期判据：门是 `#[cfg]`（三档之外 `TEST_ONLY` 只剩上面那条空表，
/// 真条目根本不进编译），而不是 `cfg!()` 那种「运行期值为假的分支」—— 后者会把那段字节照编进产物。
///
/// 实测读数（本机 Windows + Git Bash，`grep -ac` 数的是命中行数，`grep` 退出码 1 = 零命中；
/// `$HEX` = 上文 `TEST_ONLY` 里的 `public_key_hex`，`$KID` = 那条的 `key_id`）：
///
/// ```text
/// $ CARGO_TARGET_DIR=... cargo build -p unlucky-core --lib                      # exit 0
/// $ grep -ac "$HEX" .../debug/libunlucky_core.rlib                              # 0
/// $ grep -ac "$KID" .../debug/libunlucky_core.rlib                              # 0
/// $ CARGO_TARGET_DIR=... cargo build -p unlucky-core --lib --release            # exit 0
/// $ grep -ac "$HEX" .../release/libunlucky_core.rlib                            # 0
/// $ grep -ac "$KID" .../release/libunlucky_core.rlib                            # 0
/// $ CARGO_TARGET_DIR=...-key cargo build -p unlucky-core --lib --features update-test-key   # exit 0
/// $ grep -ac "$HEX" ...-key/debug/libunlucky_core.rlib                          # 2
/// $ grep -ac "$KID" ...-key/debug/libunlucky_core.rlib                          # 2
/// ```
///
/// （最后两行是对照臂：真打开那一档时这两串**都在**，所以前六行的 0 不是「grep 坏了」。）
///
/// ⚠ 这份读数只到 **rlib 为止**：core 的 `--lib` 产物里没有那两串字节。**未验**的是最终链接出来的
/// exe 在 `lto = "fat"` + `strip = "symbols"` 之后是否还有那段字节 —— 那是链接期与 strip 的事，
/// 本条的判据（编译期剔除）管不到，别把上面六行读成「exe 里也不可能有」。另：无论有没有那段字节，
/// 验签都 fail-closed（信任表里没有这把钥）。
pub(crate) fn keyring() -> &'static [PinnedKey] {
    static ALL: OnceLock<Vec<PinnedKey>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut keys = PRODUCTION.to_vec();
        // 三档之外 `TEST_ONLY` 是上面那条空表，这一段等于什么都没并；三档之内才是真条目。
        keys.extend_from_slice(TEST_ONLY);
        keys
    })
}

/// 十六进制解码（只收小写/大写十六进制，长度必须是偶数）。
pub(crate) fn decode_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let (pairs, _) = bytes.as_chunks::<2>();
    for pair in pairs {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push(u8::try_from(hi * 16 + lo).ok()?);
    }
    Some(out)
}
