# workers/vc_worker

ml_engine实现一个真实zero-shot模型worker；不带UI/麦克风/公网HTTP服务。

T002 的 feasibility.py 仅保留已验证模型实验，复用固定 MeanVC2 与资产审计，不重复探索。

T011 正在实现：protocol.py 使用 Rust 导出的 worker-media.json 定义独立二进制 PCM 帧，拒绝旧绑定、越界、非有限样本和损坏长度；部分读写共享绝对期限。windows_pipe.py 与 Rust WorkerPipeServer 的真实 Windows 互操作已经实测通过，Node 的 Job 所有者确认清理；证据仅限通信与进程，不代表模型或音频闭环。

runtime.py 是 Prepare-only 控制入口，只有固定 MeanVC2/CUDA/已授权参考。模型加载由一个独立任务执行，完成队列容量1；控制所有者回复 Heartbeat 和 Stop，不等待模型线程。warmup.py 复用已核模型入口，对真实输出、设备与预热资源预算检查后才返回 Ready，随后清空模型缓存；尚未执行的真实预热不能标 PASS。VC lookahead640 samples只表示 VC 未来上下文，不是总延迟。主机6GiB/设备4GiB是预热就绪时的限制，不是峰值性能报告或运行期资源治理证明。

Stop 的 Stopped 消息仅是协议确认；模型/GPU释放必须由 Node 确认其 Job 已空。当前媒体推理仍由 T012 接入，生产 Node 会话与输出门仍待后续任务；没有静默原声旁路或备用模型。

验收入口：`python -m unittest discover -s workers/vc_worker -p test_protocol.py -v`。软件负例不能替代真实模型、设备或变声闭环。
