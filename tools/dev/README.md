# tools/dev

T001 已创建 `doctor.py` 与只读 `windows_inventory.ps1`，仅核查真实 Windows 工具版本、安装位置与 SDK 文件。使用实际 Python 3.11+，不要使用不可运行的 WindowsApps 商店别名。

```powershell
. tools/dev/toolchain-env.ps1
& '<实际 python.exe 绝对路径>' tools/dev/doctor.py --output docs/evidence/<新 run-id>/doctor.json
& '<实际 python.exe 绝对路径>' -m unittest discover -s tools/dev -p test_doctor.py -v
```

2026-10-04 用户批准补齐工具、尽量 D 盘。Rust 安装在 `D:\Software\WitvoiceToolchain\Rust`，`toolchain-env.ps1` 仅为当前 PowerShell 设置 CARGO_HOME/RUSTUP_HOME/PATH，不修改持久环境。已经定义的 Cargo/Rustup 路径优先。

退出码 2 表示存在缺失、失败或未知前置条件；0 只代表版本/发现基线。JSON 保留固定命令、原始退出码、超时、输出截断标记和源码 SHA-256。报告只写入仓库 docs/evidence 下的新文件，不覆盖已有证据。无安装、联网、自动修复或音频访问。

MSVC/SDK 发现不代表编译成功；原生编译、音频、模型、Mac、LAN 和安装均明确未验证。T031 尚未实现。测试验证失败、超时、错误平台、无效 JSON 与证据保护；不属于应用测试。
