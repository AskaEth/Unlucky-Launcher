//! 格式扩展点：[`PackFormat`] trait + 注册表 [`PackRegistry`] + **写死的探测优先级**。
//!
//! # 可扩展性的判据是测试，不是文档
//!
//! `tests/extend.rs` 里有一个只有几十行的假格式 `ToyFormat`（单文件、`toy.json` 当清单）：
//! 它**只加一行注册**（`PackRegistry::builtin().with(&TOY)`）就能被探测、读取、渲染、写回，
//! 不改模型、不改 CurseForge / Modrinth / MultiMC 任何一行。这是"社区标准加一个格式"的
//! 最小代价证明。
//!
//! # 探测优先级写死在这里，顺序由注册表 `Vec` 位置决定
//!
//! [`PackRegistry::builtin`] 的顺序：**CurseForge → Modrinth → MultiMC**。
//!
//! - 跟 HMCL 对这三种格式的相对次序一致（`ModpackHelper.readModpackManifest`：Curse 在
//!   Modrinth 前、Modrinth 在 MultiMC 前），减少与社区工具的行为差异；
//! - 每个格式的 `detect` 都带**形状校验**（CF 要 `manifestType: minecraftModpack`、mrpack 要
//!   `formatVersion` + `files` 数组、MultiMC 要 `components` 非空），所以名字很泛的
//!   `manifest.json` 不会把 `.mrpack` 抢走；
//! - 同一份包同时命中多个格式时，[`PackMatch::also_matched`] 会记下其余命中，调用方可以提示
//!   "这包不标准"——不做静默判定，也不做自动合并；
//! - [`PackRegistry::register`] 追加到**末尾**（= 最低优先级），所以测试里改一行顺序就能
//!   证明"优先级来自注册表顺序"（`tests/detect.rs` 有一条反序注册的用例）。
//!
//! # 写：默认实现**只保证同格式往返**（这条是给 M111/M112 的边界声明）
//!
//! 默认实现（[`PackFormat::write`]）替所有格式做同一件事：写清单 entry（[`PackFormat::render`]
//! 的产物）+ 把 `manifest.overrides` 的 `(dir, side, files)` **原样**搬进 zip。因此新格式只要
//! 实现 `metadata_entry` + `render` 就同时获得导出能力；只读格式（MultiMC）不实现
//! `metadata_entry`，写会得到明确的 `crate::addon::pack::error::PackErrorKind::Unsupported`。
//!
//! **它不做目标格式映射**，具体缺四件事（都属于 M112，别以为这一层已经做了）：
//!
//! 1. **目录名映射**：mrpack 的覆盖目录名由 `side` 决定（`overrides` / `client-overrides` /
//!    `server-overrides`），CF 只有一个 `overrides`（名字由清单给）。`check_overrides` 只保证
//!    "布局表达不了就报错"，不负责改名；
//! 2. **`side` 合并/分派**：CF 表达不了 Client/Server 分侧，现在是**拒**而不是合并；
//! 3. **加载器 id 规范化**：CF 要 `forge-47.2.0` / `fabric-0.15.11`，mrpack 要
//!    `forge` / `fabric-loader` 这类键。`render` 只按模型里的 `raw`/`kind` 写，跨格式转出来的
//!    id 形态可能不被目标平台接受（尤其 MultiMC 的 `net.fabricmc.fabric-loader`）；
//! 4. **`extra` 不按格式分桶**：模型只有一份未知字段集合，跨格式导出会把别的格式的未知键
//!    一起带过去。
//!
//! 换句话说：`X.read` → `X.write` 是可靠往返（有逐字段相等的测试）；`X.read` → `Y.write`
//! 要么被 `check_overrides` 拒掉，要么"结构合法但语义不完整"。**要跨格式导出，先做映射。**
//!
//! # 映射层已经在了（M112）
//!
//! [`crate::addon::export`] 就是那层映射：它把实例目录扫成模型（覆盖目录统一到 `overrides/`、
//! 禁用位落成路径后缀 `.disabled`、`dependencies` 从实例快照取），再用
//! [`PackPayload::with_sources`] 把"包内条目名 → 实例里的源文件"交给本模块写盘。所以本文件
//! 仍然不做映射（那是格式之上的事），但"跨格式导出无路可走"已经不再成立。

use std::fmt::Debug;
use std::path::Path;

use super::archive::PackArchive;
use super::curseforge;
use super::error::{PackError, PackResult};
use super::manifest::PackManifest;
use super::modrinth;
use super::multimc;

mod payload;
mod write;

/// 测试用的限额记录钩子（`probe.rs`，**只在 `cfg(test)` 里编译**）：把"生产 `write()`
/// 到底传了哪份限额"变成可观测的，见该模块文档。
#[cfg(test)]
pub(in crate::addon::pack) mod probe;

/// 导出时的覆盖内容来源在 [`payload`] 子模块里（`payload.rs`，与本文件拆开是为了守 400 行）：
/// [`PackPayload::metadata_only`] 只写清单、[`PackPayload::from_root`] 按
/// `<root>/<dir>/<path>` 取文件、[`PackPayload::with_sources`] 按**包内条目名**查表
/// （跨格式导出 / 从实例导出用，M112）。
pub use payload::PackPayload;

/// 一种整合包格式（导入 + 导出的全部能力都在这里）。
pub trait PackFormat: Debug + Send + Sync {
    /// 稳定字符串 id（进日志/事件/CLI 参数，**不要改名**）。
    fn id(&self) -> &'static str;

    /// 人读名称。
    fn label(&self) -> &'static str;

    /// 清单文件在包内的入口名（`Some` = 本格式可导出）。
    fn metadata_entry(&self) -> Option<&'static str> {
        None
    }

    /// 这份输入是不是本格式；命中时返回**证据**（凭哪个条目认领的），未命中返回 `None`。
    ///
    /// **必须带形状校验**，不能只看文件名存在：`manifest.json` 这种泛名到处都是。
    /// "选谁"不是格式自己的事——排序与歧义在 [`PackRegistry::detect`] 那一层做
    /// （返回 [`PackMatch`]，带 `also_matched` 与本次命中的证据）。
    ///
    /// **清单在、但读不出来**时必须返回 `Err`（`pack_metadata` 一类），不要用
    /// `.ok().flatten()` 吞掉：那样"包坏了"会被上层报成"没有格式认领"，M111 的失败分级
    /// 就会把可以重下文件的包当成不支持（r1 p3-2）。
    fn detect(&self, pack: &PackArchive) -> PackResult<Option<&'static str>>;

    /// 读成中间模型。
    fn read(&self, pack: &PackArchive) -> PackResult<PackManifest>;

    /// 把模型渲染成清单 JSON（未知字段在这里回写）。
    fn render(&self, manifest: &PackManifest) -> PackResult<String>;

    /// 这份模型的**覆盖目录集合**能不能被本格式表达（默认 `write` 在写第一个字节前调用）。
    ///
    /// 默认实现不限制（"目录名原样写进包里"对同格式往返是对的）。凡是有**固定目录布局**
    /// 的格式必须收紧：mrpack 的三个目录由 `side` 决定，CF 只有一个 `overrides` 目录。
    /// 不收紧的后果不是报错而是**静默丢内容**——把一份 MultiMC 实例写成 `.mrpack` 时，
    /// 文件会落在 `.minecraft/...`，而 mrpack 导入器只认
    /// `overrides/` / `client-overrides/` / `server-overrides/`，读出来 mods 数量为 0（r1 p2-1）。
    fn check_overrides(&self, manifest: &PackManifest) -> PackResult<()> {
        let _ = manifest;
        Ok(())
    }

    /// 导出一个整包（清单 + 覆盖内容）。
    ///
    /// 默认实现做四件事：①只读格式直接拒；②[`Self::check_overrides`] 挡跨格式导出；
    /// ③`render` 出清单；④**先按声明尺寸预检 ZIP64 阈值**，再写进**临时文件**并 `rename` 到
    /// `target`（失败不留半个包）。第④步的细节见 `format::write` 模块文档。
    ///
    /// # 只保证同格式往返（写给 M111/M112）
    ///
    /// 默认 `write` 的覆盖目录策略是"把 `manifest.overrides` 里 `(dir, side, files)`
    /// 原样再打一遍"，也就是"读出来的包按原样打包"。它**不做**目标格式的目录映射、`side`
    /// 分派、加载器 id 规范化，也**不按格式分桶** `extra`。所以：
    ///
    /// - 同格式往返（`X.read` → `X.write`）可靠，有逐字段相等的测试；
    /// - 跨格式导出（`X.read` → `Y.write`）在布局不同时被 [`Self::check_overrides`] 拒掉；
    ///   但"布局恰好兼容、语义不兼容"的部分（加载器 id 形态、`extra` 里别的格式的未知键）
    ///   **仍会被原样带过去**——那是 M112 的活，这一层不替谁做映射。
    fn write(
        &self,
        manifest: &PackManifest,
        payload: PackPayload<'_>,
        target: &Path,
    ) -> PackResult<()> {
        // 生产口径只有一处来源：导入读取用的也是这三个数（见 `archive::limits`）。
        write_with_limits(
            self,
            manifest,
            payload,
            target,
            write::WriteLimits::production(),
        )
    }
}

/// [`PackFormat::write`] 的实体：限额由调用方给。
///
/// # 为什么把 limits 做成参数
///
/// 预检的判据是「**声明尺寸** ≥ 上限」，与夹具真实大小无关。生产路径只传
/// [`write::WriteLimits::production`]；测试传几百字节的小值，走的是**同一条**
/// `write::preflight` 分支——这样"闸在碰内容之前"不必再造 GiB 级夹具来证明。
/// （旧版测试用 `set_len` 造"稀疏"大文件，实测在 NTFS 上**不是稀疏的**：可用空间真的
/// 掉了那么多，见 `tests/zip64.rs` 的模块文档。）
pub(in crate::addon::pack) fn write_with_limits<F: PackFormat + ?Sized>(
    format: &F,
    manifest: &PackManifest,
    payload: PackPayload<'_>,
    target: &Path,
    limits: write::WriteLimits,
) -> PackResult<()> {
    // 测试构建里记一笔"这次传的是哪份限额"：夹具缩小之后，只有它能把
    // 「生产 `write()` 传的就是 `WriteLimits::production()`」这件事变成可观测的（见 `probe`）。
    #[cfg(test)]
    probe::record(limits);

    let Some(entry) = format.metadata_entry() else {
        return Err(PackError::unsupported(format!(
            "格式 {}（{}）只支持读取，不支持导出",
            format.id(),
            format.label()
        ))
        .with_hint("导出请选 Modrinth（.mrpack）或 CurseForge 格式"));
    };
    // 先判"能不能表达"，再碰磁盘：跨格式导出宁可报错，也不产出空包。
    format.check_overrides(manifest)?;
    // 模型可能来自外部构造：搬内容前把每条路径重查一遍。
    manifest.check_paths()?;
    let json = format.render(manifest)?;
    // 临时文件 + rename：中途失败时目标路径上不会留下"看起来能用"的半个 zip（r1 p3-1）。
    let temp = write::temporary_path(target);
    match write::container(format.id(), entry, &json, manifest, payload, &temp, limits) {
        Ok(()) => std::fs::rename(&temp, target).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            PackError::io(format!(
                "把 {} 改名成 {} 失败：{error}",
                temp.display(),
                target.display()
            ))
        }),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// 测试要能自己给 limits：把 `write` 子模块里的类型借出来（生产代码不走这条）。
#[cfg(test)]
pub(in crate::addon::pack) use write::WriteLimits;

/// 一次探测的结果。
#[derive(Debug, Clone)]
pub struct PackMatch {
    format: &'static dyn PackFormat,
    evidence: &'static str,
    also_matched: Vec<&'static str>,
}

impl PackMatch {
    /// 选中的格式。
    #[must_use]
    pub fn format(&self) -> &'static dyn PackFormat {
        self.format
    }

    /// 命中的证据条目名（探测器凭哪个文件认领的，进日志用）。
    #[must_use]
    pub fn evidence(&self) -> &'static str {
        self.evidence
    }

    /// 同样命中的其它格式 id（探测顺序里排在后面的那些）。
    #[must_use]
    pub fn also_matched(&self) -> &[&'static str] {
        &self.also_matched
    }

    /// 是否有"多格式同时命中"的歧义（不标准包）。
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        !self.also_matched.is_empty()
    }
}

/// 格式注册表：**顺序即优先级**。
#[derive(Debug, Clone, Default)]
pub struct PackRegistry {
    formats: Vec<&'static dyn PackFormat>,
}

impl PackRegistry {
    /// 一个都没有的注册表（测试"没有格式认领"用）。
    #[must_use]
    pub fn empty() -> Self {
        Self {
            formats: Vec::new(),
        }
    }

    /// 内置三格式，顺序即优先级：CurseForge → Modrinth → MultiMC。
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            formats: vec![
                &curseforge::CURSEFORGE,
                &modrinth::MODRINTH,
                &multimc::MULTIMC,
            ],
        }
    }

    /// 追加一种格式（追加 = 最低优先级）。加一种格式真的只要这一行。
    pub fn register(&mut self, format: &'static dyn PackFormat) {
        self.formats.push(format);
    }

    /// 链式注册。
    #[must_use]
    pub fn with(mut self, format: &'static dyn PackFormat) -> Self {
        self.register(format);
        self
    }

    /// 按优先级列出格式 id。
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.formats.iter().map(|format| format.id()).collect()
    }

    /// 按 id 找格式。
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&'static dyn PackFormat> {
        self.formats
            .iter()
            .copied()
            .find(|format| format.id() == id)
    }

    /// 全部命中的格式 id（诊断"这包不标准"用；**只看命中**，单个格式的读错误被忽略——
    /// 要区分"包坏了"与"不认识"请用 [`Self::detect`]）。
    #[must_use]
    pub fn detect_all(&self, pack: &PackArchive) -> Vec<&'static str> {
        self.formats
            .iter()
            .filter(|format| matches!(format.detect(pack), Ok(Some(_))))
            .map(|format| format.id())
            .collect()
    }

    /// 按优先级选第一个命中的格式。
    ///
    /// 错误语义（M111 的失败分级依赖这两条的区别）：
    ///
    /// - **一个都不命中、但某个格式的清单读不出来** → 返回那个格式的错误
    ///   （`pack_metadata` 一类：包坏了，换格式也没用）；
    /// - **一个都不命中、也没有任何格式的清单在场** → `pack_unsupported`
    ///   （真的不认识这份包）；
    /// - 命中优先于失败：包里躺着一份别人的坏 `manifest.json` 不影响真的 `.mrpack` 被认出来。
    pub fn detect(&self, pack: &PackArchive) -> PackResult<PackMatch> {
        let mut matched: Vec<(&'static dyn PackFormat, &'static str)> = Vec::new();
        // 命中优先于"某个格式的清单坏了"：包里躺着一份别人的坏 manifest.json 不该毁掉它。
        let mut failures: Vec<PackError> = Vec::new();
        for format in self.formats.iter().copied() {
            match format.detect(pack) {
                Ok(Some(evidence)) => matched.push((format, evidence)),
                Ok(None) => {}
                // 清单在、但读不出来 = 包坏了（`pack_metadata`），不是"不认识这个格式"。
                Err(error) => failures.push(error),
            }
        }
        if matched.is_empty() {
            if let Some(error) = failures.into_iter().next() {
                return Err(error);
            }
            let tried = self.ids().join(" / ");
            return Err(
                PackError::unsupported(format!("没有格式认领这份整合包（试过：{tried}）"))
                    .with_hint(
                        "确认它是 CurseForge manifest.json、Modrinth .mrpack 或 MultiMC 实例",
                    ),
            );
        }
        let (format, evidence) = matched.remove(0);
        let also_matched = matched.iter().map(|(other, _)| other.id()).collect();
        Ok(PackMatch {
            format,
            evidence,
            also_matched,
        })
    }

    /// 探测 + 读成模型（导入管线的第一步）。
    pub fn read(&self, pack: &PackArchive) -> PackResult<(&'static dyn PackFormat, PackManifest)> {
        let matched = self.detect(pack)?;
        let manifest = matched.format().read(pack)?;
        Ok((matched.format(), manifest))
    }
}
