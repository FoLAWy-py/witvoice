# T007 设备变更通知切片派发

实际角色/ID：audio_runtime / `/root/audio_runtime`；leader `/root`。基线 `43f0459d0bc06b2cd9581ddaad47ab037ebf6443`；已满足 T003。REQ-06/REQ-08，M1；整任务仍 IN_PROGRESS。

目标：补齐原生 IMMNotificationClient 注册/注销与普通控制线程上的设备失效通知，不开启录音或播放。允许 `crates/audio/src/`、`crates/audio/tests/`、`crates/audio/README.md`；禁止根 manifest/lockfile/contracts/状态账本、其他 crate。资源仅 `code-writer:audio-notifications`，无 GPU/audio lease。共享树其他成员只读/整合账本；不得回滚他人代码或递归 spawn。

先读 audio AGENTS、SPEC02/07、现有 wasmapi/转换代码和正式 T007-slice-r1。保留 PCM24/32 扩展格式修复。Windows 0.62.2 API 实际可用性需核对官方文档/已安装生成代码；需新增 feature 则先向 leader 报准确 manifest 需求，不自行改依赖。

原生通知回调只发布有界失效信号，不查设备/阻塞/分配/日志/调用音频 API；常规线程处理重新枚举/明确 UID 校验，禁止选择默认或同名替代。可以采用原子 coalesced-dirty，不必引入媒体队列；通知不是音频帧/期限证明，真正输出治理留 T008。注册/注销 RAII、线程/COM 生命周期正确，不让 panic 穿过 COM FFI；不能从回调中注销。

验收：`cargo test -p witvoice-audio --locked`、`cargo fmt -p witvoice-audio --check`、`cargo clippy -p witvoice-audio --locked --all-targets -- -D warnings`。纯单测覆盖通知失效/多事件合并/处理期间新事件不丢；如能合理测无回调分配则给真实 allocator 证据。NOT_RUN：实际设备拔出、Initialize/Start、采集/播放、Mac。不要把 synthetic notifications 当硬件验收。

保存全部真实命令、stdout、退出码、UTC、文件清单、未做项和风险，交 leader 归档 `docs/evidence/T007-20261005/notifications/`。交付停止写入，leader 冻结/整合测试后独立审查。预算沿固定任务正式最多三轮，当前只一轮首切片，不自行新建顶层任务。
