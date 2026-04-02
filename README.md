<div align="center">

<img src="src-tauri/icons/icon.png" alt="NeoCensor" width="120">

# NeoCensor

**Desktop proxy client with per-app split tunneling**

[![Release](https://img.shields.io/github/v/release/HardStack-Miska/neocensor?style=flat-square&color=6880A8)](https://github.com/HardStack-Miska/neocensor/releases)
[![Downloads](https://img.shields.io/github/downloads/HardStack-Miska/neocensor/total?style=flat-square&color=5E9A78)](https://github.com/HardStack-Miska/neocensor/releases)
[![License](https://img.shields.io/github/license/HardStack-Miska/neocensor?style=flat-square)](LICENSE)
[![Stars](https://img.shields.io/github/stars/HardStack-Miska/neocensor?style=flat-square)](https://github.com/HardStack-Miska/neocensor/stargazers)

Route each app through proxy, direct, or block — individually.

[Download](#-download) · [Features](#-features) · [Build](#%EF%B8%8F-build-from-source) · [Contributing](#-contributing)

</div>

---

## What is NeoCensor?

NeoCensor is a lightweight Windows desktop client that gives you **per-application control** over your network traffic. Instead of routing everything through a VPN, you decide which apps use the proxy, which go direct, and which get blocked entirely.

Built with [Tauri](https://tauri.app/) (Rust + React), it uses [xray-core](https://github.com/XTLS/Xray-core) as the proxy engine and Windows Filtering Platform (WFP) for kernel-level traffic routing.

## ✨ Features

- **Per-App Routing** — Set each application to Proxy, Direct, or Block mode individually
- **VLESS Protocol** — Modern proxy protocol with Reality, TLS, WebSocket, gRPC, xHTTP transports
- **Profile System** — Switch between Gaming, Work, Smart Route, Full Tunnel presets instantly
- **Subscription Support** — Import servers from Base64, Sing-Box JSON, and Clash YAML formats
- **Windows Filtering Platform** — Kernel-level traffic control, not just a system proxy
- **Auto xray-core** — Downloads and verifies (SHA256) the xray binary automatically
- **System Tray** — Runs in background, connect/disconnect from tray
- **TCP Reset Technique** — Forces Chromium browsers to re-read proxy settings on mode change
- **Lightweight** — 3.8 MB installer, ~30 MB RAM usage

## 📥 Download

| Platform | Installer | Format |
|----------|-----------|--------|
| **Windows x64** | [NeoCensor_0.1.0_x64-setup.exe](https://github.com/HardStack-Miska/neocensor/releases/latest) | NSIS installer (recommended) |
| **Windows x64** | [NeoCensor_0.1.0_x64_en-US.msi](https://github.com/HardStack-Miska/neocensor/releases/latest) | MSI installer |

> **Note:** Run as Administrator for full per-app routing (WFP requires elevated privileges). Without admin rights, the app works as a standard system proxy.

## 🖥️ Screenshots

<div align="center">

*Coming soon*

</div>

## 🔧 How It Works

NeoCensor combines three techniques for per-app routing:

```
┌─────────────────────────────────────────────────┐
│  App set to PROXY                               │
│  → Uses system proxy (PAC) → xray-core → VPN   │
├─────────────────────────────────────────────────┤
│  App set to DIRECT                              │
│  → WFP blocks proxy port → PAC fallback → ISP  │
├─────────────────────────────────────────────────┤
│  App set to BLOCK                               │
│  → WFP blocks all traffic → No internet         │
└─────────────────────────────────────────────────┘
```

1. **System Proxy + PAC** — Configures Windows proxy with a PAC file that returns `PROXY host:port; DIRECT`
2. **WFP Filters** — For DIRECT apps, blocks connections to the proxy port so the PAC fallback kicks in
3. **TCP Reset** — When switching modes, temporarily blocks the app to kill keepalive connections, forcing Chromium to re-evaluate proxy settings

## 🛠️ Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) >= 18
- [Rust](https://rustup.rs/) >= 1.75
- Windows 10/11 with [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

### Steps

```bash
# Clone
git clone https://github.com/HardStack-Miska/neocensor.git
cd neocensor

# Install frontend dependencies
npm install

# Development mode
npm run tauri dev

# Build release
npm run tauri build
```

Output:
- `src-tauri/target/release/neocensor.exe` — portable binary
- `src-tauri/target/release/bundle/nsis/` — NSIS installer
- `src-tauri/target/release/bundle/msi/` — MSI installer

## 📁 Project Structure

```
neocensor/
├── src/                        # React frontend
│   ├── components/             # UI components
│   │   ├── Sidebar/            # Server list, profiles, subscriptions
│   │   ├── Routing/            # Per-app route configuration
│   │   ├── Traffic/            # Live connection monitor
│   │   ├── Settings/           # App settings, logs
│   │   ├── Layout/             # Titlebar, window controls
│   │   └── common/             # Toast, Toggle, ErrorBoundary
│   ├── stores/                 # Zustand state management
│   └── lib/                    # Types, theme, Tauri API wrapper
│
├── src-tauri/                  # Rust backend
│   └── src/
│       ├── core/               # Business logic
│       │   ├── xray.rs         # xray-core process manager
│       │   ├── config_gen.rs   # xray config generation
│       │   ├── wfp/            # Windows Filtering Platform
│       │   ├── pac_server.rs   # PAC file HTTP server
│       │   ├── system_proxy.rs # Windows registry proxy
│       │   └── ...
│       ├── commands/           # Tauri IPC commands
│       ├── models/             # Data structures
│       ├── parsers/            # VLESS URI & subscription parsers
│       └── utils/              # Paths, constants
│
└── package.json
```

## 🔐 Security

- **No hardcoded secrets** — All server credentials stored in JSON, xray config deleted after disconnect
- **SHA256 verification** — Downloaded xray binary is verified against release checksums
- **Input validation** — Proxy host, ports, and server configs validated before use
- **Registry safety** — `reg.exe` exit codes checked, stale proxy cleaned up on startup
- **Panic recovery** — System proxy is unset on crash via panic hook

## ⚙️ Tech Stack

| Layer | Technology |
|-------|-----------|
| Framework | [Tauri 2](https://tauri.app/) |
| Frontend | React 19, TypeScript, Zustand |
| Backend | Rust, Tokio |
| Styling | Inline styles + theme system |
| Proxy Engine | [xray-core](https://github.com/XTLS/Xray-core) (VLESS) |
| Traffic Control | Windows Filtering Platform (WFP) |
| Installer | NSIS, MSI |

## 🗺️ Roadmap

- [ ] macOS / Linux support
- [ ] Multi-hop (chain) proxy
- [ ] Geo-based auto-routing (Smart mode)
- [ ] VMess, Trojan, Shadowsocks protocols
- [ ] i18n (Russian, Ukrainian)
- [ ] Auto-update mechanism
- [ ] Traffic statistics (bandwidth graphs)

## 🤝 Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'feat: add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Commit Convention

```
feat(scope): description    # New feature
fix(scope): description     # Bug fix
refactor(scope): description # Code change
```

Scopes: `vpn`, `wfp`, `ui`, `routing`, `auth`, `api`

## 📄 License

This project is licensed under the MIT License — see the [LICENSE](LICENSE) file for details.

## ⭐ Star History

If you find NeoCensor useful, please consider giving it a star!

---

<div align="center">

Made with Rust + React + Tauri

</div>
