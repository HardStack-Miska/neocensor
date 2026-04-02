<div align="center">

<img src="src-tauri/icons/icon.png" alt="NeoCensor" width="100">

# NeoCensor

Desktop proxy client with per-app split tunneling

[![Release](https://img.shields.io/github/v/release/HardStack-Miska/neocensor?style=flat-square&color=6880A8&label=version)](https://github.com/HardStack-Miska/neocensor/releases)
[![Downloads](https://img.shields.io/github/downloads/HardStack-Miska/neocensor/total?style=flat-square&color=5E9A78)](https://github.com/HardStack-Miska/neocensor/releases)
[![License](https://img.shields.io/github/license/HardStack-Miska/neocensor?style=flat-square&color=888)](LICENSE)

<br>

[<img src="https://img.shields.io/badge/Download_Installer-v0.1.0-6880A8?style=for-the-badge&logo=windows" alt="Download">](https://github.com/HardStack-Miska/neocensor/releases/download/v0.1.0/NeoCensor_0.1.0_x64-setup.exe)

<sub>Windows 10/11 x64 &nbsp;·&nbsp; 3.8 MB &nbsp;·&nbsp; [MSI version](https://github.com/HardStack-Miska/neocensor/releases/download/v0.1.0/NeoCensor_0.1.0_x64_en-US.msi)</sub>

</div>

---

## About

NeoCensor lets you control network traffic **per application**. Instead of routing everything through a VPN, you decide which apps use the proxy, which go direct, and which get blocked.

Built with [Tauri](https://tauri.app/) (Rust + React). Uses [xray-core](https://github.com/XTLS/Xray-core) as the proxy engine and Windows Filtering Platform for kernel-level routing.

## Features

**Routing**
- Set each app to Proxy, Direct, or Block individually
- Windows Filtering Platform — kernel-level traffic control, not just system proxy
- TCP reset technique forces Chromium to re-read proxy on mode switch

**Protocol**
- VLESS with Reality, TLS, WebSocket, gRPC, xHTTP transports
- Subscriptions — Base64, Sing-Box JSON, Clash YAML formats
- Auto-download xray-core with SHA256 checksum verification

**UX**
- Profile presets — Gaming, Work, Smart Route, Full Tunnel
- System tray — runs in background, connect/disconnect from tray
- 3.8 MB installer, ~30 MB RAM

## How it works

NeoCensor combines three techniques:

```
App → PROXY    System proxy (PAC) → xray-core → VPN server
App → DIRECT   WFP blocks proxy port → PAC fallback → ISP
App → BLOCK    WFP blocks all traffic → no internet
```

The PAC server returns `PROXY host:port; DIRECT`. For apps set to Direct, WFP blocks connections to the proxy port, so the browser falls back to DIRECT automatically.

## Download

| File | Size | Description |
|------|------|-------------|
| [NeoCensor_0.1.0_x64-setup.exe](https://github.com/HardStack-Miska/neocensor/releases/download/v0.1.0/NeoCensor_0.1.0_x64-setup.exe) | 3.8 MB | NSIS installer (recommended) |
| [NeoCensor_0.1.0_x64_en-US.msi](https://github.com/HardStack-Miska/neocensor/releases/download/v0.1.0/NeoCensor_0.1.0_x64_en-US.msi) | 5.6 MB | MSI installer |

Run as Administrator for full per-app routing. Without admin, works as a standard system proxy.

## Build from source

Requires: Node.js 18+, Rust 1.75+, Windows 10/11 with WebView2.

```bash
git clone https://github.com/HardStack-Miska/neocensor.git
cd neocensor
npm install
npm run tauri build
```

## Project structure

```
src/                    React frontend (Zustand, TypeScript)
  components/           Sidebar, Routing, Traffic, Settings
  stores/               Connection, server, routing, settings state
  lib/                  Types, theme, Tauri API wrapper

src-tauri/src/          Rust backend
  core/                 xray manager, WFP, PAC server, system proxy
  commands/             Tauri IPC (connect, routing, subscriptions)
  models/               Server, route, profile, settings
  parsers/              VLESS URI, subscription formats
```

## Tech stack

| | |
|---|---|
| Framework | Tauri 2 (Rust + WebView) |
| Frontend | React 19, TypeScript, Zustand |
| Proxy | xray-core (VLESS) |
| Routing | Windows Filtering Platform |
| Build | Vite, Cargo |

## Roadmap

- macOS and Linux support
- Multi-hop proxy chains
- Geo-based auto-routing
- VMess, Trojan, Shadowsocks
- Localization (ru, uk)
- Auto-update

## Contributing

1. Fork the repo
2. Create a branch (`git checkout -b feature/thing`)
3. Commit (`git commit -m 'feat(scope): description'`)
4. Push and open a PR

## License

[MIT](LICENSE)

---

<div align="center">
<sub>Tauri + React + Rust</sub>
</div>
