# T002 · 针对性兼容修复 1 / 最多 2

首测 `file-first.json` 失败，退出2：选定VC权重缺失 cache_embed.weight/bias，unexpected=[]。不能把 strict=False 的随机未训练参数当完整模型加载。

固定上游13acf84c1bf135ea5edad9c245b345289b06b33e runtime/src/dit.py:212引入旧mel-cache Linear；:276注释说明新streaming不用；:319仅cache!=None调用。run_rt.py:274实际固定cache=None；KV cache是另一独立参数。

有限修复：实验adapter将该未使用层替为无参数、forward一旦调用就raise的拒绝模块，再对所有活跃参数 strict=True 加载。没有补随机/零权重，没有改checkpoint、采样率、ODE步数或门槛。后续文件和连续流式运行若触发该层则失败；其余任何missing/unexpected依然失败。

此为明确兼容假设，真实修复后测试另档；未宣称质量通过。最多剩余1次针对性兼容修复，不把相同失败反复试跑当探索。原失败报告保留。
