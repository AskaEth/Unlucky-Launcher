//! 三个设置键的可编辑状态（M197）：`java.installRoot` / `java.vendorPriority` / `java.auto_install`。
//!
//! 用户原话（2026-11-08）：「自动下载 jdk17 作为基准版本，后续在设置中提供多版本可供用户下载。」
//! M175 把 Java 页接上了真运行时，但明确没做这三个键；本文件只负责**草稿**与**生效值**两半。
//!
//! ## 为什么又是「草稿 / 落盘真值」两层
//!
//! 与 `state.rs` 的内存草稿同一条理由：输入框里的是**文本**（用户可能刚敲了 `D:\Ja`），
//! 「现在生效什么」是**磁盘上的真值**。混成一个字段就会重演「显示的就是生效的」——而这三项里
//! `installRoot` 与 `vendorPriority` 都是**可以留空**的键，留空的语义是「跟数据目录模式走 /
//! 跟地区默认序走」，只有拿落盘值去 resolve 一次才说得清到底落在哪、按什么顺序找。
//!
//! ## 「从哪一层来」怎么表达
//!
//! [`Effective`] 是 app 层算好的**只读结论**（`prefs_store::effective`）：有效的安装根 + 它是
//! 来自 `java.installRoot` 还是 `core.data_mode`；有效的发行版顺序 + 它来自 `java.vendorPriority`
//! 还是地区默认序。渲染器只负责把这两个枚举翻成一句话，不再自己判断层级。
//!
//! ## 优先级草稿为什么是「整条顺序」而不是「用户写的那几家」
//!
//! `vendor::order` 的语义是**补集追加**（只写 oracle 就把它提前、其余按地区默认序补齐）。
//! 于是「上移 / 下移」这种操作只有在**看得见的那条完整顺序**上才有意义：用户看到 4 行，
//! 拖动其中一行，落盘的就是他自己排的那条完整链。清空（[`Prefs::reset_priority`]）= 回到
//! 「跟地区默认序走」，这也是这个键留空时唯一说得通的语义。

use unlucky_core::game::java::vendor::{self, JdkVendor};

use super::state::Refusal;

/// `java.installRoot` 的最大长度：与 `core::setting::keys` 里那个键登记的上限一致（512）。
pub const MAX_INSTALL_ROOT: usize = 512;

/// `java.auto_install` 的出厂默认值（schema 默认 `true`）。
pub const DEFAULT_AUTO_INSTALL: bool = true;

/// 三键的草稿 + 落盘真值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prefs {
    /// `java.installRoot` 草稿（空 = 跟数据目录模式走）
    pub install_root: String,
    /// 落盘的 `java.installRoot`
    pub saved_install_root: String,
    /// `java.auto_install` 草稿
    pub auto_install: bool,
    /// 落盘的 `java.auto_install`
    pub saved_auto_install: bool,
    /// `java.vendorPriority` 草稿（稳定 id，顺序即优先级；空 = 地区默认序）
    pub priority: Vec<&'static str>,
    /// 落盘的 `java.vendorPriority`
    pub saved_priority: Vec<&'static str>,
}

impl Default for Prefs {
    /// 出厂值 = schema 的默认值（**不是**零值：`auto_install` 出厂是 `true`）。
    fn default() -> Self {
        Self {
            install_root: String::new(),
            saved_install_root: String::new(),
            auto_install: DEFAULT_AUTO_INSTALL,
            saved_auto_install: DEFAULT_AUTO_INSTALL,
            priority: Vec::new(),
            saved_priority: Vec::new(),
        }
    }
}

/// 有效的安装根从哪一层来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootLayer {
    /// 设置键 `java.installRoot`
    Setting,
    /// 数据目录模式（`core.data_mode`）派生的落点
    DataMode,
}

/// 有效的发行版顺序从哪一层来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderLayer {
    /// 设置键 `java.vendorPriority`
    Setting,
    /// 地区默认序（国内 / 国外两套内置序）
    Region,
}

/// app 层每次 hydrate 重算的只读结论（渲染器只翻文案，不再判断层级）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    /// 有效的安装根（人读路径）
    pub root: String,
    /// 落点模式（`custom` / `standard` / `portable`）
    pub root_mode: &'static str,
    /// 安装根来自哪一层
    pub root_layer: RootLayer,
    /// 解析不出标准目录（`standard` 模式拿不到 `%APPDATA%` / `HOME`）
    pub root_unresolved: bool,
    /// 有效的发行版顺序（展示名）
    pub order: Vec<&'static str>,
    /// 顺序来自哪一层
    pub order_layer: OrderLayer,
    /// 这台机器被判成国内环境（地区默认序取的是国内那套）
    pub domestic: bool,
}

impl Default for Effective {
    fn default() -> Self {
        Self {
            root: String::new(),
            root_mode: "portable",
            root_layer: RootLayer::DataMode,
            root_unresolved: false,
            order: Vec::new(),
            order_layer: OrderLayer::Region,
            domestic: true,
        }
    }
}

/// 从 `launcher.json` 读回来的三键真值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// `java.installRoot`
    pub install_root: String,
    /// `java.auto_install`
    pub auto_install: bool,
    /// `java.vendorPriority`（稳定 id）
    pub priority: Vec<String>,
}

impl Prefs {
    /// 用真值刷新（**脏草稿不覆盖**，与 `State::hydrate` 同一条纪律）。返回是否变了。
    pub fn hydrate(&mut self, snapshot: Snapshot) -> bool {
        let priority: Vec<&'static str> = snapshot
            .priority
            .iter()
            .filter_map(|text| JdkVendor::parse(text).map(JdkVendor::id))
            .collect();
        let changed = self.saved_install_root != snapshot.install_root
            || self.saved_auto_install != snapshot.auto_install
            || self.saved_priority != priority;
        self.saved_install_root = snapshot.install_root.clone();
        self.saved_auto_install = snapshot.auto_install;
        self.saved_priority = priority.clone();
        if !self.dirty() {
            self.install_root = snapshot.install_root;
            self.auto_install = snapshot.auto_install;
            self.priority = priority;
        }
        changed
    }

    /// 保存成功后用**读回的真值**收尾：草稿与落盘值两半都对它。
    ///
    /// 为什么草稿也要改：这就是「改完回读验证已落盘」的界面形态——用户看到的就是磁盘上那份。
    /// 如果 schema 把值规整过（去空格、拒绝某个枚举值），界面上显示的是规整后的结果，
    /// 而不是他刚敲的那串原样（假装没变过才是最坏的）。
    pub fn adopt(&mut self, snapshot: Snapshot) {
        let priority: Vec<&'static str> = snapshot
            .priority
            .iter()
            .filter_map(|text| JdkVendor::parse(text).map(JdkVendor::id))
            .collect();
        self.install_root = snapshot.install_root.clone();
        self.auto_install = snapshot.auto_install;
        self.priority = priority.clone();
        self.saved_install_root = snapshot.install_root;
        self.saved_auto_install = snapshot.auto_install;
        self.saved_priority = priority;
    }

    /// 这一段有没有未保存的改动。
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.install_root.trim() != self.saved_install_root.trim()
            || self.auto_install != self.saved_auto_install
            || self.priority != self.saved_priority
    }

    /// 保存前的本地校验（过不了就不写盘）。
    ///
    /// # Errors
    /// 路径超过 schema 登记的长度上限时返回 [`Refusal::InstallRootTooLong`]。
    pub fn validate(&self) -> Result<(), Refusal> {
        if self.install_root.trim().chars().count() > MAX_INSTALL_ROOT {
            return Err(Refusal::InstallRootTooLong);
        }
        Ok(())
    }

    /// 草稿的优先级链 → 稳定 id 列表（落盘用：空列表 = 跟地区默认序走）。
    #[must_use]
    pub fn priority_ids(&self) -> Vec<String> {
        self.priority.iter().map(|id| (*id).to_string()).collect()
    }

    /// 草稿的优先级链 → 有效顺序（`vendor::order` 的补集追加语义，与真正下载时同源）。
    #[must_use]
    pub fn order(&self, domestic: bool) -> Vec<JdkVendor> {
        let parsed = self
            .priority
            .iter()
            .filter_map(|id| JdkVendor::parse(id))
            .collect::<Vec<_>>();
        vendor::order(&parsed, domestic)
    }

    /// 上移一行（`index` 是**有效顺序**里的下标）。
    ///
    /// 第一次动就把草稿物化成整条顺序：用户拖的是他看见的那 4 行，落盘的就该是这 4 行的顺序。
    /// `domestic` 由调用方给（app 层已经算过一次地区判定，这里不重复探测）。
    pub fn move_up(&mut self, index: usize, domestic: bool) {
        self.materialize(domestic);
        if index > 0 && index < self.priority.len() {
            self.priority.swap(index, index - 1);
        }
    }

    /// 下移一行。
    pub fn move_down(&mut self, index: usize, domestic: bool) {
        self.materialize(domestic);
        if index + 1 < self.priority.len() {
            self.priority.swap(index, index + 1);
        }
    }

    /// 草稿物化：把「留空 = 地区默认序」展开成那条完整链（已物化时不动）。
    fn materialize(&mut self, domestic: bool) {
        if self.priority.len() != JdkVendor::default_order(domestic).len() {
            self.priority = self.order(domestic).iter().map(|id| id.id()).collect();
        }
    }

    /// 恢复地区默认序（= 把这个键写回空，让地区那一层重新生效）。
    pub fn reset_priority(&mut self) {
        self.priority.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::Prefs;
    use unlucky_core::game::java::vendor::{ids, JdkVendor};

    /// 出厂值 = schema 默认（`auto_install` 为真、另两键留空）。
    ///
    /// 变异证明：把 `Prefs::default` 里 `auto_install` 改成 `false` ⇒ 本用例红。
    #[test]
    fn the_defaults_match_the_schema() {
        let prefs = Prefs::default();
        assert!(prefs.auto_install);
        assert!(prefs.install_root.is_empty());
        assert!(prefs.priority.is_empty());
        assert!(!prefs.dirty(), "出厂值不算脏");
        assert_eq!(
            prefs.order(true),
            vec![
                JdkVendor::Temurin,
                JdkVendor::Oracle,
                JdkVendor::Microsoft,
                JdkVendor::Zulu
            ],
            "留空 = 国内默认序"
        );
    }

    /// 上移 / 下移真的改顺序，而且第一条上移是空操作。
    ///
    /// 变异证明：把 `move_up` 里的 `swap(index, index - 1)` 写成 `swap(index, index + 1)`
    /// ⇒ 本用例红。
    #[test]
    fn moving_a_vendor_rewrites_the_order() {
        let mut prefs = Prefs::default();
        // 空白草稿上直接下移第一条：物化成整条链后再交换。
        prefs.move_down(0, true);
        assert_eq!(
            ids(&prefs.order(true)),
            vec!["oracle", "temurin", "microsoft", "zulu"],
            "下移第一条 = 与第二条交换"
        );
        assert_eq!(prefs.priority.len(), 4, "物化之后是整条链");
        prefs.move_up(0, true);
        assert_eq!(ids(&prefs.order(true))[0], "oracle", "第一条上移是空操作");
        prefs.move_up(1, true);
        assert_eq!(ids(&prefs.order(true))[0], "temurin");
        assert!(prefs.dirty(), "动过就该脏");
    }

    /// 恢复地区默认序 = 草稿清空，且不再脏（与落盘的空值一致）。
    #[test]
    fn resetting_goes_back_to_the_region_default() {
        let mut prefs = Prefs::default();
        prefs.move_down(0, true);
        assert!(prefs.dirty());
        prefs.reset_priority();
        assert!(!prefs.dirty());
        assert_eq!(ids(&prefs.order(true))[0], "temurin");
    }
}
