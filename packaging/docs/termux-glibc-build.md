# Termux + termux-x11 上的 glibc 构建（试验级）

本页记录**在 Termux 里构建 glibc 版 GUI，并在 termux-x11 里跑起来**的可复现步骤。
2026-11-06 在一台 OnePlus 15（Android 17、Termux、KirinSU/root 环境）上实测通过：
`--software-renderer --screenshot` 出 1180×760 PNG，中文正常，`msa client_id=configured source=build`。

**定位**：试验级、不承诺。发布矩阵里没有面向 Termux 的资产；本文只描述源码自建路径。
GUI 只在上述设备 + CPU 渲染（tiny-skia）上验证过，GPU 路径见 §5（尚未在 glibc 侧验证）。

## 1. 为什么不能直接 `cargo build`

Termux 的 `rustc` host 是 `aarch64-linux-android`（`target_os = "android"`）：

- `crates/app-iced/Cargo.toml` 的 `x11` / `wayland` feature 只挂在 `cfg(target_os = "linux")` 下；
- iced 的窗口层 winit 0.30 在 android target 上走 Android（NativeActivity）后端，
  会拉 `android-activity` 并要求 `game-activity`/`native-activity` feature，普通进程编不过也跑不了。

所以要在 Termux 里产出一个 **`aarch64-unknown-linux-gnu`（glibc）** 二进制，靠 Termux 的
[glibc 运行时](https://packages.termux.dev/apt/termux-glibc/) 原生执行。

## 2. 工具链（一次性）

以下路径用变量表示，实际可放在任意本地目录（示例放在仓库内的 `.gnu/`，已 gitignore）：

```sh
PREFIX="${PREFIX:-/data/data/com.termux/files/usr}"
G="$PWD/.gnu"
```

### 2.1 glibc 运行时

确认 `$PREFIX/glibc/lib/ld-linux-aarch64.so.1` 存在（`pkg install glibc-runner` 或装过 glibc 仓库
的包即可）。**不需要 `gcc-glibc`**：链接用 Termux 自带的 clang + lld，glibc 的 CRT/头文件由
`glibc` 包提供。

### 2.2 Rust gnu 工具链

下载 Rust 1.99.1（或与 `rust-version` 兼容的 stable）的 `rustc` / `rust-std` / `cargo`，
以及跑门禁需要的 `clippy` / `rustfmt` 五个 `aarch64-unknown-linux-gnu` 组件。官方源在国内很慢，
用 USTC 镜像：

```sh
mkdir -p "$TMPDIR/ull-gnu" && cd "$TMPDIR/ull-gnu"
for c in rustc rust-std cargo clippy rustfmt; do
  curl -fLO "https://mirrors.ustc.edu.cn/rust-static/dist/$c-1.99.1-aarch64-unknown-linux-gnu.tar.xz"
done
```

解包后按组件布局合并到 `$G/toolchain`（`rustc/bin`、`cargo/bin` → `toolchain/bin`，
两者的 `lib/` → `toolchain/lib`，`rust-std-*/lib/rustlib/aarch64-unknown-linux-gnu` →
`toolchain/lib/rustlib/`）。

### 2.3 改 interpreter / rpath

工具链里的 `rustc`、`cargo`、`rustdoc` 是 glibc ELF，默认 interpreter 是 `/lib/ld-linux-*.so.1`，
在 Termux 上不存在：

```sh
LD="$PREFIX/glibc/lib/ld-linux-aarch64.so.1"
for f in rustc cargo rustdoc clippy-driver cargo-clippy rustfmt cargo-fmt; do
  patchelf --set-interpreter "$LD" --set-rpath "$PREFIX/glibc/lib:$G/toolchain/lib" \
    "$G/toolchain/bin/$f"
done
```

> **坑（实测）**：`cargo clippy` 会在 PATH 里找 `cargo-clippy`。如果只装了 Termux 的 Rust，
> 找到的是 android 版 `cargo-clippy`，`aws-lc-sys` 等构建脚本会被当成
> `--target=aarch64-linux-android` 编译并失败。把 gnu 工具链的 `bin` 放在 PATH 最前面即可；
> `rustc`/`rustfmt` 同理。

### 2.4 gcc 运行时片段

`clang -fuse-ld=lld` 仍然需要 `crtbeginS.o` / `crtendS.o` / `libgcc.a`。从 Debian 的
`libgcc-14-dev`（arm64）取即可（只解包，不安装）：

```sh
curl -fLO https://deb.debian.org/debian/pool/main/g/gcc-14/libgcc-14-dev_14.2.0-19_arm64.deb
dpkg-deb -x libgcc-14-dev_14.2.0-19_arm64.deb "$G/gnu-support"
# crt/libgcc 在 $G/gnu-support/usr/lib/gcc/aarch64-linux-gnu/14/
```

### 2.5 linker 驱动

写一个 `$G/bin/cc-glibc`（`chmod +x`）：

```sh
#!/data/data/com.termux/files/usr/bin/sh
unset LD_PRELOAD
P=$PREFIX
S="<仓库>/.gnu/gnu-support/usr/lib/gcc/aarch64-linux-gnu/14"
C="clang --target=aarch64-unknown-linux-gnu --sysroot=/ -isystem $P/glibc/include \
   -B$P/glibc/lib -B$S -L$S -L$P/glibc/lib -fuse-ld=lld"
for a in "$@"; do
  case "$a" in -c|-E|-S) exec $C "$@" ;; esac
done
exec $C -Wl,--dynamic-linker=$P/glibc/lib/ld-linux-aarch64.so.1 -Wl,-rpath,$P/glibc/lib "$@"
```

要点：`--sysroot=/` 而不是 `--sysroot=$PREFIX/glibc`——glibc 的 `libc.so` 是含绝对路径的
linker script，带 sysroot 会被二次拼接；`-B $P/glibc/lib` 让 clang 找到 `Scrt1.o`/`crti.o`/`crtn.o`。

## 3. 构建

```sh
unset LD_PRELOAD                       # 必须：Termux 的 libtermux-exec 会被 glibc loader 当成坏 ELF
G="$PWD/.gnu"
export PATH="$G/toolchain/bin:$PATH"
export CARGO_TARGET_DIR="$G/target"
export RUSTFLAGS="-C linker=$G/bin/cc-glibc"
export CC="$G/bin/cc-glibc" CXX="$G/bin/cc-glibc" AR="$PREFIX/glibc/bin/ar"
export CARGO_HTTP_CAINFO="$PREFIX/etc/tls/cert.pem"   # glibc 版 cargo 不认 Android 的 CA 路径
export ULL_MSA_CLIENT_ID=<id>          # 可选；烘微软登录 id，值不进 git

# crates 走 USTC 稀疏索引（官方源在部分网络下只有 ~20 KB/s）
cargo --config 'source.crates-io.replace-with="ustc"' \
      --config 'source.ustc.registry="sparse+https://mirrors.ustc.edu.cn/crates.io-index/"' \
      build --release -p unlucky-app-iced --no-default-features
```

`--no-default-features` 关掉 `gpu`，只编 tiny-skia（CPU）。产物：`.gnu/target/release/unlucky-launcher-iced`。

门禁（与 CI 同一组命令，环境同上）：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets --no-fail-fast
```

Termux 上有三条**环境差异**失败（不是回归）：`launch` 测试读
`/proc/<pid>/task/<pid>/children`（Android 内核没有）、natives 事件的长路径被脱敏成
`<opaque>`、`settings_bridge` 的 mtime 热重载偶发（flaky）。PC 的 Linux CI 不受影响。

## 4. 运行（termux-x11）

```sh
# 一次性：glibc 版 X 运行库（cf 源慢时可把 sources.list 里的 packages-cf 换成 packages.termux.dev）
pkg install libx11-glibc libxkbcommon-glibc fontconfig-glibc \
            libxcursor-glibc libxrandr-glibc libxi-glibc libxext-glibc \
            libxcb-glibc libwayland-glibc

termux-x11 :0 -ac &          # 配合已安装的 termux-x11 App 使用
DISPLAY=:0 .gnu/target/release/unlucky-launcher-iced --software-renderer
```

无头取证（CI 同款通道）：

```sh
DISPLAY=:0 .gnu/target/release/unlucky-launcher-iced \
  --software-renderer --screenshot /tmp/ull-termux.png --data-dir /tmp/ull-termux-data
```

- 中文：GUI 在 Termux 上会把 `/system/fonts/NotoSansCJK-Regular.ttc` 注册进 cosmic-text
  （fontdb 不扫 Android 的字体目录），无需额外环境变量。
- 启动日志里 `[auth] msa client_id=configured source=build` 表示构建期 id 已烘入。
- **横屏优先**：Termux 上默认窗口尺寸会按当前 X 屏幕等比自适应（日志
  `[window] ... source=termux-screen`），横屏铺满、竖屏也不溢出；验收尺寸 1180×760 仍是
  默认基准，`ULL_WINDOW_SIZE=1520x900` 这类覆盖仍然优先。

### 4.1 Termux 上的 Java

`ull java install <major>` 在 Termux 上**默认先用系统包**（`openjdk-17/21/25`）：

- 内置 SAU / TUNA / USTC / 官方四个 termux-main 镜像，按顺序尝试；
- `apt-get` 的 lists/cache 隔离在 `$PREFIX/tmp/ull-java-termux/`，不改用户的 apt 配置；
  装完自动清理该目录（一次安装约 100 MB 临时占用）；
- 装完从 `$PREFIX/lib/jvm/java-<major>-openjdk` 发现，`ull java list` 直接可见；
- `--vendor termux` 只走系统包；`temurin|oracle|microsoft|zulu` 仍走归档下载链。
- 已实测：`ull java install 17` 从 SAU 镜像装完 `openjdk-17`（含 `openjdk-17-x`），
  约 20 秒；`ull java list` 随后列出 `[system]` 的 17。

### 4.2 跑 MC 需要的运行时（已手动验证）

bionic 的 Termux JVM 加载不了 MC 的 glibc natives，所以 MC 要用 **glibc JVM**：

```sh
# Temurin 21 JRE（linux-aarch64），用 glibc loader 直接跑
curl -L -o jre21.tar.gz \
  https://api.adoptium.net/v3/binary/latest/21/ga/linux/aarch64/jre/hotspot/normal/eclipse
tar -xzf jre21.tar.gz -C "$G/jre"
JRE="$G/jre/jdk-21.0.13+9-jre"
$PREFIX/glibc/lib/ld-linux-aarch64.so.1 \
  --library-path "$JRE/lib:$JRE/lib/server:$PREFIX/glibc/lib" "$JRE/bin/java" -version
```

- AWT/X11：`DISPLAY=:0` 下 Swing 窗口正常（`headless=false`、`swing-ok`）。
- GL：需要 §5 的 Mesa 26 GL 构建（`-Dgallium-drivers=zink,softpipe -Dglx=dri`）。
  `zink` 目前卡在 `DRI3 not available`；**softpipe 可用**：
  `MESA_LOADER_DRIVER_OVERRIDE=swrast LIBGL_ALWAYS_SOFTWARE=1`。
- 端到端探针：LWJGL 3.3.3 arm64 natives + GLFW + GLX → `GL_VERSION=3.3 (Compatibility
  Profile) Mesa 26.2.6`、`GL_RENDERER=softpipe`、`gl-ok`。

> **启动器已接线**（本分支）：
> - Linux/aarch64 上 LWJGL3 的 natives 自动改写成 `natives-linux-arm64`
>   （Mojang 的 `libraries.minecraft.net` 对 arm64 一律 404，下载兜底到阿里云 Maven 镜像）；
> - Termux glibc 构建启动 MC 时，子进程默认带 `MESA_LOADER_DRIVER_OVERRIDE=swrast`
>   与 `LIBGL_ALWAYS_SOFTWARE=1`（用户自定义环境变量仍可覆盖）。
>
> **还差一步**：把实例的 Java 指到 glibc JRE。Temurin 解包后 `bin/java` 默认的
> interpreter 在 Android 上不存在，先 patchelf 一次即可直接执行：
>
> ```sh
> patchelf --set-interpreter "$PREFIX/glibc/lib/ld-linux-aarch64.so.1" \
>   --set-rpath "$PREFIX/glibc/lib:$JRE/lib:$JRE/lib/server" "$JRE/bin/java"
> "$JRE/bin/java" -version
> ```
>
> 之后在实例设置里把 Java 指向 `$JRE/bin/java`（或 `--java`）即可。
>
> **真机验证（1.20.4）**：`ull install 1.20.4 --name mc-test`（740 MB）成功后，
> `ull launch mc-test --java "$JRE/bin/java" --account Tester`，
> MC 主菜单在 termux-x11 中渲染（窗口 854×480；日志在实例目录
> `versions/1.20.4/logs/latest.log`）。非致命项：narrator 的 flite 缺失、
> LWJGL JNI 版本告警、oshi 的 cpuVendor NPE（都未阻断启动）。

## 5. GPU（已打通：自编 Mesa 26.2.6 + KGSL）

termux-x11 的 X 客户端可以走硬件；glibc 侧的问题在**驱动**，不在 loader：

1. `vulkan-icd-loader-glibc` + ICD JSON 本身正常。
2. Termux 软件源的 glibc Mesa 24.2.6 不认识 Adreno 840
   （`chip_id = 44050A31 is unsupported`）。
3. Arch 的 `vulkan-freedreno 26.2.6` 能加载，但它是 Linux/DRM 构建，缺 KGSL
   （`/dev/dri` 对 `untrusted_app` 被 SELinux 拒绝）。

**解法：给 glibc 自编一份带 KGSL 的 Mesa 26.2.6。** 本机实测通过：

```sh
# 1) 源码 + 依赖
#    mesa-26.2.6.tar.xz（archive.mesa3d.org）
#    Debian libstdc++-14-dev 的头（$G/gnu-support/usr/include/c++/14）
#    meson / ninja（pip 装 meson）
# 2) 把源码里 *.py 的 shebang 从 #!/usr/bin/env python3 改成 Termux python3
#    （Android 没有 /usr/bin/env）
# 3) meson setup（cc-glibc 包装器：-c 不带链接参数、C++ 加 -stdlib=libstdc++、
#    共享库链接加 -lstdc++；详见 §3）
meson setup build3 --native-file="$G/mesa-native.ini" --wrap-mode=nofallback \
  -Dbuildtype=release -Dstrip=true -Dplatforms=x11 \
  -Dvulkan-drivers=freedreno -Dfreedreno-kmds=kgsl \
  -Dgallium-drivers=zink,softpipe -Dglx=dri -Dopengl=true -Degl=enabled \
  -Dgles1=disabled -Dgles2=enabled -Dllvm=disabled -Dgbm=disabled \
  -Dvideo-codecs= -Dzstd=enabled -Dbuild-tests=false
ninja -C build3
```

`--wrap-mode=nofallback` 是为了跳过 libarchive 子项目（它只被 freedreno 的 decode 工具需要，
且它自带的 `id_t` 兜底宏会和 glibc 头冲突）。

安装与启用：

```sh
cp build3/src/freedreno/vulkan/libvulkan_freedreno.so \
   "$PREFIX/glibc/lib/libvulkan_freedreno_ull.so"
cat > "$PREFIX/glibc/share/vulkan/icd.d/ull_freedreno.json" <<EOF
{"ICD":{"api_version":"1.4.335","library_path":"$PREFIX/glibc/lib/libvulkan_freedreno_ull.so"},"file_format_version":"1.0.0"}
EOF
```

应用启动时（glibc 构建 + Termux + 该 JSON 存在）会自动把它设进
`VK_ICD_FILENAMES` / `VK_DRIVER_FILES`（`app/boot/termux_gpu.rs`）；用户显式设了这两个
变量时不覆盖。实测日志：

```text
[probe] adapters=1 [Adreno (TM) 840 [IntegratedGpu]] hardware=true :: request=auto -> ICED_BACKEND=wgpu
[gpu] Selected: AdapterInfo { name: "Adreno (TM) 840", driver: "turnip Mesa driver",
      driver_info: "Mesa 26.2.6", backend: Vulkan }
[shot] 1657x1067 renderer=wgpu，首帧 61 ms
```

没装驱动/没有 GPU 时仍会打印具名错误并自动降级 tiny-skia（同一进程不能重建事件循环，
降级走 respawn），所以 GPU 变体在纯 CPU 环境也能跑。

## 6. 已知边界

- 没有面向 Termux 的发布资产，也不在 CI 矩阵里；每次都要源码自建。
- 只在 CPU 渲染 + 单台设备（Adreno 840 / Android 17）上验证过。
- **装好 JDK ≠ 能跑 MC**：MC 的 LWJGL natives 是 glibc `.so`，bionic 的 Termux JVM 加载不了；
  这条与「启动器 GUI 能显示」是两件事，未在本文覆盖。
- `ULL_MSA_CLIENT_ID` 属于仓库 Secret，构建时从环境/Secret 注入，**不写进任何 git 文件**。
