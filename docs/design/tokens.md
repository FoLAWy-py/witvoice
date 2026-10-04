# T021 Tokens 与布局规则

权威值在packages/ui-tokens/tokens.json；CSS由validate.py --write-css生成，frontend消费变量不复制颜色/尺寸。默认深色、浅色通过data-vc-theme显式切换，系统字体不下载外部字体。单克制蓝强调，不做装饰频谱/持续GPU动画。

语义颜色：canvas页面、surface卡片、elevated浮层；text正文、secondary次文字、border可操作边界、accent主要行动、accent_text实色按钮文字、focus焦点、success/warning/danger状态文字/图标。状态背景使用三个普通surface，彩色文字不放另一彩色背景。全局opacity不能稀释普通/次文字或安全控件对比度；禁用原因仍可读。

正文14px、行高1.5，控件最小36×36CSSpx（规格底线32）；间距4px基数，常用8/12/16/24，圆角8px。标题建议20/24px，主路由不挤为小字。焦点2px轮廓、3px偏移，边界/焦点在三个表面需≥3:1；所有正文、状态文字及实色按钮文字需≥4.5:1。静态公式参考[W3C文字对比说明](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html)和[非文字对比说明](https://www.w3.org/WAI/WCAG22/Understanding/non-text-contrast.html)，通过前不四舍五入；静态token数值不证明最终渲染像素/可访问性。

窗口最小1024×680；另验1280×800逻辑布局和125/150/200%系统缩放。狭窄时内容滚动，导航折为文本紧凑栏，路由卡片换行；不裁切Start/Stop/Mute，不固定巨大图表，不缩小正文。最上层固定控制区不得覆盖内容焦点，含足够滚动余量。Dialog限制最大可见高度、内部滚动，操作区保持可见。

meter显示≤10Hz、诊断≤2Hz，线程/PCM不进入UI。数值更新不动画成虚假连续值。短交互动效100/150ms，reduced-motion将duration变量置0；状态更新永不等动画完成。后续组件仍需尊重该变量，CSS文件自身没有播放动画。

验收命令：bundled Python3.12.14 packages/ui-tokens/validate.py（主题全部合法对比组合、规格尺寸、生成物漂移、10Node枚举覆盖）。真实渲染/键盘/缩放/实际HCI/音质签收留固定T022/T025/T029，不能用静态脚本替代。
