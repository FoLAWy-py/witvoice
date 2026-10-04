# ADR 0002 · 本地 Node 显式退出

状态：T006 实现决定，独立审查待；补齐 REQ-07/REQ-23 及 SPEC01/05 已要求的退出行为，不新增任务或外部权限。

现有 `StopSession` 只停止会话，不能实现“退出并停止”。在 Rust 权威本地 `Command` 中补 `ExitNode`，无参数；保持 v1 帧格式、长度限制、当前用户 IPC 鉴权及 request_id 语义。尚无已交付旧客户端；遇到不支持该命令的旧 Node 必须显示错误，不能把 Stop ACK 当退出成功。TypeScript/schema 从 Rust 重新生成，不手工维护另一套协议。

`ExitNode` 不进入 `PeerMessage`，远端无权终止本地 Node。鉴权、版本/字段验证和 request_id 冲突检查之后，Node 才接受退出；退出不因响应 ACK 丢失而撤销。先失效输出和会话 epoch，停止接收新请求，释放会话/worker Job 及监听资源，再退出。历史容量已满不能阻止安全退出；返回结果必须如实区分是否已经开始退出。窗口关闭/控制断开不等于该命令。

Windows worker 在 Node 拥有的非继承 Job 中运行，创建时先挂起、成功归入 Job 后恢复，Job 关闭杀掉整个进程树。普通用户 UI 启动 Node 要处理 UI Job 隔离；若父 Job 不允许必要 breakaway，应拒绝并说明，而非暗中依赖 UI 生命周期。真正 Tauri 接入仍属固定任务 T025。

这里只记录决定与契约。具体进程实现、真实退出/树回收、UI Job 隔离测试以 T006 后续证据为准，不由本文件宣告通过。

实现参考已核对的 Microsoft 原始文档：[Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)、[AssignProcessToJobObject](https://learn.microsoft.com/en-us/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject)、[Create processes](https://learn.microsoft.com/en-us/windows/win32/procthread/creating-processes)。继承必要句柄应使用明确 handle list，Job 句柄本身不可继承；不能用“先运行再 assign”留下子进程逃逸窗口。
