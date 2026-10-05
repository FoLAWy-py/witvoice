# 阻塞与前置条件

只登记实际事实。模型本机实验已通过，不能由此批准再分发或宣布M0/M5完整通过。

| ID | 任务/范围 | 证据 | 影响 | 已尝试 | 最小解阻动作 | 状态 |
|---|---|---|---|---|---|---|
| B001 | T002资产清单；T025/T028捆绑/发行 | model-feasibility/M0-20261004/RESULTS.md、assets-audit.json、dependencies.json；固定上游README | lawlict派生组件与speaker二进制许可链未闭合，不能捆绑再分发 | 核官方MeanVC2/HF、Microsoft WavLM/UniSpeech及实际dist-info；记录Apache2、MIT、CC-BY-SA3各自作用域；不套用别库许可证 | 获得适用派生代码/独立权重的明确上游许可来源，完成独立复核；不靠用户一句允许下载代替第三方许可 | BLOCKED_LICENSE_CHAIN；不抹除成功实验 |
| B002 | 当前工具执行/审查 | T005-agent-runtime.md、T001-session.md | 默认shell/node在进程创建前ACL失败；reviewer无法直接读盘/复跑 | leader限定命令与明确授权backend/audio只读中继成功；reviewer收冻结全文与完整结果 | 继续既有兼容执行；若需独立复跑，在客户端支持的安全环境执行，不改ACL/全局安全 | MITIGATED_WITH_LIMITATIONS |

后续实际前置门仍未执行：T009/T015虚拟设备与物理采集、T026真实2h、T029完整24/3质量与owner试听、T031–T035 Mac真机及有线LAN双向。状态NOT_RUN/UNTESTED，不能标成失败或PASS，也不能提前代用户授权录音/装驱动。当前缺少完整质量素材的validator结果为BLOCKED，这不阻止T004测量计划本身被独立验收。

B003（T002安全运行时，RESOLVED_EXPERIMENT_ONLY）：原Torch2.5.1落在两官方weights_only漏洞范围；固定摘要不等于安全解析。新入口阻断<2.10.0/未知，14纯回归通过；官方固定2.10.0+cu126同输入file PASS，首次paced FAIL保留，唯一隔离复测PASS（RTF0.594341/p99149.388ms）。独立/root/reviewer_t001正式r2已读完整修复/14tests/750steps/新33metadata/byte-provenance与真实离线命令，批准限定T002 C实验DONE，无新增S0/S1。仍非普遍安全认证；不复用旧429成绩，不运行恶意checkpoint/PoC。完整s3prl生产依赖pip check exit1缺3项，留固定T024安装闭合；许可B001及质量/硬件门继续生效。

B004（T016 native DNS validation，BLOCKED_TOOLCHAIN）：新11文件已保存但未编译/测试。cargo第一例windows0.62.2 rustc访问异常c0000005；隔离compile-only成功后实际应用检查syn2.0.119 MIR assert ICE101。完整failure1/2及compiler-diagnostic、compiler-integrity在docs/evidence/T016-20261005/native-dns/。官方stable清单摘要核查仍1.99.0，官方77,093,924字节编译器包SHA匹配，33已安装bin/lib文件逐字节SHA一致；syn2/syn3/windows缓存包锁摘要和解包源一致。原因UNKNOWN，当前数据不能归因硬件或证明修复。最小解阻：可修复该ICE的兼容正式工具链/独立稳定环境验证；任何降级最低Rust、改BIOS/安全设置或外传bug材料先走具体变更/授权。T008独立作用域继续，若同样compiler失败只记录，不盲试。
