# crates/platform

T006 首个 Windows IPC/凭据切片，音频 API 仍属 audio_runtime。仅依赖已锁定 Windows API crate 与权威 contracts，无 Node 网络端口、驱动或系统服务。

`PipeServer` 使用 exact current-user SID 的 protected DACL、non-inheritable handle、FILE_FLAG_FIRST_PIPE_INSTANCE、PIPE_REJECT_REMOTE_CLIENTS、单实例/单连接。双方通过 Windows 获取对端实际 PID 并查询进程 token SID，客户端另外比对其可信启动器记录的 Node PID；服务端再核对启动器通过私有 stdin 交付的 32 字节 BCrypt 系统随机令牌。客户端用 identification-only SQOS，不让管道服务端冒用客户端身份。

所有控制读写是 OVERLAPPED + 连接总期限。超时取消特定 I/O，并在释放 OVERLAPPED/event/buffer 前观察取消完成；空闲服务器仅等待连接，不保留媒体/请求队列。每帧大端长度限制复用 contracts 64 KiB，分配前检查；响应完成 ACK 和断连均受期限约束。没有音频 callback 或媒体路径。管道是内核对象，ACL 是它的私有访问边界，不创建共享磁盘 socket/token 文件。

原生行为由 voice-node/tests/windows_process.rs 真实跨进程检验。NOT_RUN：实际不同用户拒绝、管理员/已完全控制同用户进程防护、worker Job 监督、Mac IPC。不要把 same SID 校验描述为针对所有同用户恶意进程的完整隔离；不输出 SID、令牌、payload 或用户路径到 Node 错误日志。
