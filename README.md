# RemoteLab

**RemoteLab** is an open-source infrastructure toolkit designed to support remote robotics experimentation and automation workflows.

It enables researchers and engineers to manage remote devices, orchestrate experiments, collect data, and automate development pipelines for robotics systems — developed as part of robotics research workflows.

The project aims to improve **reproducibility**, **accessibility**, and **scalability** in robotics research environments.

[![Tauri 2.0](https://img.shields.io/badge/Tauri-2.0-blue?logo=tauri)](https://tauri.app)
[![React](https://img.shields.io/badge/React-18-61dafb?logo=react)](https://react.dev)
[![Rust](https://img.shields.io/badge/Rust-stable-orange?logo=rust)](https://www.rust-lang.org)
[![TypeScript](https://img.shields.io/badge/TypeScript-5-3178c6?logo=typescript)](https://www.typescriptlang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## Key Features

- **SSH Terminal** — Full-featured terminal powered by xterm.js with multi-tab support for managing multiple remote sessions simultaneously
- **Remote Desktop** — Automatic GPU detection: uses NVENC H.264 streaming via Sunshine when available, with VNC fallback for broad compatibility
- **File Manager** — SFTP-based file browser with support for upload, download, create, and delete operations
- **WireGuard VPN** — Optional VPN toggle with real-time latency monitoring for secure remote connections
- **SSH Key Management** — Generate, import, and deploy SSH keys directly from the application
- **Config Import/Export** — JSON-based backup and restore for easy configuration migration across machines
- **Config Encryption** — Optional AES-256-GCM password protection for sensitive configuration data
- **Multi-language** — Full support for English and Chinese interfaces
- **Light/Dark Theme** — Switchable themes to match your preference
- **Cross-platform** — Native builds for macOS, Linux, and Windows

RemoteLab is particularly useful for:

- **Robotics laboratories** requiring remote access to hardware platforms
- **Distributed research teams** collaborating across institutions
- **Remote hardware experimentation** with real-time monitoring
- **Automation testing pipelines** for robotics development workflows

## Architecture

RemoteLab provides a modular architecture that allows robotics systems and research infrastructure to integrate with remote experiment workflows.

```
┌─────────────────────────────────────────────────┐
│                  RemoteLab App                   │
├──────────────┬──────────────┬────────────────────┤
│ SSH Terminal │Remote Desktop│   File Manager     │
├──────────────┴──────────────┴────────────────────┤
│         Experiment Control Interface             │
├──────────────────────────────────────────────────┤
│      Remote Device Management (Tauri + Rust)     │
├──────────────────────────────────────────────────┤
│   Data Collection  │  Logging & Monitoring       │
├────────────────────┴─────────────────────────────┤
│     Automation & Workflow Management Layer       │
└──────────────────────────────────────────────────┘
```

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

## Example Use Cases

- Running remote robotics experiments from cloud environments
- Automating experiment pipelines for robotics research
- Managing multiple robots across distributed labs
- Collecting experiment logs and telemetry data
- Remote debugging and monitoring of robotics platforms

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
git clone https://github.com/ucas-liumk/RemoteLab.git
cd RemoteLab
npm install
npm run tauri dev
```

### Production build

```bash
npm run tauri build
```

### Platform-specific build notes

**macOS**

```bash
xcode-select --install
```

Produces `.dmg` and `.app` bundles.

**Linux**

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

Produces `.deb` and `.AppImage` bundles.

**Windows**

Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "C++ build tools" workload. Produces `.msi` and `.exe` installers.

## Project Goals

RemoteLab focuses on building infrastructure that supports:

- **Reproducible robotics experiments** with consistent remote environments
- **Scalable research infrastructure** for growing lab networks
- **Remote hardware access** with low-latency streaming
- **Automation of robotics workflows** from development to deployment

## Roadmap

Planned improvements include:

- Experiment workflow automation and scheduling
- Integration with robotics simulation tools (Gazebo, Isaac Sim)
- AI-assisted experiment scripting
- Experiment analytics and visualization tools
- Plugin system for custom robotics integrations

## Contributing

Contributions are welcome from the robotics and research community. Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

Maintainers review pull requests and issues regularly. Please open an issue to discuss major changes before submitting PRs.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
