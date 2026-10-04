# 当前交接

当前IN_PROGRESS / M1，授权终点M5/WINDOWS_DELIVERED；M6无真机证据仍必需，不允许COMPLETE。next_task=T007。M0及T001–T005独立批准DONE；T002-r2只批准文件源C实验，许可B001仍阻止捆绑。

T006已DONE：真实Windows IPC/监督源码冻结4eff1ae，证据6e9e7c：51workspace tests/fmt/Clippy0；默认11与feature18作者日志、失败历史和精确清理记录全部保留。独立r1已批准固定基础验收DONE/attempts1，review T006-r1.md。判定固定T006基础acceptance，不把后续依赖它的T011 Python协议/T025 Tauri/完整故障硬件验收倒置为前置；probe只是C级真实进程监督，无VC/Ready能力。

T007 native 8文件作者已停止，26纯tests/fmt/clippy0；用户明确批准两项本地<=5秒capture/silence-render，两项实际exit0（capture501packets/240480frames，render241536silenceframes；各循环约5.002秒含调度越界），硬件lease已释放；无PCM落盘/上传/原声回放/默认设备变更，整个T007仍未完成。T007 formal slice r2已批准（源码2e9366a，证据a027），整体IN_PROGRESS。leader另在3cf1393冻结notification_probe，实际Windows服务6次注册/明确UID重查/注销成功，证据registration/command+result.json；无事件递送/物理拔出/Initialize/Start/采集播放，metadata硬件lease已释放。原生audio实际角色先完成T006源码中继，再按T007-native-stream-assignment实现shared stream；独立audio路径code lease已登记，无hardware/GPU lease。未知注销状态的引用保留仍不是清理PASS。

T021正式r1独立批准DONE/attempts1：五页/十状态/键盘缩放规则和深浅tokens、50静态色对/CSS/schema覆盖0。source4eff、final evidence6e9e，review T021-r1.md。实际leader作者、frontend建议/只读中继，没有新hci派发或实际UI/HCI签收；T022/T025/T029仍须真实验证。

实际线程UUID新证据T005-thread-id-provenance.json；历史七角色不等于当前同时保留，当前4子agent以内，作者最多2、硬件最多1。共享契约/manifest/lock/账本仍leader统一。不运行真实麦克风/输出/装驱动/开防火墙/外传。会话中断前停止作者并保存next_task，不承诺后台继续。
T016依赖/manifest/lock统一leader完成，62新增registry归档与2942源码文件逐字节核对；4新Windows构建脚本全文实际frontend只读审计，未运行构建。root代码lease释放，T016-assignment已登记backend独立路径代码lease，源码功能尚未实现/不标PASS。mDNS内部cache无已证明容量，是需明确处理的事实。

T007固定验收尚缺通知→流失效连接、实际设备移除事件和格式拒绝证据；audio watched-stream增补lease先登记，backend独立T016，两代码作者，零硬件lease。USB Wireless Mic Rx是实际已测input；Steam Streaming Microphone只软件静音output。新增harness仅创建不运行。

T007 watched增补10文件交付并停止，33纯tests/fmt/clippy/examples编译0；raw HRESULT去除额外COM ErrorInfo，通知永久changed门连接prepare/start/packet前后/wait超时。root先冻结并登记metadata-only probe独占lease，无新的Start授权/执行；物理拔插需用户后续具体操作，整个T007仍IN_PROGRESS/attempts2。

当前所有代码作者已停/硬件lease释放。T007source3af8a6f frozen33tests/fmt/clippy/examplebuild0；3格式probe真实S_OK一次/S_FALSE两次closest仅报告，无Initialize。用户同意并确认拔USB，真实OS reason25(State/Default/Property)原UIDInactive/stickychanged/注销0/exit0，运行中removal仍NOT_RUN，不用metadata替代完整链路。T016身份11文件作者15tests0，尚无native mDNS/TLS/Node集成，仍IN_PROGRESS；原生DNS-SD下一窄依赖准备候选，默认networkoff。

以下保留历史过程；当前状态以本段、STATE/TASKS及最新正式review为准。
T001已DONE：源码0f283ffe460be3382aae89f481be62a73ab4d60f，13负向测试、C/Rust原生编译运行通过，独立/root/reviewer_t001第2轮批准。目录初始无Git，安全建立本地历史，无push/reset/clean，保留原有文件。

T003契约实际实现：Rust权威schema、TypeScript/Python绑定、40byte媒体黄金向量、u64字符串、状态转换/静音effects。r1两S2（loaded profile、pre-session Error）已修复，冻结1e9b9d4298183c47c862febaf177ad46ced85c8d。冻结回归11Rust+1Python通过；fmt/clippy/generator均0；当前导出schema双SHA一致。完整证据docs/evidence/T003-T004-20261005/r2*；r2-command-provenance.json保存真实argv、exit、hash和正确git diff空输出。独立/root/reviewer_t001第2轮已批准；不代表运行时IPC/音频安全已实现。

T004由实际/root/audio_runtime写六文件、leader整合。冻结后12素材测试通过。两个本地fixture摘要核实：获授权私人源7.5293125s，网络LibriSpeech CC-BY4参考14.225s。完整24源/3音色、有效参考语音时长人工复核和质量试听未完成；validator exit2/BLOCKED(errors=[])不是PASS。硬件步骤/阈值/资源锁仅计划，无预填成绩。

T002实际模型：MeanVC2上游13acf84c1bf135ea5edad9c245b345289b06b33e、HF模型39cdd19522fe896c227da691314d9a0e3b995486；五资产大小/SHA已核。HF三文件为官方LFS摘要，Microsoft两文件为官方README链接下载后的本地摘要，不能叫publisher签名。隔离Python3.12.14/Torch2.5.1+cu121在D盘.local环境。Windows原生GPU实验采用固定源码429b7c27713c4549ba6072d25dda18f48ddd4ee9，VC/speaker cuda:0、ASR cpu、原生输入输出16k、实际160ms新输入块。文件转换7.529→7.510s；60s paced为375steps，平均RTF0.645263、p99处理135.528ms、最大输入调度lag1.304ms，实验门槛通过。两WAV只在.local，std库独立解码/摘要/无削波与原始统计复核exit0。报告M0-20261004保留两次失败及成功原始JSON。

模型两次针对性兼容修复预算已用完：只拒绝未训练旧mel-cache路径；精确移除speaker训练专用分类头，全部活跃参数strict加载，入口guard真实运行。不要再盲修同候选/降阈值。初始化私有内存约7.69GiB、结束4.31GiB，GPUallocated1.786GB；模型内存拆分/CPU/16GiB机器与游戏负载未测。33项依赖元数据已记录，但完整s3prl训练依赖未装，未pip check；metadata早前异常退出码保留。独立T002审查待。

许可：MeanVC2 README明确Apache2（固定commit没有独立LICENSE文件）；lawlict派生代码、speaker独立二进制条款尚未闭合。UniSpeech根项目CC-BY-SA3，不套用MIT。发行状态BLOCKED_LICENSE_CHAIN_NOT_BUILT。实验结果不批准捆绑/再分发；没有联系上游或外传数据。

工具：Rust/Cargo1.99.0、MSVC BuildTools17.14.41安装于D:\Software\WitvoiceToolchain；SDK10.0.26100系统组件在C盘。dot-source tools/dev/toolchain-env.ps1只改当前进程。bundled Python C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe；FFmpeg D:\Resource\ffmpeg\bin；uv D:\Software\UV\uv.exe。源码、权重、下载缓存和venv尽可能D盘；无持久PATH修改。

真实角色ID及兼容方式见T005-agent-runtime.md。自定义类型派发/职责注入已验证，磁盘完整schema加载和sandbox强制UNVERIFIED。默认shell/node在创建进程前ACL失败；leader限定命令、明确授权的backend/audio固定路径中继可经auto-review执行，reviewer不提权，独立审查不冒称复跑。ml旧角色重新派发曾thread-limit失败，没有新执行，采用现有角色中继。最多4子agent/2写入者/1GPU测试，未第二leader/递归spawn。

用户已批准工具补齐、核查后模型下载尽量D盘、私人音频本机转换及网络参考；没有录音、驱动、防火墙、上传许可。GPU lease已释放；当前子任务只读中继/审查，不运行硬件或下载。下一阶段先实现独立Node，再到明确硬件门时提出最小操作。会话中断不承诺后台继续。

T007首切片已实际由/root/audio_runtime写7文件并停止写入：WASAPI元数据/UID选择/精确格式探测及无分配转换。13音频测试通过；leader整合workspace包含11契约测试后test/fmt/clippy均0。leader实际metadata-only示例0：33端点，12capture/21render，8active；25inactive等mix错误保留。UID仅.local；没有Initialize/Start/采集播放/移除。证据docs/evidence/T007-20261005。整个T007仍IN_PROGRESS，stream/notify/resampling/ASRC/MMCSS及真实输入输出验收未实现/未执行，不能关闭。

T002正式r1 changes-required归档docs/project/reviews/T002-r1.md：2S1（工作树来源门、Torch已知加载漏洞）、2S2（硬编码ASR、尾块门槛）。root已修源码/报告/版本guard，14标准库回归0，实际8Git blob快照/固定配置/375历史步复核0。原ASR cpu字段降为SOURCE_INFERRED（旧JSON保留），不视为实测。快照Git LF配置摘要与原CRLF checkout不同，已证明仅换行且JSON完全相同，详review-security/NOTES.md。compat预算仍2/2。

安全依赖：原Torch2.5.1+cu121仅历史C实验；两官方weights_only漏洞已用>=2.10.0/拒未知预发布guard阻断，两个入口mock保证load0。2.6下载已中断exit1，无遗留UV；2.10.0+cu126官方D盘安装exit0，SymPy1.13.3满足真实约束。实际pip check exit1：缺omegaconf/transformers/protobuf，33推理依赖元数据归档，不宣完整生产依赖闭合。五权重不改/不再下载。

新真实模型均绑定4fd6be043b9c5a627df36328ff00e3ef03d61ace：file-210 exit0；首次paced-210 exit2（p99259.537ms，lag986.025ms）完整保留FAILED。唯一预登记环境隔离复测暂停所有own编译测试，同模型/代码/阈值，375steps60s，RTF0.594341、p99149.388ms、maxlag13.723ms exit0。CPU8%、平衡电源、背景GPU56%→5%记录，不能证明全球空闲或归因backend。VC/speaker cuda:0、ASR cpu来自完整参数+缓冲探测。输出59.94s有限无削波，std库decode/hash/statistics复核0。仍只C级短实验，2h/并发负载/真实应用/Mac/质量都未过。新private init7.430GB、end7.701GB（整个Python进程，不伪装满足内存预算），GPUpeak allocated1.786GB；未来T026需归因和验门槛。T002 r2待独立审查，不标DONE。

T006实际/root/backend首切片完成并停止：11文件，普通用户受限named pipe/独立Node，DACL/SID/PID/私有stdin token/长度64KiB/期限3s/取消；真实3进程+5会话测试通过。root仅新增3localpackage lock项，无外部依赖升级，scoped19tests含11contracts/fmt/clippy0。未做worker Job/Tauri launcher UI Job隔离/完整shutdown/sleep，整个T006仍IN_PROGRESS，独立review待。根manifest/lock统一leader。真实契约路径04-lan-protocol.md和06-security-distribution.md。

T007 independent首切片r1发现1S2（PCM24/32应使用完整WAVEFORMATEXTENSIBLE）；实际audio已修仅2files，新2描述符字段/往返测试。leader修复后全workspace34tests（15audio+11contracts+8Node）/fmt/clippy0，证据T007-20261005/final-*，source起点hash未采集明确NOT_CAPTURED。r2只签slice，整个T007stream/removal/hardware仍未完成。

历史整合点曾所有作者停止并释放租约；原sessionIDs34002/27717/77859/20224/84074均已完成，不继续轮询。当前租约以STATE为准（backend T006代码写入，无GPU/audio）。reviewer自身规则拒绝提权只读（不是auto-review拒绝），继续冻结全文中继；不冒称独立复跑。会话中断不会保证后台继续。

T006 leader 补齐现有需求中的本地 ExitNode 契约（ADR0002），PeerMessage不增加远端退出权限。Rust schema/TS重新生成；12契约+1Python黄金向量测试、fmt/clippy/generator全部exit0，证据T006-20261005/exit-contract。测试基线20442af，源码当时未提交已明确记录；仅契约，尚无实际退出资源清理实现/独立审查。初次patch后才登记租约，后续派发先登记再开工；当前leader停止此切片写入，租约释放，backend继续T002冻结只读中继。

T006 supervision派发保存T006-supervision-assignment.md，backend尚未开始写入；leader开启既有Windows0.62.2 JobObjects feature，cargo check platform/node locked exit0，仅编译不代表进程监督通过。T007实际audio_runtime继续通知切片，已在派发前登记独立audio路径/代码租约，禁止采集/播放/GPU；backend/reviewer仍只读T002r2。

T007通知COM implement宏需要直接windows-core0.62.2路径，audio实际核缓存宏源码后报告，leader已统一root/audio manifests，offline锁生成0；26外部package版本未变，lock仅audio deps增加windows-core。通知纯测试尚在作者执行中。只读CIM设备调查有MIXLINE系列，VB-Audio/CABLE/BlackHole名称匹配0；这不是路由可用或设备绝对缺失证据，没有安装驱动/改变默认设备/采集播放。

T007 notification实际audio交5files并停止，21纯tests/fmt/clippy0，5000本地COM callback calls allocator0；leader核5LF摘要全匹配，source-hash-check.json。服务Register/Unregister/拔出/Initialize/Start均NOT_RUN，注销失败有界存储保留风险如实README，r2审查待。整合冻结后workspace回归待。

T002-r2已归档并DONE/attempts2，B003仅实验范围RESOLVED，保留原FAIL/两兼容预算/许可B001/productiondeps缺项。M0-closure-audit待独立合计核对，不把单项批准扩为M0全批准。T007通知source2e9366a整合41tests/fmt/clippy全0，完整stdout/UTC在notifications/leader-*。当前backend T006代码lease已先登记后派发，audio作者已停；无GPU/audio硬件活动。

当前补充整合：冻结4eff1ae后51workspace真实tests/fmt/featureClippy全部0；T021 final静态50色对/生成CSS/十状态0，账本自检0仅文档。完整stdout/UTC归档，T006/T021独立r1材料中继已派发；无运行测试/GPU/audio。T005增加实际子线程UUID元数据，不把历史7角色称同时存活。

T007在新写入前登记leader仅notification_probe example白名单/代码lease；实际audio/backend各只读中继T006、frontend只读T021，reviewer独立审查。探针只注册/关闭metadata通知，硬件运行另登记，尚未运行。

T007下一操作为leader notification_probe，既有私有metadata派生明确capture/render UID（不换default、不采集/播放）；源编译/clippy0，initial private native导入compile101已修public re-export，独占metadata hardware lease先登记。仅注册/初次重查/注销，无事件递送/拔出证明。

最新：T021正式r1独立批准DONE/attempts1，设计规则/token范围，不是真实UI/HCI。3cf1393冻结notification_probe：6个实际Windows服务注册/明确UID重查/注销成功（0.062917秒操作窗口），仍无服务事件递送/物理removal/Initialize/Start/采集播放；metadata硬件lease已释放。T007下一native stream派发T007-native-stream-assignment，audio先结束T006全文中继后写；代码lease提前登记，无硬件lease，录音/播放仍未授权。next_task仍T006，M1与M4不能关闭。

T007 audio实际请求event API feature；leader核缓存Windows0.62.2 CreateEventW被Win32_Security gating及WaitForSingleObject在Threading，统一audio manifest增加既有Security/Threading（无版本/lock升级），cargo check locked0仅编译。MMCSS留NOT_RUN，无采集播放/GPU。

T006正式r1关闭，last_verified_commit提升6e9e7c且明确仅基础子集，不含后续Native/硬件/UI。满足T003/T006后T016 leader依赖准备登记，root+audio最多2代码写入者，backend尚不写；普通静态下载不运行脚本/网络监听/防火墙。主线/next仍T007。
