#!/usr/bin/env bash
# PE 版本资源读回门禁（M190 判据在 CI 上的入口；M264 从 release-build.yml 拆出来）。
#
# 为什么是脚本而不是内联 step：`.github/workflows/release-build.yml` 撞着 400 行硬门禁
# （`guards.yml` 的 file-size，用户硬约束，不许抬 LIMIT），而「读回两个产物 + 同源取版本号 +
# 失败逐条转注解」内联进去必然顶上线。拆法与 `packaging/linux-smoke-docker.sh` /
# `packaging/linux-check-docker.sh` 同一思路：workflow 只留一行调用。
#
# 判据一行没改，仍在 `packaging/check_pe_version.py`：
#   ①读的是**产物字节**（构建日志里打印过不算证据）；②版本号与 `Cargo.toml` 同源；
#   ③任何一条判据没过就非 0 退出（含「产物不存在」「找不到 python」——工具缺失等于门禁失效，不降级）。
# 本脚本只做「调用 + 把失败原因变成 `::error::` 注解」：v0.2.4 的 run 43031591576 两个 Windows
# 槽位红在这里时，注解 API 上只剩一句 `Process completed with exit code 1.`，只能靠猜。
#
# 用法：
#   bash packaging/ci-pe-version-gate.sh [build_dir]     # 默认 target/release
set -euo pipefail

BUILD_DIR="${1:-target/release}"

# 仓库根按脚本位置反推：workflow 在仓库根跑，但本地从别处调用时也不该依赖 cwd。
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PY=$(command -v python3 || command -v python || true)
if [ -z "$PY" ]; then
  echo "::error::找不到 python，PE 版本资源门禁无法运行（门禁不降级）"
  exit 1
fi

# 先把「产物到底是哪个文件、多大」落成日志：这一步红了，第一件要排除的就是「读的不是那个文件」。
# `|| echo` 是刻意的：让「文件不存在」的判据由检查器自己报（它的措辞比 ls 更适合当注解）。
ls -l "$BUILD_DIR/unlucky-launcher-iced.exe" "$BUILD_DIR/ull.exe" \
  || echo "::warning::ls 没列出产物（文件可能不存在），判据交给下面的检查器"

# 版本号与 `Cargo.toml` **同源**：`[workspace.package] version = "x.y.z"`。不在这里写死任何版本。
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
echo "expect-version=$version"

log=$(mktemp "${TMPDIR:-/tmp}/pe-version.XXXXXX.log" 2>/dev/null || true)
if [ -n "$log" ]; then
  trap 'rm -f "$log"' EXIT INT TERM
  echo "log: $log"
else
  # 连临时文件都建不出来时**不判红**：判据是产物的字节，不是本脚本的管道。原因看原始输出。
  echo "::warning::mktemp 失败，失败原因只能看下面的原始输出（判据不变）"
fi

status=0
"$PY" packaging/check_pe_version.py \
  "$BUILD_DIR/unlucky-launcher-iced.exe" "$BUILD_DIR/ull.exe" \
  --expect-version "$version" 2>&1 | tee "${log:-/dev/null}" || status=$?

if [ "$status" -ne 0 ]; then
  if [ -n "$log" ]; then
    # 上限 30 行：正常失败是「每个产物几条判据」，异常失败（traceback）也不会被截掉开头。
    line_no=0
    while IFS= read -r line; do
      line_no=$((line_no + 1))
      if [ "$line_no" -le 30 ]; then echo "::error::PE 版本资源读回：$line"; fi
    done < "$log"
  fi
  exit "$status"
fi

exit 0
