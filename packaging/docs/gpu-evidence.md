# GPU 截图证据账（M249）

判据的**唯一实现**是 `packaging/check_gpu_shot.py`（零第三方依赖，19 条自测向量）。
样本是 M246 在本机的实测产物 `DevDocs/artifacts/gpu-probe/`（`DevDocs/` 被 `.gitignore:6` 忽略，
**图不进仓库**，所以这里只引它的读数，不引它的字节）。

一句话：这套判据把「这张图是 GPU 档出的」从**人读日志**变成**可复跑的退出码**，但它证明的是
「像素由哪条光栅化路径产出」，**不是**「用户看到的窗口长这样」。第 4/5 节逐条划线。

**CI 接线（M253）在第 8 节**：`--self-test` 无条件跑，真实产物判据只在有可用产物时跑；
**无 GPU 时显式跳过并写明原因，跳过与通过分开写**（跳过 ≠ 通过）。

## 1. 怎么跑

```text
python packaging/check_gpu_shot.py <stderr 日志> <PNG>              # 正例：六条判据，0/1 退出
python packaging/check_gpu_shot.py <stderr 日志> --expect-refuse [PNG]  # 负例：拒绝降级出图是否成立
python packaging/check_gpu_shot.py --self-test                     # 19 条向量（数据 tests/gpu_shot_vectors.json，脚手架 tests/gpu_shot_selftest.py）
```

退出码：`0` = 判据全过；`1` = 有判据没过（**逐条点名**，不只印 FAILED）；`2` = 用法错（少参数 / 认不出的参数）。
判据不看日志的**请求值**（`[probe] renderer = wgpu` 只是意图），只看**结果行**（`[shot] … renderer=`）。

**取日志必须带 `ULL_GPU_LOG=info`**：`Selected: AdapterInfo { … }` 那一行是 wgpu 的 info 日志，
默认级别下根本不落盘（实测：a1b 那份 24 行 stderr 里 `^\[gpu\]` **0 条**，见 §5 第 2 条）。

## 2. 本机实跑退出码（M246 产物，逐条）

| 用例 | 命令（节选） | 退出码 | 判据怎么说话 |
|------|-------------|--------|-------------|
| A1 正例 GPU | `a1-gpu.stderr.log + a1-gpu-light.png` | **0** | 6/6 过：`renderer=wgpu`、`device_type=DiscreteGpu`（RX 6600 XT / Vulkan）、1180x760、`final=wgpu result=started`、唯一色 1856 / σ 33.21 / 强边缘 0.05778 |
| A2 正例 GPU（preflight） | `a2-preflight.stderr.log + a2-preflight-gpu.png` | **0** | 6/6 过（与 A1 字节相同的图，另带 `[gpu-preflight] ok type=DiscreteGpu`） |
| A4 正例 GPU（auto→shell） | `a4-auto.stderr.log + a4-auto-shell.png` | **0** | 6/6 过：`reason=auto-hardware-adapter` 也真的落到 wgpu |
| A3 CPU 档 | `a3-cpu.stderr.log + a3-cpu-light.png` | **1** | 3 条红：`renderer=tiny-skia`、无 `Selected` 行、`final=tiny-skia`（PNG 本身没问题：唯一色 2519 / σ 35.43） |
| A3b CPU 档重复 | `a3b-cpu-repeat.stderr.log + a3b-cpu-light.png` | **1** | 同上（重复性对照，结论一致） |
| A5 CPU 档 shell | `a5-cpu-shell.stderr.log + a5-cpu-shell.png` | **1** | 同上 |
| A6 负例，按正例口径判 | `a6-negative-refuse.stderr.log + a6-negative-refuse.png` | **1** | **6 条全红**（其中 3 条是「PNG 不存在」——负例本来就没图） |
| A6 负例，`--expect-refuse`（传 PNG 路径） | `a6-negative-refuse.stderr.log --expect-refuse a6-negative-refuse.png` | **0** | **3/3** 过：`via=refuse`、无出图行、目标 PNG 不存在 |
| A6 负例，`--expect-refuse`（不传 PNG） | `a6-negative-refuse.stderr.log --expect-refuse` | **0** | **2/2** 过（判据 9 不登记：`--expect-refuse` 的 PNG 参数是可选的） |
| A1b 日志 + A1b 图 | `a1b-gpu-repeat.stderr.log + a1b-gpu-light.png` | **1** | 见 §5 第 2 条：这份日志没开 `ULL_GPU_LOG=info`，**没有** `Selected` 行 ⇒ 单独看它无法自证适配器 |
| 串味：A1 日志 + A3 的 CPU 图 | `a1-gpu.stderr.log + a3-cpu-light.png` | **0** | **这是已知盲区**，见 §5 第 1 条（两张图同为 1180x760） |

A6 的负例形状（`WGPU_BACKEND=metal`）：`Available adapters: []` → `wgpu 初始化失败（具名）` →
`拒绝降级出图` → `[backend] final=none result=failed … via=refuse`，退出码 1，**没有 PNG**。

## 3. 六条判据：每条抓得住什么、抓不住什么

判据 1–3 来自 M246 的实测口径，4–6 是为「日志与图对不上号」补的钉子。每条都能单独判红、单独打印。

| # | 判据 | 抓得住 | 抓不住 |
|---|------|--------|--------|
| 1 | **全部** `[shot] … renderer=` 行都是 `wgpu`（`set(renderers) == {"wgpu"}`） | 「其实走了 CPU 档」——CPU 档在这行写 `renderer=tiny-skia`（M97 起这行打的是**实际**后端，不是请求值，见 `crates/app-iced/src/renderer/mod.rs:52`） | 只证明这些行被打印过；日志被手工拼装、或从别的进程拷来的行，它分不清（判据只读文本）；「一次运行里混了两种 renderer」（`>>` 拼接、`via=respawn` 降级）**一律判红**，但**同种 renderer 的多次运行拼在一起**看不出来（§5 第 7 条） |
| 2 | `Selected: AdapterInfo { device_type: X }` 且 `X ∉ {Cpu, Other}`；**取不到 `X` 一律判红** | 「wgpu 真建成了设备，而且选中的不是 WARP(`Cpu`) / Gl 兼容层(`Other`)」；与 `crates/app-iced/src/probe.rs:165-167` 的硬件判据同口径 | 选中的是**哪一块**卡不判（独显 vs 集显都算硬件）；不看 `Available adapters` 那 12 条，只看 `Selected` 那条 —— 也就抓不住「枚举里有独显、实际选了集显」之外的更细的驱动行为；**多行 Debug / 字段缺失**时不知道真值是啥，只按「取不到即判红」处理（宁可假红：iced 换成 pretty-print Debug 时真 GPU 图会被判红，得回来改正则） |
| 3 | PNG 能解码且像素数 > 0 | 0 字节 / 半截写入 / 根本不是 PNG（签名、IHDR、IDAT 逐层查） | 只管「能解出来」，不管内容对不对（那是判据 6 的事）；不解调色板与隔行 PNG（沿用 `check_screenshot.py` 的同一条限制） |
| 4 | PNG 宽高 = 日志 `[shot]` 声明的宽高 | 拿错文件的第一步：日志说 1180x760、图是别的尺寸 | **同尺寸互换抓不住**（见 §5 第 1 条）；分辨率和内容是否同一帧也抓不住 |
| 5 | `[backend] final=wgpu result=started` | 后端结论行与判据 1/2 自相矛盾（例如 `final=none … via=refuse`） | 需要 `ULL_ACCEPTANCE=1` 才有这行；只看这一行的话，A6 那种「一行都没出图」和「出了图但日志没写」分不清 |
| 6 | PNG 过 `check_screenshot.py` 的非空白阈值（唯一色 ≥64 / 亮度σ ≥3.0 / 强边缘 ≥0.001） | 「进程起来了、PNG 合法、但一个像素都没画」的空帧 | 阈值很低（这是它的设计：跨主题/字体不误报），**不判视觉正确性**，也不判元素齐全 |

`--expect-refuse` 的三条（负例口径）：`via=refuse` 存在、**没有**任何 `[shot]` 出图行、目标 PNG 不存在。
它证明「该拒绝时真拒绝了」，**不证明**「GPU 能出图」——两者互为对照，缺一个都能蒙混过关。

### 3.1 判据对「行为被改坏」敏感（变异自证）

`--self-test` 的向量在 `packaging/tests/gpu_shot_vectors.json`，每条的 `expect_fail` 是**期望的失败判据
清单，逐条对**。三个变异都能被抓住（本轮 r1 又补了判据 2 的「取不到即红」）：

| 变异 | 抓它的向量 | 结果 |
|------|-----------|------|
| `DISALLOWED_DEVICE_TYPES` 里删掉 `Cpu`（= 允许 WARP 当 GPU） | 「renderer=wgpu 但 Selected 是 Cpu」 | 由红转绿 ⇒ 清单对不上 ⇒ `--self-test` 退出 1 |
| 删掉判据 1（不再要求 `renderer=wgpu`） | 「删掉 [shot] renderer 行」、「日志里混入第二种 renderer」 | 由红转绿 ⇒ 清单对不上 ⇒ `--self-test` 退出 1 |
| 判据 2 不再把「取不到 device_type」当失败（恢复旧写法） | 「Selected 换成多行 Debug」、「`Selected: AdapterInfo {` 空体」 | 由红转绿 ⇒ 清单对不上 ⇒ `--self-test` 退出 1 |

## 4. 这份证据**能**证明什么

逐条都能指到「哪个文件 + 哪条判据过了」：

1. **字形栅格与文字边缘的抗锯齿是 GPU 路径给的。** `a1-gpu-light.png` 的 1180×760 像素由 wgpu/Vulkan
   在 RX 6600 XT 上光栅化（判据 1+2+5 全过，`Selected` 是 `DiscreteGpu`），而同一屏的 CPU 档（`a3`）
   被同一套判据判红。⇒ 「12px 规格文本的覆盖率采样来自 MSAAx4 的那条路」这件事，是**图+日志**一起证的。
2. **面板 α 与玻璃只有 GPU 档能出。** CPU 档在代码层就把它们强制掉了：`crates/app-iced/src/pages/
   appearance/alpha.rs` 的 `panel_is_forced_opaque(Backend::Cpu) == true` ⇒ `effective_panel_alpha`
   恒 1.0、`effective_glass` 恒 false。所以只要是**真的** GPU 档出的图，它拍到的面板 α/玻璃是产品
   在 GPU 档下的真实合成结果，而不是「CPU 兜底也能喂出来」的近似。
3. **两个后端在同一屏上的一致性是可复核的。** `--diff` 的读数（M246）：gallery `differing=51055/896800
   (5.69%) max_delta=200 best_shift=(0,0)`、shell `19088/896800 (2.13%) max_delta=75 best_shift=(0,0)`；
   差异 **99.3%** 落在亮度边缘上、`delta>32` 的 9301 像素连成 3002 块（最大 83 px）；两个后端各自连跑
   两次 PNG **字节相同**（`52d4c1d6e464da16` ×2 / `10696a043bd9e7b9` ×2）。⇒ 「几何没有平移、差的是
   边缘覆盖率」这个结论成立，5.69% 不是抖动。
4. **`--renderer=auto` 不是「悄悄走 CPU」。** A4 的 `reason=auto-hardware-adapter` 与 `Selected:
   DiscreteGpu` 同时成立，判据 0 退出。
5. **断开的 RDP 会话也能出 GPU 图**（M246 的现场条件：`quser` 显示 `2 断开`）。⇒ 「断连 ⇒ GPU 截不了图」
   这个说法不成立；成立的是 M245 §4 的「断连打折首帧耗时与整窗 α，不打折像素内容」。
6. **「该拒绝时真拒绝」有机器判据。** A6 的 `via=refuse` + 无 PNG + 退出码 1 由 `--expect-refuse` 判 0。

## 5. 这份证据**不能**证明什么（逐条，别把「没证」读成「没问题」）

1. **图与日志的绑定只到「尺寸」一级。** 同一尺寸的两张正经图互换，本脚本判绿（实测：A1 日志 + A3 的
   CPU 图 = 退出码 **0**）。要堵这个口得让产品把 PNG 的 sha256 打进日志（改 Rust，不在本 mission）。
   ⇒ 现有用法是「一次运行、当场取日志与图」，**不是**「从一堆历史产物里随便挑两个配对」。
2. **日志级别不够时判据 2 无从判定。** `Selected: AdapterInfo` 是 wgpu 的 info 级日志：a1b / a5 那两轮
   没带 `ULL_GPU_LOG=info`，两份 stderr 里 `^\[gpu\]` 各 **0 条**。a1b 的「合格」来自它与 a1 **字节相同**
   （`52d4c1d6e464da16`），**不是**来自它自己的日志 —— 所以本脚本对 a1b 判红是**如实**（这份日志单独
   确实证不了适配器），不是误报。
3. **首帧 / 帧率不能证。** 全部 `[perf]` 都是墙钟，且冷热差距极大（A1 首次 `probe=1281ms`、`font=749ms`；
   后续进程 `584/587ms`、`55~57ms`）。A1 `3661ms` vs A3 `3146ms` **不能**读成「GPU 比 CPU 慢」。
4. **整窗 α 与圆角观感不能证，任何截图都不行。** `window::screenshot` 读的是渲染目标，而整窗 α
   （`LWA_ALPHA` 字节）与圆角（`DWMWA_WINDOW_CORNER_PREFERENCE`）由 **DWM 在合成时**施加，不在渲染
   目标里 —— 同一份判据在 Windows 上拍到的是「DWM 之前」的像素。观感只能人眼看真显示器。
5. **「窗口被 DWM 合成过」这件事没证。** 图是渲染目标，不是屏幕。DWM 是否真把这帧合成上屏、有没有掉帧、
   有没有被别的窗口遮挡，这张图一概不知道。
6. **换机器不成立。** 全部证据来自一台机器（RX 6600 XT / Raphael iGPU、12 个 adapter、RDP 断开会话）。
   `VirtualGpu` 会不会被判成硬件只在**代码层**确认（`probe.rs:165-167` 会放行），**没在真 `VirtualGpu`
   适配器上验过**（本机 12 条里一条都没有）。
7. **同种 renderer 的多次运行拼在一起，无法分辨图属于哪一次。** 判据 1 只挡住「混入**异种** renderer」
   （wgpu + tiny-skia 拼一起）；两次都是 `renderer=wgpu` 的日志 `>>` 到同一个文件时所有行都自洽，
   判据 4 又只比尺寸 ⇒ 两张同为 1180×760 的 GPU 图互换仍然判绿。要堵它得把 PNG 的 sha256 / 运行 id
   打进日志（改 Rust）。⇒ 一次取证 = 一份干净的 stderr，别往同一个文件里续写。
8. **交互启动那一支没验。** A6 验的是「取证启动（`--screenshot`）下拒绝降级出图」；交互启动的
   `via=respawn`（换进程降级）要求真窗口，本轮没开。
9. **dark 主题、其它屏、`ULL_WINDOW_SIZE` 变尺寸、macOS / Linux** 全部没跑：只有 gallery + shell 的 light。

## 6. 历史 `--diff` 基线：**永久不可复核**

`DevDocs/00-债务台账-p1.md:29` 记着一条 `differing=8134/896800 (0.91%) max_delta=60 best_shift=(0,0)`，
M246 本机实测是 **5.69% / 200**（gallery）与 **2.13% / 75**（shell）。两个数字**不可比**，原因是记录本身
缺项：那条没有 commit、没有屏幕、对应图（`DevDocs/artifacts/m187/`）**已丢失**。

⇒ 这条基线在任何将来都**无法复核**：没有图就没有分母，没有 commit 就没有树状态，重跑也只会得到新读数。
唯一还能比的是**几何判据**（`best_shift=(0,0)`），它一致。别把 0.91% 当「回归阈值」用；本账不承认它是基线。

## 7. 没验的部分（M249 自己）

- **M249 当时没接进 CI**：那一轮 `.github/workflows/**` 一行未动（接 CI 是策略决定：runner 上有真 GPU，
  还是接受「跑不起来就跳过」）。**M253 已接上**，见 §8；本节其余条目原样保留。
- **没在任何非本机环境跑过**：Windows + 上述单卡 + RDP 断开会话之外，Linux/macOS/其它 GPU 全没跑。
- **没改 Rust**：判据只读现有日志格式。若产品将来改了 `[shot]` / `Selected:` 的行格式，这里的正则
  会同时失效（`--self-test` 用的是**按现格式手拼**的合成日志，抓不到格式漂移——它守判据，不守格式）。
- **没验 `--diff` 阈值化**：本文第 4 节引的是 M246 的读数，本 mission 没有重跑 `--diff`。

## 8. CI 接线（M253）：无 GPU 时显式跳过，**跳过 ≠ 通过**

判据靠 **`.github/workflows/gpu-evidence.yml`** 跑（`push: main` / `pull_request` / `workflow_dispatch`，
以及 `workflow_call`：日常线 `dev` 经 `dev-guards.yml` 复用同一份，见下）。
**为什么新开一个文件而不是并进 `guards.yml`**：`guards.yml` 已是 399 行、红线 400（`file-size` 那道门
就扫 `*.yml`），挂一个 job 必越线；`release-build.yml` 正好 400 行、动它得先拆。与 `release-guard.yml`
当初单独成文的理由逐字相同。

**日常线 `dev` 也跑**（M253 r1 P2-1）：`dev-guards.yml` 里多一个
`gpu-evidence: uses: ./.github/workflows/gpu-evidence.yml`。`dev-guards.yml` 文件头自己写着
「门禁存在但日常线上永不运行 = 空跑的门禁，长期等于没有」，而这条 workflow 原先只在 `main`/PR 触发，
直推 `dev` 时**一次都不跑**。dev 上 `evidence` 照样显式跳过（无 GPU、无产物），所以 dev 拿到的是
`selftest` 那一半：19 条向量 + 三个退出码 + 接线自检。**刻意不加**「只跑自测」的开关 —— 那会给这道
门禁多开一个「可以合法关掉真判据」的旋钮。

三个 job，各管一件事：

| job | 什么时候跑 | 它的结论代表什么 |
|-----|-----------|-----------------|
| `selftest` | **无条件**（不需要 GPU、不需要真产物） | 「尺子合格」：19 条向量 + 三个退出码 0/1/2 用真 CLI 走一遍 |
| `probe` | 无条件，只探测、不判定 | 工作区里有没有可判的产物；没有就把原因写进 job summary 并打 `::warning::` |
| `evidence` | `if: needs.probe.outputs.available == 'true'` | 「这张图真是 GPU 档出的」：`check_gpu_shot.py` 的退出码（0 过 / 1 判据不过 / 2 用法错）即结论 |

**跳过与通过分开写**：没有可用产物时 `evidence` job 的 `if:` 不成立 ⇒ 它在 Actions 里是**灰色
skipped**，不是一个绿色 ✓；同时 `probe` 打出（本地手模拟的原样输出，见 §8.1）：

```text
GPU 证据：无硬件适配器（/dev/dri 下没有 renderD*，也没有可用的 nvidia-smi），且工作区里没有可用的样例产物
（约定路径要求 stderr 日志与同次运行的 PNG 同时存在；DevDocs/ 被 .gitignore 忽略，图不进仓库；event=push），
跳过真实产物判据（判据实现自测已过：那证明「尺子合格」，不证明「这张图是 GPU 出的」）
```

外加一条 `::warning::GPU 证据：跳过（…）—— 本次运行没有判过任何真实产物，不构成「GPU 证据已验证」`。
`available` 同时作为 job output 交给 `evidence`，所以「跳过」与「通过」在数据流上也是两条路。

四条路径的口径：

| 情形 | 结果 | 为什么这么定 |
|------|------|-------------|
| 工作区里有 `DevDocs/artifacts/gpu-probe/gpu-shot.stderr.log` + `gpu-shot.png`（约定成对） | `available=true` ⇒ `evidence` 真判 | 一次取证 = 一份干净的 stderr + 同次运行的那张图（§5 第 1/7 条）；**只给一半仍按跳过**（半套产物不算可用） |
| `workflow_dispatch` 填了 `shot_log` / `shot_png` | 同上 | 显式点名 ⇒ 真判 |
| 点了名但**文件不存在**（或正例没给 PNG） | **红**（`::error::` + exit 1） | 「明确说要判、东西不在」若能退化成跳过，就又是一次假绿 —— 有意为之的输入缺失是错误，不是 skip |
| 托管 runner 常态（无 GPU + 无产物） | 跳过 + 原因 + 黄条 | 见上；托管 runner 本来就没有硬件适配器 |

**在 CI 里怎么让它真判**（r1 复审 P2-2：原先 §8 说的「把产物放到 `DevDocs/`」在 CI 里没人做得到 ——
**checkout 只拿已跟踪文件**，而 `/DevDocs/` 在 `.gitignore:6` 里，也没有 `upload/download-artifact` 环节）：

1. 把一次取证的那一对产物**提交进你打算 dispatch 的那个 ref**：`git add -f
   DevDocs/artifacts/gpu-probe/gpu-shot.stderr.log DevDocs/artifacts/gpu-probe/gpu-shot.png`（`-f` 是必需的，
   否则被 ignore）；
2. 对该 ref 跑 `workflow_dispatch`，或干脆让它走 `push` / PR —— `probe` 会认这对约定路径，`available=true`；
3. 不想提交到 `DevDocs/` 的话，把两个文件放到任一**已跟踪**目录，再用 dispatch 的
   `shot_log` / `shot_png` 指过去（那两个输入只认「checkout 后确实存在的路径」）。

就地往 workspace 里拷文件（不提交、靠上一步的产物）**在这个设计里做不到**：没有 artifact 传递环节。

**接线自检的覆盖面**：`packaging/tests/gpu_shot_ci_selftest.py` 用 **10 条判据 + 33 条变异向量**
（r1 复审表 20 条 + 1 条补充 + r2 复审 12 条）把上面这些口径钉住，逐条见
`packaging/tests/gpu_shot_ci_vectors.json`：

1. 五步按 `id:` 必须在场且无条件（`vectors`/`exitcodes`/`wiring`/`detect`/`judge`），`exitcodes` 还得真断言 0/1/2；
2. 自测步必须**指向本脚本且是单条命令**（`|| echo 先忽略` 无从下手）；
3. **解析 `if:` 表达式后求值**判「判定挂门」（`available` 取 `'true'`/`'false'`/`''` 必须真/假/假，认不出即红）；
4. 两种调用形态都在场，且产物来自 `needs.probe.outputs.*`；
5. **判据步真跑两次**（正例必须 0、CPU 档必须非 0，`bash -e` 与裸 bash 两种模式都跑）；
6. 跳过路径**实跑 7 种输入**；
7. `continue-on-error` 按 YAML 布尔语义判（`'true'` / `${{ true }}` 都算开启）；
8. 吞码标记表（`|| true` / `; true` / `|| exit 0` / 裸 `exit 0`，兜底用）；
9. `probe.outputs.*` 必须逐个绑到 `${{ steps.detect.outputs.* }}`；
10. `on:` 必须同时有 `push`（含 `main`）/`pull_request`/`workflow_call`。

**它不覆盖**：连判据一起改（它不自守）；`if:` 的 GitHub 语义（那是本脚本的小求值器模拟的）；真 Actions 里的
job skip / required check 行为；判据 4 的「合法调用替换」（后果是假红不是假绿）；`exitcodes` 见证步自己的
断言被削弱；判据 6 那 7 种输入之外的新分支。

**载体依赖**：整个 `gpu-evidence.yml` 被删掉/替换后，**没有任何东西会跑这个自检** —— 守门人住在被守对象
自己的文件里（`guards.yml` 已经 399/400 行，塞不进去）。

本文件**不宣称**「不可能被改成总是绿」：r1 那一版这么写过，实测 19 条变异漏 11 条；r2 那一版
（6 条判据）实测 40 条新变异漏 11 条。所以这里只写**能复跑的封闭清单**（10 类 / 33 条向量）。
**这条门禁自 2026-11-11 起停止加固**：再往下是「守卫守守卫守守卫」，边际收益很低；要扩覆盖面必须由
人类新开 mission。

### 8.1 这一节**没验**的部分

- **CI 上从没真跑过**：本 workflow 是本地写的，**一次 GitHub Actions 运行都没有**（未 push）。
  本地验的是两件事，且都**已固化成每次自检都重跑的功能断言**：① `detect` 的 `run` 原文跑 7 种输入
  （跳过 / 约定成对 / 约定半套 / dispatch 成对 / 点名不存在 / 正例缺 PNG / 负例 —— 判据 6）；
  ② `judge` 的 `run` 原文跑两次（正例必须 0、CPU 档必须非 0，`bash -e` 与裸 bash 各跑一遍 —— 判据 5）。
  **这不是 CI 实测**：`if:` 求值、job skip 在 UI 上的呈现、`needs` 的输出传递在真 Actions 里仍未跑过。
- **没在真 GPU runner 上跑过**：`evidence` 的真判定只在本地用合成向量走过（0 与 1 都到达过），
  「真产物 + 真 GPU runner」这个组合没实测。
- **没验「跳过是否满足 required status check」**：本 workflow 没挂成必需检查；GitHub 对 skipped job
  与 required check 的交互口径没实测，别把这一节读成「跳过也能当门过」。
- **自检脚本自身的边界**在它自己的 docstring 里（不防「连判据一起改」；`!` 不在支持的表达式语法里，
  出现即红；只有 `probe` 那一步是功能断言，`vectors`/`exitcodes` 两步只做结构断言）。
