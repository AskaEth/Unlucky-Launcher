//! 一条规整过的包内条目。

/// 一条规整过的包内条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackEntryInfo {
    pub(in crate::addon::pack::archive) name: String,
    pub(in crate::addon::pack::archive) raw: String,
    pub(in crate::addon::pack::archive) size: u64,
    pub(in crate::addon::pack::archive) is_dir: bool,
}

impl PackEntryInfo {
    /// 规整后的相对路径（`/` 分隔，无前导斜杠、无 `.`/`..` 段）。
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 声明/实测的字节数（目录条目为 0）。
    #[must_use]
    pub const fn size(&self) -> u64 {
        self.size
    }

    /// 是否是目录条目。
    #[must_use]
    pub const fn is_dir(&self) -> bool {
        self.is_dir
    }
}
