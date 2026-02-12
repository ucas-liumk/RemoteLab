# RemoteLab

A modern, cross-platform remote lab management desktop application.

[![Tauri 2.0](https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri)](https://tauri.app)
[![React](https://img.shields.io/badge/React-18-61dafb?logo=react)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178c6?logo=typescript)](https://www.typescriptlang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## Features

- **SSH Terminal** — Full-featured terminal powered by xterm.js with multi-tab support for managing multiple sessions simultaneously.
- **Remote Desktop** — Automatic GPU detection: uses NVENC H.264 streaming via Sunshine when available, with VNC fallback for broad compatibility.
- **File Manager** — SFTP-based file browser with support for upload, download, create, and delete operations.
- **WireGuard VPN** — Optional VPN toggle with real-time latency monitoring for secure remote connections.
- **SSH Key Management** — Generate, import, and deploy SSH keys directly from the application.
- **Config Import/Export** — JSON-based backup and restore for easy configuration migration across machines.
- **Multi-language** — Full support for English and Chinese interfaces.
- **Light/Dark Theme** — Switchable themes to match your preference.
- **Config Encryption** — Optional AES-256-GCM password protection for sensitive configuration data.
- **Cross-platform** — Native builds for macOS, Linux, and Windows.

## Screenshots

<!-- Screenshots coming soon -->

## Installation

Download the latest release for your platform from the [Releases](https://github.com/ucas-liumk/RemoteLab/releases) page.

| Platform | Format |
|----------|--------|
| macOS    | `.dmg`, `.app` |
| Linux    | `.deb`, `.AppImage` |
| Windows  | `.msi`, `.exe` |

### Platform-specific notes

- **macOS**: You may need to allow the app in System Settings > Privacy & Security on first launch.
- **Linux**: For `.AppImage`, make the file executable with `chmod +x` before running.
- **Windows**: If Windows Defender SmartScreen appears, click "More info" then "Run anyway".

## Building from Source

### Prerequisites

- [Node.js](https://nodejs.org) >= 18
- [Rust](https://www.rust-lang.org/tools/install) >= 1.70
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform

### Development

```bash
npm install
npm run tauri dev
```

### Production build

```bash
npm run tauri build
```

### Platform-specific build notes

**macOS**

Install Xcode Command Line Tools:

```bash
xcode-select --install
```

Produces `.dmg` and `.app` bundles.

**Linux**

Install required system dependencies:

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Produces `.deb` and `.AppImage` bundles.

**Windows**

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "C++ build tools" workload.

Produces `.msi` and `.exe` installers.

## Usage

1. **Launch** the application.
2. **Add Device** — Click the add button and enter your remote host details (hostname, port, credentials).
3. **Connect** — Use SSH Terminal, Remote Desktop, or File Manager to interact with your device.

## Architecture

### Frontend

- **React 18** with **TypeScript** for the UI layer
- **Tailwind CSS** for utility-first styling
- **Vite** as the build tool and dev server
- **xterm.js** for terminal emulation
- **react-vnc** for VNC-based remote desktop

### Backend

- **Rust** for the native backend
- **Tauri 2.0** as the application framework
- **tokio** for async runtime
- **serde** for serialization/deserialization
- **portable-pty** for pseudo-terminal management

### Configuration

Application configuration is stored at:

```
~/.config/remotelab/config.json
```

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
