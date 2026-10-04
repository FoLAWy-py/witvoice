# crates/transport

backend负责QUIC、discovery、pairing、framing；可靠控制与有期限的PCM分开。

T016 当前切片实现公开身份、显式本机信任表、严格数字局域网地址及有界发现记录。构造对象/单测不绑定 socket。公开身份版本1，最大16KiB，公开 DER 最大4KiB；标准 P-256/SHA-256 自签证书、有效期与 node UUID CN 绑定均核验。私钥不进入公开导出、Debug或诊断；Windows持久化经平台当前用户 DPAPI。五年证书期限，不自动续期/换钥；更换指纹先撤销并明确重新配对。

信任表最多128节点，默认计算和音色传输授权均false；同名、发现地址、endpoint hint 都不是信任。批准/撤销更新checked generation，撤销和重新授权使旧Permit无效；代际溢出清空信任并永久拒绝再授权。`verify_presented`只做实际DER与固定证书核验，不证明TLS私钥持有，也不代表双向配对完成。T017负责真实mTLS握手，T018负责将吊销同步接到session停止/token与临时profile清理；目前未接入Node命令。

手动输入仅RFC1918/IPv4 link-local、IPv6 ULA或带非零数字scope的link-local，非零端口；不解析DNS，不接受WAN/loopback/multicast/mappedIPv6。同接口/同子网及绑定监听仍由T017显式LAN授权负责。发现表名称63bytes、最多128条、TTL最多120s、单调时间倒退拒绝，超限拒绝不驱逐；只接收固定公开字段，无任意TXT/路径/token。它尚未接到生产发现后端。

**mDNS adapter BLOCKED，T016保持IN_PROGRESS。** mdns-sd0.21.4 `src/dns_cache.rs:59`起有六个HashMap；`:226` add_or_update在`:277–281` entry().or_default、`:306` record_vec.insert无总容量上限，subtype也插入；其他派生entry不能证明总体上限。service_daemon.rs new_with_port先bindsocket，100/10/100频道容量不限制cache。`enable_mdns`在构造daemon前返回MdnsCacheUnbounded，不能把下游128条记录当上游资源界。最小候选解阻（由leader决定，未实现）：有严格容量/期限的标准原生DNS adapter；或经既有监督独立进程加硬内存界、溢出退出并清空发现。没有fork/改写上游或试开listener。

实际验收命令：`cargo test --locked -p witvoice-transport -p witvoice-platform`、`cargo fmt --package witvoice-transport --package witvoice-platform --check`、`cargo clippy --locked -p witvoice-transport -p witvoice-platform --all-targets -- -D warnings`。真实LAN/mDNS、QUIC/mTLS、Windows↔Mac、Mac Keychain、Node命令和撤销session集成未执行；无音频/GPU权限。
