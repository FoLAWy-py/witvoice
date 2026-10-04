# 03 · 模型、音色与 worker

## 1. 选择策略

候选顺序固定：MeanVC2 优先；只有出现可复现且记录清楚的不可用/不符合目标问题，才评估一次 X-VC 替换。不并行完成多个引擎，不把 RVC 的需训练音色冒充 zero-shot。

M0 对候选最多进行两次有具体假设的兼容修复尝试；不把同样命令原样重复当研究。候选替换也只有一次。用完这些尝试仍失败，则记录 `BLOCKED_MODEL` 与证据；可继续独立 UI/协议任务，但 M2 和完整交付不能标 PASS。

MeanVC2 上游提供实时入口与 Windows CPU 构建说明，但本项目仍需复核完整依赖、实际音频质量、资源和许可证。其 README 的 110ms 是上游报告，不是本项目跨平台或 LAN 实测。[S08]

## 2. M0 必需证据

`docs/evidence/model-feasibility/<run-id>/` 保存：

- 上游 URL、commit/tag、模型文件 SHA-256、权重来源、全部依赖版本和各自许可记录；原始模型不提交 Git。
- 实际机器 OS/CPU/GPU/驱动、Python/Torch 版本；后端实际执行情况；不以检测到 GPU 当作模型用了 GPU。
- 固定授权录音输入、固定参考音频摘要；转换后文件存在且能解码；时长、采样率、峰值/RMS、非 NaN/Inf 检查。
- 短文件转换和连续流式转换；新输入分段时间作为 RTF 分母，不能把重复 context 计入新音频长度。
- 初始化时间、预热时间、实际内存/显存、p50/p95/p99 每步耗时。
- `Windows: VERIFIED/FAILED` 与 `macOS: UNTESTED/VERIFIED/FAILED` 分开。Windows 阶段 Mac 可为 UNTESTED，不编造通过。

## 3. worker 边界

Node 负责麦克风、网络、端点路由、权限与输出治理；worker 仅处理输入张量/帧、profile 与模型。控制通道使用有长度上限的结构化消息；PCM 使用独立二进制通道。启动通道建立 session token，凭据通过受限继承句柄/安全 IPC 交付，不写命令行或日志。

Windows：受当前用户 ACL 约束的 Named Pipes；Mac：私有运行目录下 0600 的 Unix Domain Sockets。部分写/读、EOF、损坏帧、超限长度都有明确处理。stdio 仅日志/进程调试，不混入二进制音频流。

Node 管理 worker 进程：warmup、ready、heartbeat、per-frame progress、memory pressure、stop、timeout、kill、bounded retry。控制心跳和媒体推进分别监测，500ms 心跳不能充当音频期限。

单故障自动重启最多两次，之后 FailedMuted，等待用户重试。自动重启不会触发原声旁路。计算任务停止时必须确保模型/GPU资源租约释放；进程退出不遗留 worker。

## 4. VoiceProfile 不等于一个通用 speaker vector

profile 是选定引擎需要的完整条件包，可能包含 embedding、参考声学特征、token、缓存或原始参考的受控引用。由 adapter 定义，不能假设保存一个 `speaker.bin` 就支持所有模型。

必须包含：

```json
{
  "profile_schema_version": 1,
  "voice_id": "uuid",
  "display_name": "用户命名",
  "engine_id": "实际选定引擎",
  "engine_adapter_version": "已锁定版本",
  "model_sha256": "实际64位hex摘要",
  "conditioning_schema": "adapter定义",
  "required_assets": [],
  "reference_retention": "deleted|local_only|explicit_remote_cache",
  "consent_confirmed": true
}
```

这只是结构示意，提交运行 manifest 时禁止保留“实际…”占位值。实际资产清单包含大小、摘要、格式，不允许任意可执行代码。

相同引擎且兼容模型/profile schema 才能复用。Mac 与 Windows 后端不同也需验证 profile 兼容性；不可从音色名称判断。

## 5. 参考音频

本轮只接收 WAV/FLAC，10–30 秒有效参考作为默认产品限制；引擎确有不同限制时在 M0 固定并同步 UI，不在运行中猜测。限制文件大小 ≤50MiB、解码后长度 ≤60 秒、声道 ≤2；转换为安全标准格式。拒绝 zip、模型 checkpoint、脚本、任意插件和路径穿越。

UI 说明目标声音须有使用授权。默认本地解析、不上传、不写长期原始录音。创建完允许删除原始参考，但若 adapter 仍需它作为运行依赖，不能假删除或无提示破坏 profile；应解释必须保留哪些派生数据。

远端处理需要将 profile 所需条件数据传给另一设备。首次针对该设备明确告知，用户批准后才传；默认会话结束删除临时副本。用户选择远端缓存时记录授权与删除入口。声纹特征同样按敏感资产处理，不能叫它“匿名数据”。

## 6. 本轮后端支持

Windows：锁定实际可用 CPU 或 CUDA 路径。Mac：M6 验证 CPU，MPS 可作为同模型的候选加速；不可默认承诺 MPS、CoreML、ANE 可运行。CoreML 有模型算子和形状约束，必须看实测分图与实际执行设备。[S07]

允许用已经验证的 Python worker完成全部里程碑。不把“后续全部 Rust/ONNX 化”当隐藏待办。优化仅在真实验收失败、有 profile/音质/状态回归证据时开展。

## 7. 真实与测试数据分离

Identity/FakeEngine 用于音频和网络基准，但只存在于测试 feature/测试配置；生产安装包不可将它显示为一个真实音色。

黄金样本至少包括：中文/英文正常语速、快语速、短停顿、长停顿、笑声、轻声、爆破音、低输入音量。不要承诺“呼吸、情绪、韵律100%保留”；由用户试听和客观稳定性数据共同验收。
