//! 设置页「助手（AI）」分组文案（M174）：状态矩阵四档 + 五个输入项 + 测试连接六档。
//!
//! 域文件的形状见 [`crate::catalog`] 的「域装配」一节。本域在装配链里排在
//! `settings_download_entries` 之后、`core_text_entries` 之前（设置页五域的位置），
//! 所以金表那几行也要插在 `SettingsAppearanceCornerSuspended` 之后——**顺序即 `Key::ALL`**。
//!
//! 三条口径：
//!
//! 1. **失败四种各写各的句子**（鉴权 / 模型不存在 / 不可达 / 超时）：合成一句「测试失败」等于
//!    让用户自己猜是 key 错了还是地址错了；
//! 2. 密钥的**值**永远不进文案：只有 `{chars}`（已存字符数）与「未配置」两种说法；
//! 3. 技术名词不翻：`base_url` / `OpenAI` / `Anthropic` / `Ollama` 原样。

macro_rules! settings_ai_entries {
    ($next:ident, $($rest:ident,)* { $($acc:tt)* }) => {
        $next! { $($rest,)* {
            $($acc)*
            // ── 左列分组名（与 NavSettings 同源：左列是导航，标题在正文里）
            SettingsGroupAssistant => ("Assistant", "助手"),
            // ── 状态矩阵（未配置 / 已配置 / 已通过 / 测试失败）
            SettingsAiStatusLabel => ("Status: {status}", "状态：{status}"),
            SettingsAiStatusUnconfigured => ("not configured", "未配置"),
            SettingsAiStatusConfigured => ("configured", "已配置"),
            SettingsAiStatusVerified => ("verified", "连接已通过"),
            SettingsAiStatusTestFailed => ("test failed", "测试失败"),
            // ── 开关
            SettingsAiEnable => ("Enable the assistant", "启用助手"),
            SettingsAiEnableHint => (
                "With this off, the assistant stays quiet; crash diagnosis from the rule engine keeps working.",
                "关掉之后助手不再工作；规则引擎的崩溃诊断照常可用。"
            ),
            // ── YOLO（M390）：开关旁边必须有一句人话说清它是什么、有多危险。
            SettingsAiYolo => ("Skip confirmations (YOLO)", "跳过确认（YOLO）"),
            SettingsAiYoloHint => (
                "With this on the assistant stops asking before it acts: high-sensitivity writes (config changes, shell commands) run straight away. Flip it and it is already in effect — no restart, no Save.",
                "打开后 AI 动手前不再问你：高敏感写（改配置、跑命令）直接执行。一拨就生效——不用重启，也不用点「保存」。"
            ),
            // 这句是**开着才画**的（红字）：只说「不会因此多改一个键」的三条证据。
            SettingsAiYoloDanger => (
                "It only skips the question, not the answer: credential keys (ai.api_key and the rest) still cannot be written, and the audit still records yolo=true, so every auto-approved write stays traceable.",
                "它只跳过「问」这一步，不会让 AI 多改一个键：凭据类键（ai.api_key 等）永远写不到，审计照记 yolo=true，事后能查出哪一笔是自动放行的。"
            ),
            // ── 提供商
            SettingsAiProviderLabel => ("Provider", "提供商"),
            SettingsAiProviderOpenAi => ("OpenAI compatible", "OpenAI 兼容"),
            SettingsAiProviderAnthropic => ("Anthropic", "Anthropic 原生"),
            SettingsAiProviderOllama => ("Ollama", "Ollama 原生"),
            SettingsAiProviderDisabled => ("Disabled", "已禁用"),
            // ── 端点 / 模型
            SettingsAiBaseUrl => ("Endpoint (base_url)", "端点（base_url）"),
            SettingsAiBaseUrlPlaceholder => ("http://127.0.0.1:8080/v1", "例：http://127.0.0.1:8080/v1"),
            SettingsAiBaseUrlHint => (
                "The /v1 root of an OpenAI-compatible service. Leave empty to use the provider default.",
                "OpenAI 兼容服务的 /v1 根地址；留空则用该提供商的默认端点。"
            ),
            SettingsAiEffective => ("Effective: {value}", "生效：{value}"),
            SettingsAiModel => ("Model name", "模型名称"),
            SettingsAiModelPlaceholder => ("deepseek-chat", "例：deepseek-chat"),
            // ── 密钥（只有字符数与掩码，没有值）
            SettingsAiApiKey => ("API key", "API 密钥"),
            SettingsAiApiKeyPlaceholder => ("Paste the key", "粘贴密钥"),
            SettingsAiApiKeyConfigured => (
                "A key is stored ({chars} characters). Leave this empty to keep it.",
                "已保存密钥（{chars} 个字符）。留空则保持不变。"
            ),
            SettingsAiApiKeyMissing => ("No key stored yet", "还没有保存密钥"),
            SettingsAiShow => ("Show", "显示"),
            SettingsAiHide => ("Hide", "隐藏"),
            // ── 动作与回执
            SettingsAiSave => ("Save", "保存"),
            SettingsAiTest => ("Test connection", "测试连接"),
            SettingsAiTesting => ("Testing…", "正在测试…"),
            // M389：这句必须说真话，而且**不许枚举**——枚举式清单一定会再烂。
            // 查过的装配期快照键（`AiSettings::load`，`crates/cli/src/commands/agent/probe.rs:209-225`）：
            // `ai.enabled` / `ai.provider` / `ai.base_url` / `ai.model` / `ai.api_key`——**五键全是**；
            // 立刻生效的只有 `ai.yolo`（判定点现读文件）。所以只说「除 YOLO 外」，
            // 不点名字，将来加键也不会再说半真话。
            SettingsAiSaved => (
                "Saved. The YOLO switch applies immediately; the other assistant settings apply once the assistant session is rebuilt (restart the launcher, or the next message while the assistant is simulated).",
                "已保存。YOLO 开关立刻生效；其余助手设置要等助手会话重建后才生效（重启启动器，或仍是模拟助手时的下一次发送）。"
            ),
            SettingsAiRefused => ("Not saved: {reason}", "未保存：{reason}"),
            SettingsAiUnsaved => ("There are unsaved changes.", "有未保存的改动。"),
            SettingsAiNoStorage => (
                "No settings file is available in this run, so nothing can be saved or tested.",
                "这一次运行没有可用的设置文件，保存与测试都做不了。"
            ),
            SettingsAiTestDisabled => (
                "Fill in the missing field before testing: {missing}.",
                "先补上这一项再测试：{missing}。"
            ),
            // ── 缺项（四种各说各的）
            SettingsAiMissingDisabled => ("the assistant is disabled", "助手处于关闭状态"),
            SettingsAiMissingBaseUrl => ("the endpoint address", "端点地址"),
            SettingsAiMissingModel => ("the model name", "模型名称"),
            SettingsAiMissingApiKey => ("the API key", "API 密钥"),
            // ── 拒绝保存的原因（本地校验）
            SettingsAiReasonProvider => ("the provider value is not recognised", "提供商取值不认识"),
            SettingsAiReasonBaseUrl => ("the endpoint must start with http:// or https://", "端点必须以 http:// 或 https:// 开头"),
            SettingsAiReasonModel => ("the model name is required", "必须填模型名称"),
            // ── 测试结论（六档）
            SettingsAiTestPassed => ("Connected — the endpoint answered.", "连接通过 —— 端点有正常回应。"),
            SettingsAiTestAuth => ("Authentication failed: the endpoint rejected the key.", "鉴权失败：端点拒绝了这个密钥。"),
            SettingsAiTestModel => ("The endpoint answered, but this model does not exist.", "端点通了，但没有这个模型。"),
            SettingsAiTestUnreachable => ("The endpoint is unreachable.", "端点不可达。"),
            SettingsAiTestTimeout => ("Timed out while waiting for the endpoint.", "等端点回应超时了。"),
            SettingsAiTestOther => ("The request failed for another reason (see the detail below).", "请求因为别的原因失败了（详情见下）。"),
            SettingsAiTestNotConfigured => ("Nothing was sent: the configuration is incomplete.", "什么都没有发出去：配置还不完整。"),
            SettingsAiTestTarget => ("Target: {base_url} · model {model} · {ms} ms", "目标：{base_url} · 模型 {model} · {ms} 毫秒"),
            SettingsAiTestDetail => ("Detail: {detail}", "详情：{detail}"),
        } }
    };
}

pub(crate) use settings_ai_entries;
