<div align="center">

# 📊 OllamaBar

[中文](README.md) | [English](README_EN.md)

![License](https://img.shields.io/github/license/zaynzhu/ollamabar?style=for-the-badge)
![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue?style=for-the-badge)
![Release](https://img.shields.io/github/v/release/zaynzhu/ollamabar?style=for-the-badge)
![Stars](https://img.shields.io/github/stars/zaynzhu/ollamabar?style=for-the-badge)
![Last Commit](https://img.shields.io/github/last-commit/zaynzhu/ollamabar?style=for-the-badge)
![Issues](https://img.shields.io/github/issues/zaynzhu/ollamabar?style=for-the-badge)

</div>

OllamaBar is a cross-platform (Windows / macOS) tray-resident usage viewer for Ollama Cloud with multi-key support.

> [!TIP]
> Add multiple Ollama Cloud API keys and keep an eye on each account's 5-hour and weekly
> usage, reset countdowns, per-model request counts, and usage history — straight from
> the system tray. Strictly read-only: no request proxying, no tool configuration changes,
> no telemetry.

## ✨ Features

- **Multi-Key** -- Monitor any number of Ollama Cloud accounts, one card per key
- **Dual-Window Gauges** -- Ring gauges for the 5-hour window and weekly quota, colored by threshold (green / yellow / red)
- **Reset Countdown** -- Countdown to both window resets, labeled "estimated" (the API returns no reset time; it is derived client-side from verified rules)
- **Per-Model Stats** -- Bar chart of request counts per model for the weekly and 5h windows
- **Honest History** -- Sampled every 60s into SQLite (WAL); the sawtooth shape of in-window cumulative usage is rendered as-is, never smoothed
- **Three-Level Failure Alerts** -- Invalid key, stale data, and suspected API death are reported separately — no silently stale numbers
- **Tray-Resident** -- Closing the window hides it; sampling continues in the background, one click to bring it back
- **Cross-Platform** -- One Rust backend + web frontend codebase running on Windows and macOS

## 🚀 Quick Start

```bash
git clone https://github.com/zaynzhu/ollamabar.git
cd ollamabar
npm install
npm run tauri dev
```

Once the app launches, fill in an alias and your Ollama Cloud API key in the form at the bottom, click **Add key**, and data appears within 60 seconds.

> [!WARNING]
> API keys are stored in plaintext in the local config file (`%APPDATA%\com.ollamabar.app\config.json`).
> This is a deliberate trade-off for single-user machines: any program that can read your user
> directory can read these keys. Use revocable keys if that concerns you.

## 📦 Installation

### Run from Source

```bash
git clone https://github.com/zaynzhu/ollamabar.git
cd ollamabar
npm install
npm run tauri dev
```

### Build Release Bundle

```bash
npm run tauri build
```

Artifacts land in `src-tauri/target/release/`:

| Artifact | Path |
|----------|------|
| Standalone executable | `ollamabar.exe` |
| NSIS installer | `bundle/nsis/OllamaBar_x.y.z_x64-setup.exe` |
| MSI installer | `bundle/msi/OllamaBar_x.y.z_x64_en-US.msi` |

### Requirements

- Rust stable (Windows: `x86_64-pc-windows-msvc` toolchain with VS Build Tools; macOS: Xcode Command Line Tools)
- Node.js >= 18
- Windows 10/11 (WebView2 runtime, bundled with Win11)

## 💡 Usage

### Adding Keys

Use the **Add key** form at the bottom of the window: enter an alias (e.g. "work") and the API key. Each key is polled and displayed independently.

### Reading the Dashboard

- Each card shows the 5h-window ring on the left and the weekly ring on the right
- Below them, two columns show reset countdowns (labeled "estimated")
- The **weekly model calls** bar chart ranks models by request count
- The **usage chart** is an as-is sawtooth of in-window cumulative share; dashed lines mark estimated reset boundaries

### Understanding Failure Alerts

| Banner | Meaning |
|--------|---------|
| Yellow `data is N minutes stale` | Request failed; last successful data is kept |
| Red `key invalid or revoked` | The API returned 401/403; check or revoke that key |
| Dark red `API may be dead` | 24 consecutive hours without a single successful sample |

## 📚 Documentation

| Topic | Description |
|-------|-------------|
| [Design Spec](docs/superpowers/specs/2026-09-15-ollamabar-design.md) | Architecture, frozen contract, and data honesty clauses (H1–H6) |
| [Implementation Plan](docs/superpowers/plans/2026-09-15-ollamabar.md) | Task breakdown and acceptance checklists |
| [API Semantics Verification](docs/verify-usage.md) | Record of comparing API numbers against ollama.com |
| [Deferred Minors](docs/deferred-minors.md) | Low-priority items recorded during implementation reviews |

## 🔄 Comparison with CodexBar

OllamaBar's tray form factor is inspired by [CodexBar](https://github.com/steipete/CodexBar), but the focus differs:

| Feature | OllamaBar | CodexBar |
|---------|:---------:|:--------:|
| Platform | Windows + macOS | macOS only |
| Ollama Cloud usage | ✅ Dedicated | ⚠️ One of many providers |
| Multi-key support | ✅ Core scenario | ⚠️ |
| History sawtooth + model stats | ✅ | ❌ |

## 🗺️ Roadmap

| Area | Feature | Status |
|------|---------|--------|
| Data | Cross-week "most used models" statistics | 📋 Planned |
| Data | Derived usage-rate curve (local integration, labeled as derived) | 📋 Planned |
| Platform | macOS build artifacts with tray template icon | 📋 Planned |
| UX | Quota alert notifications | 📋 Planned |

## ❓ FAQ

<details>
<summary>Where are API keys stored, and is it safe?</summary>

Plaintext in `%APPDATA%\com.ollamabar.app\config.json` (macOS: `~/Library/Application Support/com.ollamabar.app/`). A deliberate trade-off for single-user machines — only use revocable keys on machines you trust.

</details>

<details>
<summary>Why are reset times labeled "estimated"?</summary>

The `/api/usage` endpoint does not return reset times. The client derives them from verified rules: the weekly window resets Monday 00:00 UTC; the 5h window advances on 5-hour Unix timestamp buckets. When the derivation rule drifts, the app raises an alarm by comparing observed reset events against estimated boundaries.

</details>

<details>
<summary>What is the relationship with CodexBar?</summary>

Form-factor inspiration (tray usage bar). OllamaBar focuses on the single Ollama Cloud scenario, runs on Windows and macOS, and natively supports multi-key plus history charts. CodexBar is a Swift, macOS-only multi-provider menu bar tool.

</details>

<details>
<summary>Why is the history chart a sawtooth?</summary>

Because that is what the data looks like: the API returns the cumulative share within the current window, which resets to zero when the window rolls over. OllamaBar does not smooth it away — the sawtooth is the true shape, and dashed vertical lines mark estimated reset boundaries.

</details>

<details>
<summary>How do I build the macOS version?</summary>

Clone the repo on macOS and run `npm install && npm run tauri build`. Xcode Command Line Tools required. The repo is developed and verified on Windows; macOS artifacts are on the roadmap.

</details>

## ⭐ Star History

<a href="https://star-history.com/#zaynzhu/ollamabar&Date">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=zaynzhu/ollamabar&type=Date&theme=dark" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=zaynzhu/ollamabar&type=Date" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=zaynzhu/ollamabar&type=Date" />
 </picture>
</a>

## 📄 License

This project is licensed under the [MIT License](LICENSE) — see the [LICENSE](LICENSE) file for details.
