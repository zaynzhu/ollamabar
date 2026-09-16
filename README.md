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

OllamaBar 是一个跨 Windows / macOS 的 Ollama Cloud 多 Key 用量托盘查看器。

> [!TIP]
> 添加多个 Ollama Cloud API Key，在系统托盘常驻查看每个 Key 的 5 小时窗口与每周用量、
> 重置倒计时、各模型调用次数和历史用量曲线。纯只读查看器——不代理请求、不修改任何工具配置、
> 不采集上报任何数据。

## ✨ Features

- **多 Key 支持** -- 同时监控任意数量的 Ollama Cloud 账号，每个 Key 一张独立卡片
- **双窗口额度** -- 5 小时窗口与每周额度环形图，颜色随用量阈值变化（绿 / 黄 / 红）
- **重置倒计时** -- 5h 窗口与每周重置时间倒计时，标注"预计"（接口不返回重置时间，由客户端按已验证规则推算）
- **模型调用统计** -- 各模型本周与 5h 窗口内的请求次数条形图
- **诚实历史曲线** -- 每 60 秒采样入库（SQLite WAL），原样呈现窗口内累计份额的锯齿形状，不做平滑修饰
- **三级失效告警** -- key 无效、数据滞后、接口疑似失效分三档提示，拒绝静默展示过期数据
- **托盘常驻** -- 关闭窗口即隐藏，后台持续采样，托盘一键唤回
- **跨平台** -- 同一份 Rust 后端 + 前端代码跑 Windows 与 macOS

## 🚀 Quick Start

```bash
git clone https://github.com/zaynzhu/ollamabar.git
cd ollamabar
npm install
npm run tauri dev
```

应用启动后，在窗口底部表单填入别名与 Ollama Cloud API Key，点"添加 key"，60 秒内出数据。

> [!WARNING]
> API Key 以明文存储在本地配置文件（`%APPDATA%\com.ollamabar.app\config.json`）中。
> 这是单机自用场景下的刻意取舍：任何能读取你用户目录的程序都能拿到 Key。介意请用可随时撤销的 Key。

## 📦 Installation

### 从源码运行

```bash
git clone https://github.com/zaynzhu/ollamabar.git
cd ollamabar
npm install
npm run tauri dev
```

### 构建正式包

```bash
npm run tauri build
```

产物位于 `src-tauri/target/release/`：

| 产物 | 路径 |
|------|------|
| 独立可执行文件 | `ollamabar.exe` |
| NSIS 安装包 | `bundle/nsis/OllamaBar_x.y.z_x64-setup.exe` |
| MSI 安装包 | `bundle/msi/OllamaBar_x.y.z_x64_en-US.msi` |

### 环境要求

- Rust stable（Windows 用 `x86_64-pc-windows-msvc` 工具链，需 VS Build Tools；macOS 需 Xcode Command Line Tools）
- Node.js >= 18
- Windows 10/11（WebView2 运行时，Win11 自带）

## 💡 Usage

### 添加与切换 Key

启动后点击 **添加 key** 表单：填别名（如"工作"）和 API Key 即可。每个 Key 独立轮询、独立展示，互不影响。

### 读懂仪表盘

- 每张卡片左侧是 5h 窗口环形图，右侧是本周环形图
- 下方两列分别是两者的重置倒计时（标"预计"）
- **本周模型调用** 条形图展示各模型请求次数
- **用量曲线** 是窗口内累计份额的锯齿折线，虚线为推算的窗口重置边界

### 理解失效提示

| 横幅 | 含义 |
|------|------|
| 黄色 `数据滞后 N 分钟` | 请求失败，保留上次成功数据 |
| 红色 `key 无效或已撤销` | 接口返回 401/403，检查或撤销该 Key |
| 深红 `接口可能已失效` | 连续 24 小时无一次成功采样 |

## 📚 Documentation

| 主题 | 描述 |
|------|------|
| [设计规格](docs/superpowers/specs/2026-09-15-ollamabar-design.md) | 架构、冻结契约与数据诚实性条款（H1~H6） |
| [实现计划](docs/superpowers/plans/2026-09-15-ollamabar.md) | 分任务实现过程与验收清单 |
| [接口口径验证](docs/verify-usage.md) | API 数字与 ollama.com 官网对照记录 |

## 🔄 与 CodexBar 对比

OllamaBar 的托盘形态参考 [CodexBar](https://github.com/steipete/CodexBar)，但定位不同：

| 特性 | OllamaBar | CodexBar |
|------|:---------:|:--------:|
| 平台 | Windows + macOS | macOS only |
| Ollama Cloud 用量 | ✅ 专用 | ⚠️ 多 provider 之一 |
| 多 Key 支持 | ✅ 核心场景 | ⚠️ |
| 历史锯齿曲线 + 模型统计 | ✅ | ❌ |

## 🗺️ Roadmap

| 区域 | 特性 | 状态 |
|------|------|------|
| 数据 | 跨周长期"最常用模型"统计 | 📋 Planned |
| 数据 | 用量速率推算曲线（本地积分，标注推算） | 📋 Planned |
| 平台 | macOS 构建产物与托盘 template 图标 | 📋 Planned |
| 体验 | 额度告警通知推送 | 📋 Planned |

## ❓ FAQ

<details>
<summary>API Key 存在哪里，安全吗？</summary>

明文存储在 `%APPDATA%\com.ollamabar.app\config.json`（macOS 为 `~/Library/Application Support/com.ollamabar.app/`）。这是单机自用场景下的刻意取舍——请只在该机器上使用可随时撤销的 Key。

</details>

<details>
<summary>重置时间为什么标"预计"？</summary>

`/api/usage` 接口不返回重置时间。客户端按已验证的规则推算：每周窗口在周一 00:00 UTC 重置，5h 窗口按 5 小时 Unix 时间桶边界持续推进。推算规则失效时应用会通过实测重置事件对比自动告警。

</details>

<details>
<summary>和 CodexBar 是什么关系？</summary>

形态参考（托盘用量条），但 OllamaBar 聚焦 Ollama Cloud 单一场景、跨 Windows/macOS、原生支持多 Key 与历史图表。CodexBar 是 Swift 实现的 macOS 专用多 provider 菜单栏工具。

</details>

<details>
<summary>历史曲线为什么是锯齿形的？</summary>

因为数据本来就是这样的：API 返回的是当前窗口内的累计份额，窗口重置后归零重来。OllamaBar 不做平滑修饰，锯齿是数据的真实形状，虚线竖线标记推算的重置边界。

</details>

<details>
<summary>macOS 版怎么构建？</summary>

在 macOS 上 clone 仓库后 `npm install && npm run tauri build`。需要 Xcode Command Line Tools。当前仓库在 Windows 上开发验证，macOS 构建产物列入 Roadmap。

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

本项目基于 [MIT License](LICENSE) 开源——详见 [LICENSE](LICENSE) 文件。
