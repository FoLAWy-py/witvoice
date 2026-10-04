# crates/session

Node 单一拥有的控制生命周期，直接调用 `witvoice-contracts::state::transition`；UI 不持有第二状态机。本切片无音频、engine 或 worker，因此 `output_is_muted` 表示无资源可授权输出，不能当作物理静音测试证据。

真实支持 GetState、无资源准备的 Preparing→Blocked 和 Stop→Stopping→Idle。准备时推进 epoch，错误准备不能成为 Ready。Start/SetMute 与尚未实现命令返回 ENGINE_NOT_READY；不会自动打开原声旁路。尚未创建实际 session/lease，不假装做了 live session_id/epoch 媒体验证。

同一变更 request_id + 规范化请求返回原响应，已记录 id 冲突返回 REQUEST_ID_CONFLICT。最多记录 128 个完整变更请求（单条不超过 contracts 64 KiB）及响应，不驱逐旧记录以避免重新执行已见变更。只读查询不占变更历史，GetState 总是读当前快照。达到容量后新变更报 BACKPRESSURE；Stop 仍先执行 fail-closed 控制清理再报 BACKPRESSURE，Idle 重复 Stop 不推进 epoch。满历史需显式重启 Node，本切片不实施无界历史或隐式轮换。历史不跨 Node 重启持久化，调用者不得重用变更请求 ID。

`cargo test --locked -p witvoice-session` 验证无资源不能 Ready/Start、epoch/状态版本、幂等与冲突、历史上限和恶意 wire。不将这些纯控制测试算作音频/硬件或完整 T006 验收。
