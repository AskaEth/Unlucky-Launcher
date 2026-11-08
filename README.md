# Unlucky Launcher (ULL)

**English** · [简体中文](README.zh-CN.md)

> **Official project home**: <https://github.com/AskaEth/Unlucky-Launcher> ｜ downloads, issues and
> release notes all live there: <https://github.com/AskaEth/Unlucky-Launcher/releases>
>
> **Version in this README: `0.2.3`** — `unlucky-launcher --version` prints the same number.

Unlucky Launcher is a free, open-source **Minecraft: Java Edition** launcher written in Rust
(Apache-2.0). It ships as **one native binary per platform** — no installer, no bundled runtime, no
WebView — and covers the launcher job from installing a version to reading a crash report: instances
and versions, mod loaders and modpacks, mods / resource packs / shaders, per-instance Java, Microsoft
/ third-party / offline accounts, and a crash diagnosis that runs locally on a YAML rule engine.
Nothing of yours goes through a service of ours: account tokens stay on the machine.

![The instances page: the local instance list on the left; on the right the selected instance's
version, loader, isolation mode and last-played time](docs/images/instances-en-light.png)

*The instances page — 1180×760, English UI, light theme. The instance names are test data, not
anybody's real setup. That `Launch failed · exit0 静默失败` line is also an honest sight of a known
gap: the reason text is still raw Chinese straight out of the launch layer (see
[Not done yet](#not-done-yet)).*

## What works today

### Instances, versions and launching

- **Instances** — a list-detail page: the local instance list on the left; on the right the selected
  instance's version, loader, isolation mode (an isolated game directory or the shared one),
  files-on-disk state and last-played time, with Play / Stop. Narrow windows switch panes instead of
  stacking them.
- **Where an instance comes from** — installing a version (with a name, loader and loader version)
  or importing a modpack. The instances page's own **New instance / Import / Duplicate / Open folder
  / Delete** buttons are **not wired yet**: they are placeholder actions that only write a line to
  stderr (`crates/app-iced/src/app/update/page_messages.rs`), and the instance wizard is the pending
  piece (`crates/app-iced/src/pages/mod.rs`, deviation #8).
- **Version management** — a three-column rail (version type / major version / search) over the
  Mojang version list: release, snapshot and ancient versions, plus loader selection and a loader
  version when you install. Cancelling an installation really stops it — the in-flight file is cut, the
  job ends at the next stage boundary, and no instance is registered
  (`crates/presenter/src/host/dispatch.rs:61-88`). The CLI can plan the same install without
  downloading (`ull install --dry-run`: per-role file counts, total size, and one source-chain sample
  per file category; the full file list is in `--json`).
- **Launching** — a 10-stage command-line assembly (`crates/core/src/launch/`), per-instance JVM and
  game arguments, a native-library pass, and a JSONL session record for every run. Process trees are
  killed as trees, not as one PID.
- **Per-instance launch arguments** — JVM arguments, game arguments, memory, the Java executable and
  the graphics backend (`default` / `opengl` / `vulkan`) live in the instance file under `launch.*`,
  and every field falls back on its own: instance → global setting → factory default, so an instance
  that only sets memory does not wipe the global JVM arguments. JVM arguments are *appended* (global
  first, instance last, so the JVM's last-one-wins rule makes the instance win). An out-of-range
  memory value counts as "not set" instead of producing `-Xmx0m`, an empty field means "follow", and
  the backend row only appears for game versions that have it. The CLI and the GUI read the same
  resolved values (`crates/core/src/game/launch_overrides.rs`).
- **Crash diagnosis is real and local** — `crates/core/src/diagnose` collects five kinds of signals
  (exit code, `latest.log`, `crash-reports`, `hs_err_pid` files, environment snapshot including the
  mod list), matches them against a YAML rule set, and shows the findings with fix actions on the
  logs page. No model, no network: it works offline and on a machine with no GPU.

### Mod loaders, packs and the content library

- **Loaders** — Fabric, Quilt, NeoForge and Forge. Forge/NeoForge versions that need the official
  installer `processors` really run them, with a managed JDK if the machine has none.
- **Modpacks** — import a CurseForge manifest, a Modrinth `.mrpack`, or a MultiMC/Prism instance;
  export as `.mrpack` or a CurseForge manifest. Cancelling an **import** is interface-level only: the
  panel closes and the result is discarded, but the import thread runs to the end, because `core`'s
  import request has no cancel token yet (`crates/presenter/src/host/addon/pack.rs:28-42`).
- **Import conflicts are decided per file** — before anything is written, the scan lists every
  conflicting file (what it is, why it conflicts, how big it is) in pages of 40, and three entries
  decide the batch: keep all, overwrite all, or choose per item — unpicked rows stay on "keep". Files
  that do not conflict are never rewritten, and the report still names every conflict afterwards.
- **Content library** — modpacks / mods / resource packs / shaders with keyword, source, version,
  loader and sort filters, paged results, and one-click *install into this instance*.
- **Online source: Modrinth, anonymously** — search goes through `/v2/search` with facets taken from
  the target instance (project type, game version, loader); project details and version lists follow,
  and a remote entry whose hash is known is completed through `/v2/version_file/<hash>`. No API key,
  and no POST: the download engine has no request-body field, so hashes are looked up one GET at a
  time. Being offline is its own state — "not connected" — never "no matching content".
- **What that page will refuse, and says so on the row** — a modpack cannot be installed *from* the
  library (packs go through the import pipeline above); CurseForge's *install* is disabled because
  its file endpoint is not wired; and an item whose game version or loader does not match the target
  instance is disabled with the mismatch spelled out. Nothing fails silently.
- **CurseForge search needs a key** — set `ULL_CF_API_KEY` before launching; without it that source
  is greyed out in the source filter. The key stays in the environment: it is not written to the
  settings file and not logged (`crates/presenter/src/host/addon.rs`).

### Java

A managed JDK per instance, with a four-vendor fallback chain ordered by region — Temurin first, then
Oracle before Microsoft inside mainland China (measured 28× faster there) and Microsoft before Oracle
elsewhere, Zulu last — overridable through the `java.vendorPriority` setting
(`crates/core/src/game/java/vendor.rs`). Mirrors are region-aware too. `java list` shows every JDK it
can actually see (including the ones the official launcher downloaded), `java install <major>` fetches
one. The game's JDK is the launcher's data, not its own dependency.

### Accounts

Three tracks on one page:

- **Microsoft** — OAuth 2.0 device code flow → Xbox Live → XSTS → `api.minecraftservices.com`.
- **Third-party community auth servers** (authlib-injector style).
- **Local offline profiles**, for isolated testing.

**Microsoft sign-in works out of the box in official builds.** Release binaries carry the Azure public
client id they need (a public client id, not a credential — it ships inside the binary), so there is
nothing to configure before signing in. Setting `ULL_MSA_CLIENT_ID=<id>` still overrides it: the
variable is read at startup, so restart the launcher after changing it. Building from source bakes in
whatever that variable holds at build time (`ULL_MSA_CLIENT_ID=<id> cargo build`). A build with no id
anywhere stops before any network call, and its alert says exactly what is missing: the title
"Microsoft sign-in is not configured", a body naming `ULL_MSA_CLIENT_ID=<id>` (set it, or build with
it, then restart), and `Code: msa_not_configured`. It offers no Retry button, because retrying in that
same process cannot help. Every other sign-in failure keeps the older shape: one category line plus
its own `Code:` line, with the full reason only in the launcher log. Community auth servers and local
profiles need no id.

### Look and language

- **One UI stack** — iced 0.14 + Material 3 Expressive, a self-drawn title bar, an animated
  navigation rail, light / dark / follow-system.
- **Appearance** — window background image with a colour pipeline (panel colours can be derived from
  the wallpaper), per-panel transparency, and whole-window transparency on Windows. On Windows 11
  and Server 2025 the system is asked for rounded corners; macOS and Linux leave corners to the
  desktop's own rules; nothing is drawn by hand.
- **CPU fallback** — `--software-renderer` keeps the interface rendering on machines with no usable
  GPU or broken drivers, and the backend probe falls back on its own when no adapter is usable. On
  that path panel-level translucency and blur are switched off (window-level transparency still
  works) and the settings panel says so.
- **Localisation** — system-language detection, an explicit override in settings, and persistence
  across restarts. Most copy comes from one message catalogue (`crates/i18n`).
  **Boundary, stated because it is easy to over-read that sentence**: the anti-hard-coding guard
  (`crates/app-iced/src/pages/instances/tests/i18n.rs`) scans the instances page sources only, not
  every screen; a hard-coded *Chinese* literal has no ASCII word for it to match, so nothing catches
  that class; and the appearance panel's older controls are still hard-coded English
  (`crates/app-iced/src/pages/appearance/section.rs`), so a Chinese UI still shows them in English.

### The assistant, and the MCP server

These are two different things and the README should not blur them:

- **The in-app assistant is a placeholder shell today.** Its transport is scripted
  (`crates/app-iced/src/presenter/transport.rs`), no real model is connected, and the UI says so in
  four places on every screen it appears — subtitle, input placeholder, empty-state body, and a
  "Simulated" chip. The session state, unread count, draft, confirm flow and named failure codes are
  real presenter state; only the model behind them is not there yet.
- **The MCP server is real.** `ull agent serve` exposes the launcher core to external agent tools as
  an MCP server over stdio or Streamable HTTP, loopback only (`127.0.0.1`, default port 47823),
  read-only unless `agent.mcp.allow_write=true`. The same ten tool specs (`crates/agent/src/tools/`)
  feed both shapes, and both go through the same executor — schema validation, sensitivity tiers,
  audit trail. `ull agent config` prints ready-made Claude Code / Cursor snippets.

### Command line

The GUI binary also answers every CLI subcommand **before any UI code runs** — no window, no renderer
probe, no font scan. A second, lighter `ull` artifact serves the same command set from a binary that
links no graphics stack at all.

```bash
unlucky-launcher                            # GUI
unlucky-launcher --software-renderer        # GUI, CPU renderer
unlucky-launcher instances list             # CLI, no window
unlucky-launcher --json launch 1.20.1       # CLI, global options before the command
unlucky-launcher --version                  # CLI, version + OS summary
```

The dispatch rule is deliberately narrow, so nothing that used to work changed meaning:

- The first argument is a bare word (not `-…`) → it sits in the subcommand position, so the CLI gets
  it. GUI flags never appear there: `--screenshot` / `--diff` / `--data-dir` take their values
  *after* the flag.
- The first argument is a flag → the line is a CLI call only if `crates/cli`'s parser also produces a
  real command (`--help`, `--version`, `--json instances list`). `--data-dir X` alone parses to "no
  subcommand", so it still opens the GUI; every GUI-only flag (`--software-renderer`,
  `--screenshot*`, `--renderer`, `--theme`, `--diff`, `--force-state`, …) fails CLI parsing, so it
  still opens the GUI.

**Unknown subcommands are an error, not a GUI launch**: `unlucky-launcher lst` prints "未知命令
`lst`", hints `unlucky-launcher --help`, and exits 1. Silently opening a window would tell a script
that its typo took effect. The subcommand table lives in exactly one place
(`crates/cli/src/cli/parse.rs`), so both binaries accept the same set by construction. `ull --help`
lists them all: `instances`, `install`, `launch`, `pack import`, `accounts`, `sessions`, `logs`,
`diag`, `config`, `java`, `update`, `agent`, `version`, `help`.

### Updates, and the stable channel

- **Self-update is real, and GUI and CLI share one core.** The settings panel's Update group and
  `ull update check` / `ull update apply` both build the same `core::update::Job`
  (`crates/core/src/update/`): fetch the manifest at `releases/latest/download/manifest.json`, compare
  versions, admit the channel, verify length + SHA256, then swap the binary atomically — rename the
  old file aside, move the new one in, roll back if that fails. A running Windows exe can be
  *renamed* but not overwritten, which is why the swap is a rename.
- **Four outcomes, never conflated**: up to date / update available / a newer version exists but ships
  no artifact for this platform / the check itself failed (offline, broken manifest, channel
  mismatch). A failed check is never reported as "up to date".
- **Only the stable channel is published.** `update.channel` defaults to `stable`; beta / alpha /
  nightly are modelled and reserved. A manifest is admitted only when both its declared channel and
  the version's own pre-release stage agree — a manifest can lie, a version number cannot.
- **Why a release has to be marked stable**: `releases/latest/download/*` only points at the newest
  **non-prerelease** release, and the manifest address is pinned there. `0.2.3` is the first release
  created without the pre-release flag (`0.2.2` stays a historical pre-release). Which trigger creates
  which marker — push tag ⇒ stable, dispatch `prerelease=true` ⇒ pre-release — is one table
  (`packaging/release_marker.py`), with a guard over it (`packaging/check_release_semantics.py`).
- **Nothing installs itself behind your back.** The check at startup can be turned off
  (`update.auto_check`, the same key the CLI reads), and installing is a button or an explicit
  `ull update apply`. Debug builds can rehearse the whole chain against a local directory source
  (`ULL_UPDATE_DIR`); release builds do not contain that path at all.

### Not done yet

This list is the reason to believe the rest. None of it is hidden behind marketing copy.

- **No installer.** You download a file and run it — there is no setup step, and updates replace that
  file in place instead of installing anything (see [Updates](#updates-and-the-stable-channel)).
- **The update path has only been exercised against a local directory source.** The whole chain
  (manifest → download → SHA256 → rename/rollback) ran with a debug build and a fixture, because the
  real `releases/latest/download/manifest.json` was unreadable while `0.2.2` was still a pre-release.
  The first observation on the live stable channel happens after `0.2.3` ships.
- **Several buttons are placeholders.** On the instances page: *New instance*, *Import*, *Duplicate*,
  *Open folder*, *Delete*. On the logs page: *Open folder*. In the settings frame: *Save*, *Reset
  section*, *Add override*. Each writes `[page] 占位动作 …` to stderr and changes nothing — instance
  management from the GUI is the instance-wizard work still outstanding, and settings change as you
  edit them rather than on a Save.
- **Linux and macOS artifacts are unverified, and this release does not even build them.** The
  reference machine is Windows; the `linux-x64`, `linux-arm64`, `macos-arm64` and `macos-x64` slots
  sit in the CI matrix but the default release run skips them, and they have never been downloaded and
  run end to end. `gh workflow run release.yml -f all_slots=true` builds them for evidence. Treat them
  as untested output, not as "supported because the matrix lists them".
- **macOS is not finalised.** It has two rounds of real-device evidence (Apple Silicon), which is why
  the build exists — but the menu-bar name, the placement of the three window buttons (macOS puts
  them top-left; ours are drawn top-right), and 300s+ steady state still need a real machine. macOS
  builds are unsigned, so the first launch needs the Gatekeeper quarantine attribute cleared
  (`xattr -d com.apple.quarantine`). The macOS runtime smoke job is a manual `workflow_dispatch`
  probe with `continue-on-error`, not a gate.
- **The content library is cramped at low window heights**, and the window height is not wired into
  the page model yet — the page lays itself out for a nominal height rather than the one it gets.
- **Launch-side failure text is not structured.** The tile takes a Chinese sentence straight from the
  launch layer (`crates/core/src/launch/exit_analysis.rs`) instead of a reason code the UI could
  translate — which is exactly the `静默失败` you can see in the English screenshot above.
- **The in-app assistant has no model behind it** (see above). The MCP server does.
- **Install progress is stage-level**, not byte-level, on the content-library install path.
- **No native file chooser.** Import takes drag-and-drop or a pasted path.
- **Android/Termux is experimental and not promised**: no released asset targets it. A GUI build is
  possible from source via the glibc toolchain (`packaging/docs/termux-glibc-build.md`, branch notes:
  [`README.termux.md`](README.termux.md)); it was verified on-device twice: once on CPU software rendering, once with GPU acceleration via a self-built Mesa/turnip stack (see the evidence in README.termux.md).

## Download and run

Grab the file for your platform from
[Releases](https://github.com/AskaEth/Unlucky-Launcher/releases). Names follow
`unlucky-launcher_<version>_<platform>_<arch>`; the single source of truth for that shape is
`packaging/make_artifacts.py::asset_stem()`:

| Slot | Asset | Shape |
|------|-------|-------|
| `windows-x64` | `unlucky-launcher_<version>_windows_x64.exe` | bare `.exe`, double-click to run |
| `windows-arm64` | `unlucky-launcher_<version>_windows_arm64.exe` | bare `.exe` — **not yet published**, see below |
| `linux-x64` | `unlucky-launcher_<version>_linux_x64.tar.gz` | one archive, one top-level directory — **not yet published** |
| `linux-arm64` | `unlucky-launcher_<version>_linux_arm64.tar.gz` | same — **not yet published** |
| `macos-arm64` | `unlucky-launcher_<version>_macos_arm64.tar.gz` | same — **not yet published** |
| `macos-x64` | `unlucky-launcher_<version>_macos_x64.tar.gz` | same — **not yet published** |
| every published slot | `ull_<version>_<platform>_<arch>[.exe]` | bare CLI binary, its own asset |
| all | `SHA256SUMS.txt` | `sha256sum -c` eats it directly |

**This release publishes `windows-x64` and nothing else.** The publish job keeps only the slots named
in `PUBLISH_SLOTS` (`.github/workflows/release-publish.yml`) and deletes every other artifact before
it writes `SHA256SUMS.txt`, so a row marked **not yet published** is a naming rule, not a downloadable
asset. Of those rows `windows-arm64` is the one CI still builds — natively, on GitHub's
`windows-11-arm` runner, for evidence — but that slot's toolchain check has not gone green yet, so its
`.exe` stays out of the release. The row stays because the architecture is still supported; it rejoins
the download list once the build is green.

Each UNIX archive contains the executable plus `LICENSE`, `NOTICE` and `README.md`, in one top-level
directory. Windows ships a bare `.exe` (a single file cannot carry the texts), so those three
documents are attached to the release next to it — downloading the Windows binary does not strip the
licence. Download links never carry implementation detail in the name, and the binary inside an
archive is always called `unlucky-launcher`.

The two shapes are not duplicates. The GUI binary needs the graphics stack when it opens a window (it
`dlopen`s X11 / Wayland / GL and links `libfontconfig` at load); `ull` needs nothing beyond glibc, so
it runs on a server or a CI container where no window can exist. On Linux that floor is glibc ≥ 2.35
(the slots build on Ubuntu 22.04) — an older distribution needs a source build
(`packaging/README.md` §4). Pick `ull` when there is no desktop, `unlucky-launcher <command>` when
there is. Both print the same CLI text and each names itself after the file you ran — `argv[0]` with
its directory and a trailing `.exe` removed, nothing else (`crates/cli/src/cli/program.rs`), so the
`ull` asset prints `ull ·`, the binary inside an archive prints `unlucky-launcher ·`, and a local
`cargo build` prints `unlucky-launcher-iced ·`.

Minimum platform baseline: **Windows 10**; Windows 8.1 / 8 / 7 are not supported. macOS 12+ on Apple
Silicon and Intel, Linux x64 / arm64 (`crates/core/src/platform/os_info.rs` holds the baseline
check). On Windows 10/11 the builds use the standard toolchain (MSVC / GNU), built the same way as
upstream cargo. On Linux, X11 is the preferred path today and Wayland is the long-term direction.
Capability differences are feature gates with runtime probing, never a compile-time "this platform
gets less".

First run with a Microsoft account? Nothing to configure — official builds carry the client id; see
[Accounts](#accounts). Nothing else needs configuring.

## Build from source

Prerequisites:

- Rust stable (`rust-toolchain.toml` selects the toolchain automatically; the gates also need the
  `rustfmt` and `clippy` components)
- Windows: MSVC toolchain (what CI uses) or MinGW-w64 (`x86_64-pc-windows-gnu`)
- Linux: `libxkbcommon-dev libfontconfig1-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev
  libxcb1-dev` — winit + wgpu/tiny-skia ask for these at build time

```bash
cargo fmt --all --check                                # formatting
cargo clippy --workspace --all-targets -- -D warnings  # lints (zero-warning gate)
cargo test --workspace                                 # tests
cargo build --release                                  # binaries land in target/release/
cargo deny check licenses bans sources                 # dependency licences (required in CI)
```

What the build produces:

- A build from source carries no Microsoft client id by default. Set `ULL_MSA_CLIENT_ID` at build time
  to bake one in (`ULL_MSA_CLIENT_ID=<id> cargo build`); official release binaries already carry one
  — see [Accounts](#accounts).
- `unlucky-launcher-iced` — the GUI, and the CLI in the same file: it answers every subcommand listed
  above. `iced` is cargo's bin name, not a download name — `packaging/make_artifacts.py` stages that
  same build as `unlucky-launcher_<version>_<platform>_<arch>` and names the binary
  `unlucky-launcher` inside the archive.
- `ull` — the CLI as a standalone, UI-free binary. Same command set, no graphics stack, smaller, and
  the right artifact for a machine that has no display at all.

Rendering switches:

- `unlucky-launcher-iced --software-renderer` forces tiny-skia (`ICED_BACKEND=tiny-skia`);
  `--screenshot <png>` writes one frame and exits — that is the acceptance channel CI's headless
  smoke job uses
- With no usable adapter the iced build's explicit probe falls back to the CPU path on its own

**CI gates.** Every push to `main` and every pull request runs fmt / clippy (warnings are errors) /
the full test suite, and the Linux headless render smoke is a hard gate on the same footing: it
renders one real frame under Xvfb, then asserts the PNG is not blank, and a red result blocks the
merge. The macOS probe is not there yet — manual dispatch only, result recorded but not enforced.

| Workflow | Content |
|----------|---------|
| `.github/workflows/ci.yml` | fmt + clippy + test + release build (Windows / Linux matrix); Linux runtime smoke (Xvfb, one real frame, PNG asserted non-blank); macOS runtime probe (manual dispatch only) |
| `.github/workflows/guards.yml` | zero-UI guards (`crates/core`, `crates/presenter`, `crates/design`), 400-lines-per-file guard, cargo-deny licence and source guard |
| `.github/workflows/release.yml` | tag `v*` → build both Windows slots (`windows-x64` + `windows-arm64`) on native runners → single-file artifacts + `SHA256SUMS.txt` → GitHub Release (publishes `windows-x64` only; `-f all_slots=true` builds all six slots for evidence) |
| `.github/workflows/release-guard.yml` | release-marker semantics: push tag ⇒ a **stable** release, dispatch `prerelease=true` ⇒ pre-release; the table lives in `packaging/release_marker.py` and the guard mutation-tests itself |

## Architecture and module map

```
Unlucky-Launcher (cargo workspace)
├── crates/core       # domain library: zero UI dependencies, eight modules
├── crates/agent      # AI agent: ToolSpec registry + built-in client + MCP server (one core, two shapes)
├── crates/presenter  # business host layer: framework-free state / host / dto / conversion
├── crates/design     # design tokens: real HCT color / type / shape / spacing / elevation / motion / icons
│                     #   / component metrics — zero UI dependencies, no third-party crates
├── crates/i18n       # language detection, message catalogue, CJK font policy
├── crates/palette-extract  # the only crate that decodes images, for the wallpaper colour pipeline
├── crates/app-iced   # the iced UI (bin `unlucky-launcher-iced`): the only GUI the workspace builds
├── crates/cli        # CLI (bin `ull`): headless, zero UI
└── packaging         # single-file artifact matrix and release scripts
```

Layering: `app-iced` → `presenter` → `core` / `agent`. `design` and `i18n` are consumed by the shell
and the presenter; none of them may depend on a rendering framework, and CI guards enforce it.

The eight modules of `crates/core`:

| Module | Responsibility |
|--------|----------------|
| `setting` | three data-directory modes, schema-based config, atomic writes, migrations |
| `download` | multi-source health scoring, mirrors, chunked downloads, hash-while-downloading |
| `game` | version / instance repository, snapshots and inheritance resolution |
| `launch` | 10-stage launch pipeline, processes, JSONL event stream |
| `auth` | Microsoft sign-in (OAuth 2.0 device code → Xbox Live → XSTS → Minecraft Services), third-party community servers, local profiles |
| `addon` | mod loader installation, modpack import, CurseForge / Modrinth, pack export |
| `diagnose` | five signal collectors, YAML rule engine, hs_err parsing |
| `platform` | OS abstraction: paths, processes, file locks, capability probing, runtime OS detection |

## License and acknowledgements

- **This project**: Apache License 2.0, see [LICENSE](LICENSE); third-party notices in
  [NOTICE](NOTICE).
- **iced** (<https://github.com/iced-rs/iced>), MIT: the framework of the UI, together with `winit`
  (Apache-2.0) and, through it, `wgpu` (MIT / Apache-2.0) / `tiny-skia` (BSD-3-Clause).
- **Google Material Color Utilities** (Apache-2.0): the color tokens in `crates/design` are a Rust
  port of its HCT / ColorSpec2021 algorithms; details and the exact ported functions are listed in
  [NOTICE](NOTICE).
- **Other major dependencies**: `rmcp` and `axum` (Apache-2.0 / MIT) for the MCP server, `tokio`
  (MIT), `reqwest` with rustls (MIT / Apache-2.0), `serde` / `serde_json` (MIT / Apache-2.0),
  `zip` / `flate2` / `tar` (MIT / Apache-2.0). The allow-list cargo-deny enforces is in `deny.toml`;
  GPL-family dependencies may not enter the main line.

Thanks to [HMCL](https://github.com/HMCL-dev/HMCL) and [PCL2](https://github.com/Meloong-Git/PCL)
(both GPL-3.0). They have been with the vast majority of Minecraft players for years, and this
project has benefited a great deal from their design choices and implementation details.
