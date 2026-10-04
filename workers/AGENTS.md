# Worker scope
先读docs/spec/03-model-runtime.md。只实现M0选定真实引擎，固定来源与权重摘要；worker不打开mic。安全解析参考音频，不加载用户任意checkpoint；实际执行后端才可宣告VERIFIED；无Mac证据保持UNTESTED。
