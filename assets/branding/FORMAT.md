# 品牌资产的格式与来源

本目录**四个**二进制文件（`ull.ico` / `ull-256.png` / `ull-64.rgba` / `ull-128.rgba`）**都是派生形式**，
版权与许可见同目录 `LICENSE.txt`（© 2026 AskaEth，保留所有权利，**不在** Apache-2.0 范围内）。

它们由 `packaging/branding/derive_assets.py` 从作者自有的 9000×9000 源图生成。**源图不入库**：
它在仓库之外，脚本以参数接收它的路径（不写死本机路径，也不把它拷进仓库）。

```bash
pip install pillow      # 只有重生成图标的本机需要
python packaging/branding/derive_assets.py "D:/Project/Image/.../ULL.png"
```

派生只做「缩放 + 编码」，不改画面内容。脚本自己守两条纪律：产出任一边长 ≤ 1024，
且本目录合计 ≤ 300 KB（超了直接红，不静默超）。CI 不跑这个脚本（不装图像栈），
跑的是 `packaging/check_branding_assets.py`——纯标准库，判据是「尺寸 / 体积 / LICENSE 在场」。

## ull.ico

标准 ICO 容器，7 档：

| 档位 | 编码 | 字节 |
| ---- | ---- | ---- |
| 16 / 24 / 32 / 48 / 64 / 128 | DIB（BITMAPINFOHEADER + 自底向上 BGRA + 全零 AND 掩码） | 1.1 KB ~ 67 KB |
| 256 | PNG（整张 PNG 文件塞进条目，ICO 目录里宽高写 0） | ~40 KB |

小尺寸走 DIB 是为了不依赖旧壳对 PNG-in-ICO 的支持；256 走 PNG 是因为 DIB 那一档单独就要
270 KB，会一个人吃掉体积预算。资源管理器/任务栏按需要的档位就近取一档，不会缩放 256 那张。

## ull-64.rgba

**裸字节**，没有文件头、没有尺寸字段、没有压缩：

- 64 × 64 像素，行优先（先第 0 行，再第 1 行……），每行 64 个像素；
- 每像素 4 字节，顺序 **R, G, B, A**（8 位无符号，非预乘 / straight alpha）；
- 总长度**必须恰好** `64 * 64 * 4 = 16384` 字节。

用途是**标题栏左侧的品牌标记**（M379）：`crates/app-iced/src/shell/title_icon.rs` 用
`iced::widget::image::Handle::from_rgba(include_bytes!(…).to_vec(), 64, 64)` 建一次纹理句柄，
再缩到 20 逻辑像素画在标题左边（边长取自 `Component::TopAppBar` 的微度量）。

一个 20px 的标记**不需要** 65 KB 的 128 档源，所以这一档没有跟着窗口图标一起升级。

## ull-128.rgba

同一套裸字节格式，只是边长 128：

- 128 × 128 像素，行优先，每像素 4 字节 R, G, B, A；
- 总长度**必须恰好** `128 * 128 * 4 = 65536` 字节。

用途是**运行时窗口图标**（窗口 / 任务栏 / Alt-Tab）：`crates/app-iced/src/app/boot/diagnostics.rs`
的 `ICON_SIZE = 128` + `ICON_PIXELS` + 那条 `const _` 断言喂给
`iced::window::icon::from_rgba(ICON_PIXELS.to_vec(), ICON_SIZE, ICON_SIZE)` —— 注意 `from_rgba` 是
`window::icon` 模块的**自由函数**（iced 0.14 里 `Icon` 只有 `into_raw()`），写成 `Icon::from_rgba`
是编不过的。

**为什么是 128 而不是 64**（M379）：150% / 200% 缩放下任务栏与 Alt-Tab 按 96 / 128 取图，64 那张
只能被放大 ⇒ 发虚。只看四个图标二进制文件：**197,620 B → 263,156 B**（多出来的正是
`128*128*4 = 65,536`）；再加上两份说明文档也仍在预算 307,200 B 之内（守卫脚本每次运行都打印实测合计）。
裸字节意味着这两条路径**都不做解码**：尺寸与像素布局在编译期钉死，
`packaging/check_branding_assets.py` 对两档长度各断言一次。

## ull-256.png

普通 PNG（8 位 RGBA），256 × 256，用作 README、关于页、将来安装包的图源。README 里那张图
就是它本身，没有第二份。
