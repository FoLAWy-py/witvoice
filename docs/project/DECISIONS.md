# 决策索引

- 基线：独立实时VC；Windows优先；三条必需链路；默认M5停止、M6才完整完成。
- 本轮单模型；候选需真实验证；Python worker是允许的最终交付实现，不强制原生化。
- 本轮依赖外部安装的虚拟音频线，不自研/捆绑驱动。
- 本地先二进制IPC，网络QUIC；全链路有界、失败静音、身份双向固定。
- 根会话leader；6种子角色；最多4并发/2写入/1硬件测试；35固定任务。

M0将实现细节写入带编号ADR；不能将尚未实施的决定记作测试结果。

- [CR-0006 / ADR0006](../adr/0006-authorized-warmup-host-gate.md)：2026-10-06用户明确授权T011主机预热门6→8GiB，设备4GiB和SPEC07规划4/3GiB保持。新f9a一次真实预热Ready/Stopped/ownedJob0通过，内存快照7.187GiB；原6GiB失败保留，不称内存优化或完整VC通过。

- [ADR 0002](../adr/0002-local-node-exit.md)：T006 补齐本地鉴权 ExitNode、输出失效与 Node-owned worker Job；不增加 LAN 退出权限。实现/独立审查待。

- ADR0003: T016标准WindowsDNS-SD替代未证明cache界的mdnsdaemon；默认关闭、显式接口/用户LANopt-in、有界自有队列/异步寿命，无新模型/驱动/服务或Mac实测claim。见 docs/adr/0003-native-dns-sd.md。

- 2026-10-05 用户明确固定本轮Windows VB-CABLE；Steam候选撤回保留历史；不自建驱动。一次<=5s marker/<=15s双端闭环授权未执行，见 ADR0004 与 vbcable-metadata。

- 2026-10-05 当前VB-CABLE两次独立真实once均已消费；最新获实际render1056frames/22ms但beforeStart拒绝，不是闭环。采用<=30ms1440 OS容量、单次提交<=960的有界软件适配；旧未执行文字为历史，硬件再次执行另获具体许可。
