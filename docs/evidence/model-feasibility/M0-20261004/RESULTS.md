# T002 · MeanVC2 Windows 本机实验

运行提交：429b7c27713c4549ba6072d25dda18f48ddd4ee9。源：MeanVC2 13acf84c1bf135ea5edad9c245b345289b06b33e；模型 HF 39cdd19522fe896c227da691314d9a0e3b995486。资产/依赖分别见 assets-audit.json、dependencies.json；未上传源语音、未录音/播放/开音频端点。

Python3.12.14/Torch2.5.1+cu121/RTX4060Laptop；VC与speaker实际cuda:0，ASR实际CPU，输入/输出原生16k。模型40ms配置的运行入口实际每次新输入2560 samples=160ms；不能把40ms配置标签当实际输入送帧时长。

实际命令：隔离环境python workers/vc_worker/feasibility.py --mode file --device cuda --output docs/evidence/model-feasibility/M0-20261004/file-repair2.json，退出0。7.5293125s源→7.510s输出，可解码、finite、peak0.645874/RMS0.060374；边界差19.3125ms明确保留，不宣称严格逐样本等长。

实际命令：同python workers/vc_worker/feasibility.py --mode paced --seconds 60 --device cuda --output docs/evidence/model-feasibility/M0-20261004/paced-60s.json，退出0。375steps/60s新输入，平均RTF0.645263（门槛≤0.70），p50/p95/p99处理103.726/126.925/135.528ms（p99≤160ms新输入），最大输入调度滞后1.304ms。稳态输出59.94s（未作文件尾flush）。报告保留每一步原始时长/lag/new_samples，不把重复语境算新输入；本实验将同授权源循环作为新的60s时间线，并非24条质量样本。

初始化7.749s、预热4.565s；GPU峰值allocated1,786,326,528bytes/reserved1,851,785,216bytes。进程私有内存初始化后8,258,564,096bytes、结束4,624,912,384bytes。该值包含Python/Torch/模型/分配器，不是准确拆分后的模型独占内存；CPU归因未测。结束总私有内存约4.31GiB，初始化约7.69GiB；必须在T026定位/解释与4GiB模型规划预算的偏差，不能隐藏。32GiB本机仍能运行，但不据此承诺16GiB机器/游戏负载通过。

唯一GPU lease由leader持有，实验结束已释放。两个针对性兼容修复均有失败证据与源码假设：未训练旧mel-cache路径改为拒绝模块（活跃参数strict加载）；精确剔除speaker训练专用5994×256分类头后strict加载。预算2/2耗尽，后续不得盲修MeanVC2或降阈值；任何生产adapter须保留相同保护与独立验收。

Windows实验实际执行；独立审查仍待。Mac UNTESTED，真实应用链路延迟/虚拟输出/2h长稳/第三方应用/质量owner签收 NOT_RUN。文件RTF、375step的p99仅是worker实验，不能用于声明应用p95≤200ms/p99≤250ms。160ms实际送帧窗口会影响后续延迟门，届时必须实测。

许可：HF card声明Apache2；Microsoftbase项目MIT；Microsoftspeaker指引与UniSpeech项目CC-BY-SA3，本地实验有官方使用说明，但独立二进制条款UNKNOWN；lawlict派生代码许可缺口尚未闭合。内部包/再分发状态BLOCKED_LICENSE_CHAIN。不套用其他ECAPA仓库许可证，不联系上游或发布数据。候选实验通过不等于许可全部通过，也不等于T002已DONE。

依赖记录：用宿主bundled Python读取隔离环境的33项dist-info元数据，退出0，保存dependencies.json；没有把License字段缺失视为许可通过。s3prl仅安装实际推理import闭包，未安装完整训练/上游声明依赖，未运行pip check。先前使用venv解释器直接读取元数据退出-1073740791且没有报告，已保留失败码；该失败不改写已成功模型实验的退出码。
