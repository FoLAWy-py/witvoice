# 多角色执行制度

## 1. 真正的角色与实际运行能力

当前官方Codex文档支持项目`.codex/agents/*.toml`，每文件定义name、description、developer_instructions；全局并发放在`.codex/config.toml`的`[agents]`。本包采用核实时文档格式，实际安装版必须验证，不假定用户客户端已升级。[S01][S12]

M0先记录`codex --version`（使用CLI时）与实际客户端信息，确认角色能被加载并实际spawn。创建角色文件不代表它们已经运行。若当前会话启动后才添加配置，且该客户端要求重开会话，保存状态后说明准确原因，不假装热加载成功。

兼容顺序：
1. 原生自定义subagent可用：直接用指定role派发，记录真实agent/thread ID。
2. 可spawn但不能读自定义role：使用真实内置worker/explorer，并把对应role文件作为任务约束显式传入，记录`builtin_with_role_instructions`。
3. 无spawn能力：用同样角色职责顺序执行并记`serial_role_emulation`，或用户在独立worktree启动多个真实Codex会话。不能谎称发生并行委派。

不要修改用户全局Codex配置/模型/授权，不启用危险sandbox选项。项目配置不是安全边界，父会话权限与本机策略仍约束子agent。[S01]

## 2. 七个角色

| 角色 | 负责 | 禁止 |
|---|---|---|
| leader | 范围、任务DAG、契约合并、根配置、资源/文件锁、证据与交付门 | 同时spawn第二leader；代替用户签收；给自己的高风险实现自审通过 |
| frontend | feature化React、真实状态接入、组件测试、Tauri表现层 | 直接处理音频/模型；自行写安全状态机；生产mock回退 |
| backend | Node、Session、QUIC/配对、存储/观测与进程安全 | 把阻塞网络塞回audio callback；碰未经授权硬件 |
| audio_runtime | Windows/Mac音频API、SPSC/ASRC、路由、性能与硬件验证 | 自研驱动；伪造硬件测试；与别的agent争设备 |
| ml_engine | 单模型可行性、worker、profile、backend验证、模型资产清单 | 同时集成多引擎；默认Mac支持；任意checkpoint加载 |
| hci | 交互图、tokens、完整状态、键盘/缩放、实际截图审查 | 借“产品感”扩张商城/账号；替实现者改状态契约；替用户试听签字 |
| reviewer | 独立只读审查、复现建议、风险与证据审查 | 修改被审实现；对未执行测试给PASS；无依据阻塞或纯风格拉扯 |

leader是主会话角色。`leader.toml`作为可复用定义提供，但不要求spawn它。其说明由启动prompt明确加载。

## 3. 并发和worktree

同时最多4个子agent，最多2个写代码的agent；reviewer/hci可以与非冲突实现并行。GPU、物理mic、虚拟audio端点、正式性能测量各只允许一个租约。

写入型任务优先单独Git worktree和分支，例如`task/T017-quic`。必须确认当前runtime真能让该agent在该worktree执行，不把“切一个目录”当已隔离。没法证明隔离则共享树串行写，或完全不重叠路径+leader锁。所有根依赖和lockfile变更集中整合，禁止多角色同时更新依赖。

每任务开工前登记`active_tasks`、agent ID、路径和资源lease。实现完成停止子agent写入，再由leader整合并在合并后的commit重跑测试。分支上的PASS不能直接代替整合后的PASS。

## 4. 任务派发格式

使用`docs/templates/TASK.md`。任务必须有：REQ IDs、当前milestone、目标、允许/禁止路径、输入契约、验收命令、必须证据、资源锁、停止/阻塞条件。

子agent的返回最少包含：实际完成/未完成、修改文件、真实运行命令+结果、证据路径、风险、所需leader动作。不得只说“已完成，请合并”。

## 5. 评审与测试

实现者不能给同任务最终批准。reviewer在冻结commit上审查源、测试和证据，可提供精确复现命令。只读sandbox下若测试需要写target/cache，由leader在隔离测试目录执行并给完整输出，reviewer不擅自提升权限。

S0：原声/敏感数据泄漏、未经授权执行、严重破坏/密钥泄漏。
S1：必需链路错误、崩溃、无限延迟、鉴权失效、硬门槛失败、虚假验收。
S2：非核心但有实际影响的问题；若违反必需需求仍不能延后。
S3：小型一致性/可维护性建议。

所有S0/S1必须修复或任务Blocked。每项发现给文件/行号、影响、最小复现、建议；不以无限“也许可以更好”阻止关闭。

## 6. 账本与中断

`STATE.json`只存当前状态；`TASKS.json`为固定任务；`DECISIONS.md`记简短决策索引；复杂决策独立ADR；`BLOCKERS.md`写可行动阻塞；`HANDOFF.md`保存恢复入口。

每次合并和里程碑变化后更新账本。上下文将尽/额度不足/权限等待时先保存真实结果再结束。下次从next_task继续，不重跑已经有效的探索，不依靠聊天记忆。

## 7. 模型和effort

本包不硬编码用户账号可能无权限的Codex模型，默认继承主会话。建议leader、协议/实时音频设计、reviewer使用当前账号可用的较强推理配置；常规frontend和确定性修复使用适中配置。不是所有任务都要最高effort。

要分别固定模型时，先确认安装版支持的model ID/effort，再添加到对应role TOML，记录M0证据。不要沿用过时博客里的旧字段，不把文本角色文件当会自动调度的运行框架。
