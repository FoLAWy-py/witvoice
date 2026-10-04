# packages/ui-tokens

hci定义权威token，frontend消费；不在每个组件复制视觉常量。

T021实际leader作者，真实frontend只读建议；当前hci原生线程不可重派，兼容来源在TASKS中明确，不假称hci新执行。权威tokens.json→validate.py --write-css生成tokens.css。无新增npm/字体/图标依赖；后续frontend导入CSS，用data-vc-theme="light"切换浅色。完整用法与约束见docs/design/tokens.md。

静态验证：bundled Python packages/ui-tokens/validate.py；输出只证明token对比/尺寸、生成一致与枚举文字覆盖，真实Tauri/键盘/缩放/截图/用户质量签收均另验。T021独立review待，不能把此README当HCI完成。
