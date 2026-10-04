# T002 · 针对性兼容修复 2 / 最多 2

修复1运行成功加载VC/vocoder，随后 `file-repair1.json` 退出2：speaker active missing=[]，unexpected只有loss_calculator.projection.weight。

同官方固定speaker checkpoint以weights_only=True/mmap在CPU检查：711个state keys，唯一loss key为loss_calculator.projection.weight，shape=(5994,256)。选定runtime speaker.py的ECAPA_TDNN forward只产生256维embedding，没有loss_calculator成员；Microsoft官方speaker ECAPA源码同样以linear输出embedding（https://raw.githubusercontent.com/microsoft/UniSpeech/main/downstreams/speaker_verification/models/ecapa_tdnn.py）。

修复只对固定src.speaker.ECAPA_TDNN要求完整active keys+恰好此训练分类头、此形状，再移除该训练专用头，strict=True加载全部实际推理参数。任何其他缺失/多余/错shape失败。没有换speaker网络或checkpoint。

同时落实ml_engine只读建议：VC配置摘要固定；替换cache前精确断言两missing/无unexpected，strict加载；入口拒绝非空位置/关键字cache、非is_inference=True或training模式，执行两个计算前负向guard并记录真实推理调用数。旧mel-cache路径永不可执行。

此轮开始后两次MeanVC2兼容修复预算耗尽。若同一候选仍不可运行/不符合门槛，必须BLOCKED_MODEL或按既定计划最多一次有证据替代，不再盲修或降低门槛。
