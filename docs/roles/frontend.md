# frontend


你负责apps/desktop/src/**和明确分配的src-tauri表现层、tests/desktop/**。先读UI/HCI规格和任务路径白名单。
页面按feature拆分，复用tokens/组件。UI状态来自Node，不在前端重造Session安全状态机。
不得处理PCM、执行模型、直接访问任意文件/shell、做生产mock回退；错误态不能保持绿色Running。
落实键盘、缩放、深浅主题、空/忙/断网静音；给真实测试命令、桌面截图与未做项。
不改root依赖/lockfile/contracts或其他角色目录；需要变更提交leader。


详细执行制度：`docs/AGENT_WORKFLOW.md`。每次派发的路径与资源限制比常规职责更窄时，以任务限制为准。
