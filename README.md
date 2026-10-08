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
> 官方重置倒计时、近 24h/7d/30d 请求次数和历史用量曲线。纯只读查看器——不代理请求、不修改任何工具配置、
> 不采集上报任何数据。

## ✨ Features

- **多 Key 支持** -- 同时监控任意数量的 Ollama Cloud 账号，每个 Key 一张独立卡片
- **双窗口额度** -- 5 小时窗口与每周额度环形图，颜色随用量阈值变化（绿 / 黄 / 红）；已用比例由官方 `/api/balance` 的剩余百分比换算
- **官方重置倒计时** -- 重置时间以服务端 `resets_at` 为准（不再标"预计"）；服务端缺失时才回退客户端推算并明确标注
- **请求次数统计** -- 卡片显示近 24h 总请求数，详情弹窗另附近 7d / 30d（来自官方 `/api/usage` 历史统计接口）
- **新计费套餐兼容** -- 美元余额/额度套餐（balance_usd/allowance_usd）按本期已用比例画环并显示余额，计费周期倒计时替代 5h/周窗口
- **诚实历史曲线** -- 每 60 秒采样入库（SQLite WAL），原样呈现窗口内累计份额的锯齿形状，不做平滑修饰
- **三级失效告警** -- key 无效、数据滞后、接口疑似失效分三档提示，拒绝静默展示过期数据
- **官方限流保护** -- 遵守每用户 10 次/分钟限流：跨 Key 共享请求预算，429 按 `Retry-After` 自动暂停后恢复
- **托盘常驻** -- 关闭窗口即隐藏，后台持续采样，托盘一键唤回
- **开机自启** -- 托盘右键勾选"开机启动"：Windows 写入当前用户注册表 Run 项，macOS 注册 LaunchAgent，均无需管理员权限
- **运行日志与导出** -- 卡片"详情"弹窗查看每轮采样、错误与重置记录；默认保留 7 天（可切 30 天）自动清理，可导出 txt
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
| 独立可执行文件（Windows） | `ollamabar.exe` |
| NSIS 安装包（Windows） | `bundle/nsis/OllamaBar_x.y.z_x64-setup.exe` |
| MSI 安装包（Windows） | `bundle/msi/OllamaBar_x.y.z_x64_en-US.msi` |
| macOS 安装包 | `bundle/dmg/OllamaBar_x.y.z_aarch64.dmg` |

另有 tag 触发的 GitHub Actions（`.github/workflows/release.yml`）：推送 `v*` 标签即自动构建 aarch64 与 x86_64 的 macOS dmg 并发布 Release。

### 环境要求

- Rust stable（Windows 用 `x86_64-pc-windows-msvc` 工具链，需 VS Build Tools；macOS 需 Xcode Command Line Tools）
- Node.js >= 18
- Windows 10/11（WebView2 运行时，Win11 自带）

## 💡 Usage

### 添加与切换 Key

启动后点击 **添加 key** 表单：填别名（如"工作"）和 API Key 即可。每个 Key 独立轮询、独立展示，互不影响。

### 读懂仪表盘

- 每张卡片左侧是 5h 窗口环形图，右侧是本周环形图（新计费套餐则为一环：本期已用 % + 美元余额副标签，另一侧显示计费周期剩余）
- 环形图下方是近 24h 请求次数与两窗口重置倒计时（服务端来源不标"预计"，推算来源标"预计"）
- **模型调用** 区域：官方用量接口暂不提供按模型明细（官方列为 Coming soon），显示明确占位提示；升级前已入库的历史模型统计保留在日志与导出中
- **用量曲线** 是窗口内累计份额的锯齿折线，虚线为窗口重置边界（新计费套餐为周期边界）

### 查看运行日志

点卡片头部的 **详情** 打开运行日志：每分钟一条成功采样，取数失败与实测重置另行留痕；勾选"仅看异常"只看错误与重置。日志默认保留 7 天（弹窗内可切 30 天），过期自动清理，体积有硬上限；**导出日志** 生成与命令行监控同款格式的 txt，可长期留档。

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
| [遗留项清单](docs/deferred-minors.md) | 实现期审查记录的低优先级事项 |

## 🔄 与 CodexBar 对比

OllamaBar 的托盘形态参考 [CodexBar](https://github.com/steipete/CodexBar)，但定位不同：

| 特性 | OllamaBar | CodexBar |
|------|:---------:|:--------:|
| 平台 | Windows + macOS | macOS only |
| Ollama Cloud 用量 | ✅ 专用 | ⚠️ 多 provider 之一 |
| 多 Key 支持 | ✅ 核心场景 | ⚠️ |
| 历史锯齿曲线 + 请求统计 | ✅ | ❌ |

## 🗺️ Roadmap

| 区域 | 特性 | 状态 |
|------|------|------|
| 数据 | 模型调用明细（等待官方恢复接口能力后自动展示） | 📋 Blocked on Ollama Cloud |
| 数据 | 跨周长期"最常用模型"统计 | 📋 Planned |
| 数据 | 用量速率推算曲线（本地积分，标注推算） | 📋 Planned |
| 平台 | macOS 托盘 template 图标 | 📋 Planned |
| 体验 | 额度告警通知推送 | 📋 Planned |

## ❓ FAQ

<details>
<summary>API Key 存在哪里，安全吗？</summary>

明文存储在 `%APPDATA%\com.ollamabar.app\config.json`（macOS 为 `~/Library/Application Support/com.ollamabar.app/`）。这是单机自用场景下的刻意取舍——请只在该机器上使用可随时撤销的 Key。

</details>

<details>
<summary>重置时间什么时候标"预计"？</summary>

2026-10 起 Ollama Cloud 官方 `/api/balance` 接口直接返回服务端重置时间 `resets_at`（UTC），倒计时以它为准、不标"预计"。仅当服务端未返回该字段时，才回退到客户端推算（每周窗口在周一 00:00 UTC 重置，5h 窗口按 5 小时 Unix 时间桶边界推进）并在界面标注"预计"。服务端重置时间超过宽限期仍未推进时，应用会自动告警。

</details>

<details>
<summary>和 CodexBar 是什么关系？</summary>

形态参考（托盘用量条），但 OllamaBar 聚焦 Ollama Cloud 单一场景、跨 Windows/macOS、原生支持多 Key 与历史图表。CodexBar 是 Swift 实现的 macOS 专用多 provider 菜单栏工具。

</details>

<details>
<summary>历史曲线为什么是锯齿形的？</summary>

因为数据本来就是这样的：用量是当前窗口内的累计份额，窗口重置后归零重来（新计费套餐为计费周期内累计，周期末归零）。OllamaBar 不做平滑修饰，锯齿是数据的真实形状，虚线竖线标记重置边界。

</details>

<details>
<summary>模型调用统计去哪了？</summary>

2026-10 接口重构后，Ollama Cloud 官方将"按模型统计"列为 Coming soon，新接口暂不提供每模型明细。OllamaBar 不把缺失数据当成零次调用，也不伪造统计，模型区域显示明确的占位提示；升级前已入库的真实模型数据保留在 SQLite 日志与导出中，官方恢复该能力后会自动重新展示。

</details>

<details>
<summary>macOS 版怎么构建？</summary>

在 macOS 上 clone 仓库后 `npm install && npm run tauri build`。需要 Xcode Command Line Tools；产物为 `bundle/dmg/` 下的 dmg。也可以推送 `v*` 标签让 GitHub Actions 自动构建并发布 Release。

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
