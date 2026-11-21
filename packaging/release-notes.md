# Unlucky Launcher {{VERSION}}

Rust-native **single-file** Minecraft launcher: iced 0.14 + Material 3 Expressive UI, built-in AI agent
(chat / setting edits / crash diagnosis) and an MCP server other agent tools can attach to.
No WebView, no bundled runtime, no JRE download needed to start the launcher itself.

## What's new in {{VERSION}}

- **Linux x64 is a published platform.** The release workflow now builds `linux-x64` on a native `ubuntu-22.04`
  runner by default (not only in manual all-slots runs) and publishes it: `unlucky-launcher_{{VERSION}}_linux_x64.tar.gz`
  plus the bare `ull_{{VERSION}}_linux_x64`. The artifact itself was started on a Linux machine under a real display
  server, rendered a frame and logged the expected theme/backend lines before this decision, and the same check is
  now a per-slot CI gate. Linux ARM64 is built too, but stays unpublished until an ARM64 machine runs it.
- **{{VERSION}} is the first release published as a stable release.** GitHub only points
  `releases/latest` at the newest **non-prerelease** release, so `0.2.2` — kept as a historical
  pre-release, untouched — never appeared there. A pushed tag now creates a stable release by default;
  a pre-release has to be asked for explicitly (`workflow_dispatch` with `prerelease=true`).
- **Self-update works end to end.** `ull update check` / `ull update apply` and the settings panel's
  Update group share one core (`crates/core/src/update/`): manifest, version and channel admission,
  length + SHA256 verification, and an atomic rename-based swap that rolls back on failure. Only the
  stable channel is published today.
- **Online content library (Modrinth, anonymous).** Search, project details and version lists come from
  Modrinth's v2 API with facets derived from the target instance, and a known hash completes a remote
  entry. Being offline is reported as "not connected", never as "no matching content".
- **Per-file decisions when importing a modpack.** The pre-import scan lists every conflicting file in
  pages of 40, and you choose keep-all, overwrite-all, or per item; files that do not conflict are
  never rewritten, and the report names every conflict afterwards.
- **Per-instance launch arguments.** JVM arguments, game arguments, memory, the Java executable and the
  graphics backend (`default` / `opengl` / `vulkan`) are set per instance, each field falling back on
  its own to the global setting and then the factory default. The CLI reads the same resolved values.

## Downloads

Asset names are `<name>_<version>_<platform>_<arch>`, underscore separated — the rule
`packaging/make_artifacts.py::asset_stem()` applies:

| Platform | Asset | Notes |
| --- | --- | --- |
| Windows 10/11 x64 | `unlucky-launcher_{{VERSION}}_windows_x64.exe` | single exe, double-click to run; this is also what you run on ARM64 Windows (x64 emulation) |
| Linux x64 | `unlucky-launcher_{{VERSION}}_linux_x64.tar.gz` | one archive, one top-level directory holding the binary plus `LICENSE` / `NOTICE` / `README.md`; needs glibc 2.35+ and the usual X11/Wayland + fontconfig stack |

**Windows ARM64 is not part of this release.** CI still builds it on a native `windows-11-arm` runner (Windows 11
Arm64 with Visual Studio 2026), but the build has not delivered an artifact yet, so
`unlucky-launcher_{{VERSION}}_windows_arm64.exe` is deliberately kept out of the download list and out of
`SHA256SUMS.txt` instead of handing out a build nobody has verified. Worth stating plainly: every ARM64 failure
so far happened in a **pre-build toolchain probe** that guessed whether the SDK's ARM64 import libraries were
present and aborted before `cargo build` ever ran — the compiler and linker have not been heard from yet. That
probe is now evidence-only (it prints what it sees and never decides the job), so the next CI run is the first
one that can actually answer the question. ARM64 rejoins this table as soon as a run produces its artifact (one
line in the workflow's `PUBLISH_SLOTS`, guarded on both sides by `packaging/check_publish_slots.py`).

**Linux ARM64 is built but not published, and it has no runtime evidence yet.** `linux-arm64` joined the default
build matrix in this release (native `ubuntu-22.04-arm`, same glibc 2.35 floor as x64), so the artifact exists as
build evidence, but nothing has ever run it on ARM64 hardware — not one frame. It stays out of the download list
and out of `SHA256SUMS.txt` until an ARM64 machine renders a frame and the theme/backend log lines come out as
expected; that run is the single unlock condition, and copying a build nobody executed is exactly what this list
is meant to prevent. The publish job prunes that artifact with an explicit "not published" warning rather than
dropping it quietly.

**macOS is not built by the release workflow at all.** The Apple Silicon and Intel slots never got past a manual
probe, so no build minutes are spent on them and nothing unverified is handed out. They stay out of the download
list and out of `SHA256SUMS.txt`; a manual workflow run can build all six slots for evidence, but the probe slots
are never published. The support matrix below marks those slots accordingly.

The Windows asset is a bare `.exe`, so `LICENSE`, `NOTICE` and `README.md` are attached to this release
next to it: the license terms travel with every download. The Linux archive unpacks to one top-level directory
of the same name holding the binary plus those three documents.

## Command line

Each published slot also ships the command line as its own small bare asset,
`ull_{{VERSION}}_<platform>_<arch>` (`.exe` on Windows):

```powershell
.\ull_{{VERSION}}_windows_x64.exe instances list   # servers and CI: no graphics stack needed
```

```bash
./ull_{{VERSION}}_linux_x64 instances list         # same binary shape on Linux, no display needed
```

`ull` runs `instances`, `install`, `pack import`, `launch`, `accounts`, `sessions`, `logs`, `diag`,
`config`, `java` and `agent`, and links no graphics library at all. The GUI binary accepts the same set
before it ever opens a window, so a desktop machine does not need `ull`.

## Updating

The launcher reads `releases/latest/download/manifest.json` on startup (switchable: `update.auto_check`)
and shows four distinct outcomes: up to date, an update is available, a newer version exists but ships no
artifact for this platform, or the check itself failed. `ull update check` prints the same verdict, and
`ull update apply` downloads, verifies SHA256 and swaps the binary atomically, rolling back if the swap
fails. Nothing installs without you asking for it.

## Support matrix

Published = shipped as a download of **this** release, listed in `SHA256SUMS.txt`.

| Platform | Status in this release | Level | Runtime requirements |
| --- | --- | --- | --- |
| Windows 10/11 x64 | **published** | first class | nothing beyond the OS (UCRT is part of Win10+) |
| Linux x64, X11 | **published** | first class | glibc 2.35+ (`libxkbcommon`, `fontconfig`, `libGL` or the software renderer, `libxcb`) |
| Linux x64, Wayland | **published** (same archive, Wayland path unverified) | supported | the same archive; `wayland-client` + `libxkbcommon`, plus the X11 set for XWayland apps |
| Windows 11 ARM64 | not published (build evidence only) | first class (launcher artifact) | nothing extra; the launcher runs natively, the game itself runs through the OS x64 emulation layer with an x64 JDK |
| Linux arm64 | built, not published (no runtime evidence) | first class (unverified) | glibc 2.35+ (native `ubuntu-22.04-arm`, same floor as x64), `libxkbcommon`, `fontconfig`, `libGL`, `libxcb` / `wayland-client` |
| macOS 12+ Apple Silicon | not built (unverified) | first class (unverified) | nothing; unsigned build, needs the Gatekeeper workaround |
| macOS 12+ Intel | not built (unverified) | first class (unverified) | same as above |
| Windows 8.1/8/7 | not built | unsupported | the baseline is Windows 10 |
| Android — Termux + X11 | not built (no CI slot) | experimental, not promised | CLI and GUI from source only; GUI verified on-device twice: CPU software rendering and self-built Mesa/turnip GPU acceleration (see `packaging/docs/termux-glibc-build.md` and `README.termux.md`) |

Two slots are published this release: `windows-x64` and `linux-x64`. Linux x64 is not a claim taken on faith — the
release artifact itself was started on a Linux machine, opened a real window under a real display server, rendered a
frame (PNG asserted non-blank) and printed the expected `[theme]` and `[backend] final=… result=started` lines, and
the release build now carries the same check as a per-slot CI gate (`packaging/docs/linux-evidence.md` has the raw
output). What that does **not** cover: dragging/resizing and font metrics / CJK shaping on a real desktop session.
Whole-window transparency and rounded corners have no code path on Linux at all (transparency needs a native window
handle the app does not get there; corners are delegated to the desktop compositor), so they are not "unverified" —
they are not applicable. Nothing at all is known about Linux ARM64: no ARM64 machine has run the binary yet, which is
why that row stays unpublished.

Both arm64 rows are formal matrix slots, not the experimental Termux path, and Linux arm64 is not published until
someone runs it on real hardware — a manual all-slots run builds it today, and one real frame on an ARM64 machine is
the unlock. The rows above and the matrix table in `packaging/docs/compliance-support-boundaries.md` §9 are the same
list — change one, change the other.

## Install

**Windows** — put the `.exe` anywhere and run it. `_windows_x64.exe` is the only Windows asset in this
release, and it is also what you run on ARM64 Windows (the OS runs it through its x64 emulation layer).
The first run creates `.ULL/` and `.minecraft/` next to the exe (portable is the default data mode); the
per-user `%APPDATA%\ULL` plus `%APPDATA%\.minecraft`, or a custom root, are selectable instead.

**Linux x64** — unpack the archive and run the binary inside it; nothing is installed system-wide:

```bash
tar -xzf unlucky-launcher_{{VERSION}}_linux_x64.tar.gz
cd unlucky-launcher_{{VERSION}}_linux_x64
./unlucky-launcher                 # X11 or Wayland; --software-renderer forces the CPU rasteriser
```

It needs glibc 2.35+ (the slot is built on Ubuntu 22.04, so that is the floor), `libxkbcommon`, `fontconfig`
and `libGL` — or `--software-renderer` if there is no usable GPU adapter. On Debian/Ubuntu:

```bash
sudo apt-get install libxkbcommon0 libfontconfig1 libgl1 libx11-6 libxcursor1 libxrandr2 libxi6
```

The bare `ull_{{VERSION}}_linux_x64` next to the archive runs on a machine with no display at all.

Linux arm64 and macOS are not part of this release; their asset names are already fixed by the same rule
(`unlucky-launcher_{{VERSION}}_linux_arm64.tar.gz`, `_macos_arm64`, `_macos_x64`) so the links will not change
shape when those slots open.

## No GPU? Software renderer

The UI renders through `wgpu` (GPU) and falls back to the `tiny-skia` CPU rasteriser when no usable
adapter is present; `--software-renderer` (or its equivalent `--renderer=cpu`) forces that path.

## Verify the download

`SHA256SUMS.txt` lists exactly the assets attached to this release (Windows x64 and Linux x64, GUI + `ull` each), and
`sha256sum -c` checks the ones you actually downloaded:

```bash
sha256sum -c SHA256SUMS.txt --ignore-missing   # Linux / WSL
shasum -a 256 -c SHA256SUMS.txt                # macOS (sha256sum is not bundled)
Get-FileHash .\unlucky-launcher_{{VERSION}}_windows_x64.exe -Algorithm SHA256   # Windows PowerShell
```

```
{{SUMS}}
```

## Known limitations of this build

- Microsoft (MSA) sign-in works out of the box: this build carries the Azure public client id it needs
  (a public client id, not a credential — it ships inside the binary), so there is nothing to configure
  before signing in. Setting `ULL_MSA_CLIENT_ID=<id>` before launching still overrides it; the variable
  is read at startup, so restart the launcher after changing it. A build with no id anywhere stops
  before any network call, and the GUI banner says so plainly: the title "Microsoft sign-in is not
  configured", a body naming `ULL_MSA_CLIENT_ID=<id>` (set it, or build with it, then restart) and
  `Code: msa_not_configured`, with no Retry button. Every other sign-in failure keeps the generic
  banner plus its own `Code:` line, with the full reason only in the launcher log. `ull accounts add
  msa` prints the same fix on the command line. Offline profiles and authlib-injector servers need no
  id.
- Binaries are not code-signed / notarized yet: Windows SmartScreen and macOS Gatekeeper warn on first
  launch. The checksums above are the integrity reference for now.
- **Linux x64 is verified, not "finished".** The published archive was built on Ubuntu 22.04 and started under a
  real display server (WSLg) where it opened a window, rendered a non-blank first frame and printed its theme and
  backend lines; CI repeats that with Xvfb on every release. What nobody has looked at yet on a real desktop
  distribution: dragging and resizing, font metrics and CJK shaping. Whole-window transparency and rounded corners
  are not "unverified" on Linux — they are not code paths there: transparency needs a native window handle the app
  does not get (`hwnd=none`), and corners are delegated to the desktop compositor (`corner=policy=system`).
- **On Linux the in-app update does not install itself.** The published Linux asset is a `.tar.gz`, and
  `ull update apply` only self-replaces the portable single-file `.exe` form — on Linux it refuses with an explicit
  message naming `portable-zip` instead of half-installing (see `crates/core/src/update/job.rs`). Updating on Linux
  means downloading the new archive and unpacking it over the old copy; the update check itself (version + channel)
  works normally.
- **Linux ARM64 has no runtime evidence at all.** It compiles, and that is the whole story; the artifact is not
  attached to this release.
- When the macOS slot ships it is the bare binary (single-file promise), not a `.app` bundle: no Dock
  icon or Finder double-click — launch it from a terminal.

## License

Apache-2.0, third-party notices in `NOTICE`. Not affiliated with Mojang or Microsoft; no game files ship.

## UI framework in this build

Every asset here is built from the iced UI (`crates/app-iced`, iced 0.14, MIT); no other UI framework is
linked into it.
