# Native error-path static precheck (not formal review)

Actual readonly /root/frontend; chunks a31cfd/afa51b/1569e9/a56fc7 exit0. No edit, hardware, native test or final approval. Actual versions windows0.62.2/windows-core0.62.2/windows-result0.4.1.

Audio generated wrapper capture GetBuffer/ReleaseBuffer at2438/2441, render3547/3551 and padding2577 use HRESULT.ok/map. windows-result/hresult34–39 negative HRESULT converts to Error. error159–165 calls ErrorInfo::from_thread; ordinary Windows non-windows_slim_errors branch274–280 gets COM GetErrorInfo and com48–52 may release returned IErrorInfo. Packet failure only extracts code and does not format Error; static proof establishes additional COM/error-info work, not definite Rust allocation on every failure. Debug/Display/message could allocate BSTR/String/FormatMessage but those are not currently packet code. Error::from_hresult110–115 only saves code+empty ErrorInfo, unlike From<HRESULT>.

Interface vtable/as_raw at interface27–48 only borrow pointer operations; no QueryInterface/AddRef/Release. Minimal needed fix assigned actual audio writer: raw HRESULT for packet capture/render/padding and release fallback; fixed static operation+i32 error, no ok/into/formatting; ordinary prepare/start/close may keep bindings. Pure failure-injected allocator tests must isolate fixture construction and cover native Rust branch, never claim OS service internal zero allocation. No global RUSTFLAGS/windows_slim_errors change.

Observed source SHA256 (original bytes):
- Windows Audio/mod.rs ced77275abee2209ffdf1327677ebde65edec28a004ed1b4981aa325216e374a
- windows-result hresult.rs 441ea1b7829e8d66ccf3cb4b98c66508e34f2d5d6eb85f2e4be85ffc910a10ea
- windows-result error.rs 1403d158ed3cb7c84dc3998a110d8fcccb842c22806e184d0619a05f775236cc
- windows-result com.rs 3111c836c9a2cabcf01b06fc65af474a1e071d9c887db178105c2b52ec78bdd5
- windows-core interface.rs c8ca6c7ea20a27495412081c0827151758e601ba9887fa75e7a222a94c36297b

Subsequent code/test results remain separate and must be frozen before T007 final r3. This precheck does not consume a formal review round or approve author's implementation.
