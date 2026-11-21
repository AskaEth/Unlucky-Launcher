//! 远端条目 → [`ModRecord`]：让「CF 包里的一个 mod」在数据层真的能落地。
//!
//! # 为什么要有这一步
//!
//! [`ModRecord`] 是本仓唯一「一个 mod 长什么样」的记录（`diagnose::environment`），重复与前置
//! 判定（`addon::mods::report`）只认它。远端检索回来的东西若不落到这个形状，就永远只能在
//! 检索结果里活着：「这个包里的 sodium 和我已经装的那份是不是同一个 / 是不是两个版本」——
//! 这个问题的两侧一个是远端条目、一个是本地记录，**只有把它们放进同一个形状才问得出来**。
//!
//! # 填了什么，没填什么
//!
//! | 字段 | 来源 | 说明 |
//! |------|------|------|
//! | `id` | 站点的 `slug`（小写） | 站点**不提供** modid（那在 jar 的元数据里）。slug 是唯一稳定的名字，站点的 slug 与 modid 常常同名（`sodium` / `jei`）。 |
//! | `name` | 站点的 `name`（`displayName` 的兜底同源） | 展示名 |
//! | `version` | 文件的 `displayName`（空则退到文件名的版本号启发式） | 版本比较走 `report::cmp_version` |
//! | `file` | 文件的 `fileName` | 磁盘上装上去就叫这个名字 |
//! | `enabled` | 恒 `true` | 远端条目不是本地磁盘状态；装上就是启用的 |
//! | `deps` | **空** | CF 的前置在另一个端点（`/mods/{id}/dependencies`），本切片没接 |
//!
//! `deps` 留空是**安全的一侧**：`report` 只会因为「声明了必需前置但没装」而报缺失，空声明等于
//! 「没有可判的」，于是这一路既不会谎报缺失，也不会漏报（漏掉的那部分是本切片明确不做的）。
//! 反过来随便塞一个猜测的依赖列表，会直接给用户报一堆假的「前置缺失」。

use super::model::{Mod, ModFile};
use crate::diagnose::collectors::ModRecord;

impl Mod {
    /// 把「本站的某个 mod + 它的某个文件」落成一条 [`ModRecord`]。
    ///
    /// 站点缺字段（没 slug / 没 name / 空 `displayName`）时**退化**而不是留空 id：
    /// [`ModRecord::from_file_name`] 的文件名启发式先给出底稿，再用站点给的更好信息逐项覆盖。
    #[must_use]
    pub fn record_of(&self, file: &ModFile) -> ModRecord {
        let mut record = ModRecord::from_file_name(&file.file_name);
        let slug = self.slug.trim().to_ascii_lowercase();
        if !slug.is_empty() {
            record.id = slug;
        }
        let name = self.name.trim();
        if !name.is_empty() {
            record.name = Some(name.to_string());
        }
        let display = file.display_name.trim();
        if !display.is_empty() {
            record.version = Some(display.to_string());
        }
        // 远端条目没有「禁用」这种状态（磁盘后缀是本地的事），装上即启用。
        record.enabled = true;
        // 见模块文档：前置不在本切片，留空是安全的一侧。
        record.deps.clear();
        record
    }
}
