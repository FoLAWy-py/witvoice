# 04 · LAN 协议与安全配对

本文件定义本项目的协议约束，不声称现成库已经自动实现这些应用层行为。

## 1. 范围与传输

仅同一局域网、两台计算机、一条活跃 session，Capture=Sink。Compute 只处理网络输入，不采集自身麦克风。默认不监听网络；用户开启“允许局域网计算”后才绑定指定接口。

采用 Quinn/QUIC：可靠流承载控制和预先授权的 profile 传输；Datagram 承载时效性媒体。Datagram 不保证送达，仍受拥塞控制；并非“无丢包/无排队的 UDP 魔法”。必须自己管理发送期限与容量。[S05][S06]

不实现 WAN、UPnP、STUN/TURN、云中继、公开端口、第三节点转发。系统防火墙只在用户同意后添加限定程序/UDP/局域网范围的规则，不关闭防火墙。

## 2. 发现与信任是两回事

mDNS 服务名 `_voice-node._udp.local`，只广播非敏感设备 ID、协议版本和监听信息。设备名称用户可改；不广播声纹、token、文件路径、详细任务或语音。

mDNS 不可用时支持手动输入地址；同名节点不是可信节点。首次授权不使用“自动接受所有自签名证书”或把六位码当安全凭据。

**本轮配对采用双向身份导入**：每端生成本地设备密钥与证书；导出包含 endpoint hint、node_id、完整证书 SHA-256 指纹的版本化身份字符串。用户从另一台设备屏幕/可信复制方式取得该字符串并在本机导入，双端分别确认。之后才建立互相验证已固定证书指纹的连接。无需自研短码交换密码协议。

手动地址或 mDNS 地址只是寻址，身份校验始终依赖已经批准的指纹。导入失败不创建“临时允许所有连接”的降级。密钥变化必须重新配对。身份字符串不含私钥；实现可包含公开证书DER以便标准TLS栈建立精确信任。除指纹固定外，仍必须验证TLS握手签名和私钥持有证明，不能只比较一个客户端自报hash。未来一键/短码配对另立安全变更，不本轮实现。

本地身份通过平台密钥存储/受保护文件保存；撤销立即断开 session，清除 session token 和临时 profile。授权粒度分为“可请求计算”“可传入音色条件数据”；远端永远不能要求本机自动打开麦克风。

## 3. 可靠控制

ALPN 项目值 `vcn/1`。控制消息使用 4-byte big-endian 长度 + UTF-8 JSON，最大64KiB；字段版本化、枚举白名单、拒绝超限与未知必需字段。

消息最少包含 request_id、protocol_version、session_id（建立会话后）、epoch、kind。所有变更有 ACK/Error；同一 request_id 重试返回既有结果，不重复分配资源。

```text
Hello / Capabilities
PrepareSession / Prepared / RejectBusy
AuthorizeProfileTransfer / ProfileChunk / ProfileReady
Start / Started
Mute / Muted
Reset / ResetReady
Stop / Stopped
Ping / Pong / Metrics / Error
```

Compute 在 Prepared 中给出真正加载的 model/profile摘要、backend、输入输出采样率、chunk/lookahead、资源预算与 capability test run ID。

Start 必须在 profile 校验、模型 warmup 和 sink准备完成后发送。用户看见 Ready 后明确启动源端采集。不能先发麦克风再等待模型加载。

本轮正常运行时不传模型权重、不下载大型资源，不让可靠流占满同一连接损害媒体。profile 传输上限50MiB、总暂存预算有界；超限拒绝，不扩容到任意大小。

## 4. 媒体格式 v1

默认每包10ms：48kHz × 0.01s = 480 samples；PCM16 mono payload=960 bytes。两个方向媒体总 payload≈1.536Mbps，尚未计QUIC/UDP/IP开销。此为格式算术，不是网络稳定性保证。

固定40-byte header，所有整数采用 network byte order；PCM payload采用little-endian：

| 字段 | bytes | 语义 |
|---|---:|---|
| version | 1 | 固定1 |
| kind | 1 | 1=SOURCE，2=CONVERTED |
| flags | 2 | 已定义标志，未定义位必须0 |
| session_tag | 8 | 连接内随机分配并与可靠控制中的session UUID绑定 |
| epoch | 4 | 配置/重连代际 |
| sequence | 4 | 单方向递增，按模2^32比较 |
| media_sample_index | 8 | 本方向媒体时间线的首样本位置 |
| source_sample_index | 8 | 对应源时间线位置；不可用时使用明确定义sentinel |
| sample_count | 2 | 默认480 |
| reserved | 2 | 必须0 |

在发送前检查 `max_datagram_size()`，它是QUIC应用datagram有效负载上限，不等于IP MTU。[S05]

默认整包1000 bytes。若当前可用上限不足，协商5ms/240samples帧；仍不足则拒绝实时模式，不做IP碎片化、不把音频转入可靠流。可用上限变化要安全重新协商/重开epoch。

M0/M3生成Rust/Python黄金向量，校验长度、端序、越界、重复、未知kind、seq wrap、错误epoch及超范围sample_count。拒绝`CONVERTED`与实际状态不一致的包。

## 5. 媒体期限、损失与时钟

发送队列和接收队列都有限制；尚未发送而已经过期的包直接丢弃，不等待重新腾出容量再播放旧话。原始序列号重排窗口有限，重复包丢弃；旧epoch全丢。

Compute 输入 jitter buffer与源端返回 jitter buffer分开记录，不叠加无依据的长缓冲。默认目标各10ms，依据观测可在5–30ms内调整；超过总预算则FailedMuted，而不是无限加buffer。

缺SOURCE数据不允许缩短源时间线。小缺口按配置补零/轻量隐藏；大缺口reset模型状态。缺CONVERTED数据同样不能直通原声。短时隐藏最多30ms。

不要求两端壁钟同步。端到端应用链路测量由源端的source索引和本地单调时钟ledger闭环计算；Compute报告自己的分段耗时。不能直接相减Windows/Mac的`time.time()`，也不能把RTT/2冒充真实单向音频延迟。

## 6. 会话租约与恢复

单节点单session，另一请求返回BUSY并说明设备状态。会话资源包括模型、profile、buffer和设备，均需lease与释放路径。

0-RTT不得用于Start/授权/profile修改等有副作用操作。重连产生新session绑定和新epoch，清空旧队列；必须重新验证信任与capabilities。睡眠/唤醒、撤销和证书变更必须回到静音待确认状态。

网络恢复不导致打开新的麦克风权限或启用未经授权的remote profile缓存。

## 7. 必需测试

未配对拒绝；错指纹拒绝；撤销后拒绝；同名伪造节点拒绝；协议版本不兼容拒绝；重复Start幂等；Stop后所有媒体无效；旧epoch回放拒绝；包重排/1%随机丢失/50ms突发缺失；发送拥塞不无限积压；双向闭环30分钟内队列年龄不增长；profile路径/长度/摘要验证。

Windows M3 单机双进程QUIC测试只证明协议实现，不证明真实LAN、Wi-Fi、Mac或物理虚拟麦克风已经通过。
