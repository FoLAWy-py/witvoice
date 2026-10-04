# ADR 0001 · 单一 Rust 契约和静音语义

状态：T003契约基线已接受，独立/root/reviewer_t001第2轮批准，冻结1e9b9d4298183c47c862febaf177ad46ced85c8d；详见docs/project/reviews/T003-T004-r2.md。依据SPEC01/02/04和实际角色建议，不代表运行时已实现。

Rust contracts 为权威类型与显式二进制解析；JSON schema 从 Rust 导出，TypeScript 由该 schema 生成，Python 复用同一媒体黄金向量。UI 不接收 PCM。local commands 与 PeerMessage 独立，LAN Start 无本地开麦语义。control length 4-byte BE，最大 65536 bytes，在分配 payload 前校验。

所有 u64 JSON 值用十进制字符串；ID 用小写 UUID 文本，摘要用小写64hex，Rust解析拒绝错误。schema描述结构，运行时校验数值范围/语义。完整鉴权/幂等缓存由 T006/T016 实现，不能将这些类型当安全 IPC 已完成。缓存按可信发起者+request_id，同ID不同payload拒绝；有界缓存满拒绝新变更，不淘汰后重执行。

媒体 v1 flags/reserved 均为0；sample_count只准协商240/480。SOURCE源索引必须等于媒体索引且不能未知。CONVERTED可用u64::MAX表示未知源索引，其延迟必须UNKNOWN。随机session_tag非零，绑定已鉴权连接/session/epoch；不是凭据。序号差2^31歧义拒绝。头BE，PCM16 LE。

纯transition表描述状态和所需guards/effects，不拥有运行时设备或模型。Mute/Stop/Fail/Reset先令输出无效并清队列；Mute/Unmute也递增epoch以拒绝迟到包。Unmute仅同有效会话明确用户操作且资源Ready；FailedMuted不能Unmute。epoch溢出失败，不能wrap复用旧代际。Node将guards与effects实际落实后才能宣称行为通过。

Capabilities只由实际测试组合创建，含真实原生采样率/模型摘要/条件schema/run ID。MeanVC2 M0文件/paced实验输入输出已实测16k，VC/speaker CUDA、ASR CPU；不能据此承诺应用性能或MPS。Prepared必填实际loaded profile摘要，与requested model/profile比较通过后才能准备Ready；session_tag须非零。建session之前Error可无context，但不因此允许无context Start/媒体。契约不提供生产测试引擎。

依赖精确固定 serde1.0.228、serde_json1.0.145、schemars0.8.22，Rust1.99.0；根lockfile由leader生成。组件文档已核版本，crates.io REST在本机返回403，Cargo实际下载结果另存证据。
