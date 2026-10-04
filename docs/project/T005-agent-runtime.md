# T005 · 实际多角色与兼容执行记录

主会话实际ID /root，直接负责leader。实际CLI codex-cli 0.153.4，来源T001 doctor版本探测；desktop客户端构建版本UNKNOWN，不把CLI版本当desktop版本。六种角色曾由真实collaboration工具派发，未另建leader、未递归spawn。canonical ID由工具返回，UUID未暴露。

| 角色 | 实际ID | 本轮实际工作 | 尚未证明 |
|---|---|---|---|
| frontend | /root/frontend | frontend角色注入探测；T005规则/配置全文只读核对 | 真实UI、磁盘配置加载因果关系 |
| backend | /root/backend | 架构契约建议；冻结T003及T005/T002文本只读中继 | Node/IPC/QUIC实现 |
| audio_runtime | /root/audio_runtime | 角色探测；实际编写T004六文件；冻结源码与日志中继 | 原生设备/虚拟线硬件运行 |
| ml_engine | /root/ml_engine | 真实角色探测；固定模型上游与缓存保护只读建议 | reviewer批准、Mac后端 |
| hci | /root/hci | 真实HCI角色职责注入探测 | 桌面截图、键盘/音质签收 |
| reviewer | /root/reviewer | 初始角色探测；需要显式提供审查全文 | 磁盘sandbox强制、独立复跑 |
| reviewer（后续实际审查） | /root/reviewer_t001 | T001两轮、T003/T004审查；与实现者不同ID | 直接读磁盘与独立复跑 |

已证实：可派发自定义agent_type、获得真实子agent、角色职责进入上下文。`.codex/agents/*.toml`字段原文与注入职责吻合；这不证明当前客户端从这些磁盘文件读取了完整schema。保留custom_roles_verified=false、project_file_schema_verified=false。采用native_custom_role_dispatch_disk_schema_unverified兼容方式，任务消息显式给规则、白名单、依赖、契约、命令和资源锁；不写成builtin或串行模拟。

默认shell/node在进程创建前因apply deny-read ACLs失败，无进程退出码。leader的限定命令经auto-review require_escalated可执行；leader又逐项授权backend/audio进行固定路径的只读文本中继，以及audio在独立tests/fixtures和tests/hardware白名单写T004。没有更改ACL、全局Codex配置或绕开系统安全。reviewer不自行提权、不写被审源码；审查基于其他agent/leader转交的冻结源码与完整结果，报告明确非独立复跑。跨agent functions.store不共享，读取返回STORE_UNAVAILABLE，后改文本中继。

并发控制：最多4子agent。T004写入期间只有leader和audio两个代码写入者，路径完全不重叠；共享contracts、root依赖和lockfile全部leader整合。其余子agent只读。GPU文件与paced实验由leader单独顺序执行，无其他audio/GPU测试，已释放。没有隔离worktree，不能冒称使用worktree。

本次重新唤醒旧ml_engine曾返回agent thread limit reached，记录FAILED_BEFORE_DISPATCH，未发生新的ml执行。没有为了获取更多slot而递归spawn或另建线程。后续采用现存backend只读中继给现存reviewer；不把中继叫并行模型开发。

35任务/32需求/7里程碑与TOML语法由tools/project/validate_spec.py核对；它不验证运行时安全强制、磁盘schema加载、应用或硬件。T001/T003关闭和独立审查后才能关闭T005。完整项目状态仍不允许COMPLETE。
