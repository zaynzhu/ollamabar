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
> usage, official reset countdowns, 24h/7d/30d request counts, and usage history — straight from
> the system tray. Strictly read-only: no request proxying, no tool configuration changes,
> no telemetry.

## ✨ Features

- **Multi-Key** -- Monitor any number of Ollama Cloud accounts, one card per key
- **Dual-Window Gauges** -- Ring gauges for the 5-hour window and weekly quota, colored by threshold (green / yellow / red); used-percent is derived from the official `/api/balance` remaining-percent
- **Official Reset Countdown** -- Reset times come from the server-provided `resets_at` (no longer labeled "estimated"); only when the server omits it does the client fall back to derivation, clearly labeled
- **Request Stats** -- The card shows total requests in the last 24h; the details dialog adds 7d / 30d (from the official `/api/usage` history endpoint)
- **New Billing Plans** -- USD balance/allowance plans draw the ring by period-used-percent with a balance sub-label; a billing-period countdown replaces the 5h/weekly windows
- **Honest History** -- Sampled every 60s into SQLite (WAL); the sawtooth shape of in-window cumulative usage is rendered as-is, never smoothed
- **Three-Level Failure Alerts** -- Invalid key, stale data, and suspected API death are reported separately — no silently stale numbers
- **Rate-Limit Compliant** -- Respects the per-user 10 req/min limit: a shared request budget across keys, and 429s auto-pause per `Retry-After` before resuming
- **Tray-Resident** -- Closing the window hides it; sampling continues in the background, one click to bring it back
- **Autostart on Boot** -- Toggle "start on boot" from the tray menu: writes a per-user Run registry entry on Windows and a LaunchAgent on macOS; no admin rights needed
- **Run Logs & Export** -- The "Details" dialog on each card lists every sample, error, and reset; retained 7 days by default (switchable to 30) with auto-cleanup, exportable as txt
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
| Standalone executable (Windows) | `ollamabar.exe` |
| NSIS installer (Windows) | `bundle/nsis/OllamaBar_x.y.z_x64-setup.exe` |
| MSI installer (Windows) | `bundle/msi/OllamaBar_x.y.z_x64_en-US.msi` |
| macOS installer | `bundle/dmg/OllamaBar_x.y.z_aarch64.dmg` |

A tag-triggered GitHub Actions workflow (`.github/workflows/release.yml`) also exists: push a `v*` tag to build aarch64 and x86_64 macOS dmgs and publish them as a Release automatically.

### Requirements

- Rust stable (Windows: `x86_64-pc-windows-msvc` toolchain with VS Build Tools; macOS: Xcode Command Line Tools)
- Node.js >= 18
- Windows 10/11 (WebView2 runtime, bundled with Win11)

## 💡 Usage

### Adding Keys

Use the **Add key** form at the bottom of the window: enter an alias (e.g. "work") and the API key. Each key is polled and displayed independently.

### Reading the Dashboard

- Each card shows the 5h-window ring on the left and the weekly ring on the right (new billing plans show a single ring: period-used percent with a USD sub-label, plus billing-period remaining beside it)
- Below the rings: requests in the last 24h and both reset countdowns (server-sourced ones are unlabeled; derived ones are labeled "estimated")
- The **model calls** section: the official usage API does not provide per-model breakdowns yet (listed as Coming soon), so a clear placeholder is shown; model stats recorded before the upgrade remain in logs and exports
- The **usage chart** is an as-is sawtooth of in-window cumulative share; dashed lines mark reset boundaries (billing-period boundaries for new plans)

### Viewing Run Logs

Click **Details** on a card to open its run log: one successful sample per minute, with fetch errors and observed resets logged separately; check "only issues" to see errors and resets alone. Logs are retained 7 days by default (switchable to 30 in the dialog) and cleaned up automatically, so their size stays bounded; **Export** produces a txt in the same format as command-line monitors for long-term archiving.

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
| History sawtooth + request stats | ✅ | ❌ |

## 🗺️ Roadmap

| Area | Feature | Status |
|------|---------|--------|
| Data | Per-model stats (auto-displayed once Ollama Cloud restores the capability) | 📋 Blocked on Ollama Cloud |
| Data | Cross-week "most used models" statistics | 📋 Planned |
| Data | Derived usage-rate curve (local integration, labeled as derived) | 📋 Planned |
| Platform | macOS tray template icon | 📋 Planned |
| UX | Quota alert notifications | 📋 Planned |

## ❓ FAQ

<details>
<summary>Where are API keys stored, and is it safe?</summary>

Plaintext in `%APPDATA%\com.ollamabar.app\config.json` (macOS: `~/Library/Application Support/com.ollamabar.app/`). A deliberate trade-off for single-user machines — only use revocable keys on machines you trust.

</details>

<details>
<summary>When are reset times labeled "estimated"?</summary>

Since October 2026 the official `/api/balance` endpoint returns the server-side reset time `resets_at` (UTC), which the countdown follows without an "estimated" label. Only when the server omits the field does the client fall back to derivation (weekly window resets Monday 00:00 UTC; the 5h window advances on 5-hour Unix timestamp buckets), labeled "estimated" in the UI. If the server-provided reset time stops advancing past its grace window, the app raises an alarm.

</details>

<details>
<summary>What is the relationship with CodexBar?</summary>

Form-factor inspiration (tray usage bar). OllamaBar focuses on the single Ollama Cloud scenario, runs on Windows and macOS, and natively supports multi-key plus history charts. CodexBar is a Swift, macOS-only multi-provider menu bar tool.

</details>

<details>
<summary>Why is the history chart a sawtooth?</summary>

Because that is what the data looks like: usage is the cumulative share within the current window, which resets to zero when the window rolls over (billing-period cumulative for new plans, zeroing at period end). OllamaBar does not smooth it away — the sawtooth is the true shape, and dashed vertical lines mark reset boundaries.

</details>

<details>
<summary>Where did the per-model stats go?</summary>

After the October 2026 API rework, Ollama Cloud lists "usage breakdowns by model" as Coming soon — the new API does not provide per-model detail yet. OllamaBar neither treats the missing data as zero calls nor fabricates stats; the model section shows a clear placeholder. Real model data recorded before the upgrade is preserved in the SQLite logs and exports, and will reappear automatically once the official capability returns.

</details>

<details>
<summary>How do I build the macOS version?</summary>

Clone the repo on macOS and run `npm install && npm run tauri build`. Xcode Command Line Tools required; the dmg lands in `bundle/dmg/`. Alternatively, push a `v*` tag and let GitHub Actions build and publish the Release for you.

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
