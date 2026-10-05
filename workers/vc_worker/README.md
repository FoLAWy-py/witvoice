# workers/vc_worker

ml_engine实现一个真实zero-shot模型worker；不带UI/麦克风/公网HTTP服务。

T002 的 feasibility.py 仅保留已验证模型实验，复用固定 MeanVC2 与资产审计，不重复探索。

T011 正在实现：protocol.py 使用 Rust 导出的 worker-media.json 定义独立二进制 PCM 帧，拒绝旧绑定、越界、非有限样本和损坏长度；部分读写共享绝对期限。它目前是软件 codec/framing，尚未接上原生 Named Pipe、真实预热与 Node 进程生命周期，不能宣称生产 worker 完成。

验收入口：`python -m unittest discover -s workers/vc_worker -p test_protocol.py -v`。软件负例不能替代真实模型、设备或变声闭环。
