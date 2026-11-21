//! 实例页这一族的**范围定义**（M338）：目录派生之后，「谁参加」由这里说清——**不再是手写清单**。
//!
//! 扫描器本身是共用的（`pages/tests/scan.rs`，三张表同一套）；这里只放本页的两条决定：
//!
//! 1. **族** = `pages/instances.rs`（页根）+ `pages/instances/**`（非测试文件），从磁盘发现
//!    ⇒ 新加的页面源文件自动参加；
//! 2. **排除** = 夹具 `instances/data.rs`（`1.20.4` / `4 GiB` / `2 hours ago` 是「实例清单会长成
//!    什么样」的样本，不是界面文案，见该文件的模块文档）——排除**自带存在性检查**：文件没了就
//!    说明理由作废，判据会红着让人回来删掉它。
//!
//! 为什么这两件事不放 `pages/tests/scan.rs`：那是全 crate 共用的扫描器，不该认识「实例页」。

use crate::pages::tests::scan;

/// M338 实测：`pages/instances.rs` + `pages/instances/**` 下的非测试文件有 16 个。
///
/// 下界而不是等号：加文件不该假红；**清空 / 扫不到**必须红（空集合上的全称断言恒真）。
const KNOWN_FAMILY_SOURCES: usize = 16;

/// 本页那一族的源码（死 key 扫描、反向锚都用它）。
///
/// 空集保护就在派生点上（[`KNOWN_FAMILY_SOURCES`]），所以读它的每个用例都受保护。
pub(super) fn family() -> Vec<scan::Source> {
    let sources = scan::family("instances");
    assert!(
        sources.len() >= KNOWN_FAMILY_SOURCES,
        "实例页这一族只扫到 {} 个源文件（已知至少 {KNOWN_FAMILY_SOURCES} 个）：目录派生读不到\
         源码树了，下面每条判据都会变成空转",
        sources.len()
    );
    sources
}

/// 页面**取词**的源码（字面量白名单用）：族里的产品部分 + `app/view/instances.rs`。
pub(super) fn text_sources() -> Vec<scan::Source> {
    let mut sources = family();

    let fixture = "instances/data.rs";
    assert!(
        sources.iter().any(|file| file.name == fixture),
        "{fixture} 不在了：「夹具不是文案」这条排除条件跟着作废，回来删掉它"
    );
    sources.retain(|file| file.name != fixture);

    // 产品把 presenter 快照翻成页面文案的那一层在 `pages/` 树外，`scan::family` 扫不到，
    // 所以单独点名；它搬家时 `scan::source_at` 直接 panic（自己红，不用人记）。
    sources.push(scan::source_at("app/view/instances.rs"));

    sources
}
