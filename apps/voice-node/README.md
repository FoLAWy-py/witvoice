# apps/voice-node

T006 首个控制切片：真实 Rust 普通用户进程，拥有 `witvoice-session::Runtime`，UI 控制连接退出不会销毁该所有者。Node 拒绝 elevated token。本切片不打开输入/输出设备、LAN、worker 或模型，不公布 Ready/Running 能力。

启动契约：`witvoice-node --endpoint <1..32 ASCII 字母/数字/连字符 tag> --bootstrap-stdin`。可信启动器用 `Secret::generate()` 生成 32 字节系统随机令牌，经私有继承 stdin 传给 Node，关闭 stdin；启动器持有 Child PID 和令牌，再交给获批准控制客户端。禁止把令牌放在命令行、环境变量、日志或仓库文件。没有完整令牌，Node 在 3 秒内失败退出。尚未实现 Tauri 启动器和 Job breakaway；不能将此 CLI 当作完成的桌面产品启动流程。

客户端使用 `PipeClient::connect(tag, launched_child_pid, timeout)` 校验服务端 PID/SID，再 `authenticate(secret)`。私有命名管道在 Windows 对象命名空间中，exact-user DACL 代替文件目录权限；其名字、用户名和秘密不输出到 Node 日志。每个连接只承载一个控制请求：32 字节令牌、4 字节大端长度、UTF-8 contracts JSON；响应同样长度前缀，客户端读取完整响应后回传单字节 `0xa5` ACK。无媒体通道。单实例/单连接、64 KiB 帧上限、客户端连接与服务端连接总期限各最多 3 秒，超长/空帧在分配 payload 前拒绝。响应 ACK 防止关闭管道时丢弃未读响应；不用阻塞的 FlushFileBuffers。

`GetState` 可实际读取状态；`PrepareSession` 因资源未接入转为 Blocked 并报 ENGINE_NOT_READY；Start/Unmute 不能启用音频。其他尚未接入的命令同样拒绝，不返回虚假能力。Stop 将控制所有者恢复 Idle。控制命令的幂等和有界历史见 session README。

真实验收命令（Windows 普通用户，无采集/GPU/LAN）：

```powershell
. tools/dev/toolchain-env.ps1
cargo fmt --all --check
cargo test --locked -p witvoice-session -p witvoice-platform -p witvoice-node -- --test-threads=1
cargo clippy --locked -p witvoice-session -p witvoice-platform -p witvoice-node --all-targets -- -D warnings
```

`tests/windows_process.rs` 启动真实 Node 和独立 `ipc-probe` 客户端进程，测试身份/令牌/长度/碎片帧、重复请求、单实例抢占、单连接上限、部分帧期限及客户端死亡后 Node 仍活；不以 stub/mock Node 代替。测试退出时回收它启动的 Child。`ipc-probe` 是开发验收程序，无音频和模型能力。

NOT_RUN / T006 未完成：worker 的 Node-owned Job 监督、worker 崩溃/重启、Tauri 用户启动器与 UI Job 隔离、实际音频资源/lease/睡眠故障与停止后不录音、不同 Windows 用户实际连接拒绝、Mac 真机。SID、ACL、PID 与启动令牌不能保证抵抗已完全控制同用户进程/管理员的攻击者；令牌及 idempotency 历史仅驻留本次 Node 内存。
