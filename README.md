# Unlucky Launcher (ULL)

**English** · [简体中文](README.zh-CN.md)

Unlucky Launcher (ULL) is a free, open-source Minecraft: Java Edition launcher written in Rust
(Apache-2.0, non-commercial), with a Slint + Metro UI. It embeds an AI agent client (chat /
configuration / crash diagnosis) and integrates with the broader agent tooling ecosystem via MCP.
Minimum baseline: Windows 10.

- Runtime: a single native Rust binary — no JVM, no WebView, no external runtime
- UI: Slint (Royalty-Free 2.0; GL preferred, CPU software-rendering fallback so older machines
  without a usable GPU still get the full UI)
- License: Apache-2.0 © AskaEth (YeiJ)

## Compliance statement

Authentication uses only official Microsoft/Mojang endpoints — Microsoft OAuth 2.0 device code
flow, Xbox Live, XSTS, and api.minecraftservices.com — so players sign in with their own Microsoft
account and launch the game they already own. Account tokens are stored only on the user's local
machine and are never sent to any server operated by the project. ULL does not distribute, mirror,
proxy or modify any Minecraft game files. Online play always requires a Microsoft account with
valid entitlements. Local offline profiles never contact Microsoft or Mojang services and cannot
join online-mode servers. Not affiliated with Mojang or Microsoft.

## Status

The core engine is complete (workspace: 983+ tests passing):

- **Microsoft sign-in**: OAuth 2.0 device code flow → Xbox Live → XSTS → Minecraft Services, with
  entitlement verification
- **Account credentials are stored only on the local machine** — never uploaded, and inaccessible
  to the AI features
- Mod loaders: Forge / NeoForge / Fabric / Quilt, verified on real machines
- Java management: Temurin / Oracle / Microsoft / Zulu with a regional fallback chain
- Multi-source download engine; YAML rule-based crash diagnosis
- Usable as an MCP server for external agent tooling (`ull agent serve`)
- Local offline profiles: for local / isolated-network testing only; they perform no Microsoft
  authentication and cannot join servers with online-mode enabled

**CLI (`ull`) is fully usable**: `install / launch / accounts / java / diagnose / agent`.

**GUI in development**: `crates/app` is currently a Slint + Metro skeleton; the theme system and
the full interface ship with subsequent versions — use the CLI until then.

## Module map

```
Unlucky-Launcher (cargo workspace)
├── crates/core     # domain library, zero UI dependencies, eight modules
├── crates/agent    # AI agent: ToolSpec registry + built-in client + MCP server (one shared core)
├── crates/app      # Slint UI: the only crate allowed to depend on a UI framework
├── crates/cli      # CLI (bin `ull`): headless, zero UI, depends on core + agent
└── packaging       # single-file artifact matrix and packaging assets (planned)
```

The eight modules of `crates/core`:

| Module | Responsibility |
|--------|----------------|
| `setting` | three data-directory modes, schema-based config, atomic writes, migrations |
| `download` | multi-source health scoring, mirrors, chunked downloads, hash-while-downloading |
| `game` | version / instance repository, snapshots and inheritance resolution |
| `launch` | 10-stage launch pipeline, processes, JSONL event stream |
| `auth` | Microsoft sign-in / local offline profiles |
| `addon` | mod loader installation, modpacks, CurseForge / Modrinth |
| `diagnose` | five signal collectors, YAML rule engine, hs_err parsing |
| `platform` | OS abstraction: paths, processes, file locks, capability probing, runtime OS detection |

## Building

Prerequisites:

- Rust stable (`rust-toolchain.toml` selects the toolchain automatically; components `rustfmt` /
  `clippy`)
- Windows: MSVC toolchain (used by CI) or MinGW-w64 (`x86_64-pc-windows-gnu`)
- Linux: `libxkbcommon-dev libfontconfig1-dev libgl1-mesa-dev libegl1-mesa-dev libwayland-dev libxcb1-dev`
  (Slint winit backend + femtovg build dependencies)

```bash
cargo fmt --all --check                               # formatting
cargo clippy --workspace --all-targets -- -D warnings # lints (zero-warning gate)
cargo test --workspace                                # tests
cargo build --release                                 # artifact: target/release/unlucky-launcher[.exe]
```

Software-rendering switch (machines without a usable GPU / with broken drivers):
`unlucky-launcher --software-renderer` (equivalent to `SLINT_BACKEND=winit-software`).

Dependency license check (optional locally, required in CI):
`cargo deny check licenses bans sources`.

## CI gates

| Workflow | Content |
|----------|---------|
| `.github/workflows/ci.yml` | fmt + clippy + test + build, Windows / Linux matrix |
| `.github/workflows/guards.yml` | zero-UI guard for core, 400-line-per-file guard, cargo-deny license guard |

## Platform baseline

Minimum baseline: **Windows 10**. Windows 8.1 / 8 / 7 are not supported:

- Windows 10/11 builds use the **standard toolchain** (MSVC / GNU) — no custom target specs, no
  `-Zbuild-std`, no import-table allowlists
- Other platforms: macOS 12+ first class; Linux x64 with X11 preferred, Wayland as the long-term
  direction; Android Termux experimental, not committed
- UI fallback: Slint software rendering (fully usable on machines without a GPU or with broken
  drivers)
- Platform independence is expressed through **separate test matrices**, not separate artifact
  pipelines; capability differences use feature gates with runtime probing

## License and acknowledgements

- This project: Apache License 2.0, see [LICENSE](LICENSE); third-party notices in [NOTICE](NOTICE)
- The GUI uses [Slint](https://slint.dev) (Royalty-Free 2.0 track). Per the RF 2.0 obligations an
  `AboutSlint` widget will be placed on the about page (P2); until then NOTICE and this section
  serve the attribution duty.

Thanks to [HMCL](https://github.com/HMCL-dev/HMCL) and
[PCL2](https://github.com/Meloong-Git/PCL) (both GPL-3.0) — these two launchers have accompanied
the vast majority of Minecraft players for many years. ULL learned a great deal about design and
implementation from them, and this project is dedicated to those two pioneers.
