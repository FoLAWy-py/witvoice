# crates/transport

backend负责QUIC、discovery、pairing、framing；可靠控制与有期限的PCM分开。

T016 当前切片实现公开身份、显式本机信任表、严格数字局域网地址及有界发现记录。构造对象/单测不绑定 socket。公开身份版本1，最大16KiB，公开 DER 最大4KiB；标准 P-256/SHA-256 自签证书、有效期与 node UUID CN 绑定均核验。私钥不进入公开导出、Debug或诊断；Windows持久化经平台当前用户 DPAPI。五年证书期限，不自动续期/换钥；更换指纹先撤销并明确重新配对。

信任表最多128节点，默认计算和音色传输授权均false；同名、发现地址、endpoint hint 都不是信任。批准/撤销更新checked generation，撤销和重新授权使旧Permit无效；代际溢出清空信任并永久拒绝再授权。`verify_presented`只做实际DER与固定证书核验，不证明TLS私钥持有，也不代表双向配对完成。T017负责真实mTLS握手，T018负责将吊销同步接到session停止/token与临时profile清理；目前未接入Node命令。

手动输入仅RFC1918/IPv4 link-local、IPv6 ULA或带非零数字scope的link-local，非零端口；不解析DNS，不接受WAN/loopback/multicast/mappedIPv6。同接口/同子网及绑定监听仍由T017显式LAN授权负责。发现表名称63bytes、最多128条、TTL最多120s、单调时间倒退拒绝，超限拒绝不驱逐；只接收固定公开字段，无任意TXT/路径/token。它尚未接到生产发现后端。

原mdns-sd0.21.4的六个cache HashMap及record vectors没有总量证明；历史身份片段因此未构造daemon。ADR0003改用现有Windows DNS-SD API，移除该未使用依赖；不fork上游，不把下游128表当上游容量证据。`NativePeers::default()`不浏览、注册或解析；显式LAN批准与非零interface才调用真实平台adapter。只广播node UUID、protocol和监听信息；发现及手动地址仍不能授权任何peer。平台queue最多32、上下文16、browser/advertiser各1、resolve同时8且3s截止；发现记录仍128/TTL120s，绝对单调expiry贯穿回调/队列/resolve，不因poll刷新。未知取消/注销保留有界quarantine并拒绝新操作；close返回false或错误不得标cleanup PASS。Windows OS DNS cache和API临时分配不属于app-owned资源界证明。

`native_discovery_probe`仅显式`--allow-lan INTERFACE NODE_UUID browse|advertise|resolve ADDRESS_OR_SERVICE DURATION_MS`，固定本项目服务、1..9000ms操作与总计10s取消观察，未收到终态则失败；它不监听QUIC/音频或配对。当前只允许编译，真实运行需leader进一步指定接口/操作。原生代码/注入测试结果以本轮日志为准；T016仍待独立review及实际DNS-SD门，Mac Bonjour/双向LAN仍必需且未执行。

实际验收命令：`cargo test --locked -p witvoice-transport -p witvoice-platform`、`cargo fmt --package witvoice-transport --package witvoice-platform --check`、`cargo clippy --locked -p witvoice-transport -p witvoice-platform --all-targets -- -D warnings`。真实LAN/mDNS、QUIC/mTLS、Windows↔Mac、Mac Keychain、Node命令和撤销session集成未执行；无音频/GPU权限。
