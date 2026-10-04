# 实时语音变声器：工程规格与 Codex 开工包

**规格基线：2026-10-04｜内部项目标识：voice-changer｜目标：Windows 优先开发，Windows ↔ Apple Silicon Mac 局域网双向处理。**

这个包是工程约束、任务账本与 agent 配置，不是已完成的应用，也不包含模型权重、驱动、签名证书或用户录音。包内应用子目录只有职责说明，Codex 需要真正实现、运行和验收。

## 从这里开始

1. 将本包内容放入一个新的项目根目录。已有仓库时只做非破坏性合并，不覆盖已有 `.codex/config.toml`、`AGENTS.md` 或业务代码。
2. 在 Windows 原生环境打开 Codex。先让它读取 `docs/SPEC.md`、`docs/PROJECT_PLAN.md`、`AGENTS.md`。
3. 粘贴 `prompts/START_WINDOWS.md`。默认交付终点是 M5 / `WINDOWS_DELIVERED`；有实际 Mac 访问能力时，再用 `prompts/CONTINUE_MAC.md` 完成 M6。
4. `.codex/agents/*.toml` 提供七个角色。主会话就是 leader，不再另起一个 leader 与自己竞争。当前客户端不支持自定义角色时，按 `docs/AGENT_WORKFLOW.md` 做明确记录的兼容处理。

## 关键文件

| 文件 | 用途 |
|---|---|
| `docs/SPEC.md` | 产品范围、不可破坏原则、32 项需求 |
| `docs/spec/01-architecture.md` | 进程、目录、平台抽象、状态与接口 |
| `docs/spec/02-audio-engine.md` | 音频时钟、缓冲、故障静音与实时安全 |
| `docs/spec/03-model-runtime.md` | 单一真实模型、兼容性证明、worker 与音色资产 |
| `docs/spec/04-lan-protocol.md` | QUIC、双向配对、媒体格式、期限与会话隔离 |
| `docs/spec/05-ui-hci.md` | 组件化 UI、交互、可访问性与视觉验收 |
| `docs/spec/06-security-distribution.md` | 权限、数据、安装包与供应链边界 |
| `docs/spec/07-tests-acceptance.md` | 数字门槛、真机证据、结束条件 |
| `docs/PROJECT_PLAN.md` | **固定 7 个里程碑、35 个顶层任务** |
| `docs/AGENT_WORKFLOW.md` | leader / frontend / backend / audio_runtime / ml_engine / hci / reviewer |
| `docs/project/STATE.json`、`TASKS.json` | 初始真实状态与任务账本，全部尚未完成 |
| `prompts/START_WINDOWS.md` | Windows 开工 prompt |
| `prompts/RESUME.md` | 换会话后的恢复 prompt |
| `prompts/CONTINUE_MAC.md` | Mac 真机与双向 LAN 验收 prompt |
| `docs/references/SOURCES.md` | 核实过的官方资料与适用边界 |

## 交付边界

- M0–M5：Windows 原生本地真实变声、第三方虚拟麦克风路由、局域网协议的真实进程实现与可复现测试、完整 UI、可安装内部测试包。
- M6：Mac 原生构建、真实模型处理、真实虚拟音频端点，以及两种 Windows ↔ Mac 闭环各自通过实机验收。
- 没有 Mac 证据时，最多称 `WINDOWS_DELIVERED`，**不能称整个项目完成**。
- 本轮不自研内核/音频驱动，不同时集成三种 VC 模型，不做云账户、商城、付费、WAN 穿透、训练平台和移动端。
- 所有性能数字是**项目拟定验收门槛或预算**，不是已经在用户机器上测出的结果。

## 工具包自检

本包提供一个只验证文档/账本/角色配置一致性的脚本：

```powershell
python tools/project/validate_spec.py
```

该脚本通过不代表应用构建、真实音频、模型或 Mac 兼容性已经通过。`tools/dev/doctor.ps1`、`tools/qa/run-windows.ps1` 等应用工程脚本是 Codex 在对应任务中必须创建的交付物，而不是本包已经实现的功能。
