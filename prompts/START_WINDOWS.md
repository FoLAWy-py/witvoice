你现在是这个仓库的主开发负责人（leader），目标是在真实Windows环境按本仓库规格实现一个独立的实时语音变声器。不要只继续讨论技术选型或重写文档：完成必要核查后，立即从最小可验收任务开始真正实现、运行和测试。

## 权威材料与边界

先读取：
1. AGENTS.md
2. docs/SPEC.md
3. docs/PROJECT_PLAN.md
4. docs/AGENT_WORKFLOW.md
5. docs/project/STATE.json、TASKS.json、HANDOFF.md、BLOCKERS.md
6. 当前任务需要的docs/spec章节和docs/roles文件。

本轮默认执行到M5 / WINDOWS_DELIVERED。完整产品仍必须在M6完成真实Mac与双向LAN验收；没有证据不能宣布COMPLETE。任务只有M0–M6和T001–T035，不能自动扩张为无限里程碑、重构、商城、云后台或自研驱动项目。

产品必须保留这三条链路：
- Windows物理麦克风→本地真实VC→Windows虚拟输入；
- Windows物理麦克风→同LAN的Mac计算→返回Windows虚拟输入；
- Mac物理麦克风→同LAN的Windows计算→返回Mac虚拟输入。

## 先做的事情

检查现有目录与Git状态，保护已有文件和用户未提交改动；不要reset/clean或重建整个仓库。
检查当前Windows原生工具链、Codex版本与权限。不要把WSL/Linux编译通过当Windows音频验收。
先运行本包的文档自检；再实现T001所需的真实doctor。不要把尚未创建的脚本说成已经执行。
在M0验证一个真实zero-shot模型、上游版本、模型摘要、依赖和实际资源；MeanVC2为优先候选，按有限探索预算工作。论文延迟不是本机数据；MPS/CoreML存在不证明模型支持Mac。

## 多角色开发

主会话直接承担leader，读取docs/roles/leader.md；不要再spawn一个leader。
使用真实可用的子agent机制建立frontend、backend、audio_runtime、ml_engine、hci、reviewer。先验证.codex/agents配置能加载，记录真实agent/thread ID；客户端不支持时按AGENT_WORKFLOW明确记录兼容方式，不能假装并行发生。

最多4个并发子agent、最多2个写代码的角色、最多1个GPU/音频硬件测试。写入任务优先隔离worktree；没有可靠隔离就串行或严格不重叠路径。子agent不得递归spawn。

每个子任务给出Task ID、REQ IDs、路径白名单、禁止改动、契约、验收命令、证据、资源锁与停止条件。leader单点管理root manifests、lockfiles、contracts、项目状态。reviewer必须独立且只读，不批准自己的实现。HCI先给交互/token/状态约束，再检查真实桌面，不仅写“漂亮”。

## 实现约束

React/Tauri只负责界面和控制；Rust voice-node独立运行；Python只做选定引擎推理。音频callback不得等待UI/Python/网络、做常规分配、写文件或打印日志。
任何故障、断线、未知模型、旧epoch、队列过期均静音，绝不能自动输出真实原声。所有队列有界；源时间线、epoch与测量边界必须明确。
本地先受限二进制IPC；网络按SPEC用可靠控制+QUIC Datagram；配对双向验证身份，mDNS不是信任。
本轮只完成一个真实zero-shot模型；不同时做三引擎、不强制全部Rust/ONNX化。Windows/Mac虚拟音频用用户独立安装的外部设备，先实现检测和路由，不擅自捆绑或自研驱动。

## 授权和证据

不得擅自关闭安全设置、提升权限安装驱动、开放防火墙、上传录音、下载大模型、使用付费API或发布仓库。真实需要时说明最小必要操作；可独立推进的固定任务继续完成。
普通可逆实现细节由你依据SPEC判断并写ADR，不要反复问我已经明确的问题。需要改变需求、降低门槛、涉及系统/隐私/许可授权的事项必须明确处理，不能自作主张。

每个任务实际跑适用测试，提供命令、退出码、环境、产物和独立review。mock、IdentityEngine、浏览器截图、论文数字、Windows双进程协议不能替代真实模型、虚拟麦克风、Mac或LAN真机证据。未知/未执行标UNKNOWN、SKIPPED或BLOCKED，不能PASS。

## 有限推进与结束

按TASKS依赖推进；每任务最多3轮正式review修复，模型候选预算见SPEC。预算耗尽或外部条件缺失时报告可复现阻塞，不无限盲试，不删测试、不调低阈值。
每次整合更新STATE/TASKS/HANDOFF。接近本轮上下文或额度边界时保存可验证进度、停止子agent写入、释放资源、给出精确next_task；不要宣称后台会继续。

M0–M5全部真实通过后生成Windows交付报告，状态设WINDOWS_DELIVERED，明确M6尚未完成，然后停止新增功能。若我明确授权继续且你实际可访问Mac，再执行M6；只有所有必需真机证据通过才能COMPLETE。

现在从T001开始。先简要报告实际环境与当前任务分工，然后立即执行可做的工作，不停留在计划书。
