# 官方资料与证据边界

核实日期：2026-10-04。网页以后可能变化；Codex在M0记录所用版本/commit。这里列的是设计依据，不是本项目测试结果。

| ID | 官方入口 | 仅支持的事实/用途 |
|---|---|---|
| S01 | https://developers.openai.com/codex/multi-agent/ （当前跳转到 https://learn.chatgpt.com/docs/agent-configuration/subagents） | 项目级`.codex/agents/*.toml`；name/description/developer_instructions；并发配置与权限继承 |
| S02 | https://developers.openai.com/codex/guides/agents-md/ | 分层AGENTS.md发现与合并、指令长度限制 |
| S03 | https://developers.openai.com/codex/windows/ | Windows原生Codex工作流及sandbox边界 |
| S04 | https://v2.tauri.app/develop/tests/webdriver/ | Tauri桌面E2E与renderer-only测试不同；测试driver的实现选项 |
| S05 | https://docs.rs/quinn/latest/quinn/struct.Connection.html | Datagram API、`max_datagram_size`；实际库版本仍要锁定 |
| S06 | https://www.rfc-editor.org/rfc/rfc9221.html | QUIC不可靠Datagram扩展；拥塞控制与应用层责任 |
| S07 | https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html | CoreML EP能力和算子/形状限制；不证明任意VC模型支持Mac |
| S08 | https://github.com/ASLP-lab/MeanVC2 | 原作者模型/实时入口/Windows说明/README许可声明；性能是作者报告 |
| S09 | https://github.com/Jerrister/X-VC | 原作者流式VC实现、依赖与LICENSE；本项目不默认集成 |
| S10 | https://vb-audio.com/Cable/ | 虚拟线输入输出路由、安装与许可入口 |
| S11 | https://github.com/ExistentialAudio/BlackHole | Mac虚拟音频路由、Apple Silicon支持及项目许可/商业集成说明 |
| S12 | https://developers.openai.com/codex/config-reference/ | 安装版配置核对；不能把旧博客配置当当前schema |

## 不作为已证实事实沿用的内容

此前讨论中的不同GPU内存、CPU百分比、音色优劣、某模型Mac后端、跨平台延迟均不是本项目实测。本文给的预算/门槛是产品决策，必须通过对应测试证明。

代码LICENSE不自动证明全部权重、训练数据、依赖、codec、声纹模块、图标与驱动都有相同再分发条件；需分别建立实际资产清单。

## M0需生成

`docs/evidence/toolchain-baseline.json`、`model-feasibility/`、`docs/adr/0001-baseline.md`：记录实际commit、版本、来源、许可检查、运行结果与尚未验证的平台。不能预填PASS。
