继续M6：完成真实Mac和Windows↔Mac两个方向的局域网闭环验收。你是leader；固定SPEC/7里程碑/35任务不变。

先核对M0–M5已通过的证据与当前commit，然后将本轮授权目标记录为COMPLETE。不要修改原Windows验收结果或降低性能要求。

确认实际授权的Apple Silicon Mac访问能力、原生工具链、麦克风/局域网权限和外部虚拟音频设备。没有真实Mac时可以准备脚本/平台代码，但相应实测必须BLOCKED_EXTERNAL，不能以Windows交叉编译或模拟结果签收。

按T031–T035执行：原生Mac构建→CoreAudio与虚拟路由→选定真实模型CPU/已验证加速后端→Win→Mac→Win和Mac→Win→Mac各自真实闭环→独立review/HCI/安装与最终报告。

远端计算节点不得打开其自身麦克风；profile传输先取得明确授权；网络故障只静音不原声旁路；旧epoch不能播放；两端时钟不能直接相减冒充延迟。

只有所有需求、硬门槛、实际音色签收、两个方向实机证据和内部安装包均完成且无S0/S1才标COMPLETE。否则精确列BLOCKED和最小解阻动作。到COMPLETE后停止新增功能、优化和依赖升级。
