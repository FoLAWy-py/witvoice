# T006 进程监督与显式退出派发

实际 owner backend `/root/backend`，整合 leader `/root`；共享树，不另建 leader/不递归 spawn。开始须等 T002 r2 中继完成及 leader 登记代码 lease。依赖 T003/T005 DONE；M1 / REQ-07、REQ-23。契约基线 `43f0459d0bc06b2cd9581ddaad47ab037ebf6443` 新增本地 ExitNode；leader 已核已安装 Windows0.62.2 并开启 platform `Win32_System_JobObjects`，未加外部依赖。

允许 apps/voice-node/src、tests、README；crates/session/src、tests、README；crates/platform/src、tests、README。禁止 contracts、根/Cargo manifests/lockfile、状态与证据账本、audio、worker；如需新 feature/bin manifest，先报告 leader。其他成员 audio 只写 crates/audio；不覆盖他人改动。

读根/相关 AGENTS、SPEC01/03/04/06/07、ADR0002、现有 IPC/session 全源码。实现非继承、Node-owned KILL_ON_JOB_CLOSE worker Job；进程先挂起、归 Job 成功才执行，失败杀并回收，不允许整个树逃逸。受限必要继承句柄，秘密仍不进入 args/env/log；固定可信程序/参数内部调用，IPC不接受任意可执行路径或shell。Node launch 与 UI Job 分离（支持 breakaway 时显式使用并核实），不允许时精确报错，不后台服务/UAC/global policy 变更。

ExitNode 通过既有本地鉴权/版本/幂等/冲突验证后，先失效会话/epoch/输出，再停止新请求并清理 Job。历史容量满/ACK消失不能阻止已接受的安全退出；同ID不同命令不能绕过冲突。保持 StopSession 仅停止会话，UI断开不退出。没有真实模型/设备不能Ready/Running；不伪造worker生产接入或音频租约。真实 Python warmup/heartbeat/PCM/retry依赖 T011，当前切片测试进程所有权。

真实进程测试：普通非管理员用户；父UI-like进程在真实 KillOnClose Job中启动独立Node，关Job/杀父后Node仍在且可鉴权查询；不允许breakaway的Job则启动失败且无遗留。worker子树在Node-owned Job，关闭/显式停止/owner被杀后全部回收；实际ExitNode后进程终止、端点不可再连接。bootstrap坏/超时/IPC失败不得遗留已启动子进程。辅助程序只是受测试入口约束的进程探针，不选为生产引擎；不把Windows进程probe当Tauri/Mac/音频验收。

验收命令 cargo fmt --all --check；cargo test --locked -p witvoice-session -p witvoice-platform -p witvoice-node -- --test-threads=1；cargo clippy --locked -p witvoice-session -p witvoice-platform -p witvoice-node --all-targets -- -D warnings。保存真实UTC/argv/完整stdout/exit/进程回收观测（PID不写user SID/token），leader归档 docs/evidence/T006-20261005/supervision。每task正式最多3轮review，当前T006尚0轮。完成后停止写入、交风险/未做项给leader冻结，随后独立review。

禁止录音/播放/GPU、驱动/防火墙/系统服务/持久环境修改、上传。只有代码写入lease，普通用户测试进程不需要硬件lease。需额外系统权限或跨用户证据时报告BLOCKED，不修改OS保护。
