# T021 交互映射

实际作者leader `/root`；真实 `/root/frontend` 已读SPEC05/hci职责及冻结43f0459状态/控制/Node/ADR，提供只读建议。历史native hci已运行过，但当前四保留子角色不含可重新派发hci；本切片兼容方式明确记leader实际写作，不称新hci线程。依赖T003；主线仍M1。本文是T021设计契约，不是已实现的Tauri或HCI签收。

产品显示名 Voice Changer，中文文案集中于后续UI文案模块。五主导航固定，路由图常驻控制台：输入 → 处理设备 → 音色 → 虚拟麦克风。单一Node是状态/能力权威；React只显示/控制，不搬运PCM或推理。所有动作等Node实际结果，错误不回退生产mock。

| 页面 | 主要信息/操作 | 空、忙、失败与验收要求 |
|---|---|---|
| 控制台 | DeviceSelector、ProcessingRoute、VoiceCard、SessionControls、StatusBadge、AudioMeter、MetricValue；Prepare、Ready后Start、Stop、独立Mute | 未安装模型/无有效虚拟输出/音色不可用：说明具体缺项，Start禁用；准备中显示真实阶段或“准备中”不伪造百分比。失败横幅常驻，Mute与Stop可达。 |
| 音色 | 导入参考→使用权确认→本机解析→引擎专属profile创建；列表、试听、删除确认 | 空状态引导10–30秒已授权参考；解析/创建可取消并由Node回收；不接受任意模型/脚本。50MiB条件传输上限不等于所有参考文件都允许。当前音色删除先提示会话影响并由Node安全停止/拒绝。试听须明确输出与用户操作。 |
| 设备与连接 | 明确UID/flow的输入、虚拟render、第三方capture提示、可选监听；附近节点/手动地址、双向身份导入、授权范围、撤销 | 名称只供展示，不能认证/替代UID。说明CABLE Input是本产品render、CABLE Output是第三方capture。未验证虚拟路由明确不可用。同名发现不当可信peer；配对不表示已发送语音。 |
| 诊断 | 实际backend/摘要/格式、队列容量/目标/年龄、RTF/p分位、underflow/drop、RAM/VRAM/CPU及来源；预览后脱敏导出 | 未知用“未知/—”，不是0；单列RTT/推理耗时。总延迟显示测得/估算/不可用及边界。导出不含PCM、参考音频、secret、私钥、原始用户路径。 |
| 设置 | 深/浅主题、文案架构、资产位置/固定版本/许可/下载体积、隐私、远端计算开关、窗口关闭行为、退出并停止 | 下载/驱动/防火墙/录音权限分别解释最小操作，不捆绑同意。远端计算默认关；Compute模式不打开本机麦克风。无权限则修复入口，不能静默提升或改默认设备。 |

窗口关闭：第一次选择“关闭窗口并继续运行”或“退出并停止”，先解释正在采集/传输的已知状态及托盘可控性。尚无可用托盘时，不能把“继续运行”作为无入口的默认行为。继续运行只断开UI；退出调用本地鉴权ExitNode并等退出/资源清理确认，Stop ACK不等于Node退出。IPC断开不能宣告退出成功或静音。

远端Prepare前分别展示目标身份、profile条件数据及保留策略；计算授权与条件传输授权分别确认。Ready后的Start再明确展示即将发送的麦克风数据。取消Start不启动捕获/媒体传输；已获授权的Prepare可能已传profile，取消须释放Node临时条件，不能声称从未发送。撤销由Node立即断开/静音，UI等待真实结果。发现/配对/授权/Running四种语义分别显示。

组件边界：共享基元Button、Dialog、Select、Slider、Switch、Tooltip、Tabs、Toast、Alert、EmptyState；共享业务组件用上表名字加PairingIdentityDialog、PermissionGuide、FailureBanner。安全状态不由disabled按钮实现；Node重复校验。目录按dashboard/voices/devices/peers/diagnostics/settings等既有feature分，不增加主导航。

键盘顺序：导航→页面标题/主要内容→路由选择→Start/Stop→Mute→次操作；Mute在加载/故障/失焦仍有普通可聚焦按钮，不靠可选全局热键。Tab/ShiftTab、Enter/Space；Dialog聚焦标题或首个安全控件、约束焦点、Esc取消、关闭回到触发者。删除/退出确认默认焦点放取消，运行中的Stop/Mute不加阻塞确认。错误与忙态使用文字+图标+aria语义，颜色不作为唯一信息。

验收入口：packages/ui-tokens/validate.py仅静态；T022真实client/组件测试、T025实际Tauri、T029真实键盘/缩放/截图及产品owner试听另验。当前Node控制切片尚不具备真实运行能力，界面设计不能把它显示成Ready。仅GetState的Response未含完整session/设备/指标；T022前由leader扩充真实typed payload，不由UI伪造ID、能力或指标。
