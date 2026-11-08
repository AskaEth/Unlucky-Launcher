//! 版本 id → 加载器：**全仓唯一**的 id 解析入口（M122），外加「种类 → 对外稳定 id」的命名词表
//! **源头**（M126，见 [`LoaderKind::as_str`]；副本只有一处，见下）。
//!
//! # 为什么要这一个模块
//!
//! 「这个实例装的什么加载器」在 UI 上是一句话，代码里却有两个来源：版本 JSON 的 `libraries[]`
//! （权威但要多读一次盘）与**版本 id 的字符串**（磁贴/列表最省事的那条）。只要有人按 id 猜就
//! 必须只有**一处**在猜——两处各算一遍迟早一边先漂，用户看到的就是「我刚装的 Fabric 显示成
//! 原版」（M120 复审的老 bug：解析端只认 `fabric-loader-…`，认不出自家生成的
//! `1.20.4-fabric-0.16.5`）。所以 presenter 的实例页、CLI 的实例总览、以后任何「按 id 认加载
//! 器」的地方一律调 [`InstanceLoader::parse`]，**不许**再写一份标记匹配。生成端
//! （[`crate::addon::loader::LoaderKind::version_id`] 拼 `<mc>-<kind>-<loader>`）与解析端的一致
//! 性由 [`tests::own_version_ids_parse_back_to_their_loader`] 钉住（**从生成端取 id**，不手抄）；
//! OptiFine / LiteLoader 本启动器不能装（见 `presenter::host::install`），只能继承进来。
//!
//! # 命名词表只有一张表（M126）
//!
//! 契约（0.2.0 已发布形状，**不许改名**）：`vanilla` / `fabric` / `quilt` / `forge` /
//! `neoforge` / `optifine` / `liteloader`。**生产代码里这份词表有两处，都要改**：① 下面那次
//! `loader_kind_vocab!` 展开——枚举、`ALL`、`as_str`、`has_marker` 全从同一张表长出来，所以
//! 「加了一支却漏进清单」物理上做不到（理由见宏的注释）；② 会打印给用户看的
//! `crates/cli/src/cli/help.rs` 里 `ull help instances` 的词表行——**逐字副本**、没有委派，
//! 由 CLI 的 `help_lists_every_stable_id`（按 `ALL` 遍历）守着。安装侧与 presenter 的 `as_str` /
//! `parse` 都是**委派**，CLI 的 `loader.kind` 直接调；唯一不进词表的字面量是安装侧收的输入别名
//! `neo-forge`（Modrinth 拼法，只进不出）。
//!
//! 加一家加载器要动：① `loader_kind_vocab!` 表里加一行（id + 认不认标记）→ 枚举 / `ALL` /
//! `as_str` / `has_marker` / 解析顺序一起长出来，漏不了；② presenter 的 `to_core`·`from_core`
//! **编不过**（逼你决定 DTO 怎么显示它），能装的话安装侧 `as_str`·`from_core` 也各差一支；
//! ③ 契约测试**会红**：presenter 的 `loader_kind_ids_are_the_released_contract` 拿 core 的 `ALL`
//! 与独立手写的七个 id 做**集合比较**，CLI 的 `help_lists_every_stable_id` 按 `ALL` 遍历 help
//! 词表，`json_rows_*` 钉真二进制输出面（0.2.0 已发出去过，所以这步是确认而不是可选）。
//!
//! ⚠️ 分工要看准：**编译器只拦「漏 match」**，新种类在消费方用 `=> Vanilla` 糊过去编得过。
//! ③ 那条集合断言拦的是**对外词表**；**显示路径另有守卫**（M134 起）：`presenter/src/host/game.rs::dto_kind`
//! 不再是自己一张手写 match，而是拿 core 的稳定 id 反查 DTO 词表——想在那一支糊 `=> Vanilla` 已经
//! 没有 match 可写，加一家加载器会在 `dto::LoaderKind::from_core` 先编不过；糊错由 presenter 的
//! `loader_kind_ids_are_the_released_contract`、以及按 `ALL` 遍历不手抄清单的
//! `every_core_loader_kind_reaches_the_tile_as_itself` 钉住。**改名**同理：往返测试绿，只有 ③ 会红。
//!
//! # 解析规则（按整段前缀匹配，大小写不敏感，**最靠左的标记胜**）
//!
//! 1. 把 id 按 `-` 切段，从左往右找第一段，它的**前缀**是某个加载器标记
//!    （`neoforge` / `fabric` / `quilt` / `forge` / `optifine` / `liteloader`）；紧跟的 `loader`
//!    段（`fabric-loader-…`）或同段里的 `_loader` 尾巴算标记的一部分，不是版本段；
//! 2. 该段里标记之后剩下的部分，加上它右边的所有段，按 `-` 拼回版本段
//!    （`_` 之类的原样保留，所以 `1.13.2-OptiFine_HD_U_E6` 的版本是 `HD_U_E6`）；
//! 3. 例外：版本段 **≥2 段**且末段像 MC 版本（`1.20.1` 这种纯数字点号）时砍掉末段——老 HMCL 的
//!    `fabric-loader-<loader>-<mc>` 写法；`0.16.5`（只剩一段）与 `0.20.0-beta.9` 不受影响；
//! 4. 一个标记都没找到 ⇒ 原版（版本段空串）。只把加载器写进版本 JSON、id 里没痕迹的实例
//!    （整包导入时被重命名成 `我的整合包` 那种）会**诚实**返回「原版」——那不等于没装加载器。

/// 一张表生成整份词表：枚举、[`LoaderKind::ALL`]、`as_str`、`has_marker` 一起长出来。
///
/// 为什么非要用宏：Rust 没有「枚举有哪几支」的编译期反射，手写枚举 + 手写清单时「加了一支却
/// 忘了进清单」既不报错也不会被测试抓住（测试同样从清单出发），新加载器被**静默**当成「认不
/// 出」；让两者从同一张表展开，这件事在物理上不可能发生。表里的顺序就是标记的匹配优先级；
/// 带 `marker` 的条目参与 id 识别，不带的（原版）不参与。
macro_rules! loader_kind_vocab {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$variant_doc:meta])*
                $variant:ident => $id:literal $(, $marker:ident)?;
            )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $vis enum $name {
            $(
                $(#[$variant_doc])*
                $variant,
            )+
        }

        impl $name {
            /// 全部种类，**顺序即标记的匹配优先级**；原版没有标记（由 `has_marker` 跳过）。
            pub const ALL: [Self; loader_kind_vocab!(@count $($variant)+)] =
                [$(Self::$variant,)+];

            /// 对外稳定 id（`--json` 的 `loader.kind`、GUI 词表、`--loader` 的取值）。
            ///
            /// 这是**生产代码里 id 的源头表**（0.2.0 已发布契约）：别处要么委派到这里，要么是逐字
            /// 副本（`ull help instances` 的词表行，由 CLI 的 `help_lists_every_stable_id` 守着）。
            /// 穷尽 match：加一支就编不过。
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $id,)+
                }
            }

            /// 这一支会不会从版本 id 的标记里认出来（原版不会）。穷尽 match：加一支必须表态。
            #[must_use]
            pub const fn has_marker(self) -> bool {
                match self {
                    $(Self::$variant => loader_kind_vocab!(@marker $($marker)?),)+
                }
            }
        }
    };
    (@marker marker) => { true };
    (@marker) => { false };
    (@count) => { 0 };
    (@count $head:ident $($tail:ident)*) => { 1 + loader_kind_vocab!(@count $($tail)*) };
}

loader_kind_vocab! {
    /// 实例面向的加载器种类（比 [`crate::addon::loader::LoaderKind`] 宽）。
    ///
    /// 那个类型只描述**本启动器能装**的四家（装不了就没有下载路线）；本类型面向**识别与展示**，
    /// 多出「原版」这一支，以及两家只能继承进来的第三方加载器。
    pub enum LoaderKind {
        /// 原版（id 里没有任何加载器标记）
        Vanilla => "vanilla";
        /// NeoForge（必须排在 `Forge` 前：两者都是段前缀匹配，顺序写死能防以后宽成「包含」）
        NeoForge => "neoforge", marker;
        /// Fabric
        Fabric => "fabric", marker;
        /// Quilt
        Quilt => "quilt", marker;
        /// Forge
        Forge => "forge", marker;
        /// OptiFine（第三方，本启动器不装，只继承）
        OptiFine => "optifine", marker;
        /// LiteLoader（第三方，本启动器不装，只继承）
        LiteLoader => "liteloader", marker;
    }
}

impl LoaderKind {
    /// 稳定 id → 种类（[`Self::as_str`] 的反查；整串全等、大小写不敏感、不认前缀与别名）。
    ///
    /// 别和 [`InstanceLoader::parse`] 搞混：那个认的是**版本 id**（切段、认前缀、给版本段）。
    /// 别名 `neo-forge` 只有安装侧收，见 [`crate::addon::loader::LoaderKind::parse`]。
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let wanted = raw.trim();
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str().eq_ignore_ascii_case(wanted))
    }
}

/// 一个版本 id 解析出来的加载器：种类 + 版本段（原版为空串）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceLoader {
    /// 加载器种类
    pub kind: LoaderKind,
    /// 加载器版本段（id 里标记之后的那一截）；原版或 id 里没写版本时为空串
    pub version: String,
}

impl InstanceLoader {
    /// 从版本 id 解析加载器（纯字符串计算，不读盘）。
    ///
    /// 规则见模块文档；认不出标记时返回 [`LoaderKind::Vanilla`] + 空版本段。
    #[must_use]
    pub fn parse(version_id: &str) -> Self {
        let segments: Vec<&str> = version_id.trim().split('-').collect();
        for (index, segment) in segments.iter().enumerate() {
            let Some((kind, rest)) = marker_of(segment) else {
                continue;
            };
            // `fabric-loader-0.16.5-1.20.1` 按 `-` 切完是 `["fabric", "loader", …]`，
            // `loader` 自己占一段；版本段要从它后面算，否则会读出 `loader-0.16.5`。
            let mut next = index + 1;
            while segments
                .get(next)
                .is_some_and(|segment| segment.eq_ignore_ascii_case("loader"))
            {
                next += 1;
            }
            let mut parts: Vec<&str> = Vec::new();
            if !rest.is_empty() {
                parts.push(rest);
            }
            parts.extend(segments.iter().skip(next).copied());
            let version = join_version(&parts);
            return Self { kind, version };
        }
        Self {
            kind: LoaderKind::Vanilla,
            version: String::new(),
        }
    }
}

/// 这一段的**前缀**是哪个加载器标记；命中时连它后面剩下的部分一起给出。
///
/// 标记串 = [`LoaderKind::as_str`]，顺序 = [`LoaderKind::ALL`]（即优先级），认不认标记由
/// [`LoaderKind::has_marker`] 表决：**解析表没有独立字面量，也没有独立清单**。
fn marker_of(segment: &str) -> Option<(LoaderKind, &str)> {
    for kind in LoaderKind::ALL.into_iter().filter(|kind| kind.has_marker()) {
        if let Some(rest) = strip_prefix_ascii_ci(segment, kind.as_str()) {
            return Some((kind, strip_loader_suffix(rest)));
        }
    }
    None
}

/// 去掉标记后面可选的 `loader` 后缀（版本段从 `0.16.5` 起算，不是 `loader-0.16.5`）。
fn strip_loader_suffix(rest: &str) -> &str {
    let trimmed = trim_separators(rest);
    match strip_prefix_ascii_ci(trimmed, "loader") {
        Some(after) => trim_separators(after),
        None => trimmed,
    }
}

/// 掐头去尾的 `-` / `_`（`OptiFine_HD_U_E6` 的标记后面紧跟的是 `_`）。
fn trim_separators(text: &str) -> &str {
    text.trim_matches(['-', '_'])
}

/// 大小写不敏感的 `strip_prefix`（用的是 ASCII 折叠，版本 id 里不会出现非 ASCII 标记）。
fn strip_prefix_ascii_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let head = text.get(..prefix.len())?;
    if head.eq_ignore_ascii_case(prefix) {
        text.get(prefix.len()..)
    } else {
        None
    }
}

/// 把标记右边剩下的段拼成版本段，并按「老 HMCL 的 `<loader>-<mc>` 尾巴」规则砍掉 MC 版本。
fn join_version(parts: &[&str]) -> String {
    let mut kept = parts.to_vec();
    if kept.len() >= 2 && kept.last().is_some_and(|last| looks_like_mc_version(last)) {
        kept.pop();
    }
    kept.join("-")
}

/// 这一段长得像不像 MC 版本（`1.20.1` 这种全数字 + 点号）；只用于「砍 HMCL 尾巴」（要求前面
/// 还有内容，所以 `0.16.5` 与 `0.20.0-beta.9` 都不受影响）。
fn looks_like_mc_version(text: &str) -> bool {
    text.contains('.')
        && text
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 自家 id 的真值表：**从生成端取 id**，不手抄。
    ///
    /// 抓得住：生成端换了 id 形状而解析端没跟、解析端退回只认老 HMCL 口径（M122 的 bug）。
    /// 抓不住：加载器版本自己带 `-` 的**其他**情形（`0.16.5-beta.1` / `0.20.0-beta.9` 两个
    /// 样本专为它写；Forge/NeoForge 的真实版本号不带 `-`，出现了只能靠人补样本）。
    #[test]
    fn own_version_ids_parse_back_to_their_loader() {
        use crate::addon::loader::LoaderKind as InstallKind;

        let cases: [(InstallKind, LoaderKind, &str, &str); 6] = [
            (InstallKind::Fabric, LoaderKind::Fabric, "1.20.4", "0.16.5"),
            (
                InstallKind::Fabric,
                LoaderKind::Fabric,
                "1.20.4",
                "0.16.5-beta.1",
            ),
            (InstallKind::Quilt, LoaderKind::Quilt, "1.20.4", "0.27.0"),
            (
                InstallKind::Quilt,
                LoaderKind::Quilt,
                "1.20.1",
                "0.20.0-beta.9",
            ),
            (InstallKind::Forge, LoaderKind::Forge, "1.20.1", "47.2.0"),
            (
                InstallKind::NeoForge,
                LoaderKind::NeoForge,
                "1.21.1",
                "21.1.80",
            ),
        ];
        for (install, kind, mc, loader_version) in cases {
            let id = install.version_id(mc, loader_version);
            let parsed = InstanceLoader::parse(&id);
            assert_eq!(parsed.kind, kind, "id `{id}` 认成了 {:?}", parsed.kind);
            assert_eq!(parsed.version, loader_version, "id `{id}` 的版本段");
        }
    }

    /// 第三方前缀（OptiFine / LiteLoader）：它们**不是** `<mc>-<kind>-<ver>` 口径。
    ///
    /// 样本来源：PCL 的 `Modules/Minecraft/ModDownload.vb:441`（`-OptiFine_…`）与
    /// `Pages/PageDownload/ModDownloadLib.vb:793`（`-LiteLoader`，**没有版本段**）；`_pre1`
    /// 尾巴来自 OptiFine 自己的 preview 命名。
    /// 抓得住：标记被当成「必须带 `-`」或「必须小写」（旧口径 `split_once("optifine")`）。
    /// 抓不住：OptiFine 的 maven 坐标形式（`optifine:OptiFine:1.12.2_HD_U_C7`），未做特判。
    #[test]
    fn third_party_prefixes_are_recognized() {
        let cases = [
            ("1.13.2-OptiFine_HD_U_E6", LoaderKind::OptiFine, "HD_U_E6"),
            ("1.20.1-OptiFine_HD_U_I6", LoaderKind::OptiFine, "HD_U_I6"),
            (
                "1.20.1-OptiFine_HD_U_I6_pre1",
                LoaderKind::OptiFine,
                "HD_U_I6_pre1",
            ),
            ("1.20.1-OptiFine", LoaderKind::OptiFine, ""),
            ("1.12.2-LiteLoader", LoaderKind::LiteLoader, ""),
        ];
        for (id, kind, version) in cases {
            let parsed = InstanceLoader::parse(id);
            assert_eq!(parsed.kind, kind, "id `{id}` 认成了 {:?}", parsed.kind);
            assert_eq!(parsed.version, version, "id `{id}` 的版本段");
        }
    }

    /// 老 HMCL / CF 口径（`fabric-loader-<loader>-<mc>`、`forge-<ver>`）：存量实例还会按它们
    /// 出现，行为不许退化。
    ///
    /// 抓得住：砍 MC 尾巴的规则被删（版本段变成 `0.16.5-1.20.1`）、`loader` 后缀被当成版本段。
    /// 抓不住：`forge-47.2.0-1.20.1` 是**手工声明**的口径，不是查到的真实格式（本仓库的老
    /// HMCL 分支已改成「清单 + patch」）。
    #[test]
    fn foreign_loader_style_ids_still_parse() {
        let cases = [
            ("fabric-loader-0.16.5-1.20.1", LoaderKind::Fabric, "0.16.5"),
            ("quilt-loader-0.24.0-1.20.1", LoaderKind::Quilt, "0.24.0"),
            (
                "quilt-loader-0.24.0-beta.9-1.20.1",
                LoaderKind::Quilt,
                "0.24.0-beta.9",
            ),
            ("forge-47.2.0", LoaderKind::Forge, "47.2.0"),
            ("forge-47.2.0-1.20.1", LoaderKind::Forge, "47.2.0"),
        ];
        for (id, kind, version) in cases {
            let parsed = InstanceLoader::parse(id);
            assert_eq!(parsed.kind, kind, "id `{id}` 认成了 {:?}", parsed.kind);
            assert_eq!(parsed.version, version, "id `{id}` 的版本段");
        }
    }

    /// 反向锚：普通版本 id（原版、快照、预发布）**不许**被认成加载器。
    ///
    /// 抓得住：解析端退化成「有 id 就当加载器」、匹配放宽成「包含子串」。
    /// 抓不住：**含加载器名字样的原版 id**（真出现 `1.20.4-forge` 这种原版 id 会被认成
    /// Forge——本模块按标记优先）。
    #[test]
    fn plain_version_ids_stay_vanilla() {
        for id in [
            "1.20.4",
            "1.20",
            "1.7.10",
            "23w45a",
            "1.20.4-rc1",
            "1.20.1-pre1",
            "",
            "   ",
        ] {
            let parsed = InstanceLoader::parse(id);
            assert_eq!(parsed.kind, LoaderKind::Vanilla, "id `{id}`");
            assert!(parsed.version.is_empty(), "id `{id}` 的版本段应为空");
        }
    }

    /// 标记按**整段前缀**匹配，不是「包含」：段里出现加载器名字样但不在段首的，不算。
    ///
    /// 抓得住：匹配规则从「段前缀」放宽成「段内包含」（`myforge-pack` 会被认成 Forge）。
    /// 抓不住：段首就是标记的误伤（`forgepack-1.0` 会认成 Forge）——这条有意不拦，因为
    /// 段首命中正是 OptiFine 的 `OptiFine_HD_U_E6` 与 CF 的 `forge-47.2.0` 依赖的形状。
    #[test]
    fn markers_only_match_at_segment_start() {
        for id in ["1.20.1-myforge-pack", "1.20.1-somequiltish"] {
            let parsed = InstanceLoader::parse(id);
            assert_eq!(parsed.kind, LoaderKind::Vanilla, "id `{id}`");
        }
    }

    /// 往返锚：**每个对外 id 喂回解析都要认成它自己**，标记顺序也得留优先级。
    ///
    /// 抓得住：标记匹配退化（段前缀变整段全等、`loader` 后缀/MC 尾巴规则被改坏）、标记串与词表
    /// 脱节、`neoforge` 被排到 `forge` 后面。抓不住：**改名**——词表与解析同源、改名自洽，本条
    /// 是绿的；抓改名是 presenter 的 `loader_kind_ids_are_the_released_contract`。
    #[test]
    fn every_stable_id_parses_back_to_its_kind() {
        for kind in LoaderKind::ALL {
            let id = kind.as_str();
            assert_eq!(LoaderKind::parse(id), Some(kind), "反查 `{id}`");
            let parsed = InstanceLoader::parse(&format!("1.20.4-{id}-0.16.5"));
            assert_eq!(parsed.kind, kind, "`{id}` 认成了 {:?}", parsed.kind);
            let expected = if kind.has_marker() { "0.16.5" } else { "" };
            assert_eq!(parsed.version, expected, "`{id}` 的版本段");
        }
        let order: Vec<LoaderKind> = LoaderKind::ALL.into_iter().collect();
        let neoforge = order.iter().position(|kind| *kind == LoaderKind::NeoForge);
        let forge = order.iter().position(|kind| *kind == LoaderKind::Forge);
        assert!(neoforge < forge, "`neoforge` 要在 `forge` 前：{order:?}");
    }
}
