# Unlucky Launcher {{VERSION}}

Rust-native **single-file** Minecraft launcher: iced 0.14 + Material 3 Expressive UI, built-in AI agent
(chat / setting edits / crash diagnosis) and an MCP server other agent tools can attach to.
No WebView, no bundled runtime, no JRE download needed to start the launcher itself.

## Downloads

Asset names are `<name>_<version>_<platform>_<arch>`, underscore separated — the rule
`packaging/make_artifacts.py::asset_stem()` applies:

| Platform | Asset | Notes |
| --- | --- | --- |
| Windows 10/11 x64 | `unlucky-launcher_{{VERSION}}_windows_x64.exe` | single exe, double-click to run; this is also what you run on ARM64 Windows (x64 emulation) |

**Windows ARM64 is not part of this release.** CI still builds it on a native `windows-11-arm` runner for
evidence, but that slot's toolchain check has not gone green yet, so
`unlucky-launcher_{{VERSION}}_windows_arm64.exe` is deliberately kept out of the download list and out of
`SHA256SUMS.txt` instead of handing out a build nobody has verified. `windows-x64` is the only published
slot; ARM64 rejoins this table as soon as its build is green (one line in the workflow's `PUBLISH_SLOTS`).

**This release ships Windows only, and CI builds Windows only.** The Linux (x64 / arm64) and macOS
(Apple Silicon / Intel) slots are not built by the release workflow at all — they have not been verified
on real machines yet, so nothing unverified is handed out and no build minutes are spent on them. They
stay out of the download list and out of `SHA256SUMS.txt` until that verification happens; a manual
workflow run can build all six slots for that purpose, but the four extra slots are never published. The
support matrix below marks those slots accordingly.

The Windows asset is a bare `.exe`, so `LICENSE`, `NOTICE` and `README.md` are attached to this release
next to it: the license terms travel with every download. Slots that ship an archive (Linux / macOS, once
published) unpack to one top-level directory of the same name holding the binary plus those three
documents.

## Command line

Each published slot also ships the command line as its own small bare asset,
`ull_{{VERSION}}_<platform>_<arch>` (`.exe` on Windows):

```powershell
.\ull_{{VERSION}}_windows_x64.exe instances list   # servers and CI: no graphics stack needed
```

`ull` runs `instances`, `install`, `pack import`, `launch`, `accounts`, `sessions`, `logs`, `diag`,
`config`, `java` and `agent`, and links no graphics library at all. The GUI binary accepts the same set
before it ever opens a window, so a desktop machine does not need `ull`.

## Support matrix

Published = shipped as a download of **this** release, listed in `SHA256SUMS.txt`.

| Platform | Status in this release | Level | Runtime requirements |
| --- | --- | --- | --- |
| Windows 10/11 x64 | **published** | first class | nothing beyond the OS (UCRT is part of Win10+) |
| Windows 11 ARM64 | not published (build chasing, unverified) | first class (launcher artifact) | nothing extra; the launcher runs natively, the game itself runs through the OS x64 emulation layer with an x64 JDK |
| macOS 12+ Apple Silicon | not built (unverified) | first class (unverified) | nothing; unsigned build, needs the Gatekeeper workaround |
| macOS 12+ Intel | not built (unverified) | first class (unverified) | same as above |
| Linux x64, X11 | not built (unverified) | first class (unverified) | `libxkbcommon`, `fontconfig`, `libGL` (or software renderer), `libxcb` |
| Linux x64, Wayland | not built (unverified) | supported (unverified) | `wayland-client`, `libxkbcommon`, plus the X11 set for XWayland apps |
| Linux arm64, X11 | not built (unverified) | first class (unverified) | glibc 2.35+, `libxkbcommon`, `fontconfig`, `libGL`, `libxcb` |
| Linux arm64, Wayland | not built (unverified) | supported (unverified) | the above plus `wayland-client` |
| Windows 8.1/8/7 | not built | unsupported | the baseline is Windows 10 |
| Android — Termux + X11 | not built (no CI slot) | experimental, not promised | CLI only and only when built from source; GUI unverified |

`windows-x64` is the only published slot this release; Windows ARM64 is kept out until its build is green
(see Downloads above). Both arm64 rows are formal matrix slots, not the experimental Termux path, and
Linux arm64 is not built by default — someone has to run it on real hardware (via a manual all-slots run)
before it joins this list. The rows above and the matrix table in `packaging/README.md` §9 are the same
list — change one, change the other.

## Install

**Windows** — put the `.exe` anywhere and run it. `_windows_x64.exe` is the only Windows asset in this
release, and it is also what you run on ARM64 Windows (the OS runs it through its x64 emulation layer).
The first run creates `.ULL/` and `.minecraft/` next to the exe (portable is the default data mode); the
per-user `%APPDATA%\ULL` plus `%APPDATA%\.minecraft`, or a custom root, are selectable instead.

Linux / macOS are not part of this release; their asset names are already fixed by the same rule
(`unlucky-launcher_{{VERSION}}_linux_x64.tar.gz`, `_linux_arm64`, `_macos_arm64`, `_macos_x64`) so the
links will not change shape when those slots open.

## No GPU? Software renderer

The UI renders through `wgpu` (GPU) and falls back to the `tiny-skia` CPU rasteriser when no usable
adapter is present; `--software-renderer` (or its equivalent `--renderer=cpu`) forces that path.

## Verify the download

`SHA256SUMS.txt` lists exactly the assets attached to this release (Windows only), and `sha256sum -c`
checks the ones you actually downloaded:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing   # Linux / WSL
shasum -a 256 -c SHA256SUMS.txt                # macOS (sha256sum is not bundled)
Get-FileHash .\unlucky-launcher_{{VERSION}}_windows_x64.exe -Algorithm SHA256   # Windows PowerShell
```

```
{{SUMS}}
```

## Known limitations of this build

- Microsoft (MSA) sign-in needs an Azure public client id, and release binaries do not carry one: set
  `ULL_MSA_CLIENT_ID=<id>` before launching, or build with that variable set. Without it sign-in stops
  before any network call, and the GUI banner says so plainly: the title "Microsoft sign-in is not
  configured", a body naming `ULL_MSA_CLIENT_ID=<id>` (set it, or build with it, then restart the
  launcher — a running process does not pick up a new variable) and `Code: msa_not_configured`, with
  no Retry button. Every other sign-in failure keeps the generic banner plus its own `Code:` line,
  with the full reason only in the launcher log. `ull accounts add msa` prints the same fix on the
  command line. Offline profiles and authlib-injector servers need no id.
- Binaries are not code-signed / notarized yet: Windows SmartScreen and macOS Gatekeeper warn on first
  launch. The checksums above are the integrity reference for now.
- When the macOS slot ships it is the bare binary (single-file promise), not a `.app` bundle: no Dock
  icon or Finder double-click — launch it from a terminal.

## License

Apache-2.0, third-party notices in `NOTICE`. Not affiliated with Mojang or Microsoft; no game files ship.

## UI framework in this build

Every asset here is built from the iced UI (`crates/app-iced`, iced 0.14, MIT); no other UI framework is
linked into it.
