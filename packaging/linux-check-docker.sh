#!/usr/bin/env bash
# 本地复跑 `.github/workflows/ci.yml` 的两条 **ubuntu 门禁**：`clippy --workspace --all-targets
# -- -D warnings` 与 `test --workspace --all-targets`。
#
# 为什么要有这条脚本（与 `linux-smoke-docker.sh` 同一动机，但受检对象不同）：
# - smoke 脚本复刻的是 `runtime-smoke` job（能不能真跑起来）；本脚本复刻的是**编译期门禁**
#   （能不能编过 / 测过）。M144 的三条红里有两条（clippy ubuntu、test ubuntu）属于后者，
#   而它们只在「Linux + `--all-targets` + `-D warnings`」三件事凑齐时才炸——
#   在 Windows 本机永远复现不了（Windows 上那些 `#[cfg(target_os = "windows")]` 分支都有使用者）。
#   所以这条脚本是「本机提前挡住那两条红」的唯一通道。
#
# 用法：
#   bash packaging/linux-check-docker.sh [仓库根目录]
#   ULL_CHECK_STAGES=clippy bash packaging/linux-check-docker.sh       # 只跑 clippy
#   ULL_CHECK_TEST_ARGS='-p unlucky-app-iced --lib' \
#     ULL_CHECK_STAGES=test bash packaging/linux-check-docker.sh       # 只跑一个 crate 的单测
#
# 前置：基础镜像 `ull-linux-smoke:bookworm`（由 `packaging/linux-smoke-docker.sh` 建）。
#       本脚本在它之上只加一层 `cmake` + `ninja-build`：`aws-lc-sys`（rustls 的汇编后端）
#       缺 cmake 编不过，M144 实测过。
#
# 诚实边界（别把它当 runner 的等价物）：
# - 基础镜像是 **Debian 12（glibc 2.36）**，官方 runner 是 **Ubuntu 24.04（glibc 2.39）**；
#   两者 rustc 走同一份 `rust-toolchain.toml`（stable），所以**编译期 lint 与单测判定**可对齐，
#   但发行版差异面（系统库版本、字体）不受本脚本保护。
# - 本脚本只跑 ci.yml 里那两条命令，不跑 fmt / build / runtime-smoke。
# - 冷编很贵（全 workspace + `--all-targets`，M144 实测 15–25 分钟）。缓存卷 `ull-linux-check-target`
#   第二次起是热的；**别**把宿主机的 target 目录挂进来（路径与指纹都对不上，等于骗自己）。

set -euo pipefail

repo_root=$(cd "${1:-.}" && pwd)
base_image=${ULL_SMOKE_IMAGE:-ull-linux-smoke:bookworm}
image=${ULL_CHECK_IMAGE:-ull-linux-check:bookworm}
cargo_volume=${ULL_CARGO_VOLUME:-ull-cargo-reg}
target_volume=${ULL_CHECK_TARGET_VOLUME:-ull-linux-check-target}
apt_mirror=${ULL_APT_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn}
stages=${ULL_CHECK_STAGES:-clippy test}
test_args=${ULL_CHECK_TEST_ARGS:---workspace --all-targets --no-fail-fast}
fail_fast=${ULL_CHECK_FAIL_FAST:-0}

# Git Bash 里路径是 MSYS 虚拟路径：直接喂给 `docker -v` 会被当成容器内路径（M62 踩过）。
if command -v cygpath >/dev/null 2>&1; then
  repo_root=$(cygpath -m "$repo_root")
fi
echo "== 仓库: $repo_root"
echo "== 阶段: $stages（test 参数: $test_args）"

if ! docker image inspect "$base_image" >/dev/null 2>&1; then
  echo "!! 缺基础镜像 $base_image —— 先跑一次：" >&2
  echo "   bash packaging/linux-smoke-docker.sh . D:/tmp/ull-smoke" >&2
  echo "   （它建的镜像里已经带了 ci.yml 那批图形栈编译依赖；本脚本只在其上加 cmake/ninja）" >&2
  exit 2
fi

# 一层派生镜像：cmake + ninja-build。只在缺镜像或显式 ULL_CHECK_REBUILD_IMAGE=1 时重建。
if ! docker image inspect "$image" >/dev/null 2>&1 || [ "${ULL_CHECK_REBUILD_IMAGE:-0}" = "1" ]; then
  context=$(mktemp -d)
  trap 'rm -rf "$context"' EXIT
  cat >"$context/Dockerfile" <<EOF
FROM $base_image
# 基础镜像已把 apt 源改到 $apt_mirror（见 linux-smoke-docker.sh）。
# aws-lc-sys（rustls 的汇编后端）构建时要 cmake + ninja，缺了就是在 C 依赖里报错退出，
# 与 Rust 侧代码无关——别被那种红骗去改源码（M144 实测）。
RUN apt-get update \\
 && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \\
      cmake ninja-build \\
 && rm -rf /var/lib/apt/lists/*
WORKDIR /src
EOF
  docker build -t "$image" "$context"
fi

# 缓存卷挂到 `registry` 子目录：直接挂 /usr/local/cargo 会盖掉镜像里的 cargo 本体（M62 踩过）。
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$repo_root:/src" \
  -v "$cargo_volume:/usr/local/cargo/registry" \
  -v "$target_volume:/target" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_TERM_COLOR=never \
  -e ULL_CHECK_STAGES="$stages" \
  -e ULL_CHECK_TEST_ARGS="$test_args" \
  -e ULL_CHECK_FAIL_FAST="$fail_fast" \
  "$image" bash -euo pipefail -c '
    rustc --version && cargo --version && cat /etc/debian_version
    cmake --version | head -1 && ninja --version

    run_clippy() { cargo clippy --workspace --all-targets -- -D warnings; }
    # 故意不加引号：ULL_CHECK_TEST_ARGS 是一串参数（默认照抄 ci.yml 的那条命令）。
    run_test()   { cargo test $ULL_CHECK_TEST_ARGS; }

    overall=0
    for stage in $ULL_CHECK_STAGES; do
      case "$stage" in
        clippy) label="cargo clippy --workspace --all-targets -- -D warnings"; run=run_clippy ;;
        test)   label="cargo test $ULL_CHECK_TEST_ARGS";                     run=run_test ;;
        *) echo "!! 未知阶段: $stage（可选 clippy / test）" >&2; exit 2 ;;
      esac
      echo "== 开始: $label"
      set +e
      "$run"
      code=$?
      set -e
      # 退出码单独成行：一眼能看出是「编译期 lint」还是「单测」，也能与 CI 注解里的码对照。
      echo "== 结束: $label → exit=$code"
      if [ "$code" -ne 0 ]; then
        overall=1
        if [ "$ULL_CHECK_FAIL_FAST" = "1" ]; then exit "$code"; fi
      fi
    done
    echo "== 汇总: $ULL_CHECK_STAGES → exit=$overall"
    exit "$overall"
  '

echo "== 两条 ubuntu 门禁已跑完（上表 exit=0 才算绿）"
