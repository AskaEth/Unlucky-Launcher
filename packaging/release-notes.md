# Unlucky Launcher {{VERSION}}

Rust-native **single-file** Minecraft launcher: Slint + Metro UI, built-in AI agent
(chat / config edits / crash diagnosis) and an MCP server other agent tools can attach to.
No WebView, no bundled runtime, no JRE download needed to start the launcher itself.

## Downloads

| Platform | Asset | Notes |
| --- | --- | --- |
| Windows 10/11 x64 | `unlucky-launcher-{{VERSION}}-windows-x64.exe` | single exe, double-click to run |
| Linux x64 (X11/Wayland) | `unlucky-launcher-{{VERSION}}-linux-x64.tar.gz` | single binary inside, no installer |
| macOS 12+ Apple Silicon | `unlucky-launcher-{{VERSION}}-macos-arm64.tar.gz` | single binary inside, no installer |
| macOS 12+ Intel | `unlucky-launcher-{{VERSION}}-macos-x64.tar.gz` | single binary inside, no installer |

`LICENSE`, `NOTICE` and `README.md` ship in every archive; on Windows they are attached to
this release next to the `.exe` (a lone exe cannot carry them), so the license terms travel
with the download on every platform.

## Support matrix

| Platform | Level | Runtime requirements |
| --- | --- | --- |
| Windows 10/11 x64 | first class | nothing beyond the OS (UCRT is part of Win10+) |
| macOS 12+ (arm64 / x64) | first class | nothing; unsigned build, see Gatekeeper note |
| Linux x64, X11 | first class | `libxkbcommon`, `fontconfig`, `libGL` (or software renderer), `libxcb` |
| Linux x64, Wayland | supported | `wayland-client`, `libxkbcommon`, plus the X11 set for XWayland apps |
| Windows 8.1/8/7 | unsupported | baseline was raised to Win10 (decision D21) |
| Android — Termux + X11 | experimental, not promised | CLI only (build from source), GUI unverified; not part of this release matrix |

## Install

**Windows** — put the `.exe` anywhere and run it. Portable data lives next to the exe when a
`portable.marker` file is present, otherwise under `%APPDATA%`.

**Linux**

```bash
tar -xzf unlucky-launcher-{{VERSION}}-linux-x64.tar.gz
cd unlucky-launcher-{{VERSION}}-linux-x64
chmod +x unlucky-launcher && ./unlucky-launcher
```

**macOS**

```bash
tar -xzf unlucky-launcher-{{VERSION}}-macos-arm64.tar.gz
cd unlucky-launcher-{{VERSION}}-macos-arm64
xattr -d com.apple.quarantine unlucky-launcher   # unsigned build, Gatekeeper quarantine
chmod +x unlucky-launcher && ./unlucky-launcher
```

## No GPU? Software renderer

The UI prefers OpenGL (femtovg) and falls back to Slint's CPU renderer when no usable GL
driver is present. Force it on any machine with:

```bash
unlucky-launcher --software-renderer          # or: SLINT_BACKEND=winit-software unlucky-launcher
```

## Verify the download

```bash
# Linux / macOS
sha256sum -c SHA256SUMS.txt --ignore-missing
# Windows (PowerShell)
Get-FileHash .\unlucky-launcher-{{VERSION}}-windows-x64.exe -Algorithm SHA256
```

## Checksums

```
{{SUMS}}
```

## Known limitations of this build

- Microsoft (MSA) account login depends on a Mojang app-id whitelist that is still pending;
  until it is granted, use offline accounts or authlib-injector (the 403 is reported with a
  readable diagnostic instead of a silent failure).
- Binaries are not code-signed / notarized yet: Windows SmartScreen and macOS Gatekeeper warn
  on first launch. Checksums above are the integrity reference for now.
- macOS ships the bare binary (single-file promise), not a `.app` bundle: no Dock icon or
  Finder double-click — launch it from a terminal.

## License

Apache-2.0. Third-party notices (including Slint's Royalty-Free 2.0 track) are in `NOTICE`.
Unlucky Launcher is not affiliated with Mojang or Microsoft; it does not ship game files.

Made with [Slint](https://slint.dev) — Slint Royalty-Free 2.0 track, unmodified.
