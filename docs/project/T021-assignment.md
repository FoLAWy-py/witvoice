# T021 设计契约与静态 tokens

M4，依赖T003 DONE；REQ-25/10/12。实际作者leader `/root`，实际只读建议 `/root/frontend`，独立审查 `/root/reviewer_t001`。历史native hci已运行但当前保留的四子角色不含可重派hci，采用明确的leader实现hci职责兼容方式，不声称新hci派发/签收。

白名单 docs/design/、packages/ui-tokens/；禁止Node/audio/worker/contracts、根依赖/lock、真实录音/截图虚构。leader另以整合责任维护STATE/TASKS/HANDOFF。与backend T006完全独立路径，不覆盖其他作者。资源code-writer:leader-ui-tokens；不GPU/audio/系统修改/下载。冻结后释放写入锁。

输入SPEC05、SPEC01控制边界、T003 Rust schema及ADR0002。最多五个短检查：五页面与共享组件；十状态及连接未知；深浅主题静态tokens；键盘/缩放规则；独立只读review。真实Tauri/截图/键盘/缩放/owner试听属于既定后续任务，不填PASS。

验收 bundled Python3.12.14 packages/ui-tokens/validate.py：50对比组合、规格尺寸、生成CSS一致、实际schema十枚举文字覆盖；完整stdout、UTC、exit、冻结commit归档docs/evidence/T021-20261005。人工规则覆盖由独立review读源码/规格核对，不替代应用测试。无新增npm/字体/图标依赖。

停止条件：产物冻结并完成适用静态检查后不再写，独立review最多3正式轮；若实际契约不支持数据，记录T022前leader补充typed payload，不生成fake状态/能力。当前仅设计契约，不宣告HCI或M4完成。
