# 01 · 架构、目录与接口

## 1. 进程拓扑

```text
Desktop UI (React / Tauri)
          │ 控制与低频状态；不携带 PCM
          ▼
voice-node (Rust，独立用户进程)
  ├─ Session / Device / Profile / Model / Peer managers
  ├─ Capture → DSP → bounded queue
  ├─ LocalProcessor → 独立 ML Worker
  ├─ RemoteProcessor → QUIC → 对端 Node → ML Worker → QUIC
  └─ processed audio → output governor → virtual cable render endpoint
                                               └─ optional headphones monitor
```

Node 与 UI 生命周期必须分开。Node 不因 WebView 被杀而被 UI 的 Job Object 一起清理；Windows 的 worker Job Object 由 Node 拥有。Mac 同样不能把音频引擎绑定到 WebView 生命周期。UI 断开只影响控制，不立即终止已明确启动的会话；托盘/菜单状态必须仍可知。关闭窗口与“退出并停止”分成两个动作；首次关闭时解释差异。

Node crash 后不自动开麦；worker crash 后先静音，可以重新加载但恢复采集/传输遵循已确认会话与明确恢复策略。计算机睡眠后会话失效，唤醒不能自动传送新采集语音。

不创建系统级常驻服务，不要求应用日常管理员/root 运行。不默认开机自启。远端计算模式不打开计算机自身的麦克风/扬声器。

## 2. 按需求划分的目录

```text
apps/
  desktop/
    src/
      features/
        dashboard/           # Start/Stop、Mute、路由与当前状态
        voices/              # 参考音频、创建、预览、删除
        devices/             # 物理输入、虚拟输出、监听与设置向导
        processing/          # 本机/远端、能力与选择
        peers/               # 发现、配对、信任与撤销
        diagnostics/         # 指标、问题定位、脱敏导出
        settings/            # 隐私、外观、存储
      components/            # 共享视觉基元和业务组件
      lib/                   # API client、类型、格式化，不放业务状态机
    src-tauri/               # 窄命令转发、权限、窗口/托盘
  voice-node/
    src/                     # 组合根、启动参数、shutdown、manager 装配
crates/
  contracts/                 # 类型、错误码、版本、协议 schema 的权威来源
  audio/
    src/{capture,playout,dsp,clock}/
  engines/
    src/{local,remote,profiles,registry}/
  transport/
    src/{quic,discovery,pairing,framing}/
  session/                   # 单会话状态机、命令幂等、epoch、资源租约
  platform/
    src/{windows,macos}/    # 音频 API、凭据、进程、权限/电源事件
  storage/                   # SQLite、迁移、manifest、受限文件 I/O
  observability/             # 无阻塞统计快照与日志/诊断脱敏
workers/
  vc_worker/
    adapters/                # 只实现 M0 选定的一个真实引擎
    protocol/                # Python 端二进制协议与生命周期
    tests/
assets/manifests/            # 非秘密、可验证的引擎资产描述
packages/ui-tokens/          # token 权威定义；UI 引用，不复制
tests/
  contract/ integration/ security/ network/ desktop/ hardware/ fixtures/
tools/
  dev/ qa/ packaging/ project/
docs/
  spec/ design/ project/ roles/ templates/ adr/ evidence/ references/
```

`tests/` 是根目录。目录按实际实现创建；概念模块优先放入现有 crate，仅在拥有清晰 API/测试边界时拆 crate。不为每个名词创建一个微服务。

依赖方向：apps → domain crates → contracts/platform adapters。contracts 不依赖 UI、Node 或第三方模型。禁止 audio 反向依赖 Tauri。前端按 feature 汇聚页面，不建一个几千行 `App.tsx`。

## 3. 控制契约

Rust contracts 是权威定义，生成 TypeScript schema/类型；Python 绑定有黄金样本进行一致性测试。不得三种语言各自手写同名但语义不同的消息。

必须至少定义：

```text
ListDevices / GetCapabilities / GetState
CreateVoice / DeleteVoice / PreviewVoice
PrepareSession / StartSession / StopSession / SetMute
SelectRoute / SetMonitor / SetGain
ListPeers / ExportIdentity / ImportPeerIdentity / RevokePeer
InstallEngineAssets / GetMetrics / ExportDiagnostics
```

每条变更命令携带 request_id；响应包含状态版本。后端返回稳定错误码与可读信息，而不是让 UI 匹配异常字符串。Prepare 不开麦、不发送 PCM；Start 要求资源全部 Ready 后由源端用户明确触发。

## 4. 状态机

```text
Idle → Preparing → Ready → Running → Stopping → Idle
              ↘ Error/Blocked
Running → Degraded → Running 或 FailedMuted
Running → Muted → Running（仅同一仍有效会话内的明确用户操作）
任意状态 → Stopping；销毁后该 session 不再接受媒体。
```

Preparing 包含设备、权限、模型、音色、peer trust 与资源预热。任何失败都没有隐式直通。

Mute 时停止向远端发送新语音；允许短时保持已加载模型，输出为零。Stop 则关闭采集、停止 session、释放计算租约并清理队列。

M0 冻结转换表与所有边界状态，后续新增状态必须补迁移和测试。

## 5. 音频/引擎接口

不要强制同步 `process(chunk) -> chunk`。引擎可能需要不同数量输入帧、上下文与 lookahead，并分批吐出输出。

逻辑契约：

```text
prepare(engine_artifact, voice_profile, requested_config) -> PreparedCapabilities
start(session_id, epoch) -> ReadyAck
try_submit(InputFrame) -> Accepted | Backpressure | Failed
poll_output(preallocated_destination) -> ProducedFrame | Pending | Failed
reset(new_epoch, reason) -> ResetAck
stop() -> StopAck
```

这些方法不在硬实时回调里调用。Audio capture/playout 只与预分配本地环形缓冲交互。

Capabilities 至少记录：引擎 ID、模型摘要、实际后端、native input/output rate、有效 chunk/lookahead、profile schema、是否 duration-preserving、通过的测试 run ID。输出同时给出对应 source timeline span；不能假设模型输入输出采样率相同。

LocalProcessor 与 RemoteProcessor 共用 Session 的非阻塞语义，但不要求它们具有相同内部线程或延迟。Compute 只执行一个本地引擎，不把远端请求再转发到第三台机器。

## 6. 共享文件和资源所有权

根 `Cargo.toml`、依赖 lockfile、contracts、schema、`.codex/`、项目状态由 leader 单点整合。各角色通过任务请求修改，不并发覆盖。

GPU/物理麦克风/虚拟音频设备各有显式测试资源锁。同一设备同一时间最多一个硬件 benchmark；否则性能报告作废。
