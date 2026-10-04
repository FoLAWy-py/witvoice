# M0 合计关闭核对（独立批准）

这里只核 M0 基线，不替代 M1–M6 应用/硬件门。固定5任务已分别获得真实独立批准：

| Task | 状态 | 独立review | 主要边界 |
|---|---|---|---|
| T001 | DONE | reviews/T001-r2.md | 工具链及13测试（含负向）/4原生编译运行，非Mac构建。 |
| T002 | DONE | reviews/T002-r2.md | 单MeanVC2安全版文件源/60秒C实验；首次FAIL保留，候选预算2/2；不含完整质量/依赖/许可发行闭合。 |
| T003 | DONE | reviews/T003-T004-r2.md | 权威Rust契约、绑定/黄金向量及状态effect；不等于音频治理/IPC实现完成。 |
| T004 | DONE | reviews/T003-T004-r2.md | 获授权fixture清单与测量计划；完整24/3素材及人工有效语音未就绪，validator BLOCKED保留。 |
| T005 | DONE | reviews/T005-r1.md | 7次真实角色spawn/职责注入，固定7/35账本；disk加载schema/sandbox UNVERIFIED，原生/兼容ID不造假。 |

各任务依赖闭合；35任务/32REQ与7里程碑保持，项目仍IN_PROGRESS。M1只有T006/T007 IN_PROGRESS；不跳过T008–T035，不宣WINDOWS_DELIVERED/COMPLETE。T006的ExitNode扩展属既有REQ-23并有ADR0002，契约12tests/生成校验0，实际shutdown待；T007新增通知仍待独立审查，M0历史任务的关闭不替其审批。

继续保留B001许可链发行阻塞、s3prl生产pipcheck3缺项、进程私有内存7.17GiB非模型独占的预算解释、Windows音频/质量/2h和Mac/LAN未过。全REQ总状态不因M0关闭改PASS；最终目标仍M5/WINDOWS_DELIVERED，完整项目M6真实硬件必需。

leader准备合计证据，实际独立reviewer已核并批准，见reviews/M0-closure.md。未开始任何录音/播放/驱动/防火墙/上传；当前仅backend进程代码租约。会话中断以STATE next_task=T006恢复，不保证后台继续。
