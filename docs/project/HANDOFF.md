# 当前交接

状态IN_PROGRESS / M0，授权终点WINDOWS_DELIVERED（M5）；M6必需且仍UNTESTED，不允许COMPLETE。恢复先读STATE/TASKS，下一Task ID为T002（独立模型证据审查），T003/T004第2轮已批准并DONE，T005独立审查待。不要重新下载或重跑有效模型实验。

T001已DONE：源码0f283ffe460be3382aae89f481be62a73ab4d60f，13负向测试、C/Rust原生编译运行通过，独立/root/reviewer_t001第2轮批准。目录初始无Git，安全建立本地历史，无push/reset/clean，保留原有文件。

T003契约实际实现：Rust权威schema、TypeScript/Python绑定、40byte媒体黄金向量、u64字符串、状态转换/静音effects。r1两S2（loaded profile、pre-session Error）已修复，冻结1e9b9d4298183c47c862febaf177ad46ced85c8d。冻结回归11Rust+1Python通过；fmt/clippy/generator均0；当前导出schema双SHA一致。完整证据docs/evidence/T003-T004-20261005/r2*；r2-command-provenance.json保存真实argv、exit、hash和正确git diff空输出。独立/root/reviewer_t001第2轮已批准；不代表运行时IPC/音频安全已实现。

T004由实际/root/audio_runtime写六文件、leader整合。冻结后12素材测试通过。两个本地fixture摘要核实：获授权私人源7.5293125s，网络LibriSpeech CC-BY4参考14.225s。完整24源/3音色、有效参考语音时长人工复核和质量试听未完成；validator exit2/BLOCKED(errors=[])不是PASS。硬件步骤/阈值/资源锁仅计划，无预填成绩。

T002实际模型：MeanVC2上游13acf84c1bf135ea5edad9c245b345289b06b33e、HF模型39cdd19522fe896c227da691314d9a0e3b995486；五资产大小/SHA已核。HF三文件为官方LFS摘要，Microsoft两文件为官方README链接下载后的本地摘要，不能叫publisher签名。隔离Python3.12.14/Torch2.5.1+cu121在D盘.local环境。Windows原生GPU实验采用固定源码429b7c27713c4549ba6072d25dda18f48ddd4ee9，VC/speaker cuda:0、ASR cpu、原生输入输出16k、实际160ms新输入块。文件转换7.529→7.510s；60s paced为375steps，平均RTF0.645263、p99处理135.528ms、最大输入调度lag1.304ms，实验门槛通过。两WAV只在.local，std库独立解码/摘要/无削波与原始统计复核exit0。报告M0-20261004保留两次失败及成功原始JSON。

模型两次针对性兼容修复预算已用完：只拒绝未训练旧mel-cache路径；精确移除speaker训练专用分类头，全部活跃参数strict加载，入口guard真实运行。不要再盲修同候选/降阈值。初始化私有内存约7.69GiB、结束4.31GiB，GPUallocated1.786GB；模型内存拆分/CPU/16GiB机器与游戏负载未测。33项依赖元数据已记录，但完整s3prl训练依赖未装，未pip check；metadata早前异常退出码保留。独立T002审查待。

许可：MeanVC2 README明确Apache2（固定commit没有独立LICENSE文件）；lawlict派生代码、speaker独立二进制条款尚未闭合。UniSpeech根项目CC-BY-SA3，不套用MIT。发行状态BLOCKED_LICENSE_CHAIN_NOT_BUILT。实验结果不批准捆绑/再分发；没有联系上游或外传数据。

工具：Rust/Cargo1.99.0、MSVC BuildTools17.14.41安装于D:\Software\WitvoiceToolchain；SDK10.0.26100系统组件在C盘。dot-source tools/dev/toolchain-env.ps1只改当前进程。bundled Python C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe；FFmpeg D:\Resource\ffmpeg\bin；uv D:\Software\UV\uv.exe。源码、权重、下载缓存和venv尽可能D盘；无持久PATH修改。

真实角色ID及兼容方式见T005-agent-runtime.md。自定义类型派发/职责注入已验证，磁盘完整schema加载和sandbox强制UNVERIFIED。默认shell/node在创建进程前ACL失败；leader限定命令、明确授权的backend/audio固定路径中继可经auto-review执行，reviewer不提权，独立审查不冒称复跑。ml旧角色重新派发曾thread-limit失败，没有新执行，采用现有角色中继。最多4子agent/2写入者/1GPU测试，未第二leader/递归spawn。

用户已批准工具补齐、核查后模型下载尽量D盘、私人音频本机转换及网络参考；没有录音、驱动、防火墙、上传许可。GPU lease已释放；当前子任务只读中继/审查，不运行硬件或下载。下一阶段先实现独立Node，再到明确硬件门时提出最小操作。会话中断不承诺后台继续。

T007下一切片准备：leader添加官方windows0.62.2与audio crate manifest，Cargo缓存下载在D盘，lock仅新增11项Windows依赖，既有15项未升级。当前lib仅scaffold，未宣称音频实现或测试PASS；实际writer需在T005中继结束后由leader派发，禁止自动采集/播放。STATE/TASKS已登记窄audio路径资源锁；当前尚无音频硬件lease。
