# crates/platform

T006 首个 Windows IPC/凭据切片，音频 API 仍属 audio_runtime。仅依赖已锁定 Windows API crate 与权威 contracts，无 Node 网络端口、驱动或系统服务。

`PipeServer` 使用 exact current-user SID 的 protected DACL、non-inheritable handle、FILE_FLAG_FIRST_PIPE_INSTANCE、PIPE_REJECT_REMOTE_CLIENTS、单实例/单连接。双方通过 Windows 获取对端实际 PID 并查询进程 token SID，客户端另外比对其可信启动器记录的 Node PID；服务端再核对启动器通过私有 stdin 交付的 32 字节 BCrypt 系统随机令牌。客户端用 identification-only SQOS，不让管道服务端冒用客户端身份。

所有控制读写是 OVERLAPPED + 连接总期限。超时取消特定 I/O，并在释放 OVERLAPPED/event/buffer 前观察取消完成；空闲服务器仅等待连接，不保留媒体/请求队列。每帧大端长度限制复用 contracts 64 KiB，分配前检查；响应完成 ACK 和断连均受期限约束。没有音频 callback 或媒体路径。管道是内核对象，ACL 是它的私有访问边界，不创建共享磁盘 socket/token 文件。

`ProcessJob` Worker 使用匿名、不可继承、KILL_ON_JOB_CLOSE、最多 8 个进程的 Job，并禁止 breakaway。CreateProcessW 的程序名明确为可信绝对路径，不使用 shell；最多 16 个参数/总计 4096 UTF-16 单元。程序先挂起，经 Job 指派与成员验证后才 ResumeThread；失败先终止并等待，资源错误不会放行用户代码。STARTUPINFOEX handle list 只传所需 stdio，Job/进程/线程/父端管道句柄不继承。秘密仍只经固定 32 字节私有 stdin，不入 argv/env/log。平台 API 仅供内部可信调用，控制 IPC 不接任意程序/参数。

Node launcher 显式请求 breakaway，并在执行前验证分离。无已知 UI witness 时要求 Node 不在任何 Job；有 witness 时核验 parent member/child nonmember。调用者必须知道 witness 覆盖全部 UI-owned 生命周期；不能以一个任意 witness 证明未知 UI-owned 祖先链独立。可保留宿主祖先 Job 的保护。开发 UI 命名 Job 的 QUERY-only witness 在报告 Node PID 前关闭，测试 root 保留唯一 owner；重复名字拒绝，绝不更改已存在 Job 的限制。worker Job 始终匿名。`Process` 的 Drop 仅关闭过程句柄，UI 析构不能误杀 Node；测试清理使用独立的显式 TERMINATE 权限句柄。

原生行为由 voice-node/tests/windows_process.rs 与显式 process-tests 的 supervision.rs 真实跨进程检验。后者检查实际 Job PID/镜像、严格 UI 1 个/worker 2 个进程及逐个终止观察。Windows GUI subsystem probe 避免隐藏 conhost 子进程，首轮发现 conhost 与一次测试清理权限缺陷的失败记录须保留。NOT_RUN：实际不同用户拒绝、管理员/已完全控制同用户进程防护、真实 Python worker 监督协议、Tauri 组合、Mac IPC。不要把 same SID 校验描述为针对所有同用户恶意进程的完整隔离；不输出 SID、令牌、payload 或用户路径到 Node 错误日志。
