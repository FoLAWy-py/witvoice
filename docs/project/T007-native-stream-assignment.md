# T007 native WASAPI shared-stream slice

M1 / REQ-06/08，依赖T003 DONE；原生角色 `/root/audio_runtime`，只读T006中继结束后再开始。当前基线3cf13933a25cde0b691166a68ebd401e64623da3；leader最新整合的notification_probe与T021不得覆盖。最多两个代码写入者，本切片只一作者；没有硬件/GPU lease。

白名单 crates/audio/src/、tests/、examples/、README.md。禁止 manifests/lock/contracts/ledger/Node/platform/worker；需要Windows feature由leader统一加。先读audio AGENTS、SPEC01/02/07、已有wasapi/notifications/format及官方IAudioClient Initialize/GetBuffer/ReleaseBuffer；实际Windows0.62.2源码核签名。

目标：明确UID/flow/native format的shared/event-driven owner与安全Capture、silence Render基础API，独立normal STA音频线程构造/关闭，保持!Send COM所有权；不把设备名当UID、不选默认设备。Initialize阶段绝不Start；Start只通过后续显式授权调用，库单测/编译不得实际开麦/输出。请求格式须得到exact支持；closest仅报告，不能暗中采用。Native rates标明，不假称已完成48k bus resampling/ASRC（T008）。

每次buffer acquire/release在同线程配对。Capture flags SILENT允许null并填零；非silent null/超容量/长度错误/nonfinite时清空整个调用者buffer并精确失败，不保留旧PCM。预分配目标/有界packet处理，不分配、不日志、不文件/网络/模型/UI、不等待其他线程、不panic跨FFI。设备/QPC timestamp与discontinuity保留为实际元数据，不伪造源主时间线。失败标记失效、禁止自动重启/default fallback。

Render本切片只提交明确silence；Start前prime零、失败/停止默认静音。暂不增加公开无权限的原声直通或capture→render连线。T008后接epoch/deadline output governor，再开放converted提交；不把本slice称真实VC/虚拟线闭环。MMCSS可在非实时构造线程作用域设置/归还，需要依赖先请求leader。

最多五个检查：原生owner/精确格式/有限buffer；capture固定目标与acquire/release；silence render及stop/reset/drop；纯布局/标志/失败静音/零分配相关测试；显式hardware smoke harness只创建，暂不运行。harness应显式选择UID/flow、Start授权参数、最大5秒、不落地PCM/不上传/不默认设备，不因缺权限绕过。真实capture/silence output/拔出需后续最小用户授权和独占硬件lease；不得预填PASS。

命令 cargo fmt --package witvoice-audio --check；cargo test -p witvoice-audio --locked；cargo clippy -p witvoice-audio --locked --all-targets -- -D warnings。实际stdout/UTC/exit/source摘要交leader，写完即停止。T007已有正式r1/r2，最多3轮总预算不重置；本新增native源码待最后独立评审，未执行硬件不关闭T007。
