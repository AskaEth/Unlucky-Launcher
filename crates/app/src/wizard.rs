//! 新建实例向导（09 §G5）的纯状态机：版本 → 加载器 → 实例名 → 确认 → 进度。
//!
//! 放纯 Rust 而不是 Slint 里：步骤推进、名字合法性、加载器约束都能单测，
//! Slint 只负责画。提交时由 host 转成 [`crate::dto::CreateRequest`] 走安装管线。

use crate::dto::LoaderKind;

/// 向导步骤。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WizardStep {
    /// 选版本
    #[default]
    Version,
    /// 选加载器
    Loader,
    /// 起名
    Name,
    /// 确认（显示将下载的项）
    Confirm,
    /// 进度（提交后由安装进度接管）
    Progress,
}

impl WizardStep {
    /// 步骤序号（0 起，用于「第 N 步」文案）。
    pub fn index(self) -> usize {
        match self {
            Self::Version => 0,
            Self::Loader => 1,
            Self::Name => 2,
            Self::Confirm => 3,
            Self::Progress => 4,
        }
    }

    /// 步骤标题。
    pub fn title(self) -> &'static str {
        match self {
            Self::Version => "选择版本",
            Self::Loader => "选择加载器",
            Self::Name => "实例名",
            Self::Confirm => "确认",
            Self::Progress => "安装中",
        }
    }

    /// 下一步（进度是终点）。
    pub fn next(self) -> Self {
        match self {
            Self::Version | Self::Progress => {
                if self == Self::Progress {
                    Self::Progress
                } else {
                    Self::Loader
                }
            }
            Self::Loader => Self::Name,
            Self::Name => Self::Confirm,
            Self::Confirm => Self::Progress,
        }
    }

    /// 上一步（版本是起点）。
    pub fn back(self) -> Self {
        match self {
            Self::Version | Self::Progress => Self::Version,
            Self::Loader => Self::Version,
            Self::Name => Self::Loader,
            Self::Confirm => Self::Name,
        }
    }
}

/// 向导状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WizardState {
    /// 是否打开（全屏模态）
    pub open: bool,
    /// 当前步骤
    pub step: WizardStep,
    /// 选中的版本 id
    pub version: String,
    /// 选中的加载器
    pub loader: LoaderKind,
    /// 加载器版本（空 = 最新）
    pub loader_version: String,
    /// 实例名草稿
    pub name: String,
    /// 校验错误（显示在向导内，不弹 Toast）
    pub error: String,
    /// 已提交（防重复点）
    pub submitting: bool,
}

impl Default for WizardState {
    fn default() -> Self {
        Self::new()
    }
}

impl WizardState {
    /// 关闭态。
    pub fn new() -> Self {
        Self {
            open: false,
            step: WizardStep::Version,
            version: String::new(),
            loader: LoaderKind::Vanilla,
            loader_version: String::new(),
            name: String::new(),
            error: String::new(),
            submitting: false,
        }
    }

    /// 打开向导并复位。
    pub fn open() -> Self {
        Self {
            open: true,
            ..Self::new()
        }
    }

    /// 选了版本：自动把实例名填成版本号（用户不改就用它）。
    pub fn select_version(&mut self, version: &str) {
        self.version = version.to_string();
        if self.name.trim().is_empty() {
            self.name = version.to_string();
        }
        self.error.clear();
    }

    /// 选加载器（原版会清掉加载器版本）。
    pub fn select_loader(&mut self, loader: LoaderKind) {
        self.loader = loader;
        if loader == LoaderKind::Vanilla {
            self.loader_version.clear();
        }
        self.error.clear();
    }

    /// 推进到下一步；校验不通过时留在原地并写 `error`。
    pub fn advance(&mut self) -> bool {
        match self.step {
            WizardStep::Version => {
                if self.version.trim().is_empty() {
                    self.error = "请先选择一个版本".to_string();
                    return false;
                }
            }
            WizardStep::Name => {
                if let Err(error) = validate_name(&self.name) {
                    self.error = error;
                    return false;
                }
            }
            WizardStep::Loader | WizardStep::Confirm | WizardStep::Progress => {}
        }
        self.error.clear();
        self.step = self.step.next();
        true
    }

    /// 回退一步。
    pub fn step_back(&mut self) {
        self.error.clear();
        self.step = self.step.back();
    }

    /// 提交前的最后校验（Confirm 步）。
    pub fn ready_to_submit(&self) -> Result<(), String> {
        if self.version.trim().is_empty() {
            return Err("版本为空".to_string());
        }
        validate_name(&self.name)?;
        Ok(())
    }

    /// 提交成功后进入进度步（关掉可编辑态）。
    pub fn submitted(&mut self) {
        self.submitting = true;
        self.step = WizardStep::Progress;
        self.error.clear();
    }
}

/// 实例名合法性（不能空、不能含路径分隔符/控制字符、长度上限 64）。
pub fn validate_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("实例名不能为空".to_string());
    }
    if trimmed.chars().count() > 64 {
        return Err("实例名最多 64 个字符".to_string());
    }
    if trimmed.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err("实例名不能包含 / \\ : * ? \" < > |".to_string());
    }
    if trimmed.chars().any(|ch| ch.is_control()) {
        return Err("实例名不能包含控制字符".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_walk_forward_and_back() {
        assert_eq!(WizardStep::Version.next(), WizardStep::Loader);
        assert_eq!(WizardStep::Loader.next(), WizardStep::Name);
        assert_eq!(WizardStep::Name.next(), WizardStep::Confirm);
        assert_eq!(WizardStep::Confirm.next(), WizardStep::Progress);
        assert_eq!(WizardStep::Progress.next(), WizardStep::Progress);
        assert_eq!(WizardStep::Confirm.back(), WizardStep::Name);
        assert_eq!(WizardStep::Version.back(), WizardStep::Version);
        assert_eq!(WizardStep::Progress.index(), 4);
    }

    #[test]
    fn version_selection_fills_name_but_does_not_overwrite() {
        let mut wizard = WizardState::open();
        assert!(wizard.open);
        wizard.select_version("1.20.4");
        assert_eq!(wizard.name, "1.20.4");
        wizard.name = "生存".to_string();
        wizard.select_version("1.21");
        assert_eq!(wizard.name, "生存", "用户改过的名字不该被覆盖");
    }

    #[test]
    fn advance_requires_a_version_and_a_valid_name() {
        let mut wizard = WizardState::open();
        assert!(!wizard.advance(), "没有版本不能进下一步");
        assert_eq!(wizard.error, "请先选择一个版本");

        wizard.select_version("1.20.4");
        assert!(wizard.advance());
        assert_eq!(wizard.step, WizardStep::Loader);
        assert!(wizard.advance());
        assert_eq!(wizard.step, WizardStep::Name);

        wizard.name = "a/b".to_string();
        assert!(!wizard.advance(), "非法名字不能确认");
        assert!(wizard.error.contains("不能包含"));

        wizard.name = "生存".to_string();
        assert!(wizard.advance());
        assert_eq!(wizard.step, WizardStep::Confirm);
        assert!(wizard.ready_to_submit().is_ok());
        wizard.submitted();
        assert_eq!(wizard.step, WizardStep::Progress);
        assert!(wizard.submitting);
    }

    #[test]
    fn loader_selection_clears_loader_version_for_vanilla() {
        let mut wizard = WizardState::open();
        wizard.loader_version = "0.16.5".to_string();
        wizard.select_loader(LoaderKind::Fabric);
        assert_eq!(wizard.loader_version, "0.16.5");
        wizard.select_loader(LoaderKind::Vanilla);
        assert_eq!(wizard.loader, LoaderKind::Vanilla);
        assert!(wizard.loader_version.is_empty());
    }

    #[test]
    fn name_validation_rejects_long_and_control_input() {
        assert!(validate_name("生存").is_ok());
        assert!(validate_name("").is_err());
        assert!(validate_name(&"x".repeat(65)).is_err());
        assert!(validate_name("a\u{7}b").is_err());
    }
}
