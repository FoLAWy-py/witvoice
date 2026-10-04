# T007 notification 切片来源

实际作者 `/root/audio_runtime`，基线43f0459d0bc06b2cd9581ddaad47ab037ebf6443，5白名单源码/README文件。leader统一root/audio Cargo及lock；仅加已有windows-core0.62.2直接宏依赖，26外部版本未升级。作者已停止写入，leader后续归档整合回归另列。

作者实际命令：rustfmt --edition 2024 所改4Rust源；cargo fmt --package witvoice-audio --check（2026-10-04T18:35:38.1931534Z—18:35:38.3485584Z）；cargo test -p witvoice-audio --locked（18:35:38.3490887Z—18:35:47.4558308Z）；cargo clippy -p witvoice-audio --locked --all-targets -- -D warnings（18:35:47.4568095Z—18:35:54.7065746Z）。全部exit0；完整结果由实际作者中继给leader。编译日志源tool chunks1594d2/43e91f；本地源码差异检查3a65d1 exit0。作者21tests：10lib（6notification、4nativeformat）+1allocation+10conversion。没有ignored测试充数。日期按作者真实UTC，目录日期不是测试时间替代。

5000次实际本地COM vtable方法调用测试仅测回调方法；对象构造/销毁排除计数窗口，allocator测得0。没有向Windows服务注册、没有事件递送、没有物理拔出，也未Initialize/Start/采集/播放。退出失败引用保留测试使用注入结果，是B级纯测试，不是实机Unregister失败。

源码LF UTF8 SHA256（作者实际提供）：lib6fc13d4c1679ead50216a1f4c8eabf211cc6c98a4f2a9d6d0a3c17bff0f5dc66；wasapi55057909ba889bde74acc4ef921bbb3023ecdece12dd248ab4eb6aff97fe2f0c；notifications7d8ae43af26085755edccc528ee170d731ca6f7966e763e982684725a733dca1；nativeb0a5d970928ca00e4fcd02612a14ce51dbe71ebec0687dee6abe4a1432496ad5；README13e5636f7be51a187aecbdd60cf89e989ff6bca71be13340d62a88e38717552a。leader应核这些摘要后冻结；不能先称已核。

T007整体仍IN_PROGRESS。PCM24/32 r1 S2fix保留，但正式第二轮独立审查尚未完成；通知源码也待只读审查。Stream/removal/hardware/时钟/governor证据仍未完成。

独立正式slice r2已批准，见docs/project/reviews/T007-slice-r2.md；整体T007仍IN_PROGRESS。环境仅引用已有T001 doctor-final.json/native-final.json实际历史基线，未在通知测试起点另采环境快照。不存在leader-fmt.txt，仅checks实际fmt_exit0。
