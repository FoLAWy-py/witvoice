继续这个仓库的既定工程，不重新设计项目。你是leader。

读取AGENTS、SPEC、PLAN、AGENT_WORKFLOW、STATE、TASKS、HANDOFF和BLOCKERS；检查Git状态及未合并worktree。先核验最后可信commit和证据是否仍适用。

报告当前里程碑、已完成/阻塞、下一Task ID，随后立即执行已满足依赖的任务。不要重复已有效完成的模型探索，不假定上轮agent仍运行。重新派发时记录真实agent ID和资源lease。

维持7里程碑/35任务、路径所有权、独立review、失败静音、真实性和授权规则。默认授权终点仍是STATE中的目标；WINDOWS_DELIVERED后不自动新增工作，M6需要真实Mac能力与明确继续授权。

达到目标则交付并停止；外部阻塞则给最小解阻动作；上下文边界保存HANDOFF与next_task。不能用“代码结构存在”冒充硬件/模型/双向LAN已通过。
