# 当前交接

状态：IN_PROGRESS / M0。授权终点：WINDOWS_DELIVERED（M5）；M6 仍必需，不能 COMPLETE。

T001 已完成：冻结源码 `0f283ffe460be3382aae89f481be62a73ab4d60f`，13 负向测试以及 C/Rust 原生编译运行通过，独立 reviewer 第 2 轮批准，详见 reviews/T001-r2.md。初始目录没有 Git，已安全建立本地历史，未 push。已有文件保留；.local 原始音频/权重/上游不入 Git。

用户批准补齐工具、下载尽量 D 盘、来源/许可核查后的 MeanVC2 权重下载、本机转换用户音频；7.57s 用户文件不足参考下限，另批准网络参考。没有录音/驱动/防火墙/上传授权。

Rust 1.99.0 安装于 D:\Software\WitvoiceToolchain\Rust；MSVC BuildTools 17.14.41 在 D:\Software\WitvoiceToolchain\VS2022BuildTools，SDK 10.0.26100.0 的系统组件在 C 盘。dot-source tools/dev/toolchain-env.ps1 只改本轮进程环境。实际 Python 3.12.14 在 C:\Users\22198\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe；WindowsApps 别名不可用。FFmpeg 已有 D:\Resource\ffmpeg\bin。uv 0.9.28 在 D:\Software\UV\uv.exe。

下一任务：T002，T003/T004 可按依赖并行。选定候选仅 MeanVC2 40ms；上游 commit 13acf84c1bf135ea5edad9c245b345289b06b33e。HF revision 39cdd19522fe896c227da691314d9a0e3b995486 的 ASR/VC/vocoder 已下载并匹配官方 LFS SHA256，未加载。speaker finetune/官方 cfg/依赖未完成；真实转换、连续流式、CUDA均 NOT_RUN。MeanVC2 runtime 原生输出源码为16k，不能猜成24k。lawlict 派生代码许可未闭合，影响发行，不能给全部资产标 Apache/MIT。

Archive.org 参考下载超过10min仍0字节，已终止确认属于本任务的下载进程；保留0字节文件为失败痕迹，不能当素材。正在有界核查网络来源；私人源转换不上传、不播放、不录音。

实际角色 ID 为 /root/{frontend,backend,audio_runtime,ml_engine,hci,reviewer}，审查实际 /root/reviewer_t001。已验证自定义类型派发/职责注入，尚不能证明磁盘角色文件完整加载。默认 shell/node sandbox 在进程创建前 apply deny-read ACLs 失败；leader 受限范围命令经 auto-review 使用 require_escalated 成功，未改 ACL/全局配置。reviewer 保持只读，由 leader 转交源码和结果。这是实际兼容运行限制，不冒称并行实现或独立复跑。

当前无 GPU/音频硬件 lease。恢复先读 STATE/TASKS 当前资源锁。没有后台继续工作的承诺，运行中的下载必须先确认当前进程/结果，不盲重试。
