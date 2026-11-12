//! 面类家族（目前只有 **ELEVATED CARD**，规格卡 `CARD / DIALOG / SNACKBAR / LIST` 的第一格）。
//!
//! 为什么单开一个家族而不是塞进 `feedback`：`feedback` 是「系统对用户动作的回应」（进度 / 对话框 /
//! snackbar / 空态），卡是**静态容器**，语义上属于版式而不是反馈。家族边界写在这里，后来加
//! 「带描边卡 / 列表行药丸」时按语义归位，不要因为「都在同一个规格卡小节里」就合进 `feedback`。

pub mod elevated;
