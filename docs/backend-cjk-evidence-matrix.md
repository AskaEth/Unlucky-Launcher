# 后端 × 中文：什么断言只在软渲染下成立，什么必须在 GPU 下看（M187）

本文件登记一条**通道级缺口**，不是一条 bug：自动化证据全部走软件渲染，所以「只有 GPU 后端
才出现」的缺陷在 CI 里永远是盲区。下次有人要验「文字/像素在 GPU 上是否正常」时，先读这一页。

## 1. 现状：所有自动出图都是 `--software-renderer`

| 通道 | 位置 | 后端 |
|---|---|---|
| CI 冒烟截屏 | `.github/workflows/ci.yml` 的 `test` 作业「出四张像素守门用的图并写进 $GITHUB_ENV（仅 Windows）」那一步 | `--software-renderer`（强制 CPU） |
| CI 交互冒烟 | 同上文件的 `runtime-smoke` 作业「在 Xvfb 里跑一帧并截图」、`runtime-smoke-macos` 作业「尝试截图」两步 | `--software-renderer` |
| Linux 容器冒烟 | `packaging/linux-smoke-docker.sh` 里「2) 跑容器：构建 → xvfb 截图 → 像素判定」那一段的 `xvfb-run` | `--software-renderer` |
| D30 视觉验收（人工，按协议出图） | `DevDocs/PlanDoc/`（视觉验收协议） | 同上（CI 同款命令） |

> **锚点一律用「作业名 + 步骤名」，不用 `文件:行号`。** 这条规则不是本次改动造出来的：本次改动的
> base（`main`）上，上面 ci.yml 那三处行号里只有 `:101` 对得上，`:246` / `:283` 早就指偏了（分别落在
> 依赖安装步和一行注释上；`linux-smoke-docker.sh:90` 那时是对的）。也就是说行号锚点本来就在腐，
> 而 workflow 只会越来越长——要定位就按上表的名字 `grep` 作业名 / 步骤名，别再写回行号。

`--renderer=auto`（默认）会挑 wgpu；`--software-renderer` 强制 tiny-skia。CI 走后者是**有意的**：
容器/无 GPU 机器上没有 Vulkan/DX12 设备，硬要 wgpu 会红在一句「adapter 创建失败」上，与皮肤无关。

**代价**：tiny-skia 与 wgpu 是两套完全不同的文字光栅化路径（前者 `cosmic-text` + CPU 位图上传，
后者经 `cryoglyph` 上传到 GPU 字形图集）。所以「GPU 侧字形图集出错」这类缺陷，**任何自动证据都抓不到**。

## 2. 这轮实测：M190 的「owned 文字在 wgpu 下丢字」在本 branch 上**没有复现**

同一份构建、同一串中文、同一字号，一次 wgpu 一次 tiny-skia：

| 图 | 后端 | 内容 |
|---|---|---|
| `DevDocs/artifacts/m187/ab-wgpu.png` | wgpu（Vulkan，AMD RX 6600 XT） | 四行：`text_role_owned` / `text_role` × 大字号 / 正文字号，逐字相同 |
| `DevDocs/artifacts/m187/ab-tinyskia.png` | tiny-skia（`--software-renderer`） | 同上，与 wgpu 那张逐字一致 |
| `DevDocs/artifacts/m187/probe-acc-wgpu.png` | wgpu | 账户页设备码面板（M190 钉的那一行「剩 14:59」在 wgpu 下**正常画出**） |

结论（只对这份构建、这台机器成立，不推广）：`frame::text_role_owned` 与 `frame::text_role`
产出的控件在小规模中文下**行为一致**，两种后端都能画；把缺陷归因到「owned 路径」在本 branch
上没有证据支撑。要复现 M190 的现象，需要它当时的构建/环境。

## 3. 分工：哪些结论只信软渲染，哪些必须在 GPU 上看

| 断言类型 | 软渲染（tiny-skia）够吗 | 必须在 GPU 下复核 | 理由 |
|---|---|---|---|
| 版面几何（位置、宽高、折行、间距） | ✅ 够 | 建议一次 | 几何由布局树决定，两后端共用 |
| 文字**在不在**（整行缺失 / 空块） | ⚠️ 只能证明 CPU 路径 | **必须** | GPU 字形图集是独立实现（`cryoglyph`） |
| 字形形状 / 连字 / 每字形字体落点（「个别字换了字体」） | ⚠️ 同上 | **必须** | 两种后备字体选择可能不同 |
| 颜色 / 对比度 / 主题令牌 | ✅ 够（纯色由令牌给） | 建议一次 | 面板混合色只有 GPU 路径可验 |
| 材质层 / 高程阴影 | ⚠️ | **必须** | 阴影在 wgpu 与 tiny-skia 是两套画法 |
| 整窗 α / 无边框圆角（Win32） | ❌ 不看 | **必须**（且只在 Windows） | 与渲染后端无关，靠 DWM 下发 |
| 背景图 / 壁纸取色 / 图片缩放 | ⚠️ | **必须** | GPU 侧走纹理采样 |
| SVG 图标（含描边宽度） | ✅ 够 | 建议一次 | 光栅化后端不同 |

读法：**「有没有画出来」与「字形对不对」这两类，只认 GPU 图**；几何与令牌只认一份即可。

## 4. 怎么在 GPU 下出图（本机命令）

```bash
# GPU（wgpu；会自动挑适配器，日志里 [gpu] Selected: AdapterInfo{…} 是「真用上 GPU」的实证）
unlucky-launcher-iced --screenshot out-gpu.png --screenshot-theme=light

# CPU（tiny-skia）
unlucky-launcher-iced --screenshot out-cpu.png --screenshot-theme=light --software-renderer

# 两图逐像素比对（本程序自带，不需要 Python/PIL）
unlucky-launcher-iced --diff out-gpu.png out-cpu.png
```

`--diff` 的退出码：`0` 一致 / `1` 有差异 / `2` 参数错。**几何应当一致**（D30 的硬项），
颜色与字形抗锯齿允许不同——所以「有差异」不等于坏，要**看着图**判。

启动失败弹窗的两态各两张（wgpu / tiny-skia）就是这么出的：

```bash
ULL_DIALOG_FIXTURE=crash-report unlucky-launcher-iced --screenshot dialog-crash-wgpu.png
ULL_DIALOG_FIXTURE=no-logs      unlucky-launcher-iced --screenshot dialog-no-logs-wgpu.png
```

## 5. 机器条件（写下来，免得下次怀疑「没 GPU」）

本机 `--renderer=auto` 的实测选择：`AMD Radeon RX 6600 XT` / Vulkan / `device_type: DiscreteGpu`。
无独显的机器上 wgpu 会落到 `Microsoft Basic Render Driver`（CPU 实现，`device_type: Cpu`）——
那种图**不能**算 GPU 证据（它仍是 CPU 光栅化，只是走了 DX12 接口）。
判据：看 `[gpu] Selected:` 那行的 `device_type`，`Cpu` 一律不算。
