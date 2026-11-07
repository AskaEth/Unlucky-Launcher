#!/usr/bin/env bash
# 本地复跑 `.github/workflows/ci.yml` 的 `runtime-smoke` job：Linux + Xvfb + 软渲染截图。
#
# 为什么要有这条脚本：CI 的 runtime-smoke 在 GitHub 的 ubuntu-latest 上跑，
# 但「改动这个 job 的人」需要在没有 Linux 桌面的开发机上先自证一遍——否则改的就是没人验过的 YAML。
# 本脚本在 Docker 里复刻 job 的步骤（同样的 apt 依赖、同样的构建、同样的 xvfb-run 命令），
# 并把截图与判定结果留在宿主目录，供人眼与 `check_screenshot.py` 复核。
#
# 用法：
#   bash packaging/linux-smoke-docker.sh [仓库根目录] [输出目录]
#   ULL_SMOKE_SKIP_BUILD=1 bash packaging/linux-smoke-docker.sh   # 复用缓存的二进制
#
# 说明（诚实边界）：
# - 基础镜像 `rust:1-bookworm` 是 **Debian 12（glibc 2.36）**，官方 runner 是 **Ubuntu 24.04（glibc 2.39）**。
#   两者图形栈（X11/xvfb/Mesa 软渲染）来源不同，因此本脚本证明的是「Linux 上这条通道能真跑出图」，
#   不能代替官方 runner 的首跑验证。
# - 国内网络慢时可换 TUNA 镜像：脚本默认就用 TUNA 的 Debian 源（`ULL_APT_MIRROR` 可覆盖）。
# - 受检 bin 是 iced 版 `unlucky-launcher-iced`，与 `ci.yml` 的 runtime-smoke 逐步对应（M118：Slint 版
#   bin/包 `unlucky-launcher`/`unlucky-app` 已随 `crates/app` 退出 workspace，改了这里要两边一起改）。

set -euo pipefail

repo_root=$(cd "${1:-.}" && pwd)
out_dir=${2:-ull-smoke}
image=${ULL_SMOKE_IMAGE:-ull-linux-smoke:bookworm}
cargo_volume=${ULL_CARGO_VOLUME:-ull-cargo-reg}
target_volume=${ULL_CARGO_TARGET_VOLUME:-ull-linux-target}
apt_mirror=${ULL_APT_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn}
rustup_mirror=${ULL_RUSTUP_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/rustup}

mkdir -p "$out_dir"
out_dir=$(cd "$out_dir" && pwd)
# Git Bash 里 /tmp、/d/x 都是 MSYS 虚拟路径：直接喂给 `docker -v` 会被当成 Linux VM 内的路径，
# 结果「脚本说成功、宿主机什么都没有」。有 cygpath 就统一转成 Windows 混合路径（D:/x）并回显真实位置。
if command -v cygpath >/dev/null 2>&1; then
  repo_root=$(cygpath -m "$repo_root")
  out_dir=$(cygpath -m "$out_dir")
fi
echo "== 仓库: $repo_root"
echo "== 输出: $out_dir（截图 $out_dir/runtime-smoke.png）"

# 1) 构建镜像：rust 工具链 + xvfb + 图形栈编译依赖 + 字体（软渲染器要从系统字体库取字形）
if ! docker image inspect "$image" >/dev/null 2>&1 || [ "${ULL_SMOKE_REBUILD_IMAGE:-0}" = "1" ]; then
  context=$(mktemp -d)
  trap 'rm -rf "$context"' EXIT
  cat >"$context/Dockerfile" <<EOF
FROM rust:1-bookworm
# 工具链组件（rustfmt/clippy）在 rust-toolchain.toml 里是必备项，rustup 会去补齐；
# 默认 dist 服务器在国内会挂成「rustc 0.6% CPU 睡 12 分钟」的假死（M62 实测），所以走 TUNA。
ENV RUSTUP_DIST_SERVER=$rustup_mirror \\
    RUSTUP_UPDATE_ROOT=$rustup_mirror/rustup
RUN sed -i 's|http://deb.debian.org|$apt_mirror|g' /etc/apt/sources.list.d/debian.sources \\
 && apt-get update \\
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \\
      xvfb xauth python3 \\
      libxkbcommon-dev libxkbcommon-x11-0 libfontconfig1-dev \\
      libgl1-mesa-dev libegl1-mesa-dev libwayland-dev libxcb1-dev \\
      libx11-6 libxcursor1 libxrandr2 libxi6 \\
      fonts-dejavu-core fonts-liberation2 \\
 && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy && rustup toolchain list
WORKDIR /src
EOF
  docker build -t "$image" "$context"
fi

# 2) 跑容器：构建 → xvfb 截图 → 像素判定
# 注意缓存卷要挂到 `registry` 子目录：直接挂 /usr/local/cargo 会把镜像里的 `bin/`（cargo 本体）盖掉，
# 变成 `cargo: command not found`（M62 实测踩到）。
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$repo_root:/src" \
  -v "$cargo_volume:/usr/local/cargo/registry" \
  -v "$target_volume:/target" \
  -v "$out_dir:/out" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e ULL_SMOKE_SKIP_BUILD="${ULL_SMOKE_SKIP_BUILD:-0}" \
  -e CARGO_TERM_COLOR=never \
  "$image" bash -euo pipefail -c '
    rustc --version && cargo --version && cat /etc/debian_version
    if [ "$ULL_SMOKE_SKIP_BUILD" != "1" ]; then
      # 共享热缓存卷可能喂陈旧产物（踩坑记录 N7）：先清掉受检 crate 再编
      # 清不掉就算了（N7 兜底用）；CI 里**没有**对应两行，别误抄进 ci.yml —— 那边每次都是干净 runner
      cargo clean -p unlucky-app-iced || true
      cargo build --bin unlucky-launcher-iced --locked -j 6
    fi
    mkdir -p /out/data
    set +e
    xvfb-run -a --server-args="-screen 0 1280x1024x24" \
      /target/debug/unlucky-launcher-iced --software-renderer \
      --screenshot /out/runtime-smoke.png --data-dir /out/data
    code=$?
    set -e
    echo "exit=$code"
    python3 packaging/check_screenshot.py /out/runtime-smoke.png
    exit $code
  '

echo "== 截图与数据目录已留在 $out_dir"
