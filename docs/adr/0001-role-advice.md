# T003 实际角色建议与后续落实边界

backend /root/backend提出local与LAN命令隔离、受限控制长度、可靠消息幂等与鉴权由Node落实，模型条件摘要不应仅信任名称。ml_engine /root/ml_engine按固定上游指出原生16k、模型/条件/backend必须由实际加载与试跑生成，缓存路径需拒绝，不能猜测24k或MPS。这些建议由leader整合至ADR0001与契约，生产落实另属后续任务。

audio_runtime /root/audio_runtime在完整读取冻结r2源码/生成物及SPEC02后给出实际只读建议，未自行审批或复跑：

- 当前schema没有新增音频阻塞；Prepared摘要与非零tag必须结合身份/session/epoch和实际backend校验，pre-session Error不授媒体权限。
- 设备按稳定UID与flow/state选择，名称不能替代UID；移除不能回退默认物理设备。虚拟render→第三方capture方向须明确。
- QueueBudget frames必须绑定实际协商帧长；capacity/target/age分开落实，网络240/480样本不能混为模型chunk。增益/限幅后mono复制stereo；监听独立buffer/clock，ASRC安全界限±1000ppm。
- Stop/Mute先原子失效output再异步清理；回调只用预分配/SPSC/atomics；计划静音、故障、欠载和过期分别统计。进入最终playout仍检查epoch/age。
- duration_preserving=false或未知source映射不能伪造app latency；RTF分母仅新输入，u64字符串不转JS浮点。真实2h/双向60min与8h模拟分别验收。

这些约束属于T007/T008/T014/T017后续实现，不把契约单测或素材文件当真实音频通过。T003正式批准仍由/root/reviewer_t001独立r2完成，leader只归档与整合。
