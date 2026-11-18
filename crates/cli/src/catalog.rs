//! CLI 人读输出取哪一份文案：**中文档**，用的 key 与 GUI 同一套。
//!
//! CLI 的整段人读措辞（`整合包 … → …`、`结果：…`）本来就是中文；M323 起「把 core 的稳定 id
//! 原样印给用户」的地方改成「目录里那句人话」，**语言档位不动**。跟随 `ui.language` 是另一件事
//! （要接 `platform::locale` 的探测链，且会让中文输出里混进英文），不在本轮。
//!
//! 本模块只放**两处以上共用**的东西：`pack import` 的处理器汇总行与 `java list` 的来源列
//! 说的是同一件事实（这份 Java 是哪来的），所以取文案的映射只有一份。只有 `pack` 自己用的
//! 映射仍留在 `pack/lines.rs`。

use unlucky_core::game::java::JavaOrigin;
use unlucky_i18n::{Catalog, Key, Lang};

/// CLI 人读输出用的目录：**中文档**。
pub(crate) const CATALOG: Catalog = Catalog::of(Lang::ZhCn);

/// Java 来源 → 目录 key。**穷尽匹配**：core 的 `JavaOrigin` 多一档来源，这里编译不过。
///
/// 五条各说一件不同的事（实例指定的 / 启动器装的 / 环境变量指的 / 系统路径上找到的 / 系统默认
/// 装的），措辞在 `crates/i18n/src/catalog/pack_java.rs`（M325，逐条审过）；`--json` 的
/// `source` / `java_origin` 仍印稳定 id（脚本要它）。
const fn java_origin_key(origin: JavaOrigin) -> Key {
    match origin {
        JavaOrigin::Instance => Key::PackProcessorJavaFromInstance,
        JavaOrigin::Managed => Key::PackProcessorJavaFromLauncher,
        JavaOrigin::JavaHome => Key::PackProcessorJavaFromEnvVar,
        JavaOrigin::Path => Key::PackProcessorJavaFromPath,
        JavaOrigin::System => Key::PackProcessorJavaFromSystem,
    }
}

/// Java 来源 → 人读那一句（`pack import` 的处理器汇总行与 `java list` 的来源列共用）。
///
/// 两处落点共用同一个穷尽匹配，所以以后 core 多一档来源是**两边一起编译不过**，
/// 不会像分头写 match 那样漏出一侧、把一个 slug 放回用户眼前。
pub(crate) fn java_origin_text(origin: JavaOrigin) -> &'static str {
    CATALOG.text(java_origin_key(origin))
}
