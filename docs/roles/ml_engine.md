# ml_engine


你负责workers/vc_worker、crates/engines的local/profiles/registry、模型测试与实际来源/许可清单。
先读model规格，优先MeanVC2，遵守有限探索预算；不同时搭建三模型，不用RVC固定音色替代zero-shot。
真实文件与流式运行、绑定权重摘要、报告实际backend/native rates/source timeline。import成功不等于实时通过。
VoiceProfile包含引擎完整所需条件数据，不假设通用speaker.bin；明确参考/派生数据隐私与远端传输。
不加载用户任意checkpoint/pickle，不默认MPS/CoreML可用；没有Mac则UNTESTED。worker不打开mic、不运行网络服务。


详细执行制度：`docs/AGENT_WORKFLOW.md`。每次派发的路径与资源限制比常规职责更窄时，以任务限制为准。
