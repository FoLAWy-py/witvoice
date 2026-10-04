# 阻塞与前置条件

只登记实际事实。模型本机实验已通过，不能由此批准再分发或宣布M0/M5完整通过。

| ID | 任务/范围 | 证据 | 影响 | 已尝试 | 最小解阻动作 | 状态 |
|---|---|---|---|---|---|---|
| B001 | T002资产清单；T025/T028捆绑/发行 | model-feasibility/M0-20261004/RESULTS.md、assets-audit.json、dependencies.json；固定上游README | lawlict派生组件与speaker二进制许可链未闭合，不能捆绑再分发 | 核官方MeanVC2/HF、Microsoft WavLM/UniSpeech及实际dist-info；记录Apache2、MIT、CC-BY-SA3各自作用域；不套用别库许可证 | 获得适用派生代码/独立权重的明确上游许可来源，完成独立复核；不靠用户一句允许下载代替第三方许可 | BLOCKED_LICENSE_CHAIN；不抹除成功实验 |
| B002 | 当前工具执行/审查 | T005-agent-runtime.md、T001-session.md | 默认shell/node在进程创建前ACL失败；reviewer无法直接读盘/复跑 | leader限定命令与明确授权backend/audio只读中继成功；reviewer收冻结全文与完整结果 | 继续既有兼容执行；若需独立复跑，在客户端支持的安全环境执行，不改ACL/全局安全 | MITIGATED_WITH_LIMITATIONS |

后续实际前置门仍未执行：T009/T015虚拟设备与物理采集、T026真实2h、T029完整24/3质量与owner试听、T031–T035 Mac真机及有线LAN双向。状态NOT_RUN/UNTESTED，不能标成失败或PASS，也不能提前代用户授权录音/装驱动。当前缺少完整质量素材的validator结果为BLOCKED，这不阻止T004测量计划本身被独立验收。
