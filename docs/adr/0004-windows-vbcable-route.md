# ADR 0004 — 本轮 Windows 固定采用用户安装的 VB-CABLE

2026-10-05；依据用户明确决定，状态 ACCEPTED。本轮 Windows 使用 VB-CABLE，恢复需求基线中的既有虚拟线方案；固定任务/门槛与模型不变。Steam Streaming Microphone 不再是当前路由候选，其 T007 静音 render 和 T009 候选元数据均只保留为历史证据。

实际方向：WitVoice 将处理后音频写入 CABLE Input（render），经 VB-CABLE，由第三方应用选择 CABLE Output（capture）。按保存的 UID、API 方向、活动状态、精确格式和实际父驱动身份核对，名称只用于定位。移除/变更/失败静音，不换同名端点、Steam 或物理扬声器。CABLE In 16ch 不属于这次选定的双声道端点对。

本轮不自建或捆绑驱动，不重装已安装驱动，不改默认设备、防火墙或系统安全。已搜索 docs/project 与 docs/adr 的 CR 文件/驱动记录，未发现自建驱动 CR；不伪造撤回事件。若后续发现旧 CR，保留历史并按这条用户指令标本轮撤回。

用户批准一次最小标记闭环：仅核实名 UID 的这两个 CABLE 端点，本机生成低幅 marker 总时长<=5s，总操作期限<=15s，缓冲有界；正常退出停止并释放，超时外部监督只结束自有进程且如实标 cleanup UNKNOWN/FAIL。无物理采集/物理播放、PCM落盘上传或默认设备修改。执行前源码独立审查、构建与资源锁登记，告知操作后执行，无需再次确认。一次权限失败也消耗，不自动重试或换设备。

只读核对证据：docs/evidence/T009-20261005/vbcable-metadata.json。两端当前均48k/2ch/Float32/active，父驱动VB-Audio Software/VBAudioVACMME/3.3.1.7。当前闭环 NOT_RUN。成功只能标“VB-CABLE 路由闭环通过”，T009还需软件检查与独立正式评审；真实VC/第三方应用/长期稳定/Mac均留各固定任务。