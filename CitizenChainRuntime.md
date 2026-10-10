## 工具与依赖的声明和供给职责（2026-10-08）

本产品完全独立管理全部流程所需的工具、依赖及其它资源需求。需求唯一依据为本仓源码、公开声明、锁文件及本产品拥有的准备配方，包括准确版本、平台、官方来源、摘要或固定提交、闭包、验真方式和失败条件；塔塔控制台按当前产品声明提供资源，不维护另一份产品需求或替产品决定版本、来源与流程步骤。

本产品必须能在没有塔塔控制台时完全独立执行全部已实现流程。独立执行时，本产品自行完成可信引导、资源获取、验真、保存、复用及任务工作视图准备，不依赖控制台源码、私有资料、安装位置或资源库。

通过塔塔控制台执行本产品流程时，本产品向控制台声明所需资源并使用其已准备好的供给。控制台先核对并复用已有的匹配工具与依赖；没有的由控制台按本产品声明下载、准备、验真并保存到控制台工具库或依赖库，再交付本产品复用。本产品负责核验交付与自身需求一致并使用资源，不因控制台缺件或供给失败改为自行下载，也不另建同一资源的永久副本；可写包管理器视图与流程过程数据仍归本产品当前任务工作目录。

两种执行方式使用本产品同一声明、锁和流程实现，仅资源供给职责随执行方式改变。该职责适用于本产品全部平台与已实现流程；控制台本身作为产品同样适用。独立模式下资源缺失由产品处理；控制台模式下资源缺失由控制台处理。显式离线缺件、交付失败、损坏、错误摘要、来源漂移或越界必须据实失败，不自动升级、覆盖可疑原件或切换执行方式。

以上为当前职责规范；本次只更新文档，不代表现有资源协议与运行代码已完成接入或通过真实流程验收。历史记录中的“可选供给”或“产品负责缺件获取”仅描述当时实现，不作为当前职责依据。

本仓现行入口以`scripts/build.mjs describe`及产品公开scripts实现为准；本文按日期保留的历史验收只描述当时结果，不作为当前工具、私有调用者或已撤销Publish实现的运行条件。独立塔塔门禁候选的职责和未验收状态见文末。

## 当前工作目录归属

Runtime 与 Node、OnChina 共用完整 CitizenChain 仓的工作边界。生成工作根仅为 `<本仓根>/target`，固定目录、串行领取、清理及输出排除合同统一见 `CitizenChainNode.md` 的“本机固定执行目录”；平台保留在执行身份中，不建立平台目录。本节规定执行职责，不表示当前全部流程已通过验收。

第8、9步完成目录与路径实现、根文档迁移及测试源码维护，未运行测试、门禁、编译或安装。本文唯一原件位于<本仓根>/CitizenChainRuntime.md；产品接口及流程直接以本仓实际代码和声明为准，业务字典库与其检查已撤销，不另建登记副本。历史验收事实不表示本轮改造已经通过验收，统一测试在第10步进行。根技术文档由本仓门禁按原文、JSON解码值及既有补丁快照扫描机密，仅报告路径；文档迁出不减少资料安全检查。


**聊天客户端的逻辑功能只能在 TataChatSDK 中实现；聊天服务端的逻辑功能只能在 CitizenServe.tatachat 中实现。公民、途遇及其他产品只依赖使用。**

CitizenChainRuntime 涉及聊天时只作为依赖使用方；本条不代表尚未接入聊天的产品已经具备聊天能力。

- 消息、会话、群组、加密、协议、传输、同步、重试、聊天存储、附件、通话及聊天界面行为，按客户端与服务端职责分别归 TataChatSDK 和 CitizenServe.tatachat；新增功能、缺陷修复和平台差异也必须在所属产品内完成。
- 消费产品只提供产品入口、身份与业务权益结果、服务地址及授权、主题和公开接口要求的平台配置；只通过公开接口接入，禁止复制、重写、包装成另一套聊天内核或维护产品专属聊天实现。CitizenServe、TuyuServe 的产品身份与权益授权不包含聊天数据面的实现职责。
- 本机开发直接依赖仓库路径；公民、途遇等产品的正式版本依赖塔塔聊天正式 Release；第三方市场分发使用公开市场版本。依赖使用不以公开市场发布为前置条件，也不改变实现归属。

# CitizenChain Runtime 技术文档

## Runtime 修改与链协议边界

任何可能改变 `<本仓根>/runtime/` 的修改、格式化、生成或批量命令，必须先列出完整绝对路径、预计差异和原因，取得用户第二次明确确认后执行。本文档修改不授予 Runtime 源码修改、编译或链上升级权限。

Runtime 账户类型必须使用 `AccountId`；账户、公钥和 SS58 的字段与文本合同统一见 `CitizenChainNode.md` 的“账户标识目标契约”。国家名称的基础字段仅为 `country_full_name`、`country_short_name`；既有语言字段由 `runtime/primitives/cid/code.rs` 的真实接口定义，不另建字段真源。行政区、机构名称和管理员集合字段的消费边界见 `CitizenChainOnChina.md` 的“行政区数据”和“CID 号”。

哈希域签名仅使用 `runtime/primitives/src/sign.rs` 的 `primitives::sign::signing_message(op_tag, scale_payload)`，结果为 `blake2_256(GMB ‖ op_tag ‖ SCALE)`；既有 `0x18/0x19` 二进制前缀域按同文件定义执行，不改为哈希域。Pallet 的非签名业务哈希必须使用所属 Pallet 的 `MODULE_TAG` 作域；提案数据归属按本文“MODULE_TAG 注册表”核验。

## Runtime 创世身份 API

ChainIdentityApi::genesis_hash保持API版本1、原方法名和32字节SCALE返回编码。调用时先检查System::BlockHash(0)实际存在，再读取并拒绝零值，最后与冻结的GENESIS_HASH比较；成功返回当前链状态保存的实际哈希，缺失、零值或不一致使API调用明确失败。该API只读，不写入storage，不新增extrinsic、覆盖参数或链身份缓存，也不改变区块执行和既有升级入口。

固定消费SDK的System::finalize在区块哈希裁剪时保留块0，因此超过BlockHashCount后仍核对同一历史身份。GENESIS_STATE_ROOT用于真实创世区块头核验，不与当前运行状态根比较。测试直接调用官方Runtime API分发入口，覆盖正常返回/原编码/只读、缺失、零值、错误、每次查询重读，以及实际System::finalize裁剪窗口边界。

生产验收必须先核对正式链真实块0及区块头，再由旧Node承载新Runtime升级。升级后的链上代码须与获准WASM一致，在同一最终确认块查询本API并与真实块0比较，同时验证持续出块和最终确认；完成后才增加Node对应身份守卫并更新各节点软件。源码API、候选WASM或本地测试不能替代链上验收。

## 合约执行依赖与Ethereum网络标识

SDK正式开发源码为`/Users/rhett/polkadot-sdk`，唯一维护仓`crcfrcn/polkadot-sdk`的main；官方上游固定`paritytech/polkadot-sdk`。它与公民链产品、只读依赖原件和构建缓存具有不同职责，禁止在原件或缓存修改SDK。官方更新必须选择稳定系列和精确SHA，检查相对当前基线的API、存储、共识、执行和工具变化，再保留最小自有改动并完成适用验收；不自动跟随官方分支。

费用兼容实现位于 SDK Revive 的严格原生分支，已准确保存并固定消费为 134a87024fbd9edb5fd5fc22d7ae2ba71090fb46。本仓仅更新 runtime/src/tests/cases.rs 的签名报价、gas 预算和费用边界测试；生产费率、最低费与付款者路由保持既定合同。固定钱包价格为 NativeToEthRatio / 10^7（至少 1 wei），并在完整性检查要求能整除原生单位；公民链为 1 gwei。估算取资源需求与既有 native_quote 换算的费用 gas 两者最大值；已签 gas 必须同时覆盖资源和实际费，钱包参数修复接受 EIP-1559 上限至少为固定价格且优先费不超过该上限，Legacy/EIP-2930 价格至少为固定价格；合法非零优先费不作为框架 tip 收取。该提交的 SDK 费用回归19项、整模块670项及全部目标检查已通过；1项上游统计用例与5项文档示例保持原有ignore，未计为通过。固定消费后的完整 Runtime 库72项已实际通过，零失败/零忽略；真实三节点基础四档费用、报价/签名参数一致及最终性已通过。MetaMask 新消费实签1 GMB交易成功，三个节点最终确认、单次FeePaid10分和回执精确乘积一致；本轮固定消费的完整Node Ethereum RPC模块5项也已实际通过，零失败/零忽略（327项无关节点测试未运行）；其余三档钱包签名、确认页费用/图标仍待本轮验收。执行前捕获同一路由报价，成功和回滚回执均以该费准确换算 gasUsed，effectiveGasPrice 保持固定价格，二者乘积等于原生费的 EVM 表达，不补扣或退款；溢出、余数及超过 u64 的 gas 拒绝。原有 Weight 双维及区块资源限制继续独立执行，非严格模式保持上游规则。API、存储和 extrinsic 编码形状不新增字段。旧固定提交 4fbac6450231e8fe4bf660a3a8f6eb2b4e28c306 的历史验收：2026年10月7日完整 Runtime 库66项测试通过，耗时278.03秒，零失败、零忽略；包含四档金额的 Legacy/EIP-1559 收费、gas缓冲、价格上限、无效价格/优先费/签名预算拒绝及原有成功、回滚和资源失败断言。源码 WASM 实际构建并用于 Node Ethereum RPC 的5项真实服务测试，全部通过；三节点四笔转账单次扣费、回执精确乘积与最终状态一致。具体证据与钱包页面待验收边界记录在唯一任务卡。

SDK本仓门禁固定Node25.2.1、Rust1.97.1（同一对象包含rust-src及wasm32-unknown-unknown目标组件）、Git2.54.0和actionlint1.7.12。公开Workflow独立维护远端输入：官方Ubuntu24.04 x64 Runner已有Clang18.1.3、libclang及CMake3.31.6须实际回读匹配；solc0.8.30、resolc1.0.0和protoc35.0从固定官方原件逐项验真。Git保留HTTPS，libcurl及expat仅安装两份已列明版本/摘要的官方开发包，系统对应运行库及zlib开发输入不匹配即失败，不联网解析其他包。一次准备在源码外本仓独占临时根内，以同对象Cargo读取准确SDK锁及Rust标准库锁，联合1556项官方原件/完整展开清单/逐文件摘要和两锁不变均通过后交付独立离线目录源。全部编译及测试保持--locked --offline、完整夹具和WASM；宿主生成器变化同时验证原生、真实WASM执行器及文档测试。SDK不依赖控制台运行。控制台按本机已保存SHA完成同提交门禁和准确push事件的远端回查，成功后仍须验证远端main未漂移。

SDK执行器测试夹具的missing_external与yet_another_missing_external通过WASM专用
link(wasm_import_module="env")属性声明为宿主导入，保留既有执行时缺失函数trap
回归；不提供假宿主实现，也不放宽其它未解析符号的链接检查。此属性只作用于该
上游测试夹具，不改变公民链生产Runtime或执行器的宿主函数策略。

Git使用官方NO_INSTALL_HARDLINKS安装选项并回读主程序链接数为1；独占普通文件校验保持不变，拒绝时报告准确路径。远端门禁的源码验证区间固定为本仓合同initial_sha至当前push的准确head，覆盖初始化后的全部自有改动；上一轮失败后的工具补修也必须重新验证金额边界相关crate和宿主接口。本机提交区间及远端push事件身份检查保持原规则。

远端Linux x64原生目标固定交付CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=--cfg=rustix_use_libc，选择原锁rustix已有的官方libc后端，避免其linux_raw内部属性与Rust1.97.1冲突。remoteTools拒绝目标值缺失、误值及全局RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS覆盖；WASM目标仍按既有真实构建路径验证。libc0.2.172和errno0.3.10均在原锁及同一联合闭包内，不改锁或原件、不增加依赖版本；完整Linux矩阵须实际验收。

WASM子Cargo设置自身RUSTFLAGS后，明确清除父进程CARGO_ENCODED_RUSTFLAGS及CARGO_CFG_RUSTIX_USE_LIBC，避免父原生目标派生cfg使子构建后端与依赖图不一致。SDK既有门禁文件的第15项回归使用公开交付的同一Rust/Cargo1.97.1、零外部依赖临时包及冻结锁，真实验证继承父cfg失败、隔离后通过；构建脚本保留已交付的Apple宿主链接器、SDKROOT和DEVELOPER_DIR，受管PATH不扩大。全部检查保持离线，临时目录自动清理。

上一轮消费清单经第8步二次准确确认固定为`add12c738a8510cb2253e4a98393e8955286e2b6`；SDK修复已通过52项RPC回归，新的产品三节点源码WASM/TLS/HTTPS/GRANDPA、分叉、迟到、原库重启及单次收费验收3项全部通过，耗时943.77秒；Runtime源码无差异，正式链升级及部署RuntimeVersion仍待受保护读取；第6步本机离线开发验收已完成，官方父基线为`0bf9cb31b867b91b2e9cca3cf59ef3014e430a0b`。现有自有差异包含网络/PoW/SS58改动与21份Workflow，必须逐项核实，不能把整个差异都认定为公民链必需功能。旧分支154590c2839f28095237dde09e81b854ccb58a66保留历史；初始化main不自动合并该分支。未来GMB整分金额与费用改动只在SDK正式源码开发，验收后保存和门禁推送；全部消费声明与锁统一到获准准确提交时，才变更公民链依赖。SDK合并成功不等于公民链升级成功。

`citizenchain/Cargo.toml`统一声明`pallet-revive`，与现有FRAME组件使用
`https://github.com/crcfrcn/polkadot-sdk.git`的固定提交
`134a87024fbd9edb5fd5fc22d7ae2ba71090fb46`。Runtime通过工作空间引用该依赖，
关闭默认feature，由自己的`std`和`try-runtime`分别传播对应feature。
REVM由Revive引入，已由Cargo在公民链自己的`Cargo.lock`中固定为`27.1.0`，
Revive解析为`0.19.0`；SDK仓库的锁文件不会被Cargo自动继承。
第1步锁解析新增110个包，没有移除当时已有包坐标；第6步当前固定消费及完整闭包见下文。
新增同名不同版本属于上游传递依赖，不新增第一方直接依赖的版本分叉。

第5步已在获批源码中注册Revive pallet35并接入Ethereum交易包装器，真实Runtime、WASM和Node执行验收已通过。Ethereum RPC在第6步获批源码中接入，本轮固定消费、原生锁验证及真实HTTPS/WSS/WASM分叉、最终链和同库重启验收均通过。
Runtime benchmark已登记Revive并传播其feature；benchmark/try-runtime检查在第5步已通过。正式业务权重保持不变。

第6步正式依赖补充已在SDK与公民链根工作空间声明同一radium0.7.0固定Git补丁，来源为`https://github.com/paritytech/radium-0.7-fork.git`，提交为`a5da15a15c90fd169d661d206cf0db592487f52b`。Cargo不继承Git依赖仓根patch，实际消费工作空间各自声明。公民链已按准确差异授权写入新固定SDK消费及完整RPC闭包，根锁1844包、SDK328包；第一方futures-timer由3.0.4统一锁为3.0.3，仍符合原version="3.0.2"范围，使用已登记的完整WASM依赖，不补下载。iana-time-zone0.1.65继续连接windows-core0.61.2，num_enum_derive0.7.6继续连接proc-macro-crate2.0.0，未接受额外重选。QR当前实际锁没有radium或SDK包，不添加未使用的补丁。根、QR及真实WASM夹具的原生锁验证已通过；本轮Runtime64项与原生收费/签名112项库测试及5项金标集成测试全部通过，benchmark和try-runtime检查通过。真实WASM执行4项已通过，覆盖实际导出上的标准预编译/非法输入/签名单次费用及既有部署/回滚；完整默认Node330项回归、默认二进制构建及真实HTTPS/WSS/WASM网络验收均已通过，包含分叉、最终链与同库重启；本轮只完成本机离线开发验收，生产证书部署、正式链升级及MetaMask实际连接归第8步。

已验收SDK经本机独立保存为`ac4a17f99d39e6e67b47a9e809351a763fe789f0`，源码与通过的验收快照完全一致。产品根与QR已统一正式消费该40位提交，正式锁及两声明回读摘要与获准候选一致。离线原件只由已保存提交物化，唯一依赖库中的旧SDK来源/174坐标已替换为新来源/328坐标；17仓83份正式Cargo文件无旧提交消费、索引零引用及新bundle包含旧提交历史均核对后，仅删除旧SDK原件。产品原生锁验证、完整Runtime64项、真实WASM执行4项及完整默认Node330项均通过；默认Node二进制构建和真实HTTPS/WSS分叉、最终链及同库重启验收通过。结果限定为本机离线开发验收，不代表远端CI、正式链升级或生产部署。

已选定Ethereum ChainId为十进制`2027`（RPC十六进制表达`0x7eb`），后续Ethereum
签名和Revive配置使用同一编号。现有`SS58_FORMAT = 2027`是地址格式前缀，与ChainId
含义和配置分别管理，不能用SS58前缀字段替代ChainId。第5步Runtime执行验收已通过，MetaMask真实连接归第8步正式网络验收。

第4步账户与交易方案及Runtime准确文件二次确认均已取得，源码与开发验收完成。
公民链已在既有`runtime/primitives/src/core_const.rs`声明独立
`ETHEREUM_CHAIN_ID: u64 = 2027`；第5步Revive配置直接绑定该唯一常量。
原32字节`AccountId`、CID/机构派生、余额账本、原生GMB业务签名及多签授权保持原合同。
Ethereum发送者只采用真实签名恢复的H160，复用官方AccountId32Mapper的
20字节地址加12字节0xEE映射；无新增余额或绑定交易，不能借地址映射接管现有账户。

严格原生模式的已签入口支持Legacy、EIP-2930和EIP-1559，全部要求准确ChainId；
未知类型、无链/错链、非法r/s/y_parity、非规范RLP和尾随数据拒绝。
低s检查仅用于交易入口，不改变ecrecover预编译的ECDSA恢复语义。
nonce沿用System和CheckNonce；上游最大值会回绕到零，严格入口提前拒绝不可递增值。
Ethereum保留标准签名格式，不能把原生GMB业务签名域拼入Ethereum交易。
来源标记SetOrigin的布尔字段不进入编码，仅在验签后由Runtime代码设置；
原生载荷、签名字节和交易编码以实际回归核对，编码不能伪造EthTransaction来源。
费用仍走第3步接口，未授权的原生业务包装入口继续拒绝。

第4步已在SDK测试Runtime验收账户和签名规则。第5步已实现并验收正式Revive Config、Ethereum交易包装器及chain-signing组装；SDK最终固定SHA已用于本机离线消费，远端门禁独立判定推送成功。第6步本机RPC开发验收已完成；MetaMask真实连接归第8步正式网络验收。
2026年10月5日完整Revive回归666项通过、0失败、1项上游既有忽略；七项新增账户/验签
回归及真实交易扩展、REVM调用通过，合约失败按确切事件记录并核对value回滚和单次扣费。
公民链primitives/onchain/chain-signing共116项通过、0失败、0忽略；其文档测试均为0项。
SDK Revive no_std wasm32编译通过；原生费用规则及两仓冻结Cargo锁摘要保持不变。

公民币仍按原生两位小数记账，单位、发行规则、最低余额及收费规则保持现有实现。
金额适配必须解决Revive当前`NativeToEthRatio: Get<u32>`无法表达`10^16`倍率的问题；
收费适配必须解决生产`FeeInfo`要求`BlockRatioFee`与当前按交易金额收费配置的差异，
不能用默认费用实现或测试用`FeeInfo = ()`替代生产设计。

依赖接入验收须覆盖固定SDK来源、REVM锁版本、默认Runtime与Node、WASM、已有
benchmark和try-runtime构建、适用回归测试及节点真实运行。2026年10月1日已完成
离线锁解析、默认Runtime release及源码WASM构建、try-runtime检查；Runtime、primitives、
onchain现有156项单元测试和5项文档测试通过。生成WASM的锁中Revive、REVM及SDK
提交与产品锁一致。WASM构建所需18项标准库依赖只在任务缓存引用已登记Rust工具原件，
未修改工具、产品源码或构建流程。

既有runtime-benchmarks首次检查失败：`runtime/misc/citizen-identity/src/benchmarks.rs`
的7处身份调用未同步当前接口和完整授权签名。经扩大范围二次确认，已补齐调用参数，
统一按包含创世哈希、当前身份版本和有效期的`CitizenIdentityAuthorization`签名。
授权有效期使用当前链上时间加既有最大期限，版本读取真实身份版本存储，保留生产验签。
此修改仅同步benchmark夹具，不更改生产身份接口、签名域、授权规则或权重值。
修复后benchmark feature原生检查通过，身份模块83项回归通过，真实验签回归通过。
普通Runtime、primitives、onchain的156项单元测试和5项文档测试再次通过，try-runtime
检查通过；默认Node构建与150项核心回归也通过，当前源码WASM策略探测和真实P2P
坏块拒绝均实际执行通过。所有结果使用本轮清空后重新准备的受控环境取得。

第一步依赖接入已完成开发验收。SquarePost基准沿用模块既有`sp_std`依赖显式导入
`vec`宏及`Vec`，修复no_std编译；联合投票两条生产回调测试仅在生产配置运行，
benchmark配置单独验证既有`Ignored`回调且发行状态不变，没有改生产回调逻辑。
本轮生产Runtime、primitives、onchain共156项单元测试及5项文档测试通过；benchmark
配置下身份83项、Runtime55项、SquarePost38项单元测试全部通过，另有2项既有文档
示例标记ignored。生产与benchmark源码WASM均构建成功；5条身份基准CLI实际执行
通过（2 steps / 1 repeat，仅验证运行路径，不用作正式权重校准），输出只写任务缓存。
当前生产WASM还通过真实节点客户端的候选Runtime策略行为探测，没有因缺少WASM跳过。
产品锁与WASM生成锁均固定Revive0.19.0、REVM27.1.0及原SDK提交；正式权重没有改动。
这只完成依赖基础，不表示已经注册Revive、提供Ethereum交易或具备EVM功能。

FRAME benchmark用于开发阶段测量执行和存储资源，生产使用事先生成的WeightInfo控制
区块资源。当前WeightToFee及LengthToFee均为零，实际金额由OnchainChargeAdapter和
primitives::fee_policy计算，benchmark不会替代公民币制度收费。生产构建不启用
runtime-benchmarks；新增EVM时资源计量与收费适配分别设计。

### 第二步金额适配（锁解析完成，待消费验收）

2026年10月4日用户确认本步方案并明确“二次确认”，授权现有两份Runtime primitives
文件及任务卡登记的SDK修改范围。金额边界回归及WASM目标源码库编译已通过，
本轮已二次授权将产品清单与锁统一到已验收SDK提交
`1af3efdc2efddee0d40f7e6920ed6dd95deae9ee`；清单已切换，Cargo原生离线锁解析完成，消费验收尚未执行，不能把正式SDK工作树中的实现视为生产已接入。

金额补丁最初保存为SDK本机候选`e539564e42fed793e2cb45e5cbb34e233e1bc8cb`，
该最初候选随后经补修提交到SDK main；最终固定提交的完整远端验收结果见下文。
SDK完整门禁前置修复仅涉及自己的工具、离线锁闭包和夹具子构建环境，保留真实Rust、
solc EVM及resolc PVM夹具编译与完整lib测试；此前跳过夹具条件下的边界回归不能替代它。
本轮完整夹具构建及3项测试、Revive完整lib的651项测试已通过（另1项为上游既有ignore）。
宿主函数生成器已按独立二次确认增加仅适用wasm32的env导入声明，修复Rust严格链接
检查下的宿主符号声明；原生及RISC-V接口保留原行为。四处SDK Runtime的完整check、
WASM构建及lib测试已通过，合计42项成功、1项上游既有ignore；全部WASM宿主函数
均明确从env导入。SDK既有WasmExecutor实际宿主调用19项及runtime-interface文档示例
4项全部通过，包含缺失宿主函数、无效UTF-8和内存释放路径。前置修复已真实保存为
`e5bd88306e05c79325aff99f6ce1e35b30a2f1f2`，该准确洁净快照的七个变化crate完整门禁及
原生1项、WASM执行器19项、文档4项均通过。独立远端准备补修已保存并推送为
`a93ed7b73119fdf4edb512943dba616bd9d53ba1`，本机准确快照完整门禁通过；远端工具准备
通过后在独占普通文件校验失败，尚未开始远端联合闭包准备及编译。后续无硬链接安装及
完整自有差异验收补修已保存并推送为`61940104202fae6c44303a6d04e49f9643b2b5e7`，
14项门禁回归、本机准确SHA门禁及远端1556项完整闭包准备通过；远端随后在首个Runtime的
WASM构建失败，因缺少同一Rust的wasm32-unknown-unknown目标组件，尚未完成完整Rust验收。
同版本WASM目标补修已保存并推送为`7c096069384c67818b49cda5d61628cdefb9505b`，
14项回归及actionlint、本机准确SHA增量门禁通过，远端main回读一致；同push
Run37270776741最终失败：WASM目标安装及1556项联合闭包准备通过，按门禁执行顺序已越过
首个Runtime的check，在其lib测试编译时遇到锁内rustix0.37.23的Linux原始系统调用后端
使用Rust1.97.1拒绝的rustc内部属性。后续完整矩阵未执行，不能标远端验收成功。
原生官方libc后端交付补修已保存并推送为`fb919c92e8f2929059f2e60a670b6ae834b4f755`，
14项回归、actionlint及本机准确SHA增量门禁通过，远端main回读一致；新push
Run37332072845最终失败：工具交付及1556项闭包准备通过，首个Runtime的lib测试触发
westend-runtime的WASM子构建时，父构建CARGO_CFG_RUSTIX_USE_LIBC泄漏使rustix0.38.42
错误选择libc后端，而子Cargo依赖图没有该后端依赖，产生28项未解析导入错误。
真实同版本Cargo离线诊断已复现继承时失败、清除该变量时通过；正式源码隔离补修本轮已二次确认，15项直接回归通过并保存为25a40b8139099753dfb12bbbda412577fb23149f。
正常App任务890700097在上传前的测试检查失败，未触发新远端Run。新增测试的子Cargo漏传已有macOS宿主链接器及SDK输入，受管PATH诊断实际报linker cc not found；已在同一获准测试中保留调用方公开交付的DEVELOPER_DIR、SDKROOT及Apple宿主目标链接器，仍使用同一Rust对象和冻结离线锁。完整矩阵尚待验收。
修正后现有交付函数的受管环境15项全部通过，正常App保存1af3efdc2efddee0d40f7e6920ed6dd95deae9ee，并通过准确SHA本地增量门禁（changed_crates为substrate-wasm-builder）；上传及远端main回读一致。完整push Run37339936197、Job111864145981最终均success。
正常App任务624947459于2026-10-05T16:49:31.524Z因等待超时收口failed/exit1，准确Run当时仍在运行。控制台回查固定上限30分钟，SDK Job固定上限240分钟；二者的终态必须分开核实，当前没有重新推送或改控制台流程。
该准确Run及Job随后completed/success，17:11:04.7752914Z门禁成功JSON覆盖initial_sha至1af3efdc的全部8crate；1556闭包、15项Node回归、8crate的check/lib及3组宿主检查按同提交严格实现完成，未跳过夹具或WASM，远端main最终回读同SHA。Rust单项计数未在成功日志输出，不能使用Mac计数替代Linux证据。SDK源代码完整Linux验收已通过。用户确认后，真实App通过现有NOOP流程回查同一成功Run；任务927260544于2026-10-05T17:28:05.526Z真实success/exit0，main与准确Run保持1af3efdc，无新提交或新Run。获准旧Run37270776741、37216489940及日志已永久清理，两者Artifact均0；远端仅保留当前成功37339936197与较新失败37332072845。原App超时失败记录保留，费用及公民链消费SHA未改。
本次App在远端执行期间先因回查连接失败收口，远端Run后续失败由准确Job日志独立核实；
二者不能混为同一错误，也不能标完整远端验收成功。
消费接入统一固定SDK提交，原费用模型保持不变；当前验收结果以唯一任务卡为准。
SDK消费提交由本仓Cargo声明与原始锁文件固定为7f115f0825a59df94faabe7074ec83c3e9463c7d；锁定Git原件保留该准确提交与完整历史，registry原件按产品锁定版本与摘要物化。消费验收使用--locked --offline，禁止解析时漂移包、版本、来源、checksum或传递连接。完整离线闭包、源码WASM、Runtime与Node的实际验收证据只记录在唯一任务卡。

原生账本永久按`u128`分记账：`TOKEN_DECIMALS=2`、最小1分、100分=1 GMB、
最低余额111分。`EVM_DECIMALS=18`仅为兼容表达，`NATIVE_TO_ETH_RATIO=10^16`。
原生转EVM使用U256精确乘法；反向转换先验证余数为零，再验证商不超过u128。
零金额允许，非整分、超界、加ED溢出都拒绝，不截断、不舍入、不另建零钱账本。

Runtime仅在`runtime/primitives/src/core_const.rs`声明上述表达常量，
`runtime/primitives/src/lib.rs`验证原生单位、ED和既有费用不变量。
转换算法唯一位于正式SDK `/Users/rhett/polkadot-sdk` main 的Revive边界。
`Config::NativeToEthRatio`扩展为`Get<u64>`，4处现有SDK Runtime继续使用原倍率。
新增`StrictNativeBalance`是编译期金额策略：整单位模式要求u128、10^16和原生押金backend；
其他SDK链默认保留原dust行为，但倍率须可安全表达为u32。

三类绕过入口分别封闭：直接bare调用、内部CALL/CREATE/转账及底层burn/transfer都验证
金额和已有dust；余额覆盖setter收为crate内部，公开RPC覆盖包装在必定恢复的状态事务；
创世先完整核对原生已分配余额，再登记元数据，非法金额不能仅告警后忽略。
保留既有dust存储格式；整单位模式遇到非零dust拒绝支付和覆盖，读取也不得将异常状态
静默转换为合法余额。未定义清理旧dust的迁移，不能自动抹除或接管非法历史。

System账户已存在不能代替原生余额检查；合约、WithInfo预编译和新收款账户的111分ED
必须从调用发起者已有余额转入，不能mint、
deactivate或在销毁时配对burn。外层CREATE也必须登记本次创建集合；构造函数内SELFDESTRUCT的待删除记录同步最终
代码哈希，防止跳过删除或删除失败留下ED。
销毁将真实余额含ED转给受益人，PGAS backend及其
迁移在严格模式下拒绝。整次bare执行和延后押金共同提交：失败或REVERT恢复本调用
未提交的余额、合约状态和ED转账；已经收取的制度费用不因此退回。

公民链费用模型保持现状：链上最低10分、链下最低1分、投票固定100分；原收费公式、
付款人、80/10/10分配、Free路径、非零tip拒绝和不退交易费均不改。
1分只是原生金额协议下限，不能覆盖上述收费。ERC-20资产精度不属于原生GMB边界。

第3步费用适配直接复用现有 `onchain::charge_details` 只读入口，计算函数体不变；
免费及业务内清算返回无外层扣款，拒绝项继续拒绝。实际付款、单次扣款、FeePaid和
80/10/10分账仍由现有OnchainChargeAdapter及OnchainFeeRouter完成。SDK不复制费率、
最低费、分类或付款账户计算。内部CALL/CREATE数量和底层余额转移不新增收费事项。

SDK的生产NativeInfo通过NativeFee读取Runtime原有报价，并在类型上要求其Charger与
交易支付扩展配置相同；不要求BlockRatioFee或费用信用hold，也不能用FeeInfo=()开放入口。
验签后的交易转换只读校验；真正付款仍在交易扩展prepare阶段。模拟复用同一收费器，
整体存储事务必定回滚，不铸币模拟费用。执行使用原生余额backend，SDK存储单价必须为零，
既有111分ED依旧从实际资金转移，不能挪用手续费或创造ED。

Ethereum gas只界定执行资源；以已测量evm_opcode权重及区块双维Weight比例换算，
proof_size也受上限约束，不以gas决定公民币费用。钱包已签金额上限只能限制授权，
不能成为扣款公式。REVERT或耗尽资源恢复业务状态，已扣交易费不退；收据不做金额
舍入补扣，实际费用以FeePaid为准，不承诺gasUsed乘gasPrice等于公民链实际费用。
原生业务包装及合约内部runtime派发在未完成逐项授权与原有费用路由前继续拒绝。

第3步源码开发验收已通过：SDK Revive完整回归659项通过、0失败、1项上游既有忽略；
公民链onchain及primitives回归110项通过、0失败，SDK no_std WASM目标编译通过。
SDK测试Runtime实际执行了验签、交易扩展扣款、REVM、失败及模拟回滚；这不代表生产接入。
上述第3步结果只证明该阶段费用适配。第5步正式Runtime账户/交易入口已完成执行验收；RPC属于第6步。
固定SDK消费SHA不因工作树改动自动切换。

### 标准Precompiles及RPC费用边界

Runtime的`Precompiles = ()`仅表示没有自定义业务预编译；官方集合仍组合Builtin，
支持0x01至0x09及P256验证0x100。KZG点求值0x0a仍为明确不支持，不能宣称完整
Cancun预编译支持。第6步只在既有Runtime测试中增加真实签名入口、共识向量、失败
输入、P256无效签名和模拟回滚覆盖，不修改Runtime生产配置或费用实现。

原生业务不增加Precompile或新ABI；任何合约内部Runtime派发仍受既有过滤、权限
和收费路由约束。不能把SDK的System/Storage内置工具当作已授权的公民业务入口。
模拟必须整体回滚账户nonce、余额、存储、事件及费用；实际签名交易继续只在原有
交易扩展扣一次制度费用，内部调用和预编译均不增加手续费、tip、deposit或退款。

Ethereum RPC回执的gasUsed/cumulativeGasUsed只描述资源，不参与GMB扣费计算。
真实费用继续以OnchainTransaction::FeePaid及既有费用查询为真源，倍率10^16、
111分ED、付款人和80/10/10分账保持原合同；非整分金额继续拒绝。第6步新增两项
标准向量/非法输入及模拟根回滚测试已包含在实际通过的完整Runtime64项中；
对应真实WASM执行4项已全部通过，禁止原生回退；完整默认Node330项及默认二进制构建通过，真实HTTPS/WSS验证签名执行、回执/日志、重组、GRANDPA最终状态及同库重启。本轮费用与ED分别核验，费用制度不变；结果限定为本机离线开发验收。

第2步的金额边界阶段未注册Revive或接入Ethereum账户/签名/交易/RPC；当前第5步源码状态以本文执行注册章节为准。本任务不改既有业务创世授权或兑换资产。
完成SDK验证并取得准确消费提交授权后，才统一公民链主体、QR协议Cargo声明及实际
受影响锁；不得修改Cargo缓存中的依赖源码。验收覆盖精确往返、非法金额、底层和内部
调用、状态覆盖恢复、真实ED出资及发行量不变。余额查询沿用SDK扣除最低余额和冻结
限制的可用余额口径，账户及RPC步骤须明确该口径，不能额外加到账本或自动补发。

Runtime是完整CitizenChain产品的wasm平台组件，受控缓存固定为 `citizenchain/target/wasm/<build|ci|release|publish>/`，不增加旧仓库层、`runs/` 或 `start/`。开发升级下载、冷签会话工作文件和回执属于 `publish/`；Runtime Build 的 Cargo 目标、日志和临时文件属于 `build/`。

## 2026-09-02 本机 WASM 编译入口

外部调用方 已登记 `citizenchain.wasm.build`。本机 Build 只读完整 CitizenChain 的 Runtime
源码，Cargo 下载与增量目标写入受控依赖和任务缓存，成功后只提交
`citizenchain/target/wasm/citizenchain.compact.compressed.wasm`。流程不启动节点、
不执行链上升级、不复制源码，也不在 CitizenChain 产品目录生成 `target`。

本文是 CitizenChain Runtime 唯一技术事实文档，统一收录运行时模块、链上数据、治理、发行、交易、权重与协议边界。

### runtime admins 技术文档

最新更新：2026-07-20。`citizenchain/runtime/admins/` 由四个 crate 组成，管理员链上状态按管理员类型分别保存在各自 pallet。

#### 模块边界

| 模块 | 职责 |
|------|------|
| `admin-primitives` | 管理员共用类型、生命周期 trait、统一查询 trait 和机构码分类策略；不放业务 storage。 |
| `public-admins` | 公权机构管理员钱包集合，包括 NRC/PRC/PRB/FRG/NJD 固定治理机构；不保存岗位或 FRG 虚拟省组。 |
| `private-admins` | 私法人及私权侧/独立非法人机构管理员：公司、协会、私立学校、个体经营、无限合伙等；公法人下属非法人不得被描述为私法人附属类型。 |
| `personal-admins` | 个人多签管理员和个人多签管理员集合变更。个人多签账户生命周期归 `runtime/entity/personal-manage`。 |

#### 唯一真源

- 机构管理员集合由 `admin-primitives::InstitutionAdmins` 表达；storage key 是 `cid_number`，value 只含 `institution_code + admins`。
- 公权、私权机构和个人多签每项统一为 `Admin { account_id, cid_number, family_name, given_name }`。非空 CID 必须是 CTZN 且与 `citizen-identity` 的 CID↔账户双向索引完全一致。admins pallet 不复制公民身份真源，也不用占位姓名伪造真实资料。
- 姓、名只展示，账户用于人员识别与签名，但账户本身没有机构业务权限；统一结构不保存岗位、权限、任期或任职来源。字段完整性由机构类型、岗位和个人多签规则分别判定。
- 机构岗位定义和任职关系归 `entity`，与管理员人员集合独立：管理员可以没有岗位，岗位可以空缺；岗位变化不得反向生成、删除或覆盖 admins。
- 个人多签继续由 `personal-admins` 独立管理业务和 storage，但管理员项与机构使用同一个 `Admin` 四字段结构；不使用机构岗位或机构任职关系。
- 各类管理员的链上管理员集合分别保存在各自 pallet 的 `AdminAccounts`。
- runtime 当前通过 `RuntimeAdminAccountQuery` 聚合读取各管理员模块；ADR-039 落地后它只能证明人员属于 admins，业务 pallet 还必须通过 entity 解析完整 `RoleSubject` 的权限和有效任职。
- 机构治理阈值由 public/private entity 的 `InstitutionGovernanceThresholds[cid_number]` 独立保存，与 admins 人数、岗位数解耦；admins pallet 不接收阈值。个人多签继续使用 `internal-vote::ActivePersonalThresholds[personal_account]`。任何路径都不建立岗位阈值。
- 创世机构本体、协议账户、机构阈值、默认法定代表人岗位、固定岗位、固定权限和创世任职由 `runtime/genesis/src/institution/seeder.rs` 唯一写入：既有公权机构写 `public-manage` / `public-admins`，公民链技术发展基金会写 `private-manage` / `private-admins`；不得跨命名空间或从岗位伪造钱包。
- 旧创世机构/管理员运行期模块已删除，不允许恢复为运行期治理模块或影子真源。

#### 管理员集合目标字段

| 字段 | 说明 |
|------|------|
| `cid_number` | 管理员集合所属机构 CID 号；个人多签没有机构 CID 时为空。 |
| `institution_code` | 管理员集合所属机构码。 |
| `admins` | 当前管理员人员集合；公权、私权机构和个人多签每项字段顺序统一为 `account_id + cid_number + family_name + given_name`。 |

私权机构和个人多签姓名缺失时，runtime 分别补为 `family_name="管理"`、`given_name="员"`。公权身份字段可保持空值，不填占位值。人员去重读取 `account_id`；机构业务授权和投票资格必须继续校验完整 `RoleSubject`、岗位权限与有效任职。

机构岗位、任期、权限和任职来源不属于本表，统一读取 entity 的 `InstitutionRole` 与 `InstitutionAdminAssignment`。

#### 机构任职来源边界

| 来源 | 写入语义 |
|------|----------|
| `Genesis` | 创世直接写入的固定岗位任职。 |
| `Registry` | 注册局登记或后续任免流程形成的岗位任职。 |
| `MutualElection` | 互选结果经选举业务模块复核后形成 entity 任职结果。 |
| `PopularElection` | 普选结果经选举业务模块复核后形成 entity 任职结果。 |
| `NominationAppointment` | 提名任免最终结果；当前只有强类型来源，尚无合法流程生产者。 |

这些来源由 entity 的 `assignment_source` 保存；admins 不复制来源字段。

#### 生命周期

- 公权机构生命周期由 `public-manage` 发起，只写 `public-admins`；固定治理机构由创世写入，同样只在 `public-admins` 承担运行期管理员治理。
- 私权机构生命周期由 `private-manage` 发起，只写 `private-admins`。
- 个人多签账户生命周期由 `personal-manage` 发起，只写 `personal-admins`；管理员更换 call 为 `PersonalAdmins(29).propose_admin_set_change(0)`。
- ADR-039 目标中，公权/私权机构创建必须在同一原子操作中建立 admins、强制 LR、至少一个初始治理岗位及权限、初始任职和初始投票规则；禁止只创建人员名单和空 LR 后赋予管理员临时启动权限。
- 已完成业务把通用机构治理结果交给 entity；entity 只更新岗位、任职和法定代表人，且任职目标必须是该机构既有管理员。
- admins 不接收岗位结果、不解释岗位变化，也不保存任职来源；管理员的新增、删除、换人和姓名更新由独立管理员维护流程处理。
- 机构管理员名册更换不得附带阈值变更；机构阈值只由对应 entity 配置，任职结果同样不能隐式改阈值。
- 国家储委会、省储委会、省储行、国家司法院固定人数；国家司法院岗位为 7 护宪、1 首席、2 次席、5 大法官。
- FRG 在 `public-admins` 只有一个含 215 个钱包的机构管理员集合；43 个省专员岗位、每岗5人的分组真源在 entity 任职 storage，不存在虚拟省组账户。省域业务只能授权对应省专员 `RoleSubject`，不能授权 FRG 全体 admins。
- 岗位任职不得驱动管理员更换；public/private admins 当前不暴露对外管理员集合变更 extrinsic，管理员维护入口在第2步单独实现。
- ADR-039 目标 NodeGuard 保护强制 LR 和创世固定岗位的码、名、固定权限及制度席位；创世机构可以依法增加普通动态岗位，不能再因存在额外岗位而拒绝。固定岗位成员可依法原子轮换。
- 私权创世的公民链基金会固定为一名程伟管理员、三个固定岗位各一席和机构阈值 2；同一程伟钱包同时任职三岗。固定岗位不能改名、停用、增删或扩席，但不能为了满足阈值而伪造多个钱包或降低阈值。
- `public-admins`、`private-admins` 没有 `WeightInfo` 和 `weights.rs`；其写入仅由 entity 生命周期内部接口调用。
- 正式创世只接受统一四字段 `Admin` SCALE 布局；旧纯账户、旧三字段记录和 runtime storage migration 均已删除，不保留兼容路径。

#### MODULE_TAG

| 模块 | MODULE_TAG |
|------|------------|
| `personal-admins` | `b"per-mgmt"` |

#### Call Index

| 模块 | 管理员更换 call |
|------|----------------|
| `personal-admins` | `29.0 propose_admin_set_change` |

#### 验证命令

```bash
cargo check --manifest-path citizenchain/Cargo.toml -p node
cargo test --manifest-path citizenchain/Cargo.toml -p public-admins -p private-admins -p personal-admins -p public-manage -p private-manage -p personal-manage -p multisig --lib
cargo test --manifest-path citizenchain/Cargo.toml -p citizenchain --lib
```

### entity-primitives 技术说明

模块：`entity-primitives`

职责：实体生命周期共用类型与 trait。该 crate 不含 storage、不含 extrinsic、不保存 CID 登记状态。

ADR-039 已冻结机构岗位权限目标模型。任务卡第 2 步已落地共享授权类型，第 3 步已落地岗位权限 storage、任职有效期和动态岗位生命周期，第 4 步已落地稳定业务动作目录与创世固定权限；投票快照和具体业务接入仍按后续步骤实施。

#### 边界

- 定义 `EntityKind`，区分公权机构、私权机构、个人多签。
- 当前定义 `InstitutionMultisigQuery`，供交易、清算、扫码验签等模块统一查询公权/私权机构账户状态和 admins 人员数据；机构内部/联合投票已改由 `VotePlan` 的 `RoleSubject` 有效任职解析，不得把该查询返回的 CID 全体 admins 用作投票主体。
- 定义 `InstitutionCidQuery`，供 `public-manage` 和 `private-manage` 互查 CID 是否已登记，防止同一 CID 在多个生命周期模块重复写入。
- 定义 `CidInstitutionVerifier`，统一 CID 机构登记、注销凭证验签接口。
- 定义 `InstitutionGovernanceAction` 与 `InstitutionGovernanceProposal`，统一表达本机构内部治理中的 `admins` 完整替换、`InstitutionRoleMutation::{Create,Rename,Delete}`、任职变更和法定代表人结构整体设置或清空；创建岗位不接收 `role_code`，必须原子携带不可变权限和初始任职。
- 定义 `InstitutionGovernanceResult`，作为创世、注册局、投票/选举引擎和本机构内部治理写入 entity 岗位、任职、法定代表人的唯一结果协议。
- 已定义 `RoleSubject { cid_number, role_code }`，作为机构业务授权和机构岗位投票资格的唯一主体。
- 已定义 `BusinessActionId { module_tag, action_code: u32 }`、`RoleBusinessPermission { role_subject, business_action_id, operation }` 和 `AuthorizationSubject` 强类型；个人多签使用 discriminant `1` 的独立 `PersonalMultisig(AccountId)` 变体。
- `RolePermissionOperation` 的 SCALE discriminant 固定为 `Propose = 0`、`Vote = 1`；`AuthorizationSubject` 固定为 `Institution = 0`、`PersonalMultisig = 1`。
- 跨端 SCALE 金标唯一文件为受保护测试资产 `citizenchain/runtime/primitives/tests/fixtures/role_permission.json`；Node 使用本 crate 共享类型逐字节解码，OnChina、CitizenApp、CitizenWallet 对同一金标严格解码并拒绝尾随字段。
- 定义 `InstitutionCapabilityPolicy` 与 `InstitutionRoleAuthorizationQuery`，供业务模块校验“CID 顶层能力 + 岗位权限 + 有效任职”；本 crate 只定义 trait，不保存权限 storage、不选择投票引擎。
- `business_action.rs` 是稳定 `module_tag/action_code` 与受保护创世岗位固定权限唯一目录；协议升级与决议发行采用同一联合权限矩阵：NRC/PRC 委员岗位拥有 `Propose + Vote`，PRB 正式 `DIRECTOR / 董事` 只有 `Vote`。该目录不选择投票引擎，也不表示尚未迁移的业务已经按岗位执法。
- 机构内 `role_code` 与 `role_name` 分别唯一；同名多人属于一个岗位的多个任职席位。一个管理员可以担任多个不同岗位，但同一岗位内不得重复占席。
- 每个机构唯一的 `LR / 法定代表人` 岗位永久存在，任职只能为 0 或 1 人；法定代表人结构与 LR 任职必须原子一致。岗位不保存阈值，阈值属于机构或业务绑定的投票计划。
- 复用 `primitives::multisig` 的账户校验、保留地址、保护地址 trait。

#### 禁止事项

- 不允许在本 crate 增加 storage。
- 不允许把公权、私权、个人多签生命周期状态写到本 crate。
- 不允许恢复单独的 entity-registry pallet。
- 不允许把 `admins`、裸账户、裸 CID 或裸岗位码定义为机构业务授权主体。
- 不允许在本 crate 定义具体业务模块使用哪个投票引擎；该决定归各业务模块。

### personal-manage 技术说明

模块：`personal-manage`

职责：个人多签账户生命周期。只负责个人多签创建、关闭和投票终态回调中的业务
pending 处理。

#### 边界

- 个人多签管理员真源归 `personal-admins`。
- 个人多签转账归 `multisig-transfer`。
- 个人多签机构码为 `PMUL`。
- 不承担公权机构、私权机构生命周期。
- 创建与关闭执行器只接受投票引擎 callback scope 内、owner/kind/stage/status 与本模块生命周期提案完全绑定的回调；关闭还必须匹配 `PendingCloseProposal[account]`，不能拿其它已通过内部提案复用。

#### MODULE_TAG

- `b"per-mgmt"`

#### 费用与 ED

- `propose_create` / `propose_close` 是个人操作，外层最低链上操作费由签名者支付；`InternalVote::cast` 才由实际投票签名者支付固定 1 元。
- 创建执行费按创建金额计算，由创建提案人支付；创建本金同样由提案人转入个人多签账户，两项均不使用机构费用账户。
- 关闭执行费按关闭前余额计算并从个人多签账户收取，剩余余额再以 `AllowDeath` 转给受益人；只有这条显式关闭路径允许个人多签账户死亡。
- 关闭不保存独立固定最低余额常量；提案时直接用统一链上费公式计算执行费，并要求扣费后的转出金额不低于链上 ED；执行时重新校验最新余额。
- 统一收费器必须完整扣款并保留 ED；收费失败不改扣其他管理员，创建/关闭业务状态保持不变。

#### 钱包扫码

- pallet index：`7`
- 创建 call：`propose_create`
- 关闭 call：`propose_close`
- call index 2 永久留洞，不复用。否决、超时和执行失败后的 pending/预留款清理由
  votingengine 终态回调自动完成，不存在人工清理交易。

### private-manage 技术说明

模块：`private-manage`。职责：私权法人、非法人机构、CID 下账户集合、岗位和任职真源。

#### 唯一模型

- 机构唯一主键是 `cid_number`；`Institutions[cid_number]` 保存机构信息。
- `InstitutionAccounts[(cid_number, account_name)]` 是账户正向真源，`AccountRegisteredCid[address]` 只是反向索引；不存在 CID 到账户的重复正向表、生命周期状态或默认账户标志。
- 普通机构必须具有主账户、费用账户；特殊制度账户统一由 `primitives::institution_constraints` 决定。协议账户恰好一个且永久不可关闭，只有 `InstitutionNamed` 可关闭。
- 第 6 步新机构创建业务必须把全部协议账户以零余额原子建立；后续入金另走账户交易。

#### 管理员与岗位

- `PrivateAdmins::AdminAccounts[cid_number].admins` 是机构可任职人员名册，不是执行授权真源；主账户、费用账户和管理员账户均不能单独授权。
- ADR-039 目标授权主体是 `RoleSubject(cid_number, role_code)`。岗位、岗位权限、任职、`InstitutionRoleNonce` 和永久 `UsedRoleCodes` 归本模块；任职只能引用既有管理员。
- CID 顶层能力封顶岗位可授予的 `RoleBusinessPermission`；权限至少区分 `Propose` 与 `Vote`。岗位权限不可修改，变更权限必须删除旧动态岗位并生成新岗位码。
- 动态岗位码固定为 `R_<32 位大写十六进制>`，由 runtime 使用本 pallet `MODULE_TAG`(`b"pri-mgmt"`) 作哈希域生成；调用方不得提供，删除后永不复用。动态岗位只允许依法改 `role_name`。
- 全部机构永久存在唯一可空缺 `LR`，任职只能为 0 或 1；法定代表人原子结构必须与 LR 任职一致。机构内岗位码和岗位名分别唯一，同名多人属于同一岗位的多个席位；管理员可兼任不同岗位。创世固定岗位码、名和权限不可修改或删除，但创世机构仍可增加普通动态岗位。
- `InstitutionGovernanceThresholds[cid_number]` 是私权机构治理阈值真源，与 admins 钱包数、岗位数分别独立。投票引擎只在建案时读取并冻结提案阈值快照。
- ADR-039 目标本机构治理、管理员更换、岗位维护和法定代表人任免分别由业务模块登记岗位权限并静态指定投票引擎；不能因为 `actor_cid_number == cid_number` 或属于 admins 就自动取得发起权。
- 注册局登记管理员同样按注册局 `RoleSubject` 授权；仅属于注册局 admins 必须拒绝。
- 法定代表人只读取 `InstitutionInfo` 三字段，不在 admins 中保存副本。
- 非营利法人“公民链技术发展基金会” `GZ018-SFGYR-201206100-2026` 是受保护私权创世机构：`LR`、`GENESIS_PRODUCT_MANAGER`、`GENESIS_PROGRAMMER` 三岗位的码、名和固定权限不可修改；三岗各固定一席，由同一程伟钱包分别任职，机构阈值保持 2；基金会仍可增加普通动态岗位。
- 公民链基金会依法换人时必须在同一治理结果中更新对应岗位任职；新任人员尚不在 admins 时再原子更新人员名册，已在 admins 时不得为了换岗伪造无关名册变化。执行使用显式 storage transaction 保证全成或全退；法定代表人账户变化还必须与 `InstitutionInfo` 三字段同步。

#### 链上入口

- call 5 已永久关闭并从 metadata/QR/钱包解码移除。普通机构创建由第 6 步的新业务模块原子提交 admins、完整零余额协议账户、强制 LR、至少一个初始治理岗位及固定权限、初始任职和初始投票规则；不得恢复旧直接创建载荷。
- `update_institution_info`（call 6）：注册局管理员更新目标机构名称。
- `add_institution_account`（call 7）：注册局管理员给目标 CID 新增自定义账户。
- `propose_institution_governance`（call 8）：本机构指定岗位任职人发起内部治理提案；SCALE 在 `actor_cid_number` 后固定编码独立 `proposer_role_code`。入口校验完整 `RoleSubject + pri-mgmt/3 + Propose`，再按同一 CID 拥有 `Vote` 权限的岗位构造内部 `VotePlan`。通过后可原子替换 `admins`、变更动态岗位/任职、整体设置或清空法定代表人结构；岗位任职来源必须是 `InstitutionGovernance`，不得伪装成普选、互选或任命结果。
- `register_institution_admins`（call 9）：注册局管理员按注册局授权直接完整替换目标机构 `admins`，用于注册局管理路径，不改岗位任职。
- `propose_close_private_institution`（call 1）：严格使用 `actor_cid_number + proposer_role_code + institution_account + origin`；只有拥有 `pri-mgmt/2 + Propose` 的有效岗位任职人可发起，投票主体来自拥有对应 `Vote` 权限的岗位，并校验账户属于该 CID 且为自定义账户。
- `apply_institution_governance_result` 是内部回调。call 0、call 4 与 call 5 永久留洞，不复用、
  不兼容；关闭提案否决、超时或执行失败后的 `InstitutionPendingClose` 只由
  votingengine 终态回调清除，不存在人工清理交易。

#### 费用与 ED

- 资料更新、新增账户、本机构治理、注册局直接登记管理员和关闭提案的外层链上操作费只从 `actor_cid_number` 的费用账户收取，管理员钱包只签名。
- 自定义账户关闭通过后，执行手续费按被关闭账户余额计算并从 actor CID 费用账户收取；被关闭账户以 `AllowDeath` 转出余额。收费、转账和索引删除原子执行。
- 普通支出与费用扣款必须保留 ED；只有明确关闭的 `InstitutionNamed` 账户允许死亡。

#### 边界与 ABI

- 只接受私权法人和非法人机构码；跨 namespace CID 重复校验通过 `entity-primitives::InstitutionCidQuery`。
- ADR-039 目标外层 origin 必须属于 admins，并对目标 `RoleSubject` 有有效任职和业务权限；注册局凭证只作背书，不构成第二授权。
- 本模块不实现投票或转账；投票归 votingengine，机构账户转账归 `multisig`。
- 本模块作为 entity 不决定具体业务使用哪个投票引擎；该决定必须静态写在对应业务模块。
- pallet index：`31`；`MODULE_TAG = b"pri-mgmt"`。
- 不兼容旧 storage、旧 call payload、旧解码或旧创世数据。

### public-manage 技术说明

模块：`public-manage`。职责：公权机构、CID 下账户集合、岗位目录与管理员任职真源。

#### 唯一身份与存储

- 机构唯一主键是 `cid_number`；机构码只从 CID 解析，主账户不得作为身份或管理员 key。
- `Institutions[cid_number]` 保存机构信息；`InstitutionAccounts[(cid_number, account_name)]` 是账户正向真源；`AccountRegisteredCid[address]` 仅作反向索引。
- 不保存机构/账户生命周期状态、默认账户标志、CID 到账户的重复正向表或创世保护旁路。
- 普通机构强制主账户和费用账户；特殊机构由 `primitives::institution_constraints::required_protocol_account_kinds` 返回完整协议账户集合。每种协议账户恰好一个且永远不可关闭，只有 `InstitutionNamed` 自定义账户可关闭。
- 逻辑账户允许零余额；第 6 步新机构创建业务必须把全部协议账户以零余额原子建立，后续入金另走账户交易。

#### 管理员、岗位与授权

- `PublicAdmins::AdminAccounts[cid_number].admins` 是机构可任职人员名册，每项为统一的 `Admin { account_id, cid_number, family_name, given_name }`。非空 CID 必须引用 `citizen-identity` 的 CTZN CID↔账户绑定。该名册不是机构业务授权真源，主账户、费用账户和管理员账户均不能单独授权。
- ADR-039 目标授权主体是 `RoleSubject(cid_number, role_code)`。`InstitutionRoles`、岗位权限、`InstitutionRoleAssignments`、`InstitutionRoleNonce` 与永久 `UsedRoleCodes` 归本模块；任职只能引用既有管理员。
- CID 顶层能力封顶岗位可授予的 `RoleBusinessPermission`；业务动作权限至少区分 `Propose` 与 `Vote`。岗位权限不可原地修改，变更权限必须删除旧动态岗位并生成新岗位码。
- 动态岗位码固定为 `R_<32 位大写十六进制>`，由 runtime 使用本 pallet `MODULE_TAG`(`b"pub-mgmt"`) 作哈希域生成；调用方不得提供，删除后永不复用。动态岗位只允许依法改 `role_name`。
- 全部机构永久存在唯一可空缺 `LR`，任职只能为 0 或 1；法定代表人原子结构必须与 LR 任职一致。机构内岗位码和岗位名分别唯一，同名多人属于同一岗位的多个席位；管理员可兼任不同岗位。创世固定岗位码、名和权限不可修改或删除，但创世机构仍可增加普通动态岗位。
- `InstitutionGovernanceThresholds[cid_number]` 是公权机构治理阈值真源，与 admins 钱包数、岗位数分别独立。投票引擎只在建案时读取并冻结提案阈值快照。
- ADR-039 目标外层标准 extrinsic 必须同时满足 origin 属于 admins、对指定 `RoleSubject` 有有效任职且岗位拥有目标业务权限。注册局凭证只表达业务背书，不得成为第二授权真源。
- 本机构治理、管理员更换、岗位维护和法定代表人任免分别由业务模块登记权限并静态指定投票引擎；不能因为 `actor_cid_number == cid_number` 或属于 admins 就自动取得发起权。
- 注册局登记管理员同样按注册局岗位主体授权；仅属于注册局 admins 必须拒绝。
- 法定代表人只读取 `InstitutionInfo` 三字段；创世没有真实资料时统一为 `None`，不得从管理员或主账户推导。

#### 链上入口

- call 5 已永久关闭并从 metadata/QR/钱包解码移除。普通机构创建由第 6 步的新业务模块原子提交 admins、完整零余额协议账户、强制 LR、至少一个初始治理岗位及固定权限、初始任职和初始投票规则；不得恢复旧直接创建载荷。
- `update_institution_info`（call 6）：注册局管理员更新目标机构名称。
- `add_institution_account`（call 7）：注册局管理员给目标 CID 批量新增自定义账户。
- `propose_institution_governance`（call 8）：本机构指定岗位任职人发起内部治理提案；SCALE 在 `actor_cid_number` 后固定编码独立 `proposer_role_code`。入口校验完整 `RoleSubject + pub-mgmt/3 + Propose`，再按同一 CID 拥有 `Vote` 权限的岗位构造内部 `VotePlan`。通过后可原子替换 `admins`、变更动态岗位/任职、整体设置或清空法定代表人结构；岗位任职来源必须是 `InstitutionGovernance`，不得伪装成普选、互选或任命结果。
- `register_institution_admins`（call 9）：注册局管理员按注册局授权直接完整替换目标机构 `admins`，用于注册局管理路径，不改岗位任职。
- `propose_close_public_institution`（call 1）：账户型交易，严格使用 `actor_cid_number + proposer_role_code + institution_account + origin`；只有拥有 `pub-mgmt/2 + Propose` 的有效岗位任职人可发起，投票主体来自拥有对应 `Vote` 权限的岗位，只允许关闭该 CID 下自定义账户。
- `apply_institution_governance_result` 是内部回调，不是 extrinsic。
- call 0、call 4 与 call 5 永久留洞，不复用、不兼容；关闭提案否决、超时或执行失败后的
  `InstitutionPendingClose` 只由 votingengine 终态回调清除，不存在人工清理交易。

#### 费用与 ED

- 资料更新、新增账户、本机构治理、注册局直接登记管理员和关闭提案的外层链上操作费只从 `actor_cid_number` 的费用账户收取，管理员钱包只签名。
- 自定义账户关闭通过后，执行手续费按被关闭账户余额计算并从 actor CID 费用账户收取；随后仅被关闭账户以 `AllowDeath` 把余额转给受益人。收费、转账和账户索引删除处于同一事务。
- 普通支出和费用账户扣款都必须保留 ED；只有显式关闭的 `InstitutionNamed` 账户允许死亡。

#### 模块边界

- 只接受公权机构码；跨公私权 CID 重复校验通过 `entity-primitives::InstitutionCidQuery`。
- 创世机构本体、协议账户、岗位和初始任职由 `runtime/genesis/src/institution/seeder.rs` 写入相同真源并校验完整账户集合。
- 本模块不实现投票或转账；投票统一归 votingengine，机构账户转账归 `multisig`。
- 本模块作为 entity 只提供岗位、权限、任职和机构生命周期真源；具体业务模块决定动作权限、静态选择投票引擎并执行通过后的业务。
- 关闭执行器必须处于 votingengine callback scope，并重新校验提案 owner、CID、账户归属、已绑定 `RoleSubject` 授权、协议账户不可关闭与受益人。

#### ABI

- pallet index：`30`
- `MODULE_TAG = b"pub-mgmt"`
- 不保留旧 storage、旧 call payload 或旧解码兼容；开发期重新创世。

#### 1. 模块职责

`genesis-pallet` 只负责：

- 保存 `Genesis` / `Operation` 链阶段；
- 保存开发者能否直接升级 runtime 的一次性开关；
- 在 block#0 写入创世宣言、国家宣言和创世人口；
- 调用 runtime 注入的机构 seeder 写入创世机构、固定岗位、固定岗位权限、创世任职和管理员钱包集合。

本模块不提供 extrinsic，不保存 PoW 出块时间，也不向节点提供出块时间 Runtime API。
PoW 六分钟是 `primitives::pow_const::POW_TARGET_BLOCK_TIME_MS` 固定的难度调整平均目标，
与链阶段无关；有效工作量证明找到后立即出块，没有最短等待或最晚期限。

##### 创世机构子模块

```text
runtime/genesis/src/institution/
├── mod.rs          # 对外只暴露 build 入口，声明职责边界
├── fixed_roles.rs  # 89 个公权受保护创世机构的固定岗位、席位与既有钱包索引映射；不写 storage
└── seeder.rs       # 唯一写入方：公权写 public-*；公民链基金会写 private-*
```

- 岗位协议常量来自 `primitives::governance_skeleton`，钱包来自既有 `CHINA_*` 常量；
- 构建前断言固定钱包数量等于席位总数，且固定岗位钱包不得重复；
- 全部机构必须写入唯一 `LR / 法定代表人` 岗位；岗位可以没有任职，公开 `legal_representative` 结构可为 `None`，不得从管理员首位、机构主账户或其它钱包推导；
- 后续依法任命法定代表人属于 entity 运行期流程，不属于 genesis 职责。

私权创世机构“公民链技术发展基金会”是 SFGY 非营利法人，也是唯一在创世明确携带法定代表人的例外：

- 基金会简称“公民链基金会”，英文全称 `CitizenChain Technology Development Foundation`，英文简称 `CitizenChain Technology Foundation`；CID `GZ018-SFGYR-201206100-2026`，主账户 `0xaa23304c7b663ba25a9d3a2fb1efafdd650ecf2504a2caedc228fe81b46b4333`，费用账户 `0xe4837d50cd5ef677c9b10ea49baf8c7d60cf11b422d748377e9e5f750640e927`；
- 法定代表人为程伟，公民 CID 引用 `CN220-CTZN2-198805200-2026`，法定代表人账户 `0x0cb1d05c0c9c7f05679b60d6f24c7e5719a3985264e41c5e899d4822dca4b06b`（model B //0，2026-07-27 重新创世）；创世不伪造第二份公民记录，后续由对应注册局依法从链上公民真源核验；
- 只创建一条程伟管理员人员记录 `Admin { account_id, cid_number, family_name, given_name }`；同一账户分别任职 `LR / 法定代表人`、`GENESIS_PRODUCT_MANAGER / 创世产品经理`、`GENESIS_PROGRAMMER / 创世程序员`，每岗一席。三岗的岗位码、岗位名和岗位权限永久固定，但基金会仍可增加普通动态岗位；
- `PrivateAdmins::AdminAccounts`、三项 `PrivateManage` 岗位/任职和 `PrivateManage::InstitutionGovernanceThresholds[cid_number] = 2` 在同一创世构建中写入。阈值 2 是机构阈值，不与唯一管理员钱包数量绑定；
- 基金会身份、主/费用协议账户以及三个固定岗位治理骨架受 NodeGuard 保护，成员依法轮换必须通过同一原子治理结果同步更新 admins 和岗位任职，不能裸改其中一侧；NodeGuard 不得禁止新增普通动态岗位。

#### 1.1 ADR-039 目标创世权限（实现中）

- 创世 seeder 必须为全部创世固定岗位写入固定 `RoleBusinessPermission`，不能只写岗位码、名称和席位。
- 创世 admins 只作为可任职人员集合，不直接取得业务权限；业务权限必须来自固定岗位的有效创世任职。
- 全部创世固定岗位使用既定固定码；动态岗位码生成使用所属 pallet 的 `MODULE_TAG` 作哈希域，但 genesis 不用动态算法替代固定码。
- 普通机构不由 genesis seeder 创建；其运行期创建必须原子建立 LR、至少一个初始治理岗位及权限、任职和投票规则。
- 本节是 ADR-039 目标，runtime/genesis 与 NodeGuard 代码迁移分别在任务卡第 3、4 步执行。

#### 2. 五个受守卫字段

| 存储项 | 类型 | 创世 RAW 形态 | 永久规则 |
|---|---|---|---|
| `Phase` | `ChainPhase` | 缺省即 `Genesis` | 只允许一次切换为 `Operation` |
| `DeveloperUpgradeEnabled` | `bool` | 缺省即 `true` | 只允许与阶段同步切换为 `false` |
| `CitizensDeclaration` | `BoundedVec<u8, MaxDeclarationLen>` | `CITIZENS` 的准确 UTF-8 字节 | 永久逐字冻结 |
| `CountryDeclaration` | `BoundedVec<u8, MaxDeclarationLen>` | `COUNTRY` 的准确 UTF-8 字节 | 永久逐字冻结 |
| `CitizenMax` | `u64` | `1_443_497_378` | 永久冻结 |

FRAME `StorageVersion` 必须保持 0。旧 `TargetBlockTimeMs` 已删除，同前缀未知 RAW key
（包括该旧字段）由 NodeGuard fail-closed 拒绝，不保留兼容或影子状态。

#### 3. 一次性阶段状态机

合法创世状态：

```text
(Phase, DeveloperUpgradeEnabled) = (Genesis, true)
```

唯一合法目标状态：

```text
(Phase, DeveloperUpgradeEnabled) = (Operation, false)
```

约束：

- 两个字段只能在同一个包含 `:code` 变化的 runtime 升级区块中原子写入；
- 禁止普通区块修改、部分修改、显式写回创世默认值、反向切换和重新启用开发者直升；
- 转为 `Operation` 后永久冻结；
- 本轮没有自动执行阶段切换，正式切换仍需单独确认迁移和治理授权。

#### 4. 公共接口

模块只保留：

```rust
pub trait DeveloperUpgradeCheck {
    fn is_enabled() -> bool;
}
```

`runtime-upgrade` 使用该接口选择当前允许的升级授权路径。旧 `GenesisPalletApi`、
`TargetBlockTime` trait、`TargetBlockTimeChanged` 事件以及未被调用的阶段事件已经删除。

#### 5. 创世固定真源

固定值来自 `runtime/primitives/src/genesis.rs`：

- `CITIZENS`：创世宣言；
- `COUNTRY`：国家宣言；
- `GENESIS_CITIZEN_MAX = 1_443_497_378`；
- `GENESIS_ISSUANCE = 14_434_973_780_000` 分。

`runtime/src/genesis.rs` 把前三项写入 `GenesisConfig`。NodeGuard 使用相同的节点编译期
真源重新构造 RAW key 和 SCALE 值，不信任 runtime metadata、getter 或 Runtime API。

#### 6. NodeGuard 执法

`node/src/guard/genesis_pallet.rs` 在四条路径执行：

1. 节点启动：读取 block#0 的整个 `GenesisPallet` 前缀，确认创世事实和缺省阶段状态；
2. 普通区块：三个创世事实和 StorageVersion 任何触碰都拒绝；
3. runtime 升级：只接受两字段唯一原子单向转换，并在 `:code` 后复核完整状态；
4. 完整状态导入：整个 pallet 前缀进入共享单遍分区，未知 key、缺失值、错误 SCALE、
   尾随字节和非规范状态全部拒绝。

#### 7. 测试与验收

- `genesis-pallet` 单元测试：默认阶段、开发者开关、trait、阶段模拟和创世配置；
- NodeGuard 策略测试：RAW key、两种规范状态、三个固定事实、未知旧字段、畸形 SCALE、
  非规范默认写回、合法原子转换、无 `:code`、部分转换、反向转换和固定事实触碰；
- NodeGuard 真实 runtime 创世完整状态测试确认五字段策略参与共享扫描和拒绝链路；
- 最终结果以任务卡中的本轮编译、WASM 和 fresh 节点真实验收记录为准。

2026-07-25 第8.1步 WASM 复验：首次 CI no_std 编译发现
`institution/seeder.rs` 使用 `vec![]` 后只导入了 `alloc::vec::Vec`。修复仅补充
`alloc::vec` 宏导入；创世集合内容、顺序、账户、岗位、管理员和余额完全不变。修复后
`genesis-pallet --no-default-features`、模块严格 Clippy 和当前源码 release WASM
构建均通过。

#### 8. 文件索引

- `citizenchain/runtime/genesis/src/lib.rs`：类型、存储、创世构建和开发者升级查询；
- `citizenchain/runtime/genesis/src/institution/fixed_roles.rs`：89 个公权受保护创世机构的固定岗位、席位和钱包索引映射；
- PRS、NLG、NSN、NRP、NSP、NED 六个国家级单例在 block#0 写精确机构身份、制度账户和唯一空缺 `LR / 法定代表人` 岗位，不写成员岗位、任职、admins 或动态阈值。首次组成前先独立登记 admins；其中 NSN、NRP、NED 还受法定成员岗位与人数区间约束。
- `citizenchain/runtime/genesis/src/institution/seeder.rs`：公私权创世机构、岗位、任职和管理员钱包唯一写入方；公民链基金会写入 `private-manage` / `private-admins`，不污染公权目录；
- `citizenchain/runtime/genesis/src/tests/mod.rs`：pallet 单元测试；
- `citizenchain/runtime/primitives/src/genesis.rs`：三个创世事实的固定真源；
- `citizenchain/runtime/src/genesis.rs`：真实 runtime genesis patch；
- `citizenchain/node/src/guard/genesis_pallet.rs`：节点独立永久规则。

#### 1. 模块职责

`grandpakey-change` 只负责国家储委会（NRC）和 43 个省储委会（PRC）各自节点的
GRANDPA authority 更换。省储行（PRB）没有 GRANDPA authority，不能使用本模块。

本模块提供两条互斥路径：

| 路径 | 适用情形 | 授权与证明 | 投票 |
| --- | --- | --- | --- |
| 正常更换 | 旧私钥仍可签名 | 目标机构 `CID + 委员岗位码 + 委员 account_id` 授权；旧、新 GRANDPA 私钥共同签署同一证明 | 不投票 |
| 紧急恢复 | 旧私钥丢失或无法签名 | 目标机构委员岗位发起；新 GRANDPA 私钥签署持钥证明 | 仅目标机构自己的委员内部投票 |

紧急恢复不是联合投票。NRC 只由 NRC 的 19 个委员岗位投票，机构阈值为 13；每个
PRC 只由本 PRC 的 9 个委员岗位投票，机构阈值为 6。投票资格、快照、阈值、计票和
终态全部由现有投票引擎提供，本业务模块不实现投票流程。

代码目录：

- `citizenchain/runtime/governance/grandpakey-change/src/`

#### 2. Runtime 接线

配置位置：

- `citizenchain/runtime/src/configs.rs`

关键配置：

- `InternalVoteEngine = VotingEngine`：仅紧急恢复创建内部投票。
- `InstitutionRoleAuthorization`：统一读取机构岗位任职和岗位业务权限真源。
- `GrandpaChangeDelay`：两条路径都调用 `pallet-grandpa::schedule_change` 延迟生效。
- `WeightInfo`：两项外部调用使用各自 benchmark 权重。

当前没有创世数据迁移，`STORAGE_VERSION = 0`，不得增加开发期 migration。

##### 3.1 `call_index(0) propose_emergency_grandpa_key_recovery`

字段顺序：

1. `actor_cid_number`
2. `actor_role_code`
3. `new_public_key`
4. `proof_nonce`
5. `proof_expires_at`
6. `new_public_key_signature`

约束：

- 调用者必须是目标 NRC/PRC 委员岗位的有效任职人，并同时满足
  `CID + 岗位码 + account_id`。
- 岗位必须拥有紧急恢复的 `Propose` 权限。
- 新私钥必须对本次完整证明载荷签名，证明节点已经持有新私钥。
- 每个机构同时最多存在一项未终结的紧急恢复。
- 创建的 `VotePlan` 只包含目标机构，不得加入其他 NRC、PRC、PRB 或公民。
- 投票通过后由投票引擎回调本模块调度 authority set；全链已有 GRANDPA pending
  change 时返回可重试结果。
- 被否决或确定不可执行时释放新公钥占用。

##### 3.2 `call_index(1) schedule_grandpa_key_rotation`

字段顺序：

1. `actor_cid_number`
2. `actor_role_code`
3. `new_public_key`
4. `proof_nonce`
5. `proof_expires_at`
6. `old_public_key_signature`
7. `new_public_key_signature`

约束：

- 调用者必须是目标 NRC/PRC 委员岗位的有效任职人，并同时满足
  `CID + 岗位码 + account_id`。
- 岗位必须拥有正常更换的 `Propose` 权限。
- 旧、新 GRANDPA 私钥必须分别签署同一份完整证明载荷。
- 不创建提案、不进入投票引擎，校验通过后直接调度延迟生效。
- 目标机构存在未终结紧急恢复时，不允许正常更换越过紧急恢复。

两个 call index 连续使用 `0`、`1`，不保留空洞或旧 wrapper。

#### 4. 持钥证明

`GrandpaKeyProofPayload` 固定绑定：

- 当前链 `genesis_hash`
- `actor_cid_number`
- `actor_role_code`
- 当前发起 `account_id`
- 当前旧 GRANDPA 公钥
- 新 GRANDPA 公钥
- 当前 GRANDPA `set_id`
- 机构级递增 `proof_nonce`
- `proof_expires_at`
- `change_kind`（正常更换或紧急恢复）

签名消息唯一使用：

`primitives::sign::signing_message(OP_SIGN_GRANDPA_KEY_CHANGE, SCALE(payload))`

因此，签名不能跨链、跨机构、跨岗位、跨账户、跨密钥、跨 set、跨路径或重复使用。
`proof_nonce` 只在完整调用成功后递增。

#### 5. 公钥校验

两条路径共同执行以下校验：

- 新公钥必须是 32 字节 ed25519 公钥，不能为全零。
- 拒绝无效曲线点和 small-order 弱公钥。
- 新公钥不能等于目标机构当前公钥。
- 新公钥不能被其他机构当前使用，也不能被另一项待处理流程占用。
- 目标机构当前旧公钥必须仍在实际 GRANDPA authority set 中。
- 替换后的 authority set 不得出现重复公钥。
- `pallet-grandpa` 或本模块已有全链 pending change 时不能再调度。

#### 6. 存储

| 存储 | 用途 |
| --- | --- |
| `CurrentGrandpaKeys` | 机构最后一次已经实际生效的 GRANDPA 公钥 |
| `GrandpaKeyOwnerByKey` | 已生效公钥到机构 CID 的唯一反向索引 |
| `ReservedGrandpaKeys` | 投票中或等待生效的新公钥占用 |
| `NextGrandpaKeyProofNonce` | 每个机构下一份证明必须使用的 nonce |
| `ActiveEmergencyRecoveryByInstitution` | 每个机构唯一的未终结紧急恢复提案 |
| `PendingGrandpaKeyChange` | 全链唯一、已调度并等待实际生效的变更 |

创世时从 `CHINA_CB` 初始化 44 个 NRC/PRC 当前公钥及反向索引，并拒绝重复初始公钥。

#### 7. 生效与节点私钥生命周期

1. 节点生成新 ed25519 私钥，并在提交前把新私钥加入 `gran` keystore，旧私钥继续保留。
2. 正常更换完成旧、新私钥双签名；紧急恢复只完成新私钥证明并等待本机构内部投票。
3. Runtime 调用 `pallet-grandpa::schedule_change`，旧、新私钥在延迟期内同时保存在节点。
4. `on_initialize` 读取实际 GRANDPA authorities；只有新公钥已出现且旧公钥已消失，
   才更新正反向索引、清理 pending，并发出 `GrandpaKeyActivated`。
5. 节点后台只读取 finalized 状态。确认新公钥已是该 CID 当前公钥、位于 authority
   set 且旧公钥已移除后，才删除旧私钥、更新本地元数据并重启节点。

链上事件尚未 finalized、仅达到预计区块、仅看到交易成功或仅看到
`GrandpaKeyActivated` 的非 finalized 分叉，都不能触发旧私钥删除。

#### 8. 紧急恢复回调边界

紧急恢复业务动作与投票提案原子绑定。回调执行前必须重新核对：

- callback owner 和 scope；
- 内部投票 kind、stage、status；
- 目标机构代码、CID 和 action；
- 当前机构岗位授权；
- 提案是否仍是该机构登记的唯一活动紧急恢复；
- 新公钥占用是否仍属于该机构。

暂时性的 `GrandpaChangePending` 返回 `RetryableFailed`，由投票引擎现有重试机制处理。
确定不可执行或被否决时关闭本业务状态并释放公钥占用。投票引擎代码不因本模块改动。

#### 9. 事件

- `EmergencyRecoveryProposed`
- `RoutineRotationScheduled`
- `EmergencyRecoveryScheduled`
- `GrandpaKeyActivated`
- `EmergencyRecoveryExecutionFailed`
- `EmergencyRecoveryClosed`

事件分别表达提案创建、调度、实际生效和紧急恢复关闭，不能把“已调度”解释为“已生效”。

#### 10. 节点接口与界面

节点后端 `node/src/core/grandpa_rotation.rs` 提供：

- `build_grandpa_key_change_request`
- `submit_grandpa_key_change`
- `get_grandpa_key_change_status`

节点页面位于 `node/frontend/keys/`。管理员选择目标机构委员任职，
输入本机解锁密码，完成管理员交易签名后提交。页面明确区分：

- 正常更换：无投票、旧新私钥双签、延迟生效；
- 紧急恢复：旧私钥不可用、目标机构委员内部投票。

非秘密的待处理状态保存为 `<app_data>/grandpa-key-change.json`；GRANDPA 私钥只保存
在节点 `gran` keystore，不写入该状态文件、日志或前端。

#### 11. 测试与验收

Runtime 测试至少覆盖：

- call index `0/1` 连续且 SCALE 布局固定；
- 正常更换岗位授权、双签、nonce、过期和延迟生效；
- 紧急恢复只生成目标机构内部投票；
- NRC `13/19`、PRC `6/9` 的机构阈值来自现有机构配置；
- PRB、跨机构委员、无任职管理员和无权限岗位被拒绝；
- 无效、弱、相同、已使用和已预留公钥被拒绝；
- pending 冲突、回调重试、否决和确定失败清理；
- 只有实际 authority set 已切换后才更新当前公钥索引。

节点验收至少覆盖：

- 新私钥写入时保留旧私钥；
- finalized 前不删除旧私钥；
- finalized 状态确认新 authority 生效后删除旧私钥；
- 未提交且证明过期的新候选私钥可安全清理；
- 两条 call 均能被节点和离线钱包按同一二维码注册表解析。

#### 0. 功能需求
`resolution-destro` 的功能需求是：为国家储委会、各省储委会、各省储行提供“机构自有资金销毁”治理流程，由拥有该业务权限的岗位有效任职人发起、岗位快照选民投票，在提案通过后自动或手动执行链上销毁。

模块必须满足以下要求：
- 仅允许 NRC、PRC、PRB 发起销毁提案；`actor_cid_number` 是机构唯一身份，`institution_account` 是具体执行账户，链端必须用账户正反索引验证二者归属一致。
- 发起权限必须按完整 `RoleSubject(actor_cid_number, proposer_role_code)` 校验；投票资格必须来自该业务 `Vote` 权限岗位的有效任职快照，不得由全体 `admins` 派生。
- 销毁金额必须大于 0，且执行时必须保证机构账户保留最小余额 `ED`。
- 提案投票通过后，系统应自动尝试执行销毁；若自动执行失败，提案保持 `STATUS_PASSED`，允许后续手动重试执行。
- 自动执行和手动重试走同一业务绑定：callback scope、owner、内部投票 kind/stage、机构码、机构账户、CID 和销毁 action 必须全部一致；执行时再次确认业务机构仍为 NRC、PRC 或 PRB。
- 自动执行失败不能回滚已通过的投票结果。
- 销毁执行通过 `Currency::slash` 减少机构账户余额与总发行量，实现链上销毁。

#### 1. 模块定位
`resolution-destro` 是"机构资金销毁治理执行"模块，负责：
- 发起机构销毁提案（内部投票提案）。
- 在提案通过后执行销毁（自动尝试 + 手动重试）。

该模块不实现投票计票逻辑，投票由 `votingengine` 承担。
提案数据、元数据、活跃提案限额均由 `votingengine` 统一管控。

代码位置：
- `<本仓根>/runtime/governance/resolution-destro/src/lib.rs`

命名说明：
- 2026-04-29 起，本模块统一使用 `resolution-destro` / `resolution_destro` / `ResolutionDestro`。
- 模块位于 `citizenchain/runtime/governance/resolution-destro/`。
- `pallet_index = 13`、call index 与 `MODULE_TAG = b"res-dst"` 保持不变。

---

#### 2. 上下游关系与 Runtime 接线
上游常量（机构、机构阈值、投票时长）：
- `<本仓根>/runtime/primitives/src/count_const.rs`
  - `NRC_ADMIN_COUNT = 19`
  - `PRC_ADMIN_COUNT = 9`
  - `PRB_ADMIN_COUNT = 9`

投票引擎依赖：
- `<本仓根>/runtime/votingengine/src/lib.rs`
- 使用 trait：
  - `InternalVoteEngine::create_internal_proposal_with_data`
  - `InternalVoteResultCallback`
- 使用方法：
  - `Pallet::get_proposal_data`
  - `Pallet::proposals`
- 状态常量：`STATUS_PASSED`
- 岗位授权：`InstitutionRoleAuthorizationQuery::{is_authorized, role_subjects_with_permission}`
- 投票计划：业务模块构造 `VotePlan`，静态指定 `VotingEngineKind::Internal`

Runtime 接线：
- `<本仓根>/runtime/src/configs/mod.rs`
  - `type Currency = Balances`
  - `type InternalVoteEngine = VotingEngine`

---

##### 动作结构
```rust
pub struct DestroyAction<AccountId, Balance> {
    pub actor_cid_number: CidNumber,
    pub institution_account: AccountId,
    pub amount: Balance,
}
```
- 编码后存入投票引擎 `ProposalData`，通过 `get_proposal_data` 读取并解码。
- `proposer_role_code` 不进入执行动作；它只用于创建阶段形成不可变的提案主体和 `VotePlan`。

##### 模块标识
- `MODULE_TAG = b"res-dst"`：存入 ProposalData 的前缀，用于区分不同业务模块，防止跨模块误解码。

##### 本模块存储
无。提案数据、元数据、活跃提案列表均已移至 `votingengine` 统一管控（lib.rs:103 注释说明）。

##### 机构与执行账户
- `actor_cid_number` 是提案主体的唯一机构主键；业务授权主体还必须包含独立 `proposer_role_code`。
- `institution_account` 是本次销毁的具体机构账户，不承担机构身份语义。
- `InstitutionMultisigQuery` 从账户反查 CID 和机构码，创建与执行阶段都必须与显式 `actor_cid_number` 一致；不得从主账户推导机构身份。

---

##### 4.1 `propose_destroy`（call index = 0）
入参：`actor_cid_number`, `proposer_role_code`, `institution_account`, `amount`

流程：
1. `ensure_signed`。
2. 校验 `amount > 0`。
3. 从 `institution_account` 正反索引读取真实 CID/机构码，校验其属于 NRC、PRC 或 PRB，并与 `actor_cid_number` 一致。
4. 以 `BusinessActionId(res-dst, ACTION_RESOLUTION_DESTROY)` 校验签名者对完整 `RoleSubject(actor_cid_number, proposer_role_code)` 具有 `Propose` 权限。
5. 查询该 CID 下拥有同一业务 `Vote` 权限的岗位主体，构造不可变内部 `VotePlan`；没有合格投票岗位时拒绝创建。
6. 将 `DestroyAction` 加 `MODULE_TAG` 编码。
7. 通过 `create_institution_proposal_with_data` 创建机构内部提案，并在同一事务中写入岗位快照、owner/data/meta（活跃提案限额由投票引擎统一检查）。
8. 发 `DestroyProposed` 事件。

##### 4.2 投票入口
本模块不提供独立投票 call。岗位快照选民统一走：

- `InternalVote::cast(proposal_id, approve)`(pallet 20.0)

投票通过后由 `InternalVoteExecutor` 回调本模块自动执行销毁;自动执行失败时保持投票引擎状态为 `STATUS_PASSED`,并发出 `DestroyExecutionFailed` 事件,不回滚已通过投票。

##### 4.3 手动重试入口

本模块不提供独立 wrapper extrinsic。手动重试统一走:

- `VotingEngine::retry_passed_proposal(proposal_id)`(pallet 9.4)

签名账户必须属于提案创建时 VotePlan 任一完整岗位主体的 `VoterSnapshot`，权限和重试次数由 `votingengine` 统一校验；仅当提案已 `STATUS_PASSED` 且存在 retry state 时可重试执行。用于“提案已通过但自动执行失败（如余额不足）”后的后续重试。

---

#### 5. 执行逻辑（`try_execute_destroy_from_action`）
1. 校验投票引擎提案状态为 `STATUS_PASSED`。
2. 重新读取 `institution_account` 的 CID/机构码正反索引，并与动作和投票提案绑定的 `actor_cid_number`、`execution_account` 复核。
3. 校验具体 `institution_account` 的 `free_balance >= amount + minimum_balance`（ED 保护）。
4. 从机构账户真源精确读取 `(actor_cid_number, InstitutionFee)` 费用账户；执行手续费只由该账户支付并保留 ED。
5. 在同一 storage transaction 中调用 `OnchainFeeCharger::charge(fee_account, amount)`，再调用 `Currency::slash` 执行本金销毁。
6. 校验 `remaining.is_zero()` 确认全额销毁成功；任一步失败时收费、分账、事件和销毁全部回滚。
7. 发 `DestroyExecuted { fee_payer, fee, ... }` 事件。
8. 返回 `ProposalExecutionOutcome::Executed`，由投票引擎统一标记 `STATUS_EXECUTED`。

重复执行防护：
- `STATUS_EXECUTED` 后提案不再是 `STATUS_PASSED`，后续重试被投票引擎 `ProposalNotRetryable` 拒绝。

---

#### 6. 关键安全设计
1. 权限边界：
   - 发起必须具备完整机构岗位主体的业务 `Propose` 权限，投票必须属于业务 `Vote` 岗位有效任职快照。
   - 手动执行必须由提案岗位有效选民快照成员触发，统一走投票引擎 retry 权限校验。

2. 投票执行解耦：
   - 自动执行错误不回滚已提交投票。

3. ED 保护：
   - 执行前强制 `free_balance >= amount + minimum_balance`，避免账户被 reap。
   - `checked_add` 防止 amount + ed 溢出。

4. slash 完整性校验：
   - `ensure!(remaining.is_zero())` 确保 slash 全额完成，防止静默部分销毁。

5. 身份闭环：
   - 创建、自动执行和统一重试都复核 CID、机构码、执行账户、提案 owner/kind/stage/callback scope，不从任一机构账户反推或替代机构身份。

6. 费用账户隔离：
   - `institution_account` 只销毁本金，执行费只从同一 actor CID 的费用账户收取；岗位任职人的签名钱包没有付款回落路径。

---

#### 7. 事件与错误
事件：
- `DestroyProposed { proposal_id, institution_code, institution, proposer, amount }`
- `DestroyVoteSubmitted { proposal_id, who, approve }`
- `DestroyExecutionFailed { proposal_id }`
- `DestroyExecuted { proposal_id, institution, fee_payer, amount, fee }`

错误：
- `InvalidInstitution`：无效机构
- `InstitutionCodeMismatch`：机构类型与 institution_code 参数不匹配
- `UnauthorizedAdmin`：稳定错误码；表示签名人不是所提交岗位的有效任职人，或该完整岗位主体没有销毁提案权限
- `ZeroAmount`：销毁金额为 0
- `ProposalActionNotFound`：找不到提案动作数据
- `ProposalNotPassed`：投票尚未通过
- `InstitutionAccountDecodeFailed`：机构账户解码失败
- `InsufficientBalance`：余额不足（含 ED 保护）
- `FeeAccountMissing`：actor CID 的费用账户无法从机构账户真源精确读取
- `FeeWithdrawFailed`：费用账户无法完整支付执行费并保留 ED

---

#### 8. Weight 策略
`WeightInfo` 由 benchmark 自动产出（`weights.rs` 由 `frame-benchmarking-cli` 生成）：
- `propose_destroy()`

注:本模块不暴露独立 execute wrapper extrinsic,手动重试走 `VotingEngine::retry_passed_proposal`(pallet 9.4),权重由投票引擎统一计入。

2026-07-19 已用当前源码导出的临时 fresh spec 和 benchmark runtime、50 steps / 20 repeats 重新生成正式权重：`239 ms / proof 584308 / 25 reads / 23 writes`。benchmark 脚本显式使用 `spec-genesis`，不读取冻结 chainspec 或裸 WASM 创世。

---

#### 9. 测试覆盖
运行命令：
```
cargo test --offline --manifest-path citizenchain/runtime/governance/resolution-destro/Cargo.toml -- --nocapture
```

当前结果：16 passed

覆盖重点：
- NRC/PRC/PRB 三种组织达阈值自动执行销毁
- 非目标岗位任职人或无该业务权限的岗位不能发起，非岗位快照选民不能投票
- 零金额拒绝 + 余额不足拒绝
- ED 保留校验（销毁全部余额被拒）
- 自动执行失败后手动执行成功
- 被拒绝提案不阻塞新提案
- 已执行提案不阻塞新提案
- 重复投票由投票引擎拒绝
- 非岗位有效选民快照成员不能触发投票引擎 retry
- 无效机构返回 None
- mock runtime 已接入岗位授权查询和机构 `VotePlan`，机构路径不写 `AdminSnapshot`

---

#### 10. 运维建议
1. 监控 `DestroyExecutionFailed` 事件，出现后优先补齐机构余额，再由提案任一冻结岗位 `VoterSnapshot` 成员调用 `VotingEngine::retry_passed_proposal`（pallet 9.4）。
2. 若 3 次手动执行仍失败，或超过 `ExecutionRetryGraceBlocks` 无人处理，提案会由投票引擎统一转 `STATUS_EXECUTION_FAILED`。
3. 业务或岗位快照读写变化后必须重新运行 benchmark；不得复用本次权重掩盖后续存储变化。

##### 0.1 模块职责
`runtime-upgrade` 负责把"Runtime wasm 升级"包装成一个受治理约束的链上流程，核心要求是：
- 仅允许 NRC 和 43 个 PRC 的 `COMMITTEE_MEMBER / 委员` 岗位有效任职账户发起升级提案，仅属于 admins 不构成授权。
- 升级提案必须先经过 `votingengine` 的联合投票。
- 联合阶段由 `VotePlan` 固定绑定 NRC + 43 PRC 委员岗位和 43 PRB `DIRECTOR / 董事` 岗位；有效任职账户用个人钱包直接上链投票，链上按机构阈值形成机构结果。PRB 董事只有投票权，没有提案权。
- 联合投票通过后才允许执行 `set_code`。
- 开发期直升通道只允许国家储委会管理员使用，并且必须受 `DeveloperUpgradeEnabled` 开关约束。
- 投票结果、执行结果必须在链上可追踪。

##### 0.2 提案创建需求
- 提案必须携带非空升级理由 `reason`。
- 提案必须携带非空 wasm `code`。
- 创建提案时同步在 `votingengine` 创建联合投票，使用投票引擎统一分配的 `proposal_id`（本模块不维护独立 ID）。
- 本模块不接收、不生成、不校验人口快照、联合签名、投票资格和计票数据；这些都属于 `votingengine`。

##### 0.3 联合投票回调需求
- 联合投票拒绝时，投票引擎状态保持 `STATUS_REJECTED`。
- 联合投票通过时，模块必须尝试执行 runtime code。
- 联合投票结束后，投票引擎侧状态保持真实业务结果：执行成功写为 `STATUS_EXECUTED`，否决保持 `STATUS_REJECTED`，执行失败写为 `STATUS_EXECUTION_FAILED`。
- 回调直接使用投票引擎的 `proposal_id`，无需映射反查。
- 回调还必须校验 callback scope、`ProposalOwner`、联合 kind、`STAGE_JOINT/STAGE_REFERENDUM`，并复算 `ProposalObject` 中 runtime code 的哈希与提案摘要一致；任何一项不符都不得执行 `set_code`。

##### 0.4 执行失败处理
- 若联合投票已通过但 `set_code` 执行失败，投票引擎状态进入 `STATUS_EXECUTION_FAILED`。
- runtime wasm 不再内嵌在摘要结构里，而是统一存入 `votingengine::ProposalObject`。
- 执行成功、拒绝、执行失败后，wasm 对象继续保留到投票引擎 90 天延迟清理统一删除，不由业务模块手工删除。

##### 0.5 可审计与运维需求
- 需要区分以下事件：提案创建、联合投票终结、升级执行成功、升级执行失败。
- 投票引擎侧状态机：
  - `VOTING → PASSED → EXECUTED`（投票通过且执行成功）
  - `VOTING → REJECTED`（投票拒绝）
  - `VOTING → PASSED → EXECUTION_FAILED`（投票通过但执行失败）
- 所有终态均为不可逆（无重试、无取消）。

#### 1. 模块定位
`runtime-upgrade` 是"协议升级治理编排模块"，负责：
- 接收 NRC/PRC 委员岗位有效任职账户提交的 wasm 升级提案；
- 调用 `votingengine` 创建联合投票；
- 在联合投票回调后执行 `set_code`；
- 摘要数据存储在 `votingengine` 的 `ProposalData`；
- 原始 wasm 对象存储在 `votingengine` 的 `ProposalObject`；
- 本模块零本地存储。

代码位置：
- `runtime/governance/runtime-upgrade/src/lib.rs`
- `node/src/upgrade/`
- `node/frontend/upgrade/`

命名说明：
- 2026-04-29 起，本模块统一使用 `runtime-upgrade` / `runtime_upgrade` / `RuntimeUpgrade`。
- 模块位于 `citizenchain/runtime/governance/runtime-upgrade/`。
- `pallet_index = 12`、call index 与 `MODULE_TAG = b"rt-upg"` 保持不变。

节点侧边界：
- node 后端 `runtime_upgrade` 只负责读取 wasm、构建协议升级 call data、生成签名请求、提交签名交易。
- node 前端 `runtime-upgrade` 只负责协议升级页面交互和签名流程。
- `developer_direct_upgrade` 属于开发者动作，node 端不提供任何入口，只由塔塔控制台冷签发起；链端 call 与 QR 登记必须保留。
- node 的 `runtime_upgrade` 不获取人口快照、不接收联合签名上下文、不拥有投票引擎状态、不展示投票终态。
- 协议升级提案详情展示真实状态时必须以 `VotingEngine::Proposals.status` 为准，`runtime-upgrade` 摘要里不保存业务状态字段。

citizenapp / citizenwallet 边界：
- citizenapp 的 `governance/runtime-upgrade` 不发起协议升级提案，不选择 WASM，不获取人口快照，不提交 `propose_runtime_upgrade`。
- citizenapp 只展示协议升级介绍、协议升级提案详情，并保留现有提案详情页投票入口。
- citizenwallet 公民钱包不恢复 runtime-upgrade SCALE decoder；大 WASM 交易继续走哈希直签例外，由用户核对显示字段中的代码哈希。

#### 2. 运行时接线
Runtime 配置位置：
- `runtime/src/configs/mod.rs`

当前接线：
- `ProposeOrigin = EnsureJointProposer`
- `InstitutionRoleAuthorization = PublicManage`
- `DeveloperUpgradeOrigin = EnsureNrcAdmin`
- `JointVoteEngine = VotingEngine`
- `RuntimeCodeExecutor = RuntimeSetCodeExecutor`
- `MaxReasonLen = RuntimeUpgradeMaxReasonLen`（1024）
- `MaxRuntimeCodeSize = RuntimeUpgradeMaxCodeSize`（5 * 1024 * 1024）
- `VotingEngine::MaxProposalDataLen = 100 * 1024`
- `VotingEngine::MaxProposalObjectLen = 10 * 1024 * 1024`
- `WeightInfo = runtime_upgrade::weights::SubstrateWeight<Runtime>`

说明：
- `finalize_joint_vote` 手工 extrinsic 已删除，call index `1` 保持空缺。
- 正常生产路径只能由投票引擎通过 `JointVoteResultCallback` 自动回调本模块，避免 Root 手工回放形成第二条执行入口。

##### 3.1 Proposal（摘要，序列化存入 votingengine ProposalData）
- `proposer: AccountId`：提案发起人（NRC 或 PRC 委员岗位的有效任职账户）
- `reason: BoundedVec<u8, MaxReasonLen>`：升级理由
- `code_hash: Hash`：升级 code 哈希，便于事件与链下审计对齐

说明：
- `Proposal` 只保存业务展示所需摘要，不保存投票状态。
- 协议升级真实状态只能读取 `votingengine::Proposals.status`。

##### 3.2 对象层数据（统一存入 votingengine ProposalObject）
- `kind = 1`：表示 runtime wasm 对象
- `object_len`：wasm 字节长度
- `object_hash`：对象哈希
- `object bytes`：原始 wasm 字节（对象层上限 10MB，业务自身继续限制 5MB）

##### 3.3 模块标识
- `MODULE_TAG = b"rt-upg"`：存入 ProposalData 的前缀，用于区分不同业务模块，防止跨模块误解码。

#### 4. 存储模型
本模块只保留一项本地审计，其余提案数据、投票数据、元数据均存储在 `votingengine`：
- `LastRuntimeUpgradeAudit`：最近一次成功执行的 runtime 升级审计，记录执行路径、code hash、
  旧/新 PoW 参数 hash、执行高度和参数激活高度，供 NodeGuard 验证 `:code` 与 PoW 参数原子绑定。
- `ProposalData`：存放 `MODULE_TAG + Proposal<T>` 摘要的 SCALE 编码
- `ProposalObjectMeta`：存放 runtime wasm 的对象元数据（kind / len / hash）
- `ProposalObject`：存放 runtime wasm 原始字节
- `ProposalMeta`：存放提案创建时间
- `Proposals`：投票引擎核心提案表（状态、阶段、截止区块等）
- `ProposalVotePlans`：一次性绑定协议升级动作、提案主体、87 个投票岗位主体、联合引擎和 runtime WASM 对象哈希
- `VoterSnapshot` / `InstitutionTicketCountSnapshot`：分别保存岗位有效任职快照和按 CID 冻结的岗位席位票据总数

##### 5.1 `propose_runtime_upgrade`（call index = 0）
流程：
1. 载荷显式接收 `actor_cid_number + actor_role_code`；校验 `ProposeOrigin`（`EnsureJointProposer`），再用 `InstitutionRoleAuthorization` 校验签名账户对该完整 `RoleSubject` 拥有协议升级 `Propose` 权限。当前顶层能力只允许 NRC/PRC `COMMITTEE_MEMBER`。
2. 校验 `reason` 与 `code` 非空，并校验 `new_pow_params` 的参数/算法版本合法。
3. 计算 `code_hash`、当前 `ActiveParams` hash 与新 PoW 参数 hash，构造摘要 `Proposal`
   并加 `MODULE_TAG` 序列化。
4. 构造固定联合 `VotePlan`：NRC + 43 PRC `COMMITTEE_MEMBER` 为可发起/可投票主体，43 PRB `DIRECTOR` 为只投票主体，`business_object_hash` 绑定 runtime WASM 对象哈希。
5. 调用 `JointVoteEngine::create_joint_proposal_with_data_and_object` 创建联合投票，并在同一事务中写入 plan、owner/data/meta、岗位选民快照和 runtime wasm 对象。
6. 发出 `RuntimeUpgradeProposed` 事件。

边界：
- 该接口接收 `origin / actor_cid_number / actor_role_code / reason / code / new_pow_params`；摘要与事件同时记录机构 CID、岗位码和签名钱包。
- PoW 参数只能随 runtime code 一起表决；`CurrentDifficulty` 不进入提案参数，仍由算法推进。
- 人口快照、联合签名、投票资格、计票与终态推进均由投票引擎内部流程负责。

##### 5.2 call index 1 空缺
原 `finalize_joint_vote` 手工入口已删除。该位置保持空缺，不再注册任何 extrinsic。

协议升级联合投票终结流程只允许从 `JointVoteResultCallback::on_joint_vote_finalized` 进入：
1. 从 `ProposalData` 加载提案摘要，并要求投票引擎 `Proposals` 必须存在。
2. 要求投票引擎状态与本次回调方向一致：通过为 `STATUS_PASSED`，否决为 `STATUS_REJECTED`。
3. 若 `approved=false`：
   - 不改写业务摘要
   - 返回 `ProposalExecutionOutcome::Executed`，投票引擎保持 `STATUS_REJECTED`
   - 发出 `JointVoteFinalized`
4. 若 `approved=true`：
   - 从 `ProposalObject` 加载 runtime wasm
   - 尝试执行 `RuntimeCodeExecutor::execute_runtime_code`
   - 成功：回调返回 `ProposalExecutionOutcome::Executed`
   - 失败：回调返回 `ProposalExecutionOutcome::FatalFailed`
   - 发出 `JointVoteFinalized` + 执行成功或失败事件
5. wasm 对象不由本模块手工删除，统一交由投票引擎 90 天延迟清理。

##### 5.3 `developer_direct_upgrade`（call index = 2）
说明：
- 开发期快捷通道：仅国家储委会委员岗位的任职管理员直接 `set_code`，不走联合投票。
- 仅在 `genesis-pallet` 的 `DeveloperUpgradeEnabled` 为 `true` 时可用。
- 链进入运行期后此调用永久失效，升级必须走 `propose_runtime_upgrade` 联合投票。

流程：
1. 交易载荷显式携带 `actor_cid_number + actor_role_code`，不得用主账户或本地登录态代替机构岗位身份。
2. 校验 `DeveloperUpgradeOrigin` 后，要求 actor CID 的机构码为 NRC，并按完整 CID、岗位码和外层签名钱包校验协议升级 `Propose` 权限；当前只允许 `COMMITTEE_MEMBER`。
3. 校验 `DeveloperUpgradeCheck::is_enabled()`，关闭则拒绝（`DeveloperUpgradeDisabled`）。
4. 校验 `code` 非空。
5. 计算 `code_hash`，调用 `RuntimeCodeExecutor::execute_runtime_code`，同样原子暂存 PoW 参数并写审计。
6. 发出 `DeveloperDirectUpgradeExecuted` 事件。

费用：开发直升是国家储委会机构操作，由该 `actor_cid_number` 的唯一费用账户支付 0.1 元；管理员钱包只提供外层签名，不允许作为回落付款人。

权重：使用 `frame_system::set_code()` 的系统权重。

##### 5.4 投票引擎状态协同

当前实现与 `votingengine` 的协作关系如下：

- 联合投票通过时，投票引擎先按通用路径把提案写成 `STATUS_PASSED`，再在同一事务中执行本模块回调
- 联合投票拒绝时，投票引擎保持 `STATUS_REJECTED`
- runtime code 执行成功时，本模块返回 `ProposalExecutionOutcome::Executed`，投票引擎写入执行成功终态
- runtime code 执行失败时，本模块返回 `ProposalExecutionOutcome::FatalFailed`，投票引擎写入执行失败终态

原因：本模块的执行逻辑运行在投票引擎 `set_status_and_emit` 的回调事务内。业务回调只返回统一执行结果，不回写任何业务状态字段；最终状态、`ProposalFinalized`、清理登记和互斥锁释放由投票引擎外层统一执行一次。

提案状态流转（投票引擎侧）：
- `VOTING → PASSED → EXECUTED`（联合投票通过且 runtime code 执行成功）
- `VOTING → REJECTED`（联合投票拒绝）
- `VOTING → PASSED → EXECUTION_FAILED`（联合投票通过，但 runtime code 执行失败）

说明：
- 节点 UI / RPC 查询层如果需要面向用户展示真实升级结果，应读取 `VotingEngine::Proposals.status`；`ProposalData` 只用于展示 proposer、reason、code_hash 等摘要信息。
  - `VotingEngine::STATUS_VOTING` / `STATUS_PASSED` → 投票中或执行待重试态
  - `VotingEngine::STATUS_REJECTED` → 已否决
  - `VotingEngine::STATUS_EXECUTED` → 已执行
  - `VotingEngine::STATUS_EXECUTION_FAILED` → 执行失败

#### 6. 回调路径
`JointVoteResultCallback::on_joint_vote_finalized`：
1. 接收投票引擎统一的 `proposal_id`（无需映射反查）
2. 调用 `apply_joint_vote_result(proposal_id, approved)`
3. 返回 `ProposalExecutionOutcome`，由投票引擎统一推进状态

Runtime 层的 `RuntimeJointVoteResultCallback` 负责路由：先尝试 `resolution-issuance`，再尝试 `runtime-upgrade`。

##### 7.1 已修复风险：执行失败误记为 Passed
旧实现中，联合投票通过后会先把提案写成 `Passed` 并清空 `code`，再尝试执行 `set_code`。如果执行失败：
- 链上状态仍显示 `Passed`
- 原始 code 已丢失

现已修复：
- 先执行，根据结果返回 `ProposalExecutionOutcome`
- 执行成功由投票引擎进入 `STATUS_EXECUTED`
- 执行失败由投票引擎进入 `STATUS_EXECUTION_FAILED`
- 业务摘要不再保存业务状态字段

##### 7.2 已修复风险：大 wasm 直接塞入 ProposalData 导致提案创建失败
旧实现中整份 runtime wasm 会直接编码进 `ProposalData`，而投票引擎通用摘要存储无法承载 MB 级对象，runtime 升级提案会在创建阶段触发 `ProposalDataTooLarge`。

现已修复：
- `ProposalData` 只存摘要
- wasm 改为统一写入投票引擎对象层 `ProposalObject`
- 创建提案时通过 `create_joint_proposal_with_data_and_object` 一次性原子写入，后续不暴露对象覆写入口
- 投票引擎摘要上限提升到 `100KB`
- 投票引擎对象层上限提升到 `10MB`
- runtime 升级业务自身 `MaxRuntimeCodeSize` 继续保持 `5MB`

##### 7.3 已修复风险：投票引擎状态与业务执行结果脱节
旧实现/旧文档把投票引擎终态过度抽象成统一的 `STATUS_EXECUTED`，无法准确表达“联合投票已通过，但 runtime code 执行失败”的差异，也容易让查询层误判真实业务结果。

现已修复：
- 联合投票通过且执行成功时写入 `STATUS_EXECUTED`
- 联合投票拒绝时保持 `STATUS_REJECTED`
- 联合投票通过但执行失败时写入 `STATUS_EXECUTION_FAILED`
- 查询层文档已明确：展示真实升级结果时以 votingengine 的 `Proposal.status` 为准，业务摘要只用于展示 proposer/reason/code_hash

##### 7.4 已修复风险：benchmark 与实际逻辑不一致
旧版 benchmark 存在偏差。现已修复：
- `propose_runtime_upgrade` benchmark 改为真实 extrinsic
- `propose_runtime_upgrade` benchmark 已删除人口快照、联合签名、省份和签名管理员公钥参数。
- benchmark 环境先构建真实创世机构，再写入 NRC/PRC 委员与 PRB 董事岗位、任职和固定权限，不再用 admins 伪装业务授权。
- 权重已用当前 benchmark runtime WASM、50 steps / 20 repeats 重算：367 reads / 281 writes，参考时间 12.483 s，并真实计入 87 个岗位快照、87 个 CID 有效选民快照与 `ProposalVotePlans`。
- `finalize_joint_vote` benchmark 与权重项已删除，终结执行成本由 `votingengine` 的联合投票终态回调路径覆盖。

##### 7.5 已收口入口
1. `finalize_joint_vote` 手工 Root 入口已删除，只保留 votingengine callback。

#### 8. 中文注释覆盖重点
本模块当前已在以下关键位置补充注释：
- `RuntimeCodeExecutor` 职责边界
- `propose_runtime_upgrade` 与 votingengine 的职责边界
- 联合投票通过后的执行/失败分叉
- `ProposalExecutionOutcome::Executed / FatalFailed` 与投票引擎状态的映射原因
- `on_joint_vote_finalized` 回调入口

#### 9. 测试覆盖
已覆盖（当前单测与框架完整性检查共 20 个测试）：
- NRC 和 PRC 委员岗位有效任职账户可发起提案，普通 staff 即使属于 admins 也被拒绝
- `VotePlan` 精确绑定 44 个委员主体、43 个董事主体、联合引擎与 runtime WASM 对象哈希
- 提案摘要与对象数据正确分别存入 votingengine
- 联合投票拒绝时保持 votingengine `STATUS_REJECTED`（含 wasm 对象保留到统一清理）
- 联合投票通过并成功执行进入 votingengine `STATUS_EXECUTED`
- 联合投票通过但执行失败进入 votingengine `STATUS_EXECUTION_FAILED`
- 联合投票通过成功时投票引擎状态进入 `STATUS_EXECUTED`
- `owns_proposal` 能正确识别本模块提案
- 已终结的提案不可重复终结（`ProposalNotVoting`）
- 不存在的提案终结失败（`ProposalNotFound`）
- 开发者直升：国家储委会管理员可直接升级
- 开发者直升：省储委会管理员拒绝（`BadOrigin`）
- 开发者直升：开关关闭时拒绝（`DeveloperUpgradeDisabled`）
- 开发者直升：非国家储委会管理员拒绝（`BadOrigin`）
- 开发者直升：空 code 拒绝（`EmptyRuntimeCode`）
- GenesisConfig 构建成功
- Runtime 完整性检查

Runtime 集成测试：
- 不存在的 proposal_id 回调返回错误
- 回调正确路由到本模块并执行拒绝流程

本地验证：
- 2026-05-10 `cargo test --manifest-path citizenchain/Cargo.toml -p runtime-upgrade --lib`：通过，17 passed。
- 2026-05-10 `cargo check --manifest-path citizenchain/Cargo.toml -p runtime-upgrade`：通过。
- 已执行格式整理与残留扫描。

#### 10. 文件索引
- 模块代码：`src/lib.rs`
- Benchmark：`src/benchmarks.rs`
- 权重：`src/weights.rs`
- 技术文档：`RUNTIMEUPGRADE_TECHNICAL.md`

#### 用途

MODULE_TAG 是各业务模块在 `votingengine` 的 `ProposalData` / `ProposalOwner` 中写入的字节标识。投票引擎本身不解析提案数据内容，但会用 `ProposalOwner` 做 owner 校验，禁止跨模块覆写。各模块在读取时仍需校验前缀或独立存储键，防止误解码。

#### 适用场景

MODULE_TAG 仅用于共享 `ProposalData` 存储的模块。若模块使用独立 `StorageMap` 存储提案动作数据，则不需要 MODULE_TAG。

#### TAG 注册表

| MODULE_TAG | 字节值 | 所属 Pallet | 数据格式 |
|------------|--------|------------|----------|
| `b"multisig"` | `[109,117,108,116,105,115,105,103]` | `multisig` | `MODULE_TAG + TransferAction (SCALE)` |
| `b"pub-mgmt"` | `[112,117,98,45,109,103,109,116]` | `public-manage` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"pri-mgmt"` | `[112,114,105,45,109,103,109,116]` | `private-manage` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"per-mgmt"` | `[112,101,114,45,109,103,109,116]` | `personal-admins` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"res-iss"` | `[114,101,115,45,105,115,115]` | `resolution-issuance` | `MODULE_TAG + IssuanceProposalData (SCALE)` |
| `b"res-dst"` | `[114,101,115,45,100,115,116]` | `resolution-destro` | `MODULE_TAG + DestroyAction (SCALE)` |
| `b"rt-upg"` | `[114,116,45,117,112,103]` | `runtime-upgrade` | `MODULE_TAG + Proposal (SCALE)`; 大对象另存 ProposalObject |
| `b"gra-key"` | `[103,114,97,45,107,101,121]` | `grandpakey-change` | `MODULE_TAG + KeyReplaceProposal (SCALE)` |

#### 使用独立 StorageMap（不需要 MODULE_TAG）的模块

| Pallet | 独立存储 | 说明 |
|--------|---------|------|
| `multisig` | `SafetyFundProposalActions`, `SweepProposalActions` | 安全基金转账和手续费划转使用独立存储；普通转账仍使用 ProposalData + MODULE_TAG |
| `offchain` | `RateProposalActions` | 费率设置提案使用独立存储 |

#### 编解码协议

**写入**（propose 阶段）：
```
let mut encoded = Vec::from(MODULE_TAG);
encoded.extend_from_slice(&action.encode());
let proposal_id = T::InternalVoteEngine::create_internal_proposal_with_data(
    proposer,
    org,
    institution,
    MODULE_TAG,
    encoded,
)?;
```

联合提案使用 `create_joint_proposal_with_data`；禁止业务模块直接调用旧的 `store_proposal_data`。机构岗位和任职变化由具体业务结果驱动，不存在 public/private admins 管理员集合变更 MODULE_TAG。

**读取**（execute/callback 阶段）：
```
let raw = get_proposal_data(proposal_id);
let tag = MODULE_TAG;
assert!(raw.starts_with(tag), "MODULE_TAG mismatch");
let action = Action::decode(&mut &raw[tag.len()..]);
```

#### 设计原则

1. TAG 均为 ASCII 可读字符；需要升级数据结构时必须显式增加版本后缀
2. 校验失败时返回错误或忽略非本模块提案，不做回退尝试
3. `public-manage` / `private-manage` 在 TAG 后增加 1 字节 ACTION_CODE 区分 create/close/其他操作
4. `runtime-upgrade` 的 runtime wasm 大对象通过 `ProposalObject` 单独存储，ProposalData 中仅存摘要

---

### Governance 目录说明

本目录用于承载 CitizenChain runtime 下的治理相关 pallet 与文档。
当前治理相关 crate 已统一放在本目录下，后续新增治理 pallet 也必须直接落在这里。

#### 1. 定位

`citizen-issuance` 是公民**首次上链**认证奖励模块。模块不提供外部交易，只接收
`citizen-identity` 的 `OnVotingIdentityRegistered` 回调。

首次上链发币不区分身份类型：投票身份登记(`register_voting_identity`)与竞选身份首建
(`upgrade_to_candidate_identity` 且该 CID 尚无投票身份)同权,二者都触发一次性奖励;竞选
身份不要求先有投票身份(有人只投票、有人一开始就竞选)。已有投票身份再升竞选不重复触发,
发行侧按 CID 与账户永久去重亦会拦截。节点守卫按状态(本块新建投票身份 + 双向绑定 + 去重)
复核,不区分触发的具体 extrinsic,竞选首次上链与投票首次上链状态同构。

奖励金额、档位人数、总人数上限和一次性规则来自 `primitives::citizen_const`。这些规则
已同步实现于节点原生 `NodeGuard`，runtime 升级可以改变代码，但不能让遵守当前节点
二进制的节点接受突破永久规则的区块。

#### 2. 同块两阶段结算

身份登记 extrinsic 与实际铸发必须在同一区块完成，但分为两个可独立验证阶段：

1. `citizen-identity` 校验注册局权限、公民钱包签名、CID 和居住地作用域，写入投票身份；
2. 回调校验永久与本块临时双重防重、人数上限和奖励非零，只写入待发队列；
3. 节点从 finalize 前视图读取完整队列，独立复核首次身份、规范 `cid_number`、反向索引、领取资格和奖励档位；
4. runtime 在同块 `on_finalize` 逐项 `deposit_creating`，写入永久防重状态与累计人数并清空临时状态；
5. 节点把公民奖励和全节点 PoW 奖励汇总为同一个 finalize 发行计划，精确核对账户与总发行变化。

回调阶段不会提前改变余额。这样既保持“登记成功同块到账”的产品语义，又避免 extrinsic
内的手续费、转账或其他余额变化掩盖奖励金额。

#### 3. 存储

永久状态：

- `RewardedCount`：累计成功领取人数，同时决定下一笔奖励所处档位；
- `IdentityRewardClaimed`：按完整规范 `cid_number` 防止同一公民身份重复领取；
- `AccountRewarded`：按钱包账户防止换绑 CID 后重复领取。

仅在本块 finalize 前存在的临时状态：

- `PendingRewardCount`：本块待发数量；
- `PendingRewards[index]`：连续序号对应的 `(account_id, cid_number)`；
- `PendingIdentityRewardClaimed`：本块完整 CID 防重；
- `PendingAccountRewarded`：本块账户防重。

`on_finalize` 后四类临时状态必须全部删除；缺号、重复、残留、非规范 key 或提前改写永久
状态均会被节点拒绝。

#### 4. 奖励与防重规则

- `index < CITIZEN_ISSUANCE_HIGH_REWARD_COUNT` 时使用高额奖励；之后使用常规奖励；
- `RewardedCount` 不得超过 `CITIZEN_ISSUANCE_MAX_COUNT`；
- 同一 CID 和同一账户都只能成功领取一次；必须同时是未领取的新 CID 与未领取的新账户才可发放；
- 同块重复登记由临时防重表拦截，跨块重复由永久防重表拦截；
- 节点从编译期常量和父状态累计数逐项推导金额，不信任 runtime 事件或 metadata；
- 同一账户同时是本块矿工和新公民时，两笔奖励必须按账户求和后精确到账。

#### 5. 事件

- `CertificationRewardIssued { account_id, cid_number, reward }`：实际铸发并写入永久状态后发出；
- `CertificationRewardSkipped { account_id, cid_number, reason }`：回调因永久/临时重复、上限或金额转换失败而跳过。

`Balances::Issued` 在对应 `CertificationRewardIssued` 之前发生；事件只用于审计，不是节点守卫的信任输入。

#### 6. Weight 与 benchmark

回调 weight 同时覆盖排队和同块 `on_finalize` 的最坏路径。`src/benchmarks.rs` 真实执行回调与
finalize，`src/weights.rs` 由 Substrate benchmark CLI 重新生成；当前测量为 7 次读取、8 次写入，
估算 proof size 3,593 bytes。不得用手写估值替代生成权重。

#### 7. 节点永久守卫

`citizenchain/node/src/guard/citizen_issuance.rs` 使用 RAW storage key 和节点本地 SCALE
镜像，不读取 runtime metadata。它检查：

- 创世只能包含 FRAME 规范空状态：存储版本 0、两个计数的精确零值；
- 父状态不得残留待发队列，finalize 前队列必须连续且不超过本块 extrinsic 数；
- 身份必须首次以 `VotingIdentityByCid` 出现，队列中的完整 CID、`AccountIdByCid` 与 `CidByAccountId` 必须双向一致；
- 永久和临时双重防重、人数边界、档位金额必须逐项一致；
- finalize 后队列必须清空，永久标记和累计数必须精确推进；
- 未登记的 `CitizenIssuance` key 变化、收款账户变化或总发行变化一律 fail-closed。

#### 8. 验收基线（2026-07-10）

- `citizen-issuance` 单元测试：13/13 通过；
- `integration_citizen_identity`：5/5 通过；
- `node_guard`：38/38 通过；`constitution`：38/38 通过；
- runtime benchmark feature 编译及 release benchmark 实跑通过；
- 当前源码 WASM 的隔离双节点真实登记 CID `GD000-CTZN6-616532784-2026`；矿工节点产出
  block#1，禁用挖矿的全节点通过 WSS 导入相同哈希
  `0x702e65e7b64ae7df80dbfb1e16e99ea9909ba302628c3c9d6fc722f6714050c5`；
- `RewardedCount=1`，待发计数不存在，身份、CID/账户永久防重标记均存在；
- Alice 同时是矿工和新公民：两笔奖励合计 1,999,800 分，扣身份登记制度费 100 分后，
  free balance 净增 1,999,700 分；
- 第二轮由 Alice 出块、Bob 作为新公民：block#1 双端哈希一致为
  `0x26d751b62ef23cc5d5884153c1782f67a5922b1d2246f16c5e610e5e034823a6`；Alice 获 PoW
  奖励并支付登记费后净增 999,800 分，Bob 新账户精确收到公民奖励 999,900 分；
- 临时 chainspec、节点数据库、测试签名代码和测试密钥材料已全部删除。

#### 9. CID 直接键与双重防重验收（2026-07-30）

- `IdentityRewardClaimed` 与 `PendingIdentityRewardClaimed` 已从派生哈希改为完整规范
  `cid_number`；账户永久/临时表继续保留，任一键重复即拒绝；
- FRAME benchmark 50×20 真实记录 7 reads / 8 writes，CID map 最大值 49 字节，
  proof size 3,593 字节；
- 单元测试 16/16、身份集成测试 7/7、NodeGuard 发行专项 9/9 通过；
- 生产 release Node 与当前源码 WASM 构建成功；隔离 fresh 节点通过 NodeGuard 自检。

#### 0. 功能需求
`fullnode-issuance` 的功能需求是：在固定的 PoW 奖励区块高度区间内，按照制度常量为成功出块的全节点作者发放固定金额奖励，并允许矿工自行管理奖励接收账户。

模块必须满足以下要求：
- 奖励金额、起始高度、结束高度必须由制度常量固定，不能被链上治理动态修改。
- 只有在奖励区间内，且能够从当前区块共识 digest 识别出作者时，才允许发放奖励。
- 未绑定奖励接收账户时，奖励默认发到矿工自身账户；绑定后发到当前奖励接收账户。
- 矿工无需先出块即可预绑定奖励接收账户，并且可以自行重绑到新账户。
- 奖励接收账户必须与矿工身份账户分离；`rebind` 必须真正切换到不同的新账户。
- 奖励结算必须发生在 `on_finalize`，只对"已经完成的出块行为"进行结算，不做预测性预发。
- 发行都必须链上可审计，便于后续核账。
- 模块不维护逐块奖励列表，只保留节点守卫逐块复算与 warp 累计核验所需的最小审计状态。

#### 1. 模块定位
`fullnode-issuance` 是一个 FRAME pallet，用于在 PoW 链上按固定制度发放全节点铸块奖励。

核心目标：
- 奖励规则常量化（金额、起止高度写死在 `primitives::pow_const`）。
- 奖励触发客观化（仅基于区块高度 + 共识层 `FindAuthor`）。
- 发放可审计（发放均有链上事件）。
- 矿工自主管理奖励接收账户（支持未出块预绑定 + 重绑，无需治理）。

代码位置：
- `<本仓根>/runtime/issuance/fullnode-issuance/src/lib.rs`

---

#### 2. 关键常量与配置
来自 `primitives::pow_const`：
- `FULLNODE_REWARD_START_BLOCK`（当前为 `1`）
- `FULLNODE_REWARD_END_BLOCK`（当前为 `9_999_999`）
- `FULLNODE_BLOCK_REWARD`（每块固定奖励）

Runtime 注入配置：
- `Config::Currency`：奖励铸造与记账货币实现
- `Config::FindAuthor`：从 PreRuntime Digest 解析区块作者

依赖边界：
- `fullnode-issuance` 不直接使用 `sp-std`，Cargo 依赖保持最小化，避免为未使用 crate 传播额外 feature。

---

#### 3. 存储结构
- `LastAuthoredBlockByMiner: Map<AccountId -> u32>`
  - key：矿工身份账户（出块作者）
  - value：该账户最近一次被 PoW digest 证明为区块作者的区块高度
  - 语义：作为节点守卫复算的出块审计状态；不再作为绑定资格来源
- `RewardAccountIdByMiner: Map<AccountId -> AccountId>`
  - key：矿工身份账户（出块作者）
  - value：奖励接收账户
  - 语义：绑定后奖励发放到该账户；未绑定时奖励发到矿工自身账户；重绑会覆盖旧值
- `RewardedBlockCount: u32`
  - 已按固定规则成功发放奖励的区块数；节点按当前高度独立计算期望值
- `TotalFullnodeIssued: Balance`
  - 全节点 PoW 奖励累计发行额；必须恒等于 `RewardedBlockCount × FULLNODE_BLOCK_REWARD`
- `LastRewardAudit: Option<(u32, AccountId, AccountId, Balance)>`
  - 最近一次奖励的区块高度、PoW 作者、实际收款账户与金额
  - 三个审计字段都不是制度真源；节点二进制中的高度范围、金额常量和 PoW digest 才是判定依据

---

#### 4. 事件与错误
主要事件：
- `RewardAccountBound { miner_account_id, reward_account_id }`
- `RewardAccountRebound { miner_account_id, new_reward_account_id }`
- `FullnodeIssuanceIssued { block, miner_account_id, reward_account_id, amount }`
- `FullnodeIssuanceSkippedNoAuthor { block }`

主要错误：
- `RewardAccountAlreadyBound`
- `RewardAccountNotBound`
- `RewardAccountCannotBeMiner`
- `RewardAccountUnchanged`

---

##### 5.1 `bind_reward_account(reward_account_id)`（call index = 0）
- 权限：`Signed`
- 逻辑：
1. 校验调用者未绑定过奖励接收账户
2. 校验 `reward_account_id != miner_account_id`
3. 写入 `RewardAccountIdByMiner`
4. 发送 `RewardAccountBound`
- weight：`T::WeightInfo::bind_reward_account()`

说明：
- 当前允许未出块账户预绑定奖励接收账户；绑定表只在该账户真实成为区块作者时生效。
- 未预绑定时，首笔奖励默认发到矿工身份账户；矿工之后仍可绑定独立奖励接收账户。
- 当前要求奖励接收账户必须与矿工身份账户不同，避免矿工身份与奖励接收账户混同。
- 防垃圾登记由签名交易手续费承担，绑定不会赋予出块资格或改变 PoW 作者身份。

##### 5.2 `rebind_reward_account(new_reward_account_id)`（call index = 1）
- 权限：`Signed`
- 逻辑：
1. 读取当前绑定；未绑定则拒绝
2. 校验 `new_reward_account_id != miner_account_id`
3. 校验 `new_reward_account_id != current_reward_account_id`
4. 覆盖写入新奖励接收账户
5. 发送 `RewardAccountRebound`
- weight：`T::WeightInfo::rebind_reward_account()`

说明：
- 当前不设冷却期；矿工可按需重绑，但必须真正切换到不同的新奖励接收账户。

---

##### 6.1 `on_initialize(n)`: finalize 预算预申报
- 行为：
  - 将 `n` 饱和转换为 `u64` 后判断奖励区间，避免 pallet 对 runtime `BlockNumber` 形成 `Into<u32>` 编译期耦合。
  - 当 `n` 在奖励区间 `[FULLNODE_REWARD_START_BLOCK, FULLNODE_REWARD_END_BLOCK]` 时，返回 `T::DbWeight::get().reads_writes(5, 7)`。
  - 区间外返回 `Weight::zero()`。
- 目的：为 `on_finalize` 的最坏路径预留区块 weight 预算，避免 finalize 工作"未计重"。

##### 6.2 `on_finalize(n)`: 实际奖励结算
流程：
1. 将区块高度饱和转换为 `u64` 做奖励区间判断，若不在奖励区间则直接返回。
2. 进入固定奖励区间后，再转为 `u32` 写入存储和事件字段；该区间上界 `FULLNODE_REWARD_END_BLOCK = 9,999,999` 本身在 `u32` 范围内。
3. 从 `frame_system::digest()` 读取 pre-runtime digest。
4. 通过 `T::FindAuthor::find_author(...)` 解析作者：
   - `None`：发 `FullnodeIssuanceSkippedNoAuthor { block }` 并返回。
5. 写入 `LastAuthoredBlockByMiner[author] = block`，记录该账户已真实出块。
6. 查询 `RewardAccountIdByMiner`：
   - `Some(reward_account_id)`：奖励发到绑定的奖励接收账户。
   - `None`：奖励发到矿工自身账户。
7. 以 `FULLNODE_BLOCK_REWARD` 铸造并发放到收款地址（`deposit_creating`）。
8. 原子更新 `RewardedBlockCount`、`TotalFullnodeIssued` 与 `LastRewardAudit`。
9. 发 `FullnodeIssuanceIssued { block, miner_account_id, reward_account_id, amount }`。

---

#### 7. Weight 策略
当前策略：
- 用户调用（bind/rebind）使用 benchmark 生成的 `T::WeightInfo`。
- `on_finalize` 的执行预算由 `on_initialize` 统一预申报。

注意事项：
- 2026-08-02 已使用 Substrate Benchmark CLI 53.0.0 按 `steps=50`、`repeat=20` 重新生成
  `src/weights.rs`，没有手工改写权重。
- `bind_reward_account` 为 1 次读取 + 1 次写入，生成权重为 `6_000_000` ref time、
  `3545` proof size，再加 1 次数据库读取与 1 次写入。
- `rebind_reward_account` 为 1 次读取 + 1 次写入，生成权重为 `7_000_000` ref time、
  `3545` proof size，再加 1 次数据库读取与 1 次写入。
- `on_finalize` 的预算通过 `on_initialize` 的 `reads_writes(5,7)` 预申报，包含三个审计字段的读写。
- Cargo feature：`runtime-benchmarks` 会向测试/benchmark runtime 使用的 `pallet-balances` 传播；`primitives` 当前不暴露 benchmark feature，不在传播列表中。

---

#### 8. 测试覆盖（当前）
`cargo test --manifest-path citizenchain/runtime/issuance/fullnode-issuance/Cargo.toml` 当前 20 项通过，覆盖：
- 一次性绑定与重复绑定拒绝
- 从未真实出块的账户可以预绑定奖励接收账户
- 首次出块会记录 `LastAuthoredBlockByMiner`
- 预绑定账户收到矿工后续区块奖励
- 绑定矿工自身账户为奖励接收账户会被拒绝
- 起始边界块（`1`）发放
- 结束边界块（`9_999_999`）发放
- 区间外（`0`、`end+1`）不发放
- 未绑定奖励接收账户时奖励发到矿工自身账户
- 多区块累计奖励正确
- `FindAuthor = None` 跳过事件
- 未绑定奖励接收账户时奖励发到矿工并 emit FullnodeIssuanceIssued
- `rebind` 正常路径与未绑定拒绝
- `rebind` 到矿工自身账户会被拒绝
- `rebind` 到当前已绑定奖励接收账户会被拒绝
- `rebind` 后奖励端到端流向新奖励接收账户
- `on_initialize` 区间内外 weight 声明行为
- 每块审计计数、累计发行额和最近奖励审计元组
- 无作者、区间外与多区块累计时审计状态不被错误推进

节点侧 `cargo test --manifest-path citizenchain/node/Cargo.toml node_guard` 当前 22 项通过，其中全节点发行策略覆盖固定边界、绑定/未绑定奖励接收账户、错误奖励金额、错误余额与总发行增量、缺失总发行状态、finalize 前篡改、截止后继续发行、创世基准和 warp 累计状态。

---

#### 9. 运维与审计建议
- 发行审计同时检查累计状态与事件：
  - `RewardedBlockCount`
  - `TotalFullnodeIssued`
  - `LastRewardAudit`
  - 成功：`FullnodeIssuanceIssued`
  - 跳过（无作者）：`FullnodeIssuanceSkippedNoAuthor`
- 奖励接收账户管理建议：
  - 矿工启动后即可出块获得奖励（首次未绑定时默认发到矿工自身账户）。
  - 矿工可在首次出块前通过 `bind_reward_account` 预绑定独立奖励接收账户。
  - 账户迁移或风险处置时使用 `rebind_reward_account` 主动切换奖励接收账户。

##### 10.1 设计原则
矿工密钥有且仅有一把，唯一来源是 keystore 文件。

- **node 进程**（Substrate 框架）：唯一生成密钥的地方
- **keystore**：唯一存储密钥的地方
- **节点桌面端**：只读取 keystore 中的密钥，不生成、不写入

##### 10.2 密钥生成（node 进程）
代码位置：`node/src/service.rs` → `ensure_powr_key()`

节点启动时自检 keystore 中是否已有 `powr` 类型密钥：
- 有 → 直接使用，不再生成
- 没有 → 调用 `sr25519_generate_new(POW_AUTHOR_KEY_TYPE, None)`

Substrate 框架的 `sr25519_generate_new(key_type, None)` 行为：
1. 生成 12 个单词的 BIP39 助记词
2. 从助记词推导 sr25519 密钥对
3. 将助记词写入 keystore 磁盘文件（JSON 编码字符串）
4. 文件名格式：`{key_type_hex}{pubkey_hex}`

注意：`sr25519_generate_new(key_type, Some(suri))` 只存内存不写磁盘，进程退出后丢失，不可用。

##### 10.3 奖励接收账户绑定（node 自定义 RPC）
代码位置：`node/src/core/rpc.rs` → `reward_bindAccount` / `reward_rebindAccount`

绑定/重绑奖励接收账户完全由 node 端完成：
1. 节点桌面端调用 node 的自定义 RPC `reward_bindAccount(reward_account_id)` 或
   `reward_rebindAccount(new_reward_account_id)`；边界文本进入交易前严格解析为 `AccountId`
2. node 从 keystore 读取 `powr` 公钥，使用 `keystore.sr25519_sign()` 签名交易
3. 构造完整的 `UncheckedExtrinsic` 并提交到交易池

节点桌面端 **不读取私钥、不签名**，仅传入奖励接收账户的 SS58 边界输入。签名使用与出块相同的 `sp_core` 密钥推导路径，确保签名身份与出块作者身份一致。

##### 10.4 节点桌面端的角色
- 只读取默认链（`citizenchain`）keystore 文件名中的公钥（`local_powr_miner_account_hex`），用于前端展示矿工身份；不遍历其他链目录，避免旧链残留 keystore 导致身份错位
- 设置奖励接收账户时在同步路径提前校验 `reward_account_id != miner_account_id`，避免先存后验
- 通过 `state_getStorage` 查询链上 `RewardAccountIdByMiner` 状态，判断是否需要 bind 或 rebind
- 所有签名和交易提交委托给 node 端 RPC

##### 10.5 密钥使用流程
1. 用户首次启动节点 → node 的 `ensure_powr_key()` 生成密钥并写入 keystore
2. 节点出块 → `author_pre_digest()` 从 keystore 读取公钥作为区块作者
3. 用户可在出块前或出块后设置奖励接收账户 → 节点桌面端调用 node RPC
   `reward_bindAccount` → node 用 keystore 密钥签名并提交
4. 链上 `bind_reward_account` 记录 `RewardAccountIdByMiner`，但不改变矿工身份或出块资格
5. 节点出块后，链上 `on_finalize` 解析 PoW digest、记录 `LastAuthoredBlockByMiner`，并把奖励
   发到当前奖励接收账户；未绑定时发到矿工自身账户

由于出块和绑定使用的是同一把 keystore 密钥（同一个 `sr25519_sign` 路径），
`RewardAccountIdByMiner` 映射的 key 与出块作者一致，奖励能正确发到绑定账户。

---

#### 11. 审查结论与建议
当前审查结论：

当前状态：
1. `bind_reward_account` 允许未出块账户预绑定；绑定表只在真实成为区块作者时被读取，不赋予
   出块资格，垃圾登记由签名交易手续费约束。
2. 已明确禁止把矿工身份账户本身作为奖励接收账户，避免身份账户与奖励接收账户混同。
3. `rebind` 当前不设冷却期，但必须切换到不同的新奖励接收账户；重复绑定当前账户会直接拒绝，
   避免无意义写操作和事件。
4. 绑定 benchmark、测试前置与辅助函数残留已清理，权重已用当前 runtime 重新生成；
   `on_finalize` 预算仍由 `on_initialize` 预申报。
5. …32451 tokens truncated…建立岗位阈值；达阈值后回调本模块自动执行 sweep。

##### 3. try_execute_sweep（内部方法）

1. 校验提案状态为 `STATUS_PASSED`
2. `InstitutionAsset::can_spend` 检查（action = `OffchainFeeSweepExecute`）
3. 计算手续费：`calculate_onchain_fee(amount)` — 费率 0.1%，有最低值
4. **余额检查**：划转和手续费支出后，费用账户余额必须 `>= ED`
5. **Cap 检查**：`amount <= (fee_balance - ED) * 80 / 100`
6. 在同一 storage transaction 中先由 `OnchainFeeCharger::charge(fee_account, amount)` 收取执行手续费，再执行 `Currency::transfer` 从 fee_account 到 main_account（KeepAlive）
7. 任一失败时手续费、分账和本金划转全部回滚
8. 成功手续费经 `OnchainExecutionFeeDistributor` -> `OnchainFeeRouter` 按 80/10/10 分账
9. 触发 `SweepToMainExecuted` 事件（含 `reserve_left` 余额）
10. 返回 `ProposalExecutionOutcome::Executed`，由投票引擎设置提案状态为 `STATUS_EXECUTED`

#### 手续费分账路径

`OnchainExecutionFeeDistributor` 将执行期 `NegativeImbalance` 等额转换为 `Credit`，传递给 `OnchainFeeRouter`：
- 80% -> 当前区块矿工（PoW 全节点）
- 10% -> 国家储委会费用账户
- 10% -> 国家储委会安全基金账户

#### 错误码

| 错误 | 触发条件 |
|------|----------|
| `InvalidSweepAmount` | 金额为 0 |
| `InvalidInstitution` | 机构非 NRC/PRB |
| `UnauthorizedAdmin` | 稳定错误码：调用者不是所提交岗位的有效任职人，或该岗位没有 sweep 提案权限 |
| `SweepProposalNotFound` | proposal_id 无对应记录 |
| `SweepProposalNotPassed` | 提案未通过 |
| `InsufficientFeeReserve` | 余额不足以覆盖划转+手续费+保留 |
| `SweepAmountExceedsCap` | 超过可用余额 80% 上限 |
| `InstitutionSpendNotAllowed` | 资产保护检查未通过 |

### offchain-transaction · 扫码支付 Step 1 技术说明

- **日期**:2026-04-19
- **范围**:本模块内扫码支付清算体系 Step 1(同清算行内 MVP)的**已落地代码**
- **上层 ADR**:`CitizenChainRuntime.md`
- **总任务卡**:`tasks/20260419-扫码支付-step1-同行MVP.md`

---

#### 1. 本步范围

Step 1 已落地:L3 绑定清算行 + 充值 + 提现 + 切换的链上接口,以及清算行合法性判定和 L3 支付意图签名结构。**同行扫码支付的批次上链**逻辑定义了数据结构和 nonce 辅助,但**batch 执行流**(settlement.rs)由 Step 2 落地,避免一次性重写现有"省储行清算"代码。

**并存策略**:本步新增的 Storage / Event / Error / Call 与现有 `submit_offchain_batch` / `execute_batch` / `InstitutionRateBp` / `RecipientClearingInstitution` 等完全独立,不触碰旧逻辑。Step 2 开始正式替换旧省储行清算模型。

#### 2. 新增文件

```
src/
├── batch_item.rs    # Step 1 新增:PaymentIntent 结构 + 签名域常量 + 单测
├── bank_check.rs    # Step 1 新增:清算行合法性判定 + CidAccountQuery trait
├── deposit.rs       # Step 1 新增:bind/deposit/withdraw/switch 四函数实现
└── nonce.rs         # Step 1 新增:L3PaymentNonce 消费辅助
```

#### 3. 清算行合法性模型

清算行 = `K1=S` 私法人或 `K1=F` 非法人(两者皆私权机构),对应 `citizenchain/onchina/src/codes/category.rs` 的 `InstitutionCategory::PrivateInstitution`。

链上**不新增** CID 枚举,而是直接对实体生命周期模块登记的 `cid_number` 字节做 K1 字节匹配。

```rust
pub fn subject_property_is_private_institution(cid_bytes: &[u8]) -> bool {
    matches!(cid_bytes.get(6), Some(b'S' | b'F'))
}
```

合法清算行的六条并列条件(`ensure_can_be_bound`):
1. 在 `AccountRegisteredCid` 有登记
2. `name` 段等于 `"主账户"`(3 字节 UTF-8 × 3 字 = 9 字节)
3. K1 ∈ {S, F}
4. `InstitutionAccounts[(cid_number, "主账户")].status == Active`
5. `CidAccountQuery::is_clearing_bank_eligible(bank_main)` 通过。2026-05-02 起 OnChina 系统负责 `eligible-search` 候选筛选,链上不再保存 `subject_property/sub_type/parent_cid_number` 元数据,这里只确认账户属于已注册且 Active 的 CID 机构账户
6. `ClearingBankNodes[cid_number]` 已声明,确保用户不能绑定到"合法机构但未加入清算网络"的节点

#### 4. 解耦抽象 `CidAccountQuery`

`bank_check` 不直接依赖具体实体生命周期 pallet,而是通过 trait 抽象:

```rust
pub trait CidAccountQuery<AccountId> {
    fn account_info(addr: &AccountId) -> Option<(Vec<u8>, Vec<u8>)>;
    fn find_account(cid_number: &[u8], account_name: &[u8]) -> Option<AccountId>;
    fn account_exists(addr: &AccountId) -> bool;
    fn is_institution_admin(cid_number: &[u8], who: &AccountId) -> bool;
    fn is_clearing_bank_eligible(addr: &AccountId) -> bool;
    fn is_registered_clearing_node(bank: &AccountId) -> bool;
}

// 默认 () 实现返回未登记,供测试用。
```

**runtime 层实现**（`citizenchain/runtime/src/configs.rs` 的 `MultisigCidAccountQuery`）：按公权、私权机构 CID 聚合账户索引：
- `PublicManage::AccountRegisteredCid` / `PrivateManage::AccountRegisteredCid` → `account_info`
- `PublicManage::InstitutionAccounts[(cid_number, account_name)]` / `PrivateManage::InstitutionAccounts[...]` → `find_account`、`account_exists`、`is_clearing_bank_eligible`
- `RuntimeInstitutionAdminQuery` 以 `AdminAccounts[cid_number].admins` → `is_institution_admin`
- `ClearingBankNodes` → `is_registered_clearing_node`

**好处**:
- offchain-transaction 的 Cargo.toml 不新增实体生命周期 pallet 依赖
- pallet 的 `Config` trait 不强制 `T: public_manage::Config` 或 `T: private_manage::Config`
- 现有 tests 可直接用 `type CidAccountQuery = ();`,不破坏已有测试

#### 5. L3 PaymentIntent 签名格式

```rust
pub struct PaymentIntent<AccountId, BlockNumber> {
    pub tx_id: H256,
    pub payer_account_id: AccountId,
    pub payer_bank_cid: InstitutionCidNumber,
    pub recipient_account_id: AccountId,
    pub recipient_bank_cid: InstitutionCidNumber,
    pub amount: u128,
    pub fee: u128,
    pub nonce: u64,
    pub expires_at: BlockNumber,
}

// 签名消息 = signing_message(OP_SIGN_L3_PAY, SCALE(intent))
pub use primitives::sign::OP_SIGN_L3_PAY;
```

citizenapp（Dart 端）必须逐字节对齐唯一
`signing_message(OP_SIGN_L3_PAY, SCALE(intent))` 与字段顺序，否则链上验签失败。runtime
清算 pallet 的签名账户在编译期固定为官方 `AccountId32`，并以完整32字节构造 sr25519
`Public`；禁止截取泛型 SCALE 编码推断公钥。

#### 6. 新增 Storage

| Storage | 类型 | 语义 |
|---|---|---|
| `UserBank` | `StorageMap<L3, BankMain, OptionQuery>` | L3 → 绑定的清算行主账户 |
| `DepositBalance` | `StorageDoubleMap<BankMain, L3, u128>` | 权威账本:各 L3 在该清算行的存款(分) |
| `BankTotalDeposits` | `StorageMap<BankMain, u128>` | 清算行存款总额(偿付对账) |
| `L3PaymentNonce` | `StorageMap<L3, u64>` | L3 单调递增 nonce(防重放,Step 2 起激活) |

不变式:`BankTotalDeposits[bank] == Σ DepositBalance[bank][*]`。
Step 2 起增加偿付自动保护:`主账户链上余额 ≥ BankTotalDeposits[bank]`。

#### 7. 新增 Call(call_index 30-33)

| call_index | 方法 | 费用归类 | 说明 |
|---|---|---|---|
| 30 | `bind_clearing_bank(bank_main)` | 签名者链上操作 0.1 元 | 绑定即开户无预存 |
| 31 | `deposit(amount)` | 链上资金 0.1% 最低 0.1 元 | 自持 → 清算行主账户 |
| 32 | `withdraw(amount)` | 链上资金 0.1% 最低 0.1 元 | 清算行主账户 → 自持 |
| 33 | `switch_bank(new_bank)` | 签名者链上操作 0.1 元 | 前置:旧清算行余额为 0 |

费用与付款方由 `citizenchain/runtime/src/configs.rs::RuntimeFeeRouter` 的 `OffchainTransaction` 分支显式返回统一 `FeeRoute`，不走兜底。

#### 8. 新增 Event(4 个)

```
BankBound      { user, bank }
Deposited      { user, bank, amount }
Withdrawn      { user, bank, amount }
BankSwitched   { user, old_bank, new_bank }
```

#### 9. 新增 Error(19 个)

清算行身份类:`NotRegisteredClearingBank` / `NotMainAccount` / `NotPrivateInstitution` /
`ClearingBankNotActive` / `FeeAccountNameTooLong` / `FeeAccountNotFound`

绑定/切换类:`AlreadyHasBank` / `NoOpenedBank` / `NewBankSameAsCurrent` /
`MustClearBalanceFirst`

存取类:`DepositAmountTooSmall` / `WithdrawAmountTooSmall` /
`InsufficientDepositBalance` / `InsufficientBankLiquidity` / `DepositForbidden` /
`WithdrawForbidden`

L3 签名类:`L3NonceOverflow` / `InvalidL3Nonce`(Step 2 启用)

#### 10. `institution-asset` 新增 4 个 Action

```
L3DepositIn      # L3 充值入清算行主账户
L3WithdrawOut    # 清算行主账户对 L3 提现
L2ClearingDebit  # Step 2:扫码清算时扣 payer_bank
L2FeeCollect     # Step 2:扫码清算时向 fee_account 收费
```

`()` 默认 fail-open,runtime 的 `RuntimeInstitutionAsset` 会按"清算行主账户是否合法"严格裁决。

#### 11. 与现有模块的边界

| 模块 | 动向 |
|---|---|
| `public-manage` / `private-manage` | 清算行注册复用实体生命周期登记与账户状态 |
| `multisig-transfer` | **不动** |
| `onchain-transaction` | **不动** |
| `citizen-identity` | **不动** |
| `institution-asset` | **扩展 4 枚举**(代码改动见上) |
| `offchain-transaction` 现有省储行清算逻辑 | **不动**,Step 2 替换 |

#### 12. 编译验证

```
$ cargo check -p offchain-transaction
$ cargo check -p institution-asset
### (新增 struct/trait impl/match arm),无结构性风险,留 CI 把关。
```

#### 13. 后续 Step 2 / Step 3 展望

**Step 2**:
- 新增 `settlement.rs`:同行/跨行 `execute_batch` 分账
- 新增 `fee_config.rs`:`L2FeeRateBp` + 延迟生效
- 新增 `solvency.rs`:偿付自动保护
- 废弃旧 `submit_offchain_batch` 的省储行模型

**Step 3**:
- 新增 `dispute.rs` / `reserve.rs`
- 新增 `close_clearing_bank`
- 白皮书 5.4.4 发布

#### 14. 测试覆盖(Step 1)

本步 tests 模块继续跑原有 `submit_offchain_batch` 相关测试(未被破坏)。
新增的 bind/deposit/withdraw/switch 的**负向路径**(未登记 → `NotRegisteredClearingBank`)可通过 `type CidAccountQuery = ()` 的默认实现自动覆盖。
**正向路径**测试需 mock `CidAccountQuery` 返回 Some,建议在 Step 2 引入完整 mock 一并实现。

#### 15. 变更记录

- 2026-04-19:Step 1 首次落地,新增 4 文件 + lib.rs 聚合扩展 + 跨模块配置改动。
- 2026-05-02:清算行资格元数据从链上移除。CID 负责候选资格筛选,链上只确认机构账户已注册、Active 且已声明节点。

---

### 省储行费率治理(LEGACY · 已下线)

> ⚠️ **Step 2b-iv-b(2026-04-20)已彻底删除** ADR-006 宣布退出的"省储行即时清算"
> 体系。本文件描述的 `propose_institution_rate` / `vote_institution_rate` /
> `InstitutionRateBp` / `RateProposalActions` 等 Call/Storage 均已从 runtime
> 物理移除。当前清算行(L2)体系费率治理见:
>
> - `call_index 40 propose_l2_fee_rate` + `41 set_max_l2_fee_rate`(Root / 联合投票)
> - Storage `L2FeeRateBp` / `L2FeeRateProposed` / `MaxL2FeeRateBp`
> - 延迟 7 天生效机制由 `on_initialize` + `fee_config::activate_pending_rates` 落实
> - 技术文档:`STEP2B_IV_B_RUNTIME_CLEANUP.md`(清理记录)与 runtime pallet
>   源码 `src/fee_config.rs`
>
> 下方内容仅作历史参考。

#### 概述

各省储行的链下交易费率通过内部投票（InternalVoteEngine）进行治理。省储行管理员（PRB admin）可发起费率变更提案，经内部投票通过后自动生效。

#### 费率范围

| 参数 | 值 | 说明 |
|------|------|------|
| OFFCHAIN_RATE_BP_MIN | 1 bp | 0.01% |
| OFFCHAIN_RATE_BP_MAX | 10 bp | 0.1% |
| BP_DENOMINATOR | 10,000 | 基点转换分母 |
| OFFCHAIN_MIN_FEE | 1 分 | 单笔最低手续费 0.01 元 |

费率单位为基点（bp），1 bp = 0.01%。合法范围 1~10 bp，对应 0.01%~0.1%。

#### 默认费率

未设置费率的省储行（`InstitutionRateBp` 存储值为 0）按最低费率 `OFFCHAIN_RATE_BP_MIN`（1 bp = 0.01%）执行。由 `ensure_rate_and_institution` 内部处理。

#### 存储

```rust
// 各省储行链下清算费率（bp，范围1~10）
pub type InstitutionRateBp<T> =
    StorageMap<_, Blake2_128Concat, InstitutionPalletId, u32, ValueQuery>;

// 费率治理提案动作
pub type RateProposalActions<T: Config> =
    StorageMap<_, Blake2_128Concat, u64, RateProposalAction, OptionQuery>;

pub struct RateProposalAction {
    pub institution: InstitutionPalletId,
    pub new_rate_bp: u32,
}
```

##### 1. 发起提案（propose_institution_rate）

- **调用者**：省储行管理员（PRB admin，通过 `is_prb_admin` 验证）
- **参数**：institution（省储行 PalletId）、new_rate_bp（目标费率）
- **校验**：new_rate_bp 必须在 1~10 范围内
- **操作**：
  1. 通过 InternalVoteEngine 创建内部提案（org=ORG_PRB）
  2. 将 RateProposalAction 写入 RateProposalActions 存储
  3. 触发 InstitutionRateProposed 事件

##### 2. 投票（vote_institution_rate）

- **调用者**：同一省储行的其他管理员
- **参数**：proposal_id、approve（赞成/反对）
- **操作**：
  1. 验证 proposal_id 对应的 RateProposalAction 存在
  2. 验证调用者是该省储行管理员
  3. 调用 InternalVoteEngine::cast_internal_vote 记录投票
  4. 若赞成票且提案状态变为 PASSED，立即尝试执行

##### 3. 自动执行（try_execute_rate）

投票通过后在同一交易中自动执行，使用 `with_transaction` 保证原子性：

1. 验证提案状态为 PASSED、kind 为 INTERNAL、institution 匹配
2. 将 new_rate_bp 写入 `InstitutionRateBp` 存储
3. 触发 InstitutionRateUpdated 事件
4. 设置提案状态为 EXECUTED

若执行失败，回滚并触发 InternalProposalExecutionFailed 事件。

#### 费用计算公式

```
fee = max(amount * rate_bp / 10000, OFFCHAIN_MIN_FEE)
```

node 端 `calc_offchain_fee` 与链上计算逻辑保持一致。

#### 提案清理

过期或已执行的提案可通过 `prune_rate_proposal`（call_index=12）清理，移除 RateProposalActions 中的记录。

#### 源码位置

- `citizenchain/runtime/transaction/offchain-transaction/src/lib.rs`
  - `propose_institution_rate`（call_index=1）
  - `vote_institution_rate`（call_index=2）
  - `try_execute_rate`（内部方法）
  - `ensure_rate_and_institution`（内部方法）

---

### OFFCHAIN_TECHNICAL(LEGACY · 已下线)

> ⚠️ **Step 2b-iv-b(2026-04-20)已彻底删除** 本文描述的省储行清算体系:
> `submit_offchain_batch` / `enqueue_offchain_batch` / `process_queued_batch` /
> `bind_clearing_institution` / `propose_institution_rate` /
> `vote_institution_rate` / 相关清理 Calls 全部从 runtime 物理移除;
> `InstitutionRateBp` / `RecipientClearingInstitution` / `QueuedBatches` /
> `RateProposalActions` 等 Storage 同步删除。
>
> **当前清算行(L2)体系**技术文档:
> - `CitizenChainRuntime.md`(决策)
> - `CitizenChainRuntime.md`(清理记录)
> - `CitizenChainRuntime.md`(集成测试)
> - `CitizenChainRuntime.md`(Step 2a 实现)
> - `CitizenChainRuntime.md`(Step 1 骨架)
>
> 下方内容仅作历史参考,**不反映当前 runtime 行为**。

模块：`offchain-transaction`(LEGACY)
范围：省储行链下清算批次的上链验证、队列重试、治理配置与清理

##### 0.1 核心职责
- 为省储行链下清算批次提供链上验证、入队、出队执行和审计留痕能力。
- 保证批次执行严格按机构内 `batch_seq` 单调推进，不允许跨序号乱序落账。
- 将链下手续费独立结转到机构 `fee_account`，并支持后续治理归集。

##### 0.2 机构与账户模型需求
- 每个收款账户必须先绑定所属清算省储行，未绑定不得作为链下批次收款方。
- 每个机构必须具备独立费率、验签密钥、relay 提交白名单和手续费账户。
- 机构主账户只负责初始化默认验签密钥和 relay 白名单；后续调整必须走内部治理。

##### 0.3 批次校验需求
- 批次中的每条交易必须包含 `tx_id`、`payer`、`recipient`、`transfer_amount`、`offchain_fee_amount`。
- `transfer_amount` 必须大于 0，`payer != recipient`，且付款源不能是制度保护地址。
- `tx_id` 必须在批次内唯一，且不能命中已处理窗口或待处理队列索引。
- `recipient` 必须已绑定清算机构，且绑定机构必须与批次机构一致。
- `offchain_fee_amount` 必须严格等于按当前制度费率计算出的链下手续费。

##### 0.4 提交与队列需求
- 直接提交路径 `submit_offchain_batch` 与入队路径 `enqueue_offchain_batch` 都必须校验 relay 白名单。
- 直接提交必须在不存在待处理 backlog 的前提下执行。
- 入队时必须验证批次签名、锁定费率快照、锁定验签密钥纪元，并建立 `QueuedTxIndex` 防重。
- 出队重试必须只处理 `Pending` 批次；处理成功、失败、取消都必须留下可观测状态。
- 批次因换钥失效时，只有当前队头批次允许推进执行序号，不能跨序号跳过更早批次。

##### 0.5 签名与换钥需求
- 批次签名消息必须固定为 `signing_message(OP_SIGN_OFFCHAIN_BATCH, SCALE(institution, batch_seq, batch))`。
- 入队和直接提交必须使用当前生效验签密钥完成验签。
- 普通换钥必须走内部投票并延迟生效；紧急换钥必须要求至少两名管理员确认。
- 已入队批次必须记录 `verify_key_epoch_snapshot`；当纪元落后于当前值时，批次必须作废且不得继续执行。

##### 0.6 资金与治理需求
- 批次执行时，主金额必须从 `payer` 转给 `recipient`，链下手续费必须从 `payer` 转给机构 `fee_account`。
- `fee_account` 余额只能通过内部治理提案划转到机构主账户，且划转后必须保留最低储备并受单次比例上限约束。
- 费率、验签密钥、relay 白名单、手续费归集都必须通过内部治理提案驱动并留下事件。
- 若 `payer` 或 `fee_account` 命中制度账户白名单边界，还必须通过 `institution-asset` 的资金动作检查。

##### 0.7 存储治理与可运维需求
- 模块必须保存 processed tx、防重窗口、队列记录、批次摘要和提案动作映射，支持手动与自动清理。
- `on_initialize` 必须负责普通换钥激活，`on_idle` 必须在剩余权重内做有界清理。
- 清理 pending 队列或 stale 队列时，必须遵守“只能推进队头序列”的约束，避免破坏执行顺序。

#### 1. 目标与边界
- 本模块不负责链下“确认终态”，仅负责批次上链执行与治理控制。
- 链下系统负责业务撮合和不可变确认；本模块负责链上可审计落账。
- 上链手续费由机构手续费账户承担。

#### 2. 核心业务口径
1. 收款账户需先绑定清算省储行。
2. 机构费率由治理维护，范围 `1..=10 bp`。
3. 链下已确认交易可按批次上链（直接提交或先入队后处理）。
4. 每条批次项包含：
   - `tx_id`
   - `payer`
   - `recipient`
   - `transfer_amount`
   - `offchain_fee_amount`
5. 要求：
   - `transfer_amount > 0`
   - `payer != recipient`
   - `tx_id` 在批次内唯一
   - `recipient` 已绑定且绑定机构一致

#### 3. 签名与验证密钥
- 批次消息为 `signing_message(OP_SIGN_OFFCHAIN_BATCH, SCALE(institution, batch_seq, batch))`。
- 入队路径必须验签（当前机构生效密钥）。
- 出队处理路径不重复验签；依赖入队验签结果与密钥纪元机制防止旧签名继续执行。

##### 3.1 密钥轮换（普通）
- 普通治理轮换在通过后进入 `PendingVerifyKeys`，延迟生效。
- `on_initialize` 到激活高度时切换到新 key，并递增 `VerifyKeyEpoch`。
- 若机构已有 pending 轮换，再次通过新轮换会被拒绝（防覆盖）。

##### 3.2 密钥轮换（紧急）
- `emergency_rotate_verify_key` 采用双管理员确认（至少 `EMERGENCY_ROTATE_MIN_ADMINS=2`）。
- 支持 `cancel_emergency_rotation_approval` 撤回审批。
- 紧急轮换成功后：
  - 立即替换当前 key
  - 清空 `PendingVerifyKeys`
  - 从 `PendingRotationInstitutions` 主动移除机构
  - 递增 `VerifyKeyEpoch`
  - 清空该机构下所有紧急审批键空间（`clear_prefix`）

#### 4. 防重放与队列
- 已执行交易防重：`ProcessedOffchainTx + ProcessedOffchainTxAt`（带保留窗口）。
- 待处理队列防重：`QueuedTxIndex` 防止跨入队批次重复 `tx_id`。
- 批次顺序：
  - 执行序号：`LastBatchSeq`
  - 入队序号：`NextEnqueueBatchSeq`
- 队列状态：`Pending | Processed | Failed | Cancelled`。

##### 4.1 密钥纪元失效
- 入队时记录 `verify_key_epoch_snapshot`。
- 处理时若与当前 `VerifyKeyEpoch` 不一致，批次会自动标记 `Cancelled` 并释放 `QueuedTxIndex`，不会执行资金转移。

#### 5. 执行与重试
- 直接路径：`submit_offchain_batch`（事务包裹执行）。
- 队列路径：
  - `enqueue_offchain_batch`（持久化）
  - `process_queued_batch`（重试执行）
- 失败策略：
  - 可重试错误保留队列并累计 `retry_count`
  - 超过 `MAX_QUEUE_RETRY_COUNT` 变为 `Failed`
  - 管理员可 `skip_failed_batch` 推进序号
  - 管理员可 `cancel_queued_batch` 取消队头 pending 批次
  - 管理员可 `cancel_stale_queued_batches` 批量取消过期 pending（按队头可推进序列原则）

#### 6. 清理与存储治理
- 手动清理入口：
  - `prune_queued_batch`
  - `prune_batch_summary`
  - `prune_processed_tx`
  - `prune_expired_proposal_action`
- 自动清理入口：
  - `on_idle` 有界清理 `processed/queued/summary`
- 过期 pending 队列：
  - `auto_prune_one_queued_batch` 对过期 pending 会直接清理存储与 `QueuedTxIndex`。
  - `prune_queued_batch` 对 pending 也可用 `enqueued_at` 判定过期后清理。

#### 7. 治理动作
- 费率治理：`propose/vote_institution_rate`。
- 验签密钥治理：`propose/vote_verify_key`。
- 机构资金归集治理：`propose/vote_sweep_to_main`。
- relay 白名单治理：`propose/vote_relay_submitters`。
- 对“已通过但执行失败”的提案：`retry_execute_proposal`。

#### 7.1 治理提案执行与 STATUS_EXECUTED

四类治理动作（rate、verify_key、sweep、relay_submitters）在投票通过后由投票引擎回调本模块执行。执行成功后，本模块调用 `votingengine::Pallet::<T>::set_status_and_emit(proposal_id, STATUS_EXECUTED)` 将投票引擎侧的提案状态标记为已执行，防止同一提案被重复执行。

各动作执行函数：
- `try_execute_rate`：执行成功后调用 `set_status_and_emit(proposal_id, STATUS_EXECUTED)`
- `try_execute_verify_key`：执行成功后调用 `set_status_and_emit(proposal_id, STATUS_EXECUTED)`
- `try_execute_sweep`：执行成功后调用 `set_status_and_emit(proposal_id, STATUS_EXECUTED)`
- `try_execute_relay_submitters`：执行成功后调用 `set_status_and_emit(proposal_id, STATUS_EXECUTED)`

提案状态流转：`VOTING → PASSED → EXECUTED`

行为变更：
- 执行成功后不再立即删除 `ProposalActions` 中的动作数据，而是保留原始数据用于审计。
- 动作数据由投票引擎的 90 天延迟清理机制统一回收，避免执行后立即丢失提案上下文。

#### 8. 权重与完整性约束
- 关键权重按最坏项上界声明（含 `MaxBatchSize` 影响）。
- 关键常量完整性在 `integrity_test` 中断言：
  - 费率上下界
  - 紧急管理员阈值
  - 分母与阈值非零
  - `MaxBatchSize > 0`

#### 9. 存储版本
- pallet 已声明 `#[pallet::storage_version]`，当前版本为 `1`，用于未来安全迁移判定。

#### 10. 关键事件（观测建议）
- 批次：`Submitted/Queued/Processed/Failed/Cancelled/Pruned`
- 紧急换钥：`Approval`、`ApprovalCancelled`、`EmergencyRotated`
- 密钥普通轮换：`RotationScheduled`、`VerifyKeyRotated`
- 治理执行：`InternalProposalExecutionFailed`、`ProposalExecutionRetried`

#### 11. 当前测试覆盖（摘要）
- 紧急换钥双管理员门限、审批撤回、单审批不生效。
- 密钥纪元失效导致已入队批次自动取消。
- pending key 不覆盖保护。
- retry_execute_proposal 成功/失败分支。
- on_idle 自动清理（含 stale pending）与 on_initialize 轮换 epoch 递增。
- 批量取消 stale pending、序列推进与队列行为一致性。

---

### offchain-transaction · 扫码支付 Step 2a 技术说明(Runtime 重写层)

- **日期**:2026-04-19
- **范围**:扫码支付清算体系 Step 2 在 **Runtime 层**的落地代码(本体)
- **上层 ADR**:`CitizenChainRuntime.md`(其第 5 节 "Step 2 留档"即本步)
- **总任务卡**:`tasks/20260419-扫码支付-step1-同行MVP.md`
- **前置文档**:`STEP1_TECHNICAL.md`(Step 1 Runtime)
- **后续文档**:`STEP2B_NODE.md`(节点接入)、`STEP2C_CITIZENAPP.md`(前端)、`STEP2D_CLEANUP.md`(删除旧代码)

---

#### 1. 本步范围

Step 2a 只做 **Runtime 新增**,与旧"省储行清算"代码路径**共存**:

- 新增 3 个子模块:`fee_config.rs` / `solvency.rs` / `settlement.rs`
- 新增 1 个结构:`batch_item::OffchainBatchItem`
- 新增 4 个 Storage:`L2FeeRateBp` / `L2FeeRateProposed` / `MaxL2FeeRateBp` / `LastClearingBatchSeq`
- 新增 5 个 Event:`L2FeeRateProposed` / `L2FeeRateActivated` / `MaxL2FeeRateUpdated` / `PaymentSettled` / `ClearingBankBatchSettled`
- 新增 Error 覆盖清算行结算、费率、偿付、batch 签名与用户绑定一致性:
  `InstitutionMismatch` / `ExpiredIntent` / `L2FeeRateNotConfigured` / `InvalidL3Signature` /
  `InvalidL2FeeRate` / `SolvencyProtected` / `InvalidBatchSignature` / `InvalidBatchSeq` /
  `UserBankMismatch`
- 新增 3 个 Call:`submit_offchain_batch`(34)/ `propose_l2_fee_rate`(40)/ `set_max_l2_fee_rate`(41)
- 机构授权 trait 方法统一为 `CidAccountQuery::is_institution_admin(actor_cid_number, who)`；账户查询只证明具体机构账户归属，不承担管理员身份语义
- 扩展 `on_initialize`:激活到期费率提案
- runtime 层 `MultisigCidAccountQuery` 按 CID 实现 `is_institution_admin`
- runtime 层当前由唯一 `RuntimeFeeRouter` 将相关 call 显式映射为 `FeeRoute`，费用类别和付款账户不可分离

**明确不做**(留 Step 2b/2c/2d):
- Node 层接入(Step 2b)
- citizenapp 改造(Step 2c)
- 删除旧 `submit_offchain_batch` / `bind_clearing_institution` / `RecipientClearingInstitution` / `InstitutionRateBp`(Step 2d)
- 联合投票回调 `set_max_l2_fee_rate`(Step 2b 接入;当前为 Root Origin)
- 争议仲裁 / 保证金(Step 3)

##### 2.1 `fee_config.rs`

清算行费率自治:
- `L2_FEE_RATE_BP_MIN = 1` / `L2_FEE_RATE_BP_MAX = 10`(bp)
- `RATE_CHANGE_DELAY_BLOCKS = 1_680`（7 × 240 块；按六分钟平均目标换算，不承诺自然日最晚出块）
- `do_propose_l2_fee_rate(who, actor_cid_number, institution_account, new_rate)`:按 CID 校验管理员和主账户归属后写 `L2FeeRateProposed`
- `do_set_max_l2_fee_rate(new_max)`:设全局上限(Step 2b 改为联合投票回调)
- `activate_pending_rates(now)`:`on_initialize` 调用,搬到期提案到 `L2FeeRateBp`
- `current_rate_bp(bank)`:查当前生效费率(供 settlement 查收款方费率用)

##### 2.2 `solvency.rs`

偿付能力自动保护:
- `ensure_can_debit(bank_main, debit_fen)`:校验扣款后主账户余额仍 ≥ `BankTotalDeposits`
- `solvency_ratio_bp(bank_main)`:返回偿付率(万分之)供监控用
- Step 3 追加 `emit_warning_if_low` 告警事件 + 自动冻结

##### 2.3 `settlement.rs`

清算行批次的新 execute 路径:
- `execute_clearing_bank_batch(submitter, actor_cid_number, institution_account_id, batch)`:批次级执行入口
  - 批次级预检:submitter 管理员身份 / batch_signature / batch_seq / UserBank 绑定一致性 / 费率正确性 / 偿付充足
  - 逐笔 `execute_single_item`:L3 签名验证 / nonce / 分账(同行 vs 跨行)/ 防重放
- 费率按 **收款方清算行** `L2FeeRateBp[recipient_bank]` 计算
- 手续费**全部归收款方清算行的费用账户**,无省储行分成

##### 2.4 `batch_item::OffchainBatchItem`

当前批次项：
```rust
pub struct OffchainBatchItem<AccountId, BlockNumber> {
    pub tx_id: H256,
    pub payer_account_id: AccountId,
    pub payer_bank_cid: InstitutionCidNumber,
    pub recipient_account_id: AccountId,
    pub recipient_bank_cid: InstitutionCidNumber,
    pub transfer_amount: u128,
    pub fee_amount: u128,
    pub payer_sig: [u8; 64],   // L3 sr25519 签名
    pub payer_nonce: u64,
    pub expires_at: BlockNumber,
}
```

`to_intent()` 反向构造 `PaymentIntent` 用于重算签名哈希验签。

##### 2.4.1 `AccountId32` 与 sr25519 验签边界

- offchain pallet 的 `Config` 在编译期限定
  `frame_system::Config<AccountId = sp_runtime::AccountId32>`；L3 和批次管理员签名均固定为
  sr25519。
- `AccountId32` 使用完整32字节转换为 sr25519 `Public`，禁止把泛型 `AccountId` 的 SCALE
  编码截取前32字节。未来若更换账户类型，必须先显式重审清算签名模型，否则编译失败。
- 此约束不修改 Storage、Extrinsic、SCALE 字段、签名消息、扣费或清算规则；有效的现有
  sr25519 `AccountId32` 行为不变。

##### 2.5 `bank_check::CidAccountQuery::is_institution_admin`

trait 按 `actor_cid_number + who` 查询，`()` 默认返回 false。runtime 侧 `MultisigCidAccountQuery` 直接读取该 CID 对应的机构 admins 真源；个人多签不进入清算行机构授权路径。

2026-06-26 补齐：清算行管理员真源不再由业务账户表镜像保存，所有内部投票和清算权限统一读取 `admins` 分类模块。

##### 3.1 Storage

```rust
L2FeeRateBp<Bank, u32>                           // 当前生效费率
L2FeeRateProposed<Bank, (u32, BlockNumber)>      // 待生效提案
MaxL2FeeRateBp: StorageValue<u32>                // 全局上限
LastClearingBatchSeq<Bank, u64>                  // 已成功落账的最新批次序号
```

2026-04-28 补齐:`LastClearingBatchSeq` 与 batch 级签名一起启用。`submit_offchain_batch`
要求 `batch_seq == LastClearingBatchSeq[bank] + 1`,并只在 settlement 成功后推进序号。

##### 3.2 Call(3 个新)

| call_index | 方法 | 费用 | 归类 |
|---|---|---|---|
| 34 | `submit_offchain_batch` | sum(fee) × 0.1% 最低 0.1 元 | 链下资金交易 |
| 40 | `propose_l2_fee_rate(actor_cid_number, institution_account, new_rate)` | 0.1 元/次，由 actor CID 的唯一费用账户支付 | 机构链上操作 |
| 41 | `set_max_l2_fee_rate(new_max)`(Root) | 免费 | 治理执行 |

##### 3.3 on_initialize

每块调用 `fee_config::activate_pending_rates(now)`,把到期费率搬到生效位置,发 `L2FeeRateActivated` 事件。

##### 3.4 权重入口

2026-04-29 补齐:清算行 pallet 已从裸 `T::DbWeight` 和空 `WeightInfo`
迁移为统一权重入口:

- `runtime/src/configs/mod.rs`:生产 runtime 使用
  `offchain_transaction::weights::SubstrateWeight<Runtime>`
- `weights.rs`:为 `bind_clearing_bank` / `deposit` / `withdraw` /
  `switch_bank` / `submit_offchain_batch(items)` / 费率治理 /
  清算行节点声明三类 Call 提供非零保守权重
- `benchmarks.rs`:当前不挂空的 `#[benchmarks]` 模块，避免
  `frame-benchmarking` 在全 runtime `runtime-benchmarks` 构建时生成非法代码。
  由于该 pallet 通过 `CidAccountQuery` 解耦实体生命周期模块,正式自动生成权重
  需要在完整 runtime + benchmarking runtime api 的 WASM 下构造机构、管理员和
  清算行节点 fixture 后，再恢复可执行 benchmark 入口。

当前权重不是自动 benchmark 产物,但已经替换掉空权重占位,并覆盖
`submit_offchain_batch` 的按 item 线性增长。

#### 4. 与 Step 1 的兼容关系

| Step 1 结构 | Step 2a 状态 |
|---|---|
| `OffchainBatchItem`(旧) | 保留,`submit_offchain_batch`(call_index 0)继续走旧 `execute_batch` |
| `RecipientClearingInstitution`(旧绑省储行) | 保留,Step 2d 删 |
| `InstitutionRateBp`(旧省储行费率) | 保留,Step 2d 删 |
| `bind_clearing_institution`(call_index 9) | 保留,Step 2d 删 |
| `bind_clearing_bank` / `deposit` / `withdraw` / `switch_bank`(30~33) | 保留,依然工作 |
| `UserBank` / `DepositBalance` / `BankTotalDeposits` / `L3PaymentNonce` | 保留，被当前清算路径正式使用 |

2026-04-28 补齐：批量结算现在显式要求 `UserBank[payer] == item.payer_bank`
且 `UserBank[recipient] == item.recipient_bank`,防止移动端或节点绕过 UI 构造出绑定漂移的批次。

#### 5. 编译验证

```
$ cargo check -p offchain-transaction
   Checking offchain-transaction v1.0.0
   Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.69s
```

零 warning 零 error。

#### 6. 后续 Step 2b / 2c / 2d 清单

**Step 2b · Node**:
- `offchain/settlement/packer.rs::pack_and_submit` 补实现:取 ledger pending → 组 `OffchainBatchItem` → 多签 → 调 `submit_offchain_batch`
- `offchain/ledger.rs.accept_payment` 完整实现(签名验证 + 本地扣款 + 加入 pending)
- `offchain/gossip.rs` 新建:清算行间 libp2p 协议推送 `{intent, a_sig, sender_ack}`
- `offchain/rpc.rs` 增补 `offchain_submitPayment` + WS 订阅
- `service.rs` / `rpc.rs` 按节点角色启动 + 注册 RPC namespace
- 删除旧 `offchain_ledger.rs` / `offchain_packer.rs` / `offchain_gossip.rs`(Step 2b 完成时)

**Step 2c · citizenapp**:
- 重写 `trade/offchain/offchain_pay_page.dart`:走清算行节点 `offchain_submitPayment`,每笔 L3 签名
- 改造 `wallet/ui/receive_qr_page.dart`:`body.bank` 填清算行主账户 SS58
- 冷钱包 QR 签名接入绑定/充值/提现(沿用旧 `bind_clearing_page.dart` 模式)
- 删除 `wallet/ui/bind_clearing_page.dart` + `trade/offchain/clearing_banks.dart` + `rpc/offchain.dart` + `rpc/onchain.dart.bindClearingInstitution`

**Step 2d · 清理**:
- 删除 pallet 内旧 `submit_offchain_batch` / `enqueue_offchain_batch` / `process_queued_batch`(走省储行分账的老路径)
- 删除 `RecipientClearingInstitution` / `InstitutionRateBp` / `bind_clearing_institution` / `propose_institution_rate` / `vote_institution_rate`
- 清理 Event / Error 中的旧变体
- 删除现有 `pallet::execute_batch` 与 `validate_batch_items` 等辅助函数
- 运行完整单元/集成测试

**Step 2b `set_max_l2_fee_rate` 联合投票**:
把本步的 `ensure_root(origin)` 入口改为接 `votingengine` 联合投票 pallet 的 `JointVoteEngine::execute_if_passed`,让提案通过后再由投票引擎回调本 pallet 的内部执行函数。

#### 7. 风险与验证后续

- **BlockLength 5 MB → 16 MB 升级**:Step 2b 做 runtime setCode 前必须升
- **批量 sr25519 验签**:当前 settlement 逐笔 `sr25519_verify`,10 万笔需要 ~5s,Step 2b 前切到 `sp_io::crypto::sr25519_batch_verify`
- **解码 H256 → T::Hash**:本步用 `T::Hash::decode` 跨类型兼容,依赖 runtime 中 `T::Hash == H256`(frame_system 默认)。若将来改 hasher 要同步改

#### 8. 变更记录

- 2026-04-19：Step 2a 落地，Runtime 新增 3 子模块 + 当前批量结构 + 3 Storage + 3 Call + hook 扩展，零编译错误。
- 2026-04-28:批次级安全补齐:新增 `LastClearingBatchSeq`,严格校验
  `batch_signature` / `batch_seq`,settlement 增加 `UserBank` 绑定一致性校验;
  runtime `spec_version` 3 → 4,`transaction_version` 保持 2;单测增至 23 个并通过。
- 2026-04-29:权重收口:新增 `SubstrateWeight<Runtime>` 生产配置,所有清算行
  Call 改走 `T::WeightInfo`,不再使用空 `WeightInfo` 占位;`cargo test -p
  offchain-transaction --lib` 23 个测试通过。

#### 1. 模块定位

代码目录：`citizenchain/runtime/transaction/onchain/`。

本模块提供三项能力：

- `OnchainChargeAdapter`：实现 `pallet-transaction-payment::OnChargeTransaction`，消费 runtime 给出的唯一费用路由并执行链上交易费、投票费扣款。
- `OnchainExecutionFeeCharger`：供投票通过后的业务回调从已经核验的确切账户收取链上资金执行费；计算、ED、事件和分账与外层交易完全一致。
- `OnchainFeeRouter`：将已扣手续费按 80% / 10% / 10% 分给当前块作者绑定的奖励接收账户、国家储委会费用账户和安全基金账户。

本模块不维护费用类别表、不维护机构身份或管理员表，也不使用 weight、length 或动态 multiplier 计算制度费用。

#### 2. 唯一费用协议

费用类型、付款账户、常量和链上费公式的唯一协议源位于：

- `citizenchain/runtime/primitives/src/fee_policy.rs`

统一类型为 `FeeRoute<AccountId, Balance>`：

- `Free`：系统、Root 回调或内部维护调用，外层不扣交易费。
- `Onchain { transaction_amount, payer }`：按 `max(round(amount × 0.1%), 0.1 元)` 收取；机构普通操作传零金额，所以固定为 0.1 元。
- `Offchain { fee_amount, payer: OffchainFeePayer::BatchItemPayers }`：链下清算批次可有多个付款公民；每个 item 的 `payer` 从其 L2 存款支付对应 `fee_amount`，外层适配器不重复扣款。收款方机构费用账户是手续费收款账户，不是付款账户。
- `Vote { payer }`：只有实际投票行为，由投票签名者固定支付 1 元。
- `Reject`：调用未分类、未开放、CID/账户不匹配、签名者不是该 CID 的 `admins`，或费用账户无法唯一解析时拒绝入池和执行。

`FeeRoute` 的收费分支都强制携带确切付款账户，不存在 `Option<payer>`、默认付款人或回落到签名者的表达空间。

`runtime/src/configs.rs::RuntimeFeeRouter` 是 `RuntimeCall -> FeeRoute` 的唯一映射。`CallFeeRoute` 只是依赖注入接口，不定义第二套费用类型。

#### 3. 机构费用路由

机构身份主键为 `actor_cid_number`，外层 `origin` 是管理员钱包签名；机构账户没有私钥。

机构路由按以下顺序严格校验：

1. 从 `actor_cid_number` 解析机构码。
2. 通过 `RuntimeInstitutionAdminQuery` 校验签名者属于该 CID 的 `admins`。
3. 从 `InstitutionAccounts[(cid_number, InstitutionFee)]` 读取费用账户。
4. 校验 `AccountRegisteredCid[fee_account]` 的 CID 和账户名反向索引完全一致。
5. 若交易携带 `institution_account` / `funding_account`，再校验该账户与同一 CID 的正反索引完全一致。

任何一步失败都返回 `FeeRoute::Reject`。公权/私权存储同时出现同一 CID、费用账户缺失、正反索引不一致、跨 CID 使用账户或非管理员签名都不允许改扣管理员钱包。

机构发起提案、机构资料操作和机构账户操作属于链上操作，由该 CID 的费用账户支付最低 0.1 元。只有后续管理员执行 `cast_*` 等实际投票时，才由投票签名者支付 1 元。

Fullnode 不是机构。`bind_reward_account` / `rebind_reward_account` 由全节点自己的 `powr`
账户签名，并由该签名者支付 0.1 元。

#### 4. tip 与框架费用

- `primitives::fee_policy::TRANSACTION_TIP = 0` 是唯一协议值。
- Rust 统一签名构造器和 CitizenApp 都只编码 `tip=0`。
- CitizenWallet 在签名前解析 `Compact<tip>`，非零立即拒签。
- runtime 的 `can_withdraw_fee` 和 `withdraw_fee` 对非零 tip 返回 `InvalidTransaction::Payment`。
- `WeightToFee = 0`、`LengthToFee = 0`、固定 multiplier，不产生第六类框架费用。

tip 不属于交易费，不参与 `FeePaid`、RPC 聚合或 80/10/10 分账。

#### 5. 扣款语义

`OnchainChargeAdapter` 只消费 `FeeRoute`：

- `Onchain`：调用 `primitives::fee_policy::calculate_onchain_fee`，从路由中的 `payer` 扣款。
- `Vote`：从路由中的 `payer` 扣固定 `VOTE_FLAT_FEE`。
- `Offchain` / `Free`：外层不扣款。
- `Reject`：返回 `InvalidTransaction::Call`。

余额扣款使用 `Precision::Exact + Preservation::Preserve + Fortitude::Polite`：必须完整扣除，并保证普通支出后不低于 ED；否则整笔交易失败。适配器不尝试第二付款账户，不做执行后退款。

投票回调没有新的外层 extrinsic。业务模块只能把已核验的 `payer + transaction_amount`
交给 `OnchainExecutionFeeCharger`；它仍使用 `calculate_onchain_fee`，以 `KeepAlive`
完整扣款并进入同一分账。机构转账、机构安全基金转账、机构账户关闭和决议销毁的
执行手续费只允许由 actor CID 的费用账户支付；个人多签由个人账户支付。手续费和
本金变化处于同一 storage transaction，任一失败全部回滚。

成功扣款发出：

```text
FeePaid { who: 实际付款账户, fee: 完整手续费 }
```

其中 `fee` 已是完整链上交易费或投票费；协议不存在额外 tip。

#### 6. 分账路由

分账常量同样来自 `primitives::fee_policy`：

- 全节点奖励接收账户：80%。
- 国家储委会费用账户：10%。
- 安全基金账户：10%。

块作者缺失、奖励接收账户未绑定或任一制度账户无法安全入账时，对应 credit 被销毁并发出 `FeeShareBurnt { reason, amount }`，绝不转给未知账户。原因包括：

- `AuthorMissing`
- `WalletUnbound`
- `FullnodeResolveFailed`
- `NrcMissing`
- `NrcResolveFailed`
- `SafetyFundResolveFailed`

#### 7. 外部同步

- `citizenchain/crates/signing/`：统一构造 `tip=0` 的交易扩展。
- `citizenchain/node/src/core/rpc.rs`：`fee_blockFees` 只累计 `FeePaid.fee`，不再拼接 FRAME tip 事件。
- `citizenapp/lib/rpc/signed_extrinsic_builder.dart`：热签 payload 和 extrinsic 固定 `tip=0`。
- `citizenwallet/lib/signer/payload_decoder.dart`：冷签前拒绝非零 tip。
- `citizenchain/onchina/`：机构身份仍只使用 CID、机构账户和 `admins`；不建立费用付款方缓存或第二路由表。

#### 8. 测试要求

必须覆盖：

- 五种 `FeeRoute` 行为。
- 链上费四舍五入、最低 0.1 元和极大金额。
- 实际投票由签名者扣 1 元。
- 机构操作只扣精确费用账户，管理员余额不变。
- 费用账户缺失或映射不一致时直接拒绝，不回落管理员。
- 投票回调执行期使用确切付款账户，手续费不足时本金、分账和事件全部回滚。
- Fullnode 绑定由签名者扣 0.1 元。
- 非零 tip 在 runtime 和 CitizenWallet 两端拒绝。
- 80/10/10 正常分账与所有安全销毁路径。
- `WeightToFee` / `LengthToFee` 不产生费用。

主要命令：

- `cargo test -p primitives -p onchain -p citizenchain`
- `cargo test -p chain-signing -p node -p onchina`
- `flutter test` 与 `flutter analyze`（CitizenApp、CitizenWallet）

### Transaction 目录说明

本目录用于承载 CitizenChain runtime 下的交易相关 pallet 与文档。
当前交易相关 crate 已统一放在本目录下，后续新增交易 pallet 也必须直接落在这里。

补充：

- `institution-asset` 虽然不是 pallet，但属于交易资金边界的公共 crate，也统一放在本目录。

#### 定位

`votingengine` 是链上中国 runtime 的统一投票引擎。

全部投票流程只归投票引擎。业务模块只提交提案语义并调用既定投票接口，不得实现、复刻、绕过或内嵌投票流程，不得自行处理人口快照、投票资格、联合签名、状态推进、计票、通过判定或清理状态机。

ADR-039 已于 2026-07-19 冻结机构岗位主体目标。任务卡第 5A、5B、5C 已依次完成联合、内部、立法和选举投票迁移；全部机构 Track 都按 `VotePlan` 中的完整岗位主体冻结资格，不再以 CID 全体 admins 作为发起或投票资格。个人多签仍使用独立管理员主体。

#### 内部投票与业务权限

- `internal-vote` 是机构岗位主体与个人多签共用的投票程序；两类授权主体必须用 `AuthorizationSubject` 强类型分离。
- 共享 `VotePlan` 的 SCALE 字段顺序固定为 `business_action_id`、`proposal_owner`、`proposer_subject`、`voter_subjects`、`voting_engine`、`business_object_hash`；最多绑定 256 个投票主体。
- `VotingEngineKind` discriminant 固定为 `Internal = 0`、`Joint = 1`、`Election = 2`、`Legislation = 3`。构造器强制 owner 与 module tag 相同、投票主体不重复，并禁止机构岗位与个人多签主体混用；只有 Popular 选举允许 voter subjects 为空，因为其选民资格来自人口快照，其他引擎一律要求非空岗位/个人主体。
- 机构内部投票和联合投票创建时都必须携带完整 `VotePlan`；核心引擎一次性写入 `ProposalVotePlans`，要求 `ProposalOwner` 与 plan 相同，拒绝重复绑定。个人多签使用独立 personal 创建接口和 `AdminSnapshot`，不得伪造机构岗位主体。
- 目标机构提案由业务模块先校验 `RoleSubject(cid_number, role_code)` 的 `Propose` 权限，并静态选择唯一投票引擎、绑定 `VotePlan`。引擎不得接受调用方选择引擎，也不得把“属于 admins”当业务准入。
- 机构投票资格按 VotePlan 中一个或多个 voter `RoleSubject` 的有效任职账户建立不可变快照；不得自动快照该 CID 的全体 admins。个人多签仍按 personal_account 的管理员集合快照。
- `VoterSnapshot[(proposal_id, RoleSubject)]` 保存每个岗位主体的有效任职账户；`InstitutionTicketCountSnapshot[(proposal_id, cid_number)]` 冻结该机构岗位席位票据总数。同一账户兼任多岗时按 `RoleSubject + account_id` 分别行使各岗位票权，不按账户合并，也不改变机构阈值。
- 投票引擎只负责快照、资格、票据、阈值、计票、终态、重试和清理；业务合法性、业务对象绑定及通过后的具体执行归对应业务 pallet。
- `multisig` 转账允许已登记机构账户和个人多签账户；机构调用显式携带 `actor_cid_number + proposer_role_code + institution_account`，反向索引只校验账户归属，业务模块再校验完整岗位主体权限。个人多签必须同时携带 `actor_cid_number=None`、`proposer_role_code=None`。
- `resolution-destroy` 只允许 NRC、PRC、PRB 对应固定岗位；`grandpakey-change` 只允许 NRC、PRC 委员岗位。业务限制和业务动作权限不得下沉到 `internal-vote`。
- FRG 是一个 CID 机构并拥有多个协议账户和 215 名管理员；省域 5 人岗位组属于注册业务权限，目标投票和发起资格必须解析对应省专员 `RoleSubject`，不能把 FRG 全体 admins 纳入。
- 协议升级与决议发行都固定使用联合投票：NRC/43 个 PRC 委员岗位可发起和投票，43 个 PRB `DIRECTOR / 董事` 岗位只投票；岗位只提供资格主体，不能选择引擎或自带岗位阈值。
- 联合投票引擎还会校验 proposer 是 plan 中 NRC/PRC 委员主体的有效任职账户，并要求 plan 精确覆盖 44 个 CHINA_CB 委员主体和 43 个 CHINA_CH 董事主体；缺失、多出、重复或跨类型主体均 fail-closed。

##### 机构阈值与提案快照

- 岗位没有阈值 storage，也不得把岗位数或 admins 钱包数误作阈值；`VotePlan` 只决定哪些岗位任职席位进入快照。
- public/private entity 的 `InstitutionGovernanceThresholds[cid_number]` 是机构治理阈值真源。`internal-vote` 在建案事务中通过 runtime provider 读取并写入 `InternalThresholdSnapshot[proposal_id]`；建案后机构配置变化不得改变既有提案阈值。
- 投票引擎不再保存机构 Active/Pending 阈值表，也不通过管理员更换隐式改阈值。个人多签继续使用 `ActivePersonalThresholds[personal_account]`，不受机构阈值重构影响。
- 固定创世机构的阈值由 genesis 写入 entity，不是岗位阈值。公民链基金会只有一个程伟管理员钱包，但三个固定岗位各一席、机构阈值仍为 2；同一钱包的三项岗位任职已形成三张独立岗位票据，并通过内部投票阈值验收。

#### 公民身份真源

投票资格和参选资格统一通过 `CitizenIdentityReader` 读取 `citizen-identity`：

- `voting_subject(who, scope)`：账户在当前作用域内有投票资格时返回完整 `CitizenSubject`。
- `candidate_subject(who, scope)`：账户在当前作用域内有参选资格时返回完整 `CitizenSubject`。
- `population_data(scope)`：人口日期完整就绪时返回作用域、人口分母、资格 revision 和
  护照判定日期；日期尚未推进完成或身份模块处于人口维护故障时返回 `None`。该数据只能
  由 `citizen-identity` 产生，裸 `population_count()` 接口已经删除。
- `voting_subject_at(who, population_data)`：按人口数据中的 revision 和日期查询永久 CID 身份历史，并返回 CID + 当前签名钱包的完整主体。

`citizen-identity` 是人口数据和身份历史唯一真源，但不生成、不编号、不保存提案快照。
`votingengine::create_population_snapshot(proposal_id, scope)` 遇到 `None` 返回
`PopulationDataNotReady`，不写入任何快照；取得完整数据后才写入
`ProposalPopulationSnapshots[proposal_id]`。投票、分母读取和 90 天清理都只围绕该
proposal_id 快照进行。

OnChina 本地数据库只能用于注册局录入和界面提示，不能作为链上投票资格真源。

2026-07-30 资格读取统一返回 `CitizenSubject { cid_number, account_id }`。联合公投、立法公投和 Popular 选举均以 `(proposal_id, cid_number)` 唯一去重，票据值保存完整主体；同一永久 CID 更换绑定账户后不能再投一票。候选快照和当选结果保存完整主体，候选计票表以候选 CID 为唯一键；当前绑定账户只用于签名授权与审计。人口快照仍只保存作用域、有效总数、资格 revision、判定日期和创建区块，不枚举公民名单。

#### 人口作用域

`PopulationScope` 支持四级：

- `Country`
- `Province(province_code)`
- `City(province_code, city_code)`
- `Town(province_code, city_code, town_code)`

联合公投和立法特别案在创建提案的同一存储事务内调用
`votingengine::create_population_snapshot(proposal_id, scope)`；引擎从 `citizen-identity`
读取人口数据并在自身 storage 形成不可变提案快照。联合公投固定使用
全国作用域；立法特别案只从 `actor_cid_number` 的合法 CID 和法定机构码推导国家、
省或市作用域。任一后续写入失败时快照与提案一起回滚，不存在公开准备入口、待消费
快照、客户端作用域参数、独立 snapshot_id 或第二人口真源。

#### 联合投票

- 内部阶段：`JointVote::cast_admin(proposal_id, institution, approve)`。
- 联合公投阶段：`JointVote::cast_referendum(proposal_id, approve)`。
- 联合公投按 `proposal_id + cid_number` 去重，`CitizenReferendumTicket` 保存完整公民主体和票值。
- 联合公投资格由 `CitizenIdentityReader::voting_subject_at(who, population_data)` 返回；提案创建后新增、迁居或被撤销的当前身份均不能改变已有提案的成员集合。
- 公投分母与成员资格来自同一个投票引擎提案快照，快照数据只由 `citizen-identity` 提供；累计票数达到该分母后拒绝继续写票，参与率不得超过 100%。
- 联合业务回调必须同时绑定 `ProposalOwner`、联合 proposal kind、`STAGE_JOINT/STAGE_REFERENDUM`、业务摘要和对象摘要；联合阶段直接通过与转入公投后通过都必须执行同一项已绑定业务。

#### 判定与业务执行

- 投票门槛一旦命中，只提交 `STATUS_PASSED`、释放相应活跃名额并写入 `PendingProposalExecutions`；最后一票不再同步执行转账、销毁或 `set_code` 等业务。
- `on_initialize` 按 `MaxExecutionWeightPerBlock` 与 `MaxAutoFinalizePerBlock` 双重上限消费执行队列，每项按包含 `SystemWeightInfo::set_code()` 的最重成本预留。
- 业务回调返回 `DispatchError` 时只回滚本次执行尝试，不撤销已成立的投票结果；失败按指数退避重试，达到上限转 `STATUS_EXECUTION_FAILED` 并发出 dead-letter 事件。
- 回调 `Err`、`Ignored`、结果应用错误和 Track 后处理错误统一进入同一失败处理器；每次递增 attempts，孤儿或状态不匹配的队列项立即删除。
- 达到自动执行上限后，业务执行队列永久停止；`PendingTerminalFinalizations` 只补终态副作用并拥有独立退避/dead-letter，绝不重新调用业务执行回调。
- `ProposalExecutionOutcome::RetryableFailed` 继续进入手动重试宽限期；机构提案只有创建时任一 `VoterSnapshot` 中的有效岗位选民可重试/取消，个人多签才读取 `AdminSnapshot`。执行成功或失败终态仍统一触发互斥释放、业务终态通知和 90 天延迟清理。
- 自动超时 finalizer 自身返回错误时使用独立有限退避状态；达到上限或重试桶已满后写入 `AutoFinalizeDeadLetters`，不会反复阻塞同一区块的其余维护管线，公开 `finalize_proposal` 仍可在修复数据后人工恢复。

#### 立法投票

- 人口快照：仅特别案在创建事务内按 `actor_cid_number` 内联创建并绑定；普通案和
  重大案不创建。
- 代表机构表决：`cast_representative_vote(proposal_id, approve)`。
- 特别案公投：`cast_referendum_vote(proposal_id, approve)`。
- `legislation-yuan` 的新法、修法、废法动作分别固定为 `leg-yuan/0,1,2`；发起调用必须显式携带 `proposer_role_code`，业务模块按 `actor CID + 岗位码 + action + Propose` 校验后固定选择立法投票引擎。
- 当前创世只强制创建各机构唯一 LR；NRP/NSN/NED/PRP/PSN/CLEG/CEDU/CSLF 的成员岗位不在创世预造，必须由机构以后依法创建。建案时业务模块从链上岗位权限中解析准确代表岗位并写入 VotePlan。
- 行政签署和国家/省级三人会签固定使用相关机构 LR，护宪终审固定使用 NJD `CONSTITUTION_GUARD`；全部按建案时岗位任职快照判定，不按机构全体管理员快照判定。护宪岗位不是 LR，且必须在建案时恰好冻结 7 个不重复账户。
- `legislation-yuan` 在创建提案和投票通过写入前分别复核一次法定路由：固定院序、发起机构、行政签署机构、会签机构、active 账户和 CID 行政区必须全部一致。客户端携带的路由字段不是授权真源。

##### 固定框架

```text
legislation-vote/
├── representative/   # 单机构、顺序多机构和逐机构计票
├── legislation/      # 法律专属公投、签署、会签和护宪终审
├── types.rs          # 路线、数学规则、后续程序强类型
├── rules.rs          # 三类数学门槛唯一实现
├── result.rs         # ProposalOwner 业务结果路由边界
└── cleanup.rs        # 代表票据和法律票据清理边界
```

- `RepresentativeRoute::Single/Sequential` 只表达一个或多个代表机构的推进顺序，不把教育委员会误称为立法院一院。
- `RepresentativeVoteRule::Regular/Major/Special` 是投票引擎唯一数学规则；教育等业务分类不进入引擎规则枚举。
- `VoteProcedure::RepresentativeOnly` 表决完成即把结果交给任免、预算等业务；`Legislation` 才继续执行法律专属程序。
- `RepresentativeMetas` 与 `LegislationMetas` 分离；非法律业务不得写法律元数据。
- `RepresentativeTallies[proposal_id][body_index]` 独立保存每个机构计票。
- `RepresentativeVotesByTicket[proposal_id][(body_index, InstitutionVoteTicket)]` 按完整机构岗位席位去重，同一钱包可在不同代表岗位分别投票。
- 终局回调由各业务模块依据 `ProposalOwner/MODULE_TAG` 认领；投票引擎不解析法律、任免职书或预算正文。

#### 选举投票

- `election-vote` 统一承载普选、互选的提案、选民/候选快照、投票、计票、结果快照和清理。
- `term_start`、`term_end` 使用自纪元起 `u32` 天，不使用区块高度表达法定任期。
- `election-vote` 只产生不可变当选结果快照，不解释职位、席位、任期或目标机构业务规则，也不得构造 `InstitutionGovernanceResult` 直写 entity。
- 普选/互选底层创建 extrinsic 已物理删除；当前外部只保留 `cast_popular_vote` 与 `cast_mutual_vote`。
- 真实创建必须由 `runtime/public/` 下对应的具体选举业务模块校验本机构发起岗位、目标 `role_code`、候选人、选民范围、席位和任期后调用引擎；结果也必须先回到原具体业务模块复核，再由业务模块调用 entity 任职入口。
- `runtime/public/citizen-election` 为公民选举公职人员的业务模块，复用 pallet index 32；当前仅占位，不提供交易入口、业务存储或事件。后续逐步实现具体选举业务规则并接入 `election-vote`，本次不恢复已删除的开发期通用选举实现。
- 机构只能发起本机构岗位选举。最终元数据只保留 `actor_cid_number + role_code`，发起岗位、互选岗位和被选举岗位的 CID 必须相同。
- 提案实例只使用投票引擎生成的全链唯一 `proposal_id`，业务类型由 `BusinessActionId` 表达；不得保留无权威规则表支撑的通用规则编号。

##### 资格真源与快照边界

- 普选必须使用 `citizen-identity` 的 `PopulationScope`、完整公民主体资格、`population_data` 和快照时资格查询；投票引擎按 proposal_id 保存不可变人口数据，不接收、不枚举、不保存全国/省/市/镇完整选民列表。
- 互选属于机构岗位业务；目标选民必须来自业务模块 VotePlan 指定的 voter `RoleSubject` 有效任职快照。调用方不得提交或删减选民集合。
- `election-vote` 创建入口按 `ElectionMode` 强制检查资格来源：Popular 必须有人口作用域，Mutual 必须取得已绑定岗位主体快照。
- 普选人口作用域写入 `ElectionMeta`，完整人口数据写入核心 `ProposalPopulationSnapshots`；互选不写公民作用域，按 VotePlan 中属于唯一 `actor_cid_number` 的一个或多个 `RoleSubject` 写入核心 `VoterSnapshot`，并以 `MutualElectionVotesByTicket` 保存完整岗位票据。`MutualVoters`、调用方选民参数和 `MaxMutualVoters` 已删除。
- 多席位计票允许完整落入剩余席位的并列组共同当选；并列组跨越席位边界时拒绝结果。
- 候选快照和当选结果保存 `CitizenSubject`，Popular 票据保存当次完整投票主体，候选计票表
  以候选 `cid_number` 为唯一键。候选人必须由 `candidate_subject` 校验为竞选身份；
  当前钱包账户只证明签名授权，换绑不得形成第二候选人或第二张公民票。

#### 清理

提案否决、超时、执行成功或执行失败终态都统一进入投票引擎维护管线。引擎先调用
所属业务 Track 的终态回调清除业务 pending 锁，再按保留期调度清理投票记录、人口
快照、提案对象和反向索引；业务 pallet 不暴露人工拒绝清理交易。

- `ScheduledCleanups + ScheduledCleanupHead/Tail` 是 90 天保留期的延迟 FIFO；固定保留期保证写入顺序就是到期顺序，不再使用有界区块桶或向后扫描候选桶。
- 到期任务转入 `PendingCleanupQueue + PendingCleanupQueueHead/Tail` 就绪 FIFO；每个提案每轮只执行一个有界步骤，未完成任务排回队尾。
- 清理阶段固定为 `AdminSnapshots → VoterSnapshots → InstitutionTicketCounts → TrackData → ProposalObject → FinalCleanup`；`TrackData` 只派发到提案所属 Track，不再空扫四类 sub-pallet。`FinalCleanup` 同步删除 `ProposalVotePlans`。
- 激活数、清理步骤数和 `MaxCleanupWeightPerBlock` 同时限流；单个大型公投不能阻塞后续提案，也不能挤占自动终结或业务执行的独立预算。

#### 生产代码职责边界

- `votingengine/src/lib.rs` 只保留 pallet 配置、存储、事件、错误、hooks 和 lifecycle extrinsic。
- `expiry.rs` 承载到期索引、自动终结、有限退避和 dead-letter。
- `execution.rs` 承载异步业务执行、重试期限与管理员恢复入口。
- `lifecycle.rs` 承载状态迁移、终态副作用、回调作用域和统一事件。
- `maintenance.rs` 承载有界票据清理和提案对象清理步骤。
- `tracks.rs` 定义单 Track 生命周期接口和递归 tuple 派发；核心不匹配具体 mode/stage。
- `traits.rs` 只作为稳定 re-export 门面；实际 trait 按 engines、providers、callbacks、finalizers、cleanup 分组。
- `internal-vote` 的 proposal、threshold、vote、cleanup 各自独立；`legislation-vote` 的代表表决、公投、签署、护宪、结果和清理实现均位于对应真实模块。
- 生产源码单文件不得超过 800 行，不得用纯注释文件或空实现伪造职责边界。

#### 2026-07-14 第一步安全修复

- 立法签署、三人会签和护宪终审的公开超时入口统一要求当前区块严格大于提案截止区块，防止任意账户提前终结提案。
- 自动到期结算失败不再回插原到期桶；第三步已补充独立计数、指数退避和 dead-letter，确定性错误不会永久阻塞执行重试和历史清理管线。
- 立法特别案公投统一调用 `primitives::constitution::referendum_passed`，不再保留重复数学实现。
- 注册多签待激活动态阈值按 `proposal_id` 隔离，避免同机构并发注册提案互相覆盖。

#### 2026-07-14 第三步安全收口

- `on_initialize` 在到期桶尚未排空时不再提前返回，执行重试、终态清理和 90 天清理管线每块都能继续获得有界处理机会。
- 投票判定与业务执行已通过 `PendingProposalExecutions` 解耦；队列按 weight 预算执行，错误指数退避并在三次失败后 dead-letter，既有手动重试和终态清理契约已完成适配。
- `finalize_proposal` 只承担投票判定与执行入队，不再叠加 `set_code`；`set_code` 最重成本只归入 `process_pending_execution` 异步执行预算。五个投票 crate 的正式 benchmark 已生成并写入生产权重。
- `joint-vote` 本 crate 现有 12 项直属测试，除原有 `cast_admin`、`cast_referendum`、105 票全票、机构否决和超时转公投外，直接覆盖 `VotePlan`/岗位快照绑定、同 CID 去重和跨 CID 独立投票；`internal-vote` 继续提供跨 pallet 回归覆盖。
- `legislation-vote` 的 signing、guard、referendum、result、cleanup 文件已承载实际规则或清理辅助，不再是纯注释残桩。
- 该步骤当时曾把五个投票 pallet 及触达模块设为开发期 `StorageVersion = 1`；2026-07-21 最终创世决策已取代该版本口径，当前五个投票 pallet 及全部项目 pallet storage version 已统一为 `0`，不保留开发期 migration。

#### 2026-07-14 Track 与维护调度收口

- Runtime 以 `(InternalVote, (JointVote, (LegislationVote, (ElectionVote, ()))))` 注册递归 Track tuple；手动超时、自动超时、模式清理和内部阈值副作用走同一类型路由。
- 自动终结、异步业务执行、历史清理分别使用 `MaxAutoFinalizeWeightPerBlock`、`MaxExecutionWeightPerBlock`、`MaxCleanupWeightPerBlock` 独立预算；生产配置当前分别为最大区块权重的 `1/4`、`1/4`、`1/8`。
- 延迟 FIFO 与就绪 FIFO 均以单调 `u64` 序号键控，不存在单区块桶容量、顺延窗口或严格头部反复处理。
- 新增公平轮转测试和 Track 隔离清理测试：大任务存在时小任务仍推进，内部提案清理不会删除联合投票账本。

#### 2026-07-14 资格快照与执行重试加固

- `citizen-identity` 以全局 `eligibility_revision` 和每账户不可变版本历史冻结创建时资格；同一区块多次身份写入也能确定顺序，账户查询按版本数二分定位。
- 联合公投、立法公投和 Popular 选举统一写入 `ProposalPopulationSnapshots[proposal_id]`；`citizen-identity::PopulationSnapshots/NextSnapshotId`、核心 `ProposalPopulationSnapshotIds`、`Proposal.citizen_eligible_total`、`ReferendumScopes`、`LegislationMeta.referendum_scope`、Popular 全量选民表及 `MaxElectionVoters` 已删除。
- Popular 不受完整选区人数的 `BoundedVec` 限制；Mutual 只使用核心岗位快照，`MutualVoters` 和 `MaxMutualVoters` 已删除。
- `joint-vote` crate 直属测试当前为 12 项，直接覆盖 `cast_admin`、`cast_referendum`、105 票全票执行、机构否决转公投、超时转公投、创建后新增选民拒绝、岗位快照绑定和有效选民去重。
- 自动执行结果应用阶段的确定性错误不再每块无限重排；R2 回归用例覆盖 `Ignored` 三次退避后 dead-letter，以及孤儿执行队列立即删除。

#### 2026-07-14 生产 Benchmark 与动态权重

- 目标环境：Apple M5 Pro / arm64、Rust 1.94.0、FRAME Benchmark CLI 53.0.0；WASM compiled，`steps=50`，`repeat=20`。
- Runtime registry 共注册 19 条：核心 4、内部 2、联合 5、立法 6、选举 2。
- 核心权重为 35/24/10/22 ms，其中公开终结 35 ms 是保守调度包络并另叠加实际 Track 权重；`process_pending_execution` 另显式叠加 `SystemWeightInfo::set_code()`，runtime 升级不再被普通回调静态权重掩盖。
- 内部权重沿用既有实测；联合和立法已按资格历史存储重新生成，公民公投写票均真实计入 snapshot 绑定、快照元数据和账户资格版本读取。
- 2026-07-30 使用当前源码 WASM、50 steps、20 repeats 重算选举权重。最后一票按候选人数
  `c` 线性计费：Popular 基础约 `54.4m ps`、Mutual 基础约 `51.1m ps`，两者每候选人
  约增加 `1.94m ps`；Popular 读为 `12+c`、Mutual 读为 `11+c`，写均为 10。
- joint 人口准备和立法签署类 benchmark 无法在通用基准创世态完整构造生产 provider 权限，调用注解使用“实测主体与生产 provider 保守上界取 max”，重生生成文件不会移除安全上界。
- `ProposalTrackHandler` 同时返回 stage timeout、Track chunk cleanup、Track terminal cleanup 权重；手动终结、自动终结和清理维护均按具体 Track 实际值计账。
- 三条维护预算维持最大区块权重的 `1/4`、`1/4`、`1/8`，合计 62.5%；在 60 秒最大计算区块下，每条管线均可容纳至少一个最重任务。

##### 覆盖率口径

- 原生 LLVM coverage 排除测试、benchmark、weights 和纯声明 `traits/types/data` 后，可执行业务代码共 4,324 行，命中 3,537 行，行覆盖率 81.80%。
- 若把纯接口与类型声明也计入，五个投票 crate 全源码为 71.60%。文档同时保留两项，80% 门禁只使用可执行业务代码口径。
- election-vote 现有测试文件内建立完整 mock runtime，覆盖普选/互选创建、人口/岗位快照、人口未就绪原子回滚、资格拒绝、写票、超时、结果回调与分块清理；当前为 14 项。

#### 2026-07-19 第 5B 岗位主体内部投票收口

- `InternalVoteEngine` 的机构创建接口强制接收业务模块构造的 `VotePlan`；创建事务按 plan 中每个 `RoleSubject` 读取有效任职、写入 `VoterSnapshot`，并按 CID 累加 `InstitutionTicketCountSnapshot`。机构路径不再写 `AdminSnapshot`。
- `internal-vote::cast` 的机构调用必须显式声明 `voter_role_code`，以 `InternalVoteTicket::Institution(RoleSubject + account)` 校验和防双投；个人多签使用 `InternalVoteTicket::Personal(account)` 并读取 `AdminSnapshot`。两类主体没有兼容回落。
- 核心投票引擎不再暴露把机构人员名册写入个人多签快照的入口。机构 `admins` 查询只允许业务模块和 entity 用于确认人员名册归属，不能生成投票资格。
- votingengine Config、runtime 接线和测试 runtime 不再存在机构管理员人数 provider。机构阈值只由 entity 提供，岗位席位数只来自提案冻结的票据快照；不得恢复通过管理员人数推导机构计票的第二路径。
- 机构阈值继续来自机构固定阈值或机构动态阈值；岗位只决定选民集合，不新增岗位阈值。创建时若机构阈值无法由本次有效岗位快照达到，整笔提案回滚。
- 手动重试和取消同样按提案主体分流：机构读取有效岗位选民快照，个人多签读取个人管理员快照。
- 已接入业务为 public/private 本机构治理与关闭、决议销毁、GRANDPA 密钥紧急恢复、机构普通转账、NRC 安全基金转账、费用账户划转主账户和公民链基金会平台调价。每个业务自己校验 `RoleSubject + BusinessActionId + Propose`、枚举拥有 `Vote` 权限的岗位并固定使用内部投票引擎。GRANDPA 正常更换由目标机构单个委员完成旧、新私钥双签后直接延迟调度，不进入投票引擎。
- 正式 FRAME benchmark 使用当前源码导出的临时 `citizenchain-fresh` spec、50 steps / 20 repeats。`resolution-destroy` 为 25 reads / 23 writes，`grandpakey-change` 为 25/23，`multisig::propose_transfer` 为 31/23；`internal-vote` 与核心 `votingengine` 已按机构有效岗位快照路径重算。public/private 完整凭证治理与 square 调价尚无可执行全调用夹具，生产权重使用 400 ms、700 KB proof、35 reads / 30 writes 的显式保守上界。
- 旧权重benchmark脚本及模板已删除；既有 `weights.rs` 数值和算法未因目录整理变更。后续重新生成权重须单独确定并验收所属Runtime入口。

#### 验收

- `cargo test -p votingengine`
- `cargo test -p joint-vote`
- `cargo test -p legislation-vote`
- `cargo test -p internal-vote`
- `cargo test -p election-vote`
- `cargo test -p citizenchain`
- `cargo check -p citizenchain --features runtime-benchmarks`
- `cargo check -p citizenchain --features try-runtime`

2026-07-14 执行重试与资格快照最终验收：`citizen-identity` 23、`internal-vote` 96、`joint-vote` 10、`legislation-vote` 33、`election-vote` 13、runtime 40 项专项测试及 `cargo test --workspace` 全部通过；六个相关 crate 的 `no_std`、runtime benchmark/try-runtime 编译和最终 release WASM 构建通过。当前源码以 `citizenchain-fresh --tmp` 真实启动成功，genesis hash 为 `0xd81962210c603a4a0f078b2cc022bac3daab344cd7dce8c6fc3501973d1552ab`，`isSyncing=false`，metadata RPC 响应 418,806 字节，runtime `specVersion/systemVersion/stateVersion` 均为 1；验收节点已停止。

2026-07-14 第 3 步最终运行态：`WASM_BUILD_FROM_SOURCE=1` release 构建通过；当前源码 WASM 以全新 base path 启动隔离 `citizenchain-fresh` 节点，NodeGuard 与创世装载通过。block#0/genesis hash 为 `0x8d3fc4c4567796d8056e61a8dbf431f04230126a1023a49ffecde7b5bff25390`，state root 为 `0x51ef488b720c9f049c501367f31e3779dd7a3711c295ce8cc79ddbe7688413ca`，runtime `specVersion=1`，`system_health.isSyncing=false`，metadata RPC 响应 415,442 字节；验收节点已停止。fresh 链无交易且无同 genesis peer 时按“空块不提交 + 离线不挖矿”规则保持 block 0。

2026-07-14 结构收口第 1 步：四个超限生产文件已完成物理拆分，核心 `lib.rs` 785 行、traits 门面 16 行、internal `lib.rs` 496 行、legislation `lib.rs` 572 行；五个投票 crate、runtime 40 项测试和五 crate `no_std` 构建通过。当前源码 `citizenchain-fresh --tmp` 真实启动成功，block#0 为 `0x15b19408800b8ab685b49e8076f861ed76b4713abea54a216a7be2dc0cee41ea`，`isSyncing=false`，验收节点已停止。

2026-07-14 结构收口第 2 步：Track tuple、两级公平清理 FIFO、三条独立 weight 预算和五 pallet `StorageVersion = 1` 最终创世布局已落地。全工作区测试目标检查通过；personal-manage 23、internal 94、joint 3、legislation 33、election 3、runtime 40 项测试通过；五 crate `no_std` 与 runtime 普通/benchmark/try-runtime 构建通过。当前源码 fresh 节点 block#0 为 `0xf20b42ad98756fa464678ab2473abc6f0be089dceae290c587cea80c1ead9ab1`，`isSyncing=false`，metadata RPC 响应 415,442 字节，验收节点已停止。

结构、Track、公平维护、正式 benchmark、覆盖率和 fresh genesis 三步门禁均已完成。正式创世发布仍需统一烘焙冻结 chainspec 并切换同 genesis bootnode；本任务未修改冻结 chainspec、未推送、未部署。

2026-07-13 第四步 B1 验收：runtime 37、`legislation-vote` 32、`legislation-yuan` 30、
OnChina 120 项测试全部通过；node、runtime `no_std` 和 OnChina 生产构建通过。当前源码 production
WASM 的 fresh 临时节点正常启动，block#0 为
`0xf5f7bb30535ead9b5cd5b0159b61124dd0116635ebe78b6b550eb3aa7dc169fe`；真实 metadata
已确认新代表机构存储与 `cast_representative_vote` 生效，被替换的旧存储和旧调用名不存在。

2026-07-13 第四步 B2 曾把 `election-vote` 结果直接封装为 entity 的通用
`InstitutionGovernanceResult`；该过渡实现缺少选举业务层复核，已在 2026-07-14 治理职责第 3 步撤销，
不得恢复为投票引擎直写任职。

2026-07-14 治理职责收口第 1 步：内部投票与业务权限完成分层。FRG 账户上下文不再错误绑定省域 5 人组；多签转账统一从 entity 解析所有机构；销毁仍由业务模块固定 NRC/PRC/PRB，GRANDPA 密钥仍固定 NRC/PRC。专项测试通过：`internal-vote` 88、`multisig` 24、`resolution-destroy` 15、`grandpakey-change` 17，runtime 整体 `cargo check` 通过。

2026-07-17 的“普通内部事项按提案管理员快照派生严格过半”口径已被 ADR-039 取代；目标按 VotePlan 指定岗位主体的有效任职快照派生。组成结果仍只写岗位和任职，绝不派生 admins。

2026-07-14 治理职责收口第 3 步：业务执行端新增 owner/kind/stage/code/account/CID/action 全绑定；联合业务接受联合阶段或公投阶段的合法通过终态；立法路由改为链端双重复校验；选举引擎删除外部创建入口和直写 entity 路径。投票引擎继续只负责投票流程，业务权限与执行前复核留在业务 pallet。

2026-07-22 公民主体快照接口最终验收：`voting_subject`、`candidate_subject`、`voting_subject_at` 全部返回完整 `CitizenSubject`；联合公投与立法公投按永久 CID 去重，票据值与事件保存 CID + 当前签名钱包，钱包更换不能形成第二票。候选、Popular 票据、候选计票和结果仍是第 6 步边界。五个投票 crate、runtime 46 项及受影响业务模块测试，全 workspace 测试目标，`no_std`、WASM、benchmark/try-runtime 和 release Node 构建均通过。真实 fresh 节点 block #0 为 `0x69b4a0025356d050004cff3ef176167a6520b59c9086c9ac6b9a45c4b9e9c0e6`，state root 为 `0x0b066c3567ed25c15cfa96b7d249b6235df4746a253144db21c87dfd2ed2333e`，metadata 二进制 220,197 字节，runtime 六项项目版本均为 `0`；节点已停止。

2026-07-22 选举投票模型最终验收：`ElectionMeta` 只保留唯一机构 `actor_cid_number + role_code`；候选快照、普选票据、候选计票和当选结果全部使用完整 `CitizenSubject`。普选按永久 CID 去重，互选继续按机构 CID + 岗位码 + 钱包票据去重，同一管理员可按多个不同岗位各投一票。`election-vote` 17 项、votingengine 4 项、runtime 46 项、全 workspace 测试目标、`no_std`、WASM、benchmark/try-runtime 和 release Node 构建通过。真实 fresh 节点 block #0 为 `0x285ca7f4ab0f24771baff6a6fc10141ee281fbbd6ce1a8f9dcd1d7676501a41b`，state root 为 `0x27ecdc5b73ce195df4bdfe6c05fe68ef0b682c58751f5f145868a69a1f4672bd`，metadata 二进制 220,398 字节，runtime 六项项目版本均为 `0`；节点已停止。

2026-07-22 全端协议验收：QR 唯一注册表已登记 `ElectionVote.cast_popular_vote = 0x1602` 和 `cast_mutual_vote = 0x1603`；CitizenWallet 严格解码提案编号、完整候选 `CitizenSubject`，互选另解码选民岗位码，旧裸钱包载荷和任何截断、尾随载荷均拒签。CitizenApp 不直接创建选举，也不开放不存在的通用选举业务入口；未来只能由具体公权选举业务模块创建并绑定本引擎。第 8 步未修改 runtime，正式重算权重和 fresh Node/真实交易验收属于第 9 步。

2026-07-22 第 9 步正式权重与最终验收：使用当前源码 WASM、临时 fresh spec、FRAME Benchmark CLI 53.0.0、50 steps、20 repeats，重新生成核心 `votingengine`、`joint-vote`、`legislation-vote`、`election-vote` 的生产权重；联合投票超时夹具先由 `citizen-identity` 准备人口真源，再由投票引擎生成提案快照。`election-vote` 最后一票继续按候选人数线性计费，范围 `1..=256`。五个相关 crate 与 runtime 测试、benchmark 编译和当前源码 release Node 构建均通过；最终 fresh 节点 block #0 为 `0x4bd7e3f65f5ad4788e6ac8917abce9b0683f0c93d286766a7512854084ff0dd9`，state root 为 `0xd15b1a20d972f0cc5f64aa9a08a09f6793fe51886f9445c6dc953c0f9d438f7b`，六项项目 Runtime 版本均为 `0`，metadata 二进制 220,247 字节；节点已停止。

### 公民币订阅与税务架构索引

> 状态：原组合草案已拆分；本文件只保留导航，不再承载可执行方案
> 修订：2026-07-18

原文把平台订阅、创作者订阅、外部支付、收入台账和税务分期写在同一份预研方案中，包含已经废弃的双轨付款、弱交易镜像、旧目录和未确认税务设计。为避免它继续成为第二套实现真源，订阅部分已全部收敛到以下当前文件：

- 订阅技术架构：`CitizenChainRuntime.md`
- 订阅决策：`CitizenChainRuntime.md`
- 订阅执行任务：`历史实施记录《公民币平台订阅与创作者订阅统一改造》（卡已删除，规范以本文为准）`
- 跨端协议：`CitizenChainRuntime.md` 的 P-TX-014 与 P-STORAGE-006

当前订阅边界固定为：

- 平台与创作者订阅统一使用链上公民币；CitizenChain 是价格、扣款、状态和真实公历到期时间的唯一真源。
- CitizenApp 只对订阅、取消、换套餐和创作者设置套餐分别签名一次；自动续费由 runtime 内部执行。
- Cloudflare 只保存 finalized 完整交易证明、可重建订阅镜像和创作者展示资料，不计算日期、不触发扣款、不保存未来扣款价格真源。
- OnChina 与 CitizenWallet 只承接公民链基金会平台调价提案的统一签名流程：OnChina 展示请求二维码，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫响应后提交；投票流程完全复用统一投票引擎。

税务仍属于独立业务范围，只能以 ADR-038 及其后续经用户确认的独立任务为准。不得从本文件恢复税务 pallet、收入台账、税率、目录、索引、回调或分期安排，也不得因为订阅已经实施而推断税务方案获得授权。

---

### 公民币平台订阅与创作者订阅技术架构

> 状态：统一目标架构
> 任务卡：`历史实施记录《公民币平台订阅与创作者订阅统一改造》（卡已删除，规范以本文为准）`
> 决策：`CitizenChainRuntime.md`

#### 1. 目标

平台订阅和创作者订阅统一使用公民币付款，并复用现有 `SquarePost` pallet。订阅期限和自动扣款都以区块唯一的共识 unix 毫秒时间戳为依据；runtime 使用确定性的 UTC 公历算法计算月、季、年，绝不使用区块高度、固定天数或固定毫秒替代真实日期。

当前处于开发期零用户阶段，本契约随全网重新创世直接启用。禁止 storage migration、
旧键读取、旧订阅格式和任何双轨兼容。

#### 2. 系统边界

| 能力 | 唯一负责方 | 边界 |
|---|---|---|
| 平台价格、创作者付款套餐 | CitizenChain | 每次真实扣款读取当前链上价格 |
| 扣款、收款、订阅状态 | CitizenChain | 首扣和到期自动扣款均在 runtime 原子执行 |
| 真实公历到期时间 | CitizenChain | 从当前区块共识时间戳确定性计算 |
| 自动续费调度 | CitizenChain | 到期后按时间顺序处理；停链期间到期周期在恢复出块后补扣 |
| 创作者展示资料 | Cloudflare/D1 | 只保存名称、说明、权益和媒体资料 |
| finalized 订阅镜像 | Cloudflare/D1 | 只做低频缓存、展示和门禁加速，可从链重建 |
| 平台调价 | 统一投票引擎 | `SquarePost` 只创建提案并接收终态回调 |

CitizenApp 对订阅、取消、换套餐以及创作者覆盖设置自己的套餐分别只签名一次，并展示链上时间戳。Cloudflare 不计算日期、不发起扣款、不扫描全链订阅、不决定价格或到期时间；App、设备和 Cloudflare 是否在线均不影响续费。

#### 3. 链上模块

- 路径：`citizenchain/runtime/misc/square-post/`
- pallet：`SquarePost`
- pallet index：`34 / 0x22`
- 发帖和订阅共享 pallet，但类型、storage、扣款和治理代码分文件维护。
- 不新建订阅 pallet；使用有界到期索引和确定性整数公历算法，不引入外部日历服务。

##### 4.1 基础类型

```rust
enum MembershipLevel { Freedom = 0, Democracy = 1, Spark = 2 }
enum IssuerKey<CidNumber> { Platform = 0, Creator(CidNumber) = 1 }
enum BillingPeriod { Monthly = 0, Quarterly = 1, Yearly = 2 }
```

- 平台计划固定使用 `Monthly`。
- 创作者计划可使用月、季、年。
- `BillingPeriod` 表达业务公历周期；runtime 分别增加一个月、三个月或十二个月，并在目标月份没有原日期时使用该月最后一个有效日期，保留 UTC 时分秒和毫秒。

##### 4.2 创作者付款套餐

```rust
struct PeriodPrice {
    billing_period: BillingPeriod,
    price_fen: u128,
}

struct CreatorTier {
    tier_id: TierId,
    prices_fen: PeriodPrices,
}
```

- `tier_id` 在同一创作者下唯一。
- 每档至少包含一个不重复周期，价格必须大于零。
- 链上不保存档位名称、说明、权益文案或媒体。

##### 4.3 订阅计划与状态

```rust
enum SubscriptionPlan {
    Platform { membership_level: MembershipLevel },
    Creator { tier_id: TierId, billing_period: BillingPeriod },
}

enum SubscriptionStatus {
    Active = 0,
    Cancelled = 1,
    Terminated = 2,
    Suspended = 3,
    CreatorPaused = 4,
}

enum SuspendReason {
    NeedReconsent = 0,
    InsufficientBalance = 1,
    IdentityBindingUnavailable = 2,
}

struct SubscriptionState {
    plan: SubscriptionPlan,
    started_at: u64,
    last_charged_at: u64,
    last_charged_price_fen: u128,
    paid_until: u64,
    subscription_status: SubscriptionStatus,
    authorized_price_fen: u128,
    suspend_reason: Option<SuspendReason>,
}
```

所有时间字段均为 unix 毫秒时间戳。字段顺序即固定 SCALE 顺序，Dart、TypeScript、JSON 金标和 runtime 必须逐字节一致。

- `Active`：授权有效且在续费调度内。
- `Cancelled`：用户签名取消，保留当前已付权益至 `paid_until`。
- `Suspended`：暂停续扣、保留粉丝关系、退出调度，等用户动作恢复；`suspend_reason`
  说明原因（创作者改价待再签名 / 余额不足待充值再签 / CID 当前双向绑定不可用）。
- `CreatorPaused`：创作者掉平台会员，粉丝暂停扣费但**仍留调度**，创作者恢复即自动续。
- `Terminated`：仅显式关闭/清档，不由余额不足或掉会员自动进入。
- `authorized_price_fen`：订阅者已授权用于自动续费的价格（创作者改价重签检测与换挡折算基准）。

#### 5. Storage

```rust
Subscriptions<(SubscriberCidNumber, IssuerKey<CidNumber>)> -> SubscriptionState
PlatformPrice<MembershipLevel> -> u128        // 创世 genesis_build 播种，仅内部投票可改
CreatorPlans<CreatorCidNumber> -> CreatorTiers
RenewalSchedule<(due_at_be, SubscriberCidNumber, IssuerKey<CidNumber>)> -> ()
RenewalIndex<(SubscriberCidNumber, IssuerKey<CidNumber>)> -> due_at
```

平台机构 CID 永久固定为**创世常量**（公民链基金会 `CITIZENCHAIN_FOUNDATION`），不是可写存储；无 `PlatformCidNumber` 存储、无迁移（开发期零用户、重新创世）。`RenewalSchedule` 用大端时间戳键保持到期顺序，`RenewalIndex` 保证每个订阅只有一个当前到期项。不保存下次扣款区块、外部续费账户、设备状态、链下扣款密钥或第二份展示套餐。

#### 6. Call 契约

| index | 调用 | 签名者 |
|---:|---|---|
| `0` | `publish_post(...)` | 发帖账户 |
| `1` | `subscribe(issuer, plan, expected_price_fen)` | 订阅者 |
| `2` | `cancel(issuer)` | 订阅者 |
| `3` | `set_creator_plans(tiers)` | 创作者 |
| `4` | `change_subscription_plan(issuer, new_plan, expected_price_fen)` | 订阅者 |
| `5` | `propose_set_platform_price(actor_cid_number, membership_level, new_price_fen)` | 公民链基金会治理岗位任职账户 |
旧 call、旧 SCALE tag 和旧交易载荷不兼容；不存在外部 `renew` 或周期确认 call。

#### 7. 首次订阅与自动调度

1. CitizenApp 从 finalized storage 读取当前价格。
2. CitizenApp 提交 `subscribe`；runtime 重新读取当前价格并校验 `expected_price_fen`。
3. runtime 从 signed origin 解析 active `subscriber_cid_number`，以 CID 写订阅键，并在同一
   storage layer 从该笔交易签名账户首扣；以当前区块唯一 `Timestamp.Now` 写入
   `started_at`、`last_charged_at` 和审计价格。
4. runtime 按 UTC 真实公历计算 `paid_until`，写入 `Active`，并把该到期时间登记到调度索引。
5. CitizenApp 等待交易 finalized 后读取并显示链上 `started_at` 与 `paid_until`；不再提交第二笔确认交易。

平台收款账户从**创世常量** CID + `RESERVED_NAME_FEE` 派生（公民链基金会费用账户，
创世已播种）。创作者以 `creator_cid_number` 定位，扣款时解析其当前双向绑定账户并全额
收款；自订阅按 CID 拒绝，且创作者必须拥有有效平台订阅。

#### 8. runtime 自动续费

1. 续费在 `on_initialize` 执行并使用上一块已确认时间；正常出块时最多延后一个区块，
   绝不提前扣款。余额变化因此进入 NodeGuard 的 finalize 前状态，不会被误判为原生发行；
   每块仍受 `MaxSubscriptionRenewalsPerBlock` 硬上限约束，并按实际处理笔数记账。
2. runtime 从最早到期项开始处理 `due_at <= previous_confirmed_timestamp`，单块最多处理
   `MaxSubscriptionRenewalsPerBlock` 项，返回权重按实际处理笔数计算；超出硬上限的同刻到期项
   在后续区块继续。
3. 每次续费先由 `subscriber_cid_number` 解析并复核当前双向绑定账户，只能从当前账户
   扣款；绑定缺失、CID inactive 或双向不一致时转
   `Suspended(IdentityBindingUnavailable)`，绝不回退扣历史账户。
4. 每个周期扣款均读取当时最新链上价格；平台治理改价自动按新价续。创作者改价（当前价 ≠ `authorized_price_fen`）则**不自动续**，转 `Suspended(NeedReconsent)` 待订阅者再签名。
5. 停链期间无法发生状态变更；恢复出块后按到期顺序补扣所有已到期周期，未完成部分在后续区块继续。
6. 续费失败按原因分流（不再一律 `Terminated`）：余额不足 → `Suspended(InsufficientBalance)`（离调度）；创作者掉平台会员 → `CreatorPaused`（留调度、下周期重试、恢复即续）；档位/周期删除 → `Suspended(NeedReconsent)`；公历换算失效等 → `Terminated`。

续费不需要账户再次签名，不依赖 CitizenApp、设备或 Cloudflare 在线，也不存在任何外部续费提交者。

#### 9. 挂起、取消和换套餐

- 挂起恢复：`Suspended` 由订阅者再签名（改价场景）/充值后再签（缺钱场景）恢复，走 `subscribe` 落到首扣路径；`CreatorPaused` 随创作者恢复平台会员在下周期重试时自动续。
- 取消：写 `Cancelled`，保留已确认的 `paid_until`，到期前已付权益仍有效。
- 换套餐（`change_subscription_plan`）**立即生效并折算**：剩余权益 `y = 已授权价 × (paid_until−now) ÷ (paid_until−last_charged_at)`；升档补扣 `新价−y`、新周期从现在起算；降档不扣、余额按新档单价折算成延长时长。
- 再订阅（同计划未过期的 `Cancelled`）：恢复原调度继续扣费，不重扣。
- 不退款、不补差价、不按日折算。

#### 10. 创作者套餐与平台调价

- `set_creator_plans` 覆盖式写入创作者自己的链上付款字段。
- 新订阅和下一次真实续费读取最新价格；当前已付周期不变。
- 创作者在 CitizenApp 同一次业务提交中填写档位标识、名称和周期价格，并只签名一次 `set_creator_plans` 交易；finalized 后 App 把交易哈希、区块哈希和完整已签名 extrinsic 连同展示资料提交给 Cloudflare，Worker 严格复核交易包含关系和同一区块链上状态后保存镜像。
- Cloudflare 展示资料必须引用 finalized 的 `creator_cid_number + tier_id`；`creator_account_id`
  仅可作为该笔交易或当前收款账户的审计字段，不参与业务定位。边缘不得保存第二份扣款真源
  价格；finalized 后的镜像只用 Bearer 会话和链读复核，不生成设备请求签名。
- `propose_set_platform_price` 只调用统一内部投票引擎；人口快照、资格、计票和状态推进不进入业务 pallet。

#### 11. 信任边界

订阅授权由订阅者第一次签名建立，并持续有效到订阅者签名取消。公历到期时间、自动扣款和状态变更由 runtime 唯一决定；CitizenApp 与 Cloudflare 都不能伪造、延长或触发续费。共识时间戳仍受区块生产与 Timestamp pallet 规则约束，但在同一链状态下所有节点执行完全相同的整数公历结果。

#### 12. Cloudflare/D1

- CID 是所有用户订阅镜像的业务主键：平台订阅主键为 `cid_number`，创作者档位主键为
  `(creator_cid_number, tier_id)`，创作者订阅主键为
  `(subscriber_cid_number, creator_cid_number)`。
- confirm 请求固定携带 `tx_hash`、`block_hash`、`signed_extrinsic_hex` 和业务动作；订阅或换档
  携带目标档位，创作者订阅同时携带 `creator_cid_number`、`tier_id`、
  `billing_period`，创作者套餐保存同时携带展示档位数组。
- Worker 重新计算 extrinsic 哈希，严格解码签名者、pallet/call index 与 SCALE 参数，校验签名者等于 Bearer 会话钱包、指定区块属于 finalized 主链且确实包含该完整 extrinsic，再读取同一区块 `Timestamp.Now`、`Subscriptions` 或 `CreatorPlans`。请求中的价格、状态和期限从不作为真源。
- `chain_transaction_confirmations` 将一笔 finalized 交易首次绑定到钱包、区块、extrinsic 序号、动作和规范化请求哈希；完全相同的 HTTP 重试幂等成功，同一交易换钱包、换动作或换展示资料一律冲突拒绝。
- `square_memberships` 和 `square_creator_subscriptions` 镜像完整链上状态、finalized 锚点及最近一次交易哈希；`last_charged_price_fen` 只是已发生扣款的审计镜像，不能作为下一次扣款价格真源。
- `square_creator_tiers` 按档位规范化保存展示名称和 finalized `CreatorPlans` 镜像；覆盖保存使用 D1 batch 原子替换，不保留退役档位残行。
- `chain_clock` 只接受更高 finalized 区块，保存同一区块链时间戳和本地观测时刻。门禁统一要求状态为 `Active` 或尚在已付期内的 `Cancelled`、`chain_timestamp < paid_until` 且链时钟未陈旧；`Terminated`、未知状态、缺时钟、未来观测、陈旧时钟和到期全部 fail-closed。
- Cron 每轮只读取一次 finalized 头和时间戳，只查询 `Active AND paid_until <= chain_timestamp` 的到期候选，按固定上限逐行纠偏；不扫描未到期全表，不计算公历，不触发扣款或续费。
- 平台发布、上传预留、平台用量与创作者管理等 Cloudflare 资源入口都调用同一平台门禁；创作者订阅专属资源必须在签发数据或短效资源地址前调用创作者订阅门禁。仓库当前没有创作者专属内容路由，因此本步骤不虚构该产品功能。
- CitizenApp 按钱包保存最近 finalized 证明和有界镜像待重试队列；App 再次运行时只重试 Bearer HTTP，不再次签名或提交链上交易。Cloudflare 不可用不阻断链上订阅操作。
- 直接端到端 P2P 媒体不占用 Cloudflare 存储或中转，App 的发送端与接收端仍执行本地大小门禁；这类端到端数据不应被表述为 Cloudflare 可集中强制的订阅权益。Cloudflare 承载的上传、存储、中转和签名 URL 则全部由服务端门禁强制执行。

#### 13. 重新创世

- 开发期全网重新创世直接生成 CID 键布局，不读取、转换或保留任何 AccountId 旧键。
- SquarePost 不提供 storage migration、兼容枚举、旧 call 或双轨查询。
- 同批重建 Worker D1 与 KV；边缘镜像全部由新链 finalized 状态重新生成。
- 创世后必须以真实本地链验证：CID A 绑定账户 1 完成订阅，换绑到账户 2 后关系不变，
  自动续费只扣账户 2；CID 或双向绑定不可用时不得扣账户 1。

#### 14. CitizenApp 与签名体验

- 订阅、取消、换套餐和创作者设置套餐各自使用一笔热钱包标准 extrinsic，并等待 finalized；同一业务操作不得追加第二次账户签名，自动续费没有用户交易或签名。
- 第三步已经接入完整页面流程、finalized 状态读取、真实日期展示和创作者一次签名后边缘镜像重试，不实现续费编排。
- 第四步已把 finalized 证明扩展为交易哈希、区块哈希和完整已签名 extrinsic；App 按钱包持久化有限证明历史与待重试队列，镜像失败不重复签名。
- 页面显示使用 `DateTime.fromMillisecondsSinceEpoch(...).toLocal()` 展示真实日期和时间，不显示区块高度或“固定天数”。
- Cloudflare 暂时不可用时，App 仍以 finalized 链上价格、档位和订阅状态工作；展示名称可使用本地兜底。

#### 14.1 交易收费

- `subscribe`、`cancel`、`change_subscription_plan`、`set_creator_plans` 都是账户签名的非系统链上交易，统一经过 runtime 交易支付扩展和签名账户收费路由。
- 业务转账金额为零时仍收取最低链上交易费；不得把取消订阅或只改状态误判成免费操作。
- runtime 到期自动扣款在区块执行阶段内部运行，不是外部交易，因此不追加用户交易费。

#### 14.2 OnChina 平台调价与机构工作台

- 所有机构管理员都从链上中国统一入口扫码登录。登录态必须携带节点绑定的准确 `institution_cid_number`，工作台由后端根据准确 CID、机构类型和链上权限下发；前端不得根据机构码猜测工作台。
- 注册局、私权、司法、立法、其它公权和非法人机构使用不同工作台。私权机构只查看本机构信息、链上 `admins` 和被授权模块，不复用注册局的公民、机构目录或登记页面。
- 平台会员价格模块是实例级授权：只有当前绑定 CID 与**创世常量**平台机构 CID（公民链基金会）精确相等时才下发。OnChina 不在 PostgreSQL 保存平台价格或平台 CID 副本。
- 调价 API 为 `GET /api/membership/platform-prices` 与 `POST /api/membership/platform-prices/propose`。prepare 和 submit 都重新检查节点绑定、准确平台 CID 和链上 active `admins`，任何无法确认都 fail-closed。
- 所有 OnChina 链交易共用 `POST /api/admin/chain/submit` 与同一 core 提交器。流程固定为：OnChina 展示请求二维码，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫后验签、dry-run、提交并等待进块。禁止业务模块另建提交 URL、二维码协议或签名流程。
- 平台调价动作在唯一 QR registry 中为 `propose_set_platform_price`；CitizenWallet 必须中文展示公民链基金会 CID、目标平台档位和新价格，未知或不完整载荷直接拒签。
- `propose_set_platform_price` 只创建统一内部投票提案。资格、计票、推进和终态执行归投票引擎，OnChina 和 SquarePost 不实现第二套投票。

#### 15. 真实验收

- runtime 单元测试、金标 SCALE、benchmark 编译、完整 runtime 测试和 WASM 构建通过。
- 在全新本地链确认 SquarePost 只生成 CID 键，不存在 AccountId 旧键或迁移入口。
- runtime 覆盖月末、闰年、跨年、季和年周期计算、自动续费、停链后补扣和余额不足终止。
- Cloudflare 严格解码新状态，拒绝尾随字节和非法标签，且不包含任何日期计算。
- Cloudflare 必须验证 finalized 主链中完整已签名 extrinsic、同一区块状态和首次请求绑定；旧区块证明不能刷新链时钟。
- 本地 Worker、D1 与 HTTP 必须实测缺设备证明的 finalized 镜像请求可进入业务校验，而其它受保护写请求仍保持设备证明门禁。
- OnChina 已在隔离本地 PostgreSQL 上连接真实本地链并完成链投影同步；平台价格、调价提案和统一提交接口在无登录态时均 fail-closed，旧公民专属提交入口已移除。
- OnChina、CitizenWallet 与统一二维码注册表已完成编译、静态分析和自动测试；最终跨端调价交易、内部投票终态及完整订阅生命周期纳入第 6 步总验收。
- 后续步骤必须完成真机、真实本地链、真实 Worker/D1/HTTP 的端到端验收。

#### 16. 禁止事项

- 禁止用区块高度、固定天数或固定毫秒表示订阅周期。
- 禁止在 CitizenApp 或 Cloudflare 计算并提交订阅到期时间。
- 禁止外部 renew、周期确认、设备签名或 keeper。
- 禁止跳过停链期间已经到期的周期或恢复旧订阅协议。
- 禁止把 D1 镜像当作订阅真源。
- 禁止在业务模块实现投票流程。
- 禁止保留旧字段、旧注释、旧 UI 文案或兼容分支。

---

## Release 全量构建（第 7.4 步）

正式 Release 固定从干净源码执行全量构建，显式关闭 Rust 增量编译及工具链内置缓存，不读取CI作业缓存且不复用本机编译中间物。版本、签名、校验、产物和发布流程保持原有产品合同。

## 双仓统一流程最终收口（第 7.5 步）

本产品执行统一流程规则：本机编译中间物只进入本轮塔塔缓存库的build目录并按终态规则清理；GitHub CI 的作业过程数据只进入该次Runner任务空间；正式Release从干净编译状态执行。源码不进入塔塔缓存库、塔塔依赖库或塔塔产物库。
### 完整产品仓组织重构的Runtime边界

Node、Runtime、OnChina同属完整citizenchain仓。原聚合Cargo工作空间及其锁完整保留到QR协议工具工作空间，公民链主体工作空间仍包含原有全部Runtime成员；Polkadot SDK准确提交保持1aa4447575d446ab393e89b86cd8ec0a8fca100d。

本轮Runtime目录共16个文件：primitives/src/sign.rs的两条说明、primitives/tests/fixtures/signing_domain_vectors.json的_comment，以及14份既有weights.rs注释中的模板绝对路径。权重文件只删除已退役聚合仓包装层，所有生成命令参数、权重数值及运行代码不变。签名说明中生产发布授权使用“本机生产发布授权”，不在公开源码记载控制台名称。OP_SIGN_PUBLISH=0x24、GMB签名域、所有签名金标数据与运行代码保持逐字相同。EVM仍为独立暂停任务，不随组织重构实施。执行前必须对这16个完整路径及Cargo间接影响取得二次确认；验收只运行锁定依赖和既有测试，不更新金标或格式化Runtime源码。

## 完整产品组织与执行合同

所有者：`citizenchain`，正式源码根 `<本仓根>`；本说明属于该完整产品内的Runtime组件资料。组件不会拆成独立仓库或目录产品。所有执行身份统一为 `产品.平台.流程`；工作目录合同由 `CitizenChainNode.md` 的“本机固定执行目录”唯一承载，不建立平台工作目录层。

真实平台目标：`macos`、`windows`、`linux-arm`、`linux-amd`、`wasm`。

仓库推送仅上传本仓已经保存的main提交。控制台推送的唯一实现为console/tuisong.mjs，每仓一次生物识别，授权成功后建立独立任务，任务栏记录Git进度、准确SHA、取消及成功/失败终态。只执行Git与GitHub main只读回查，不执行源码、依赖、注释、文档、测试、签名或资源门禁；不派发产品Workflow、不运行hooks、不续签或重复认证、不自动重试、合并或强推。

本仓已移除GitHub main推送门禁触发器；main上传后不自动运行产品自动化。自动化由用户单独发起，产品仍拥有自己的Workflow、声明、资源、测试和产物实现；产品不导入控制台源码，不依赖控制台工具库、私有规则或其它仓库工作树。控制台只是可选Git客户端。各仓可独立使用公开Git接口完成仓库操作，公开SDK依赖不构成流程耦合。


技术文档由所属完整产品仓根唯一持有；私有规则和任务库由控制台私仓持有，公开产品不读取它们。公开门禁不依赖私仓资料、安装包源码、其它本机产品或个人账号；必要链真源只读本仓明确固定的公开40位SHA，不在门禁中跟随main。本机开发跨产品验收仍比较三仓已保存快照与各端真实镜像。

### 门禁与开发审查职责

准确中文注释按开发阶段逐项复核，不以保留源码每文件包含汉字作为仓库门禁的开发凭证。初始完整内容、生成文件和上游原件保持原文；真实第一方临时注释、机密、源码输出、Workflow、依赖和适用测试仍由本仓同提交门禁验真。公民门禁只把scripts中的Node命令行结果报告识别为CLI输出；本仓实际执行测试的准确协议拒绝断言不属于新运行协议，字符串、注释、模板和未登记测试中的同文不豁免。保存及推送仍逐仓独立授权，并以本机门禁和同SHA的GitHub门禁双成功为唯一终态。

本次依赖统一同时覆盖归档差分测试的第一方smoldot C ABI适配及hex/parking_lot直接声明；对应Cargo锁与SDK冻结摘要原子同步。上游PoW与libp2p内部闭包仍按来源保留，不把第一方适配当成上游例外。全17仓直接声明回归按准确源码归属检查Cargo、Pub与npm，不只比较依赖库索引。

QR协议的 self_occupy_cid 动作0x0a05对齐现有Runtime单cid_number参数；修改仅属crates/protocol，不改变Runtime。

## 第5步执行注册与空间资源边界

第5步已取得六份Runtime既有文件二次确认、两份SDK空间补修范围、具体验收目录及本轮一次远端固定准备确认，已完成本机执行注册验收；本阶段未开放网络RPC或操作生产节点。
严格原生模式复用SDK已有RuntimeCosts，在日志空间计量中加入数据长度加256字节固定结构上界，在不可变数据中加入数据长度和AccountId编码长度；非严格模式不加此空间量，既有计时测量权重不改。这只限制状态增长，与公民链费用分类、公式、最低费、付款和分账无关。计划区块仍60秒及普通75%，proof_size有限8MiB；Runtime真实integrity与空间耗尽回归已验证128MiB/512MiB配置和有限空间预算。

本轮获批Runtime源码已使用官方UncheckedExtrinsic包装器、pallet35及完整原有交易扩展，SetOrigin置于CheckMetadataHash后、WeightReclaim前；原生默认标记编码为空，chain-signing保留旧payload字节验收。RuntimeNativeFee报价只读复用charge_details，付款预检和实际收费均绑定现有OnChargeTransaction；只新增Ethereum金额严格整分后进入现有链上路由，其余Revive原生业务入口拒绝。区块proof_size设8MiB，时间60秒、普通75%、费用权重和字节价格均保留原值。Runtime/Executive集成回归和原生编码回归已实际通过；真实源码WASM执行也已验证部署、存储、日志、内部调用、模拟恢复及失败单次收费分账。默认Node构建、完整节点回归及runtime-benchmarks/try-runtime编译检查均已通过。

本机开发使用已保存固定SDK提交7f115f0825a59df94faabe7074ec83c3e9463c7d及不可变Git bundle，仍不读取SDK开发工作树；本机SDK源码验收与本机门禁通过后即可离线消费，不要求先等远端。远端门禁只用于独立判定本次推送成功，运行中不冒充成功。174项SDK来源统一，包版本、checksum及非SDK锁连接保留。

Revive benchmark特性传播在原锁中补入pallet-revive到已有rand_pcg0.3.1的一条连接；不新增包或版本，不改checksum及非SDK连接，后续正式验收仍强制--locked --offline。

交易扩展末两项使用官方TransactionExtension嵌套元组，将SetOrigin和WeightReclaim组合在同一项；顺序仍在CheckMetadataHash之后，避免Rust标准元组Debug/Eq上限。默认标记及隐式数据仍为空编码，原生完整签名载荷已由字节回归证明不变；收费器预检显式指定既有Runtime类型，不改变收费配置或计算。

## MLS公钥登记签名域

`runtime/primitives/src/sign.rs` 中唯一登记常量为 `OP_SIGN_MLS_DEVICE_BIND: u8 = 0x1C`。它授权当前 CID 绑定账户登记本机同一32字节 MLS 公钥，链下验签仍使用 `blake2_256(GMB ‖ op_tag ‖ SCALE)`。载荷顺序为 `cid_number`、`binding_revision u64LE`、`account_id`、`public_key`、`issued_at u64LE`；账户和公钥文本均为小写 `0x` 加64位十六进制。

签名金标闭集为16个当前哈希域；删除的编号不重用、不提供别名或兼容常量。链上交易、二进制前缀签名与其他现行授权域保持其既有语义。共享 QR 注册表以动作13承载 MLS 登记，正式导出到钱包和链内消费者；生成文件禁止手改。


### 第4步实施中：远端路由当前声明


本次同步路线读取、热更新和失败边界用例，未运行测试、语法检查、编译、签名、安装或下载。第4步仍在开发中：Publish执行器、聊天安装器、Start、固定菜单声明与完整程序摘要的其余实际耦合尚未解除，不能报告该步或整项任务完成。

### WASM Release公开入口



本产品scripts/build.mjs的模块初始化与CLI执行分离：私有异步runCLI承载原命令主体，仅在直接执行文件时启动，拒绝时输出错误并以退出码1失败。模块求值先完成，scripts/build.mjs可反向导入同一checkWork、requirements和平台校验，不复制实现或增加启动入口；普通import不启动CLI。现有公开参数、JSON请求、--offline、必要重入、资源/准备/编译/适用签名安装回读步骤以及取消与结果合同保持。离线缺件和非法输入必须真实失败，禁止以未完成顶层await退出替代完整结果。对应真实CLI回归只在自有target测试现场替换资源供给边界，验证反向导入、参数与错误传播，不据此声称实际产品编译通过。


本仓平台命名门禁仍扫描完整Git跟踪路径和正文，仅在内存副本识别scripts/build.mjs中唯一规范的toolDefinitions与flutterPatch声明。规范JSON回读及唯一工具身份阻断重复键、转义、歧义和重复声明；使用Flutter时核验准确官方来源、版本对应归档和本仓补丁来源与全文摘要，未使用Flutter时只接受已核实固定来源与全文SHA-256的共同原补丁。仅处理官方native_assets_host.dart中与准确文件头、行号、lipoDylibs签名及紧邻调用同时闭合的一行原上下文注释，其它新增、删除、上下文、源码和路径的旧平台名称继续拒绝；实际资源源码、补丁、版本、锁和原件不变。目录边界回归以unlinkSync删除自身合成目录符号链接，继续完整验证根target普通目录可用、嵌套target/目录链接/普通文件拒绝；生产目录边界规则不变。回归使用本仓真实门禁与完整Git跟踪合成文件，只在本产品准确target测试现场运行，不将扫描夹具作为真实产品编译或发布证据。

本仓门禁的测试子进程白名单仅保留已有PRODUCT_GIT_BIN准确执行器路径，供完整Git索引夹具使用；缺少该准确入口时回归失败，不查询PATH、不回退系统Git、不传凭据或其它产品材料。不新增工具版本、声明字段、公开参数或生产资源获取步骤。


## 只读塔塔门禁与功能验收边界（2026-10-10）

`.github/tatagate/tatagate.mjs` 只读核对本仓主检出、HTTPS 来源、目录闭集、流程调用方向、Node 语法与本仓 QR 金标和 Pallet 注册表。门禁不准备资源、不执行产品编译或功能测试，也不调用 `scripts/build.mjs`。功能测试由所属 Build 或各平台自动化执行；旧门禁资源准备函数和配方已从 Build 清除。当前改动只完成静态检查，真实编译和正式门禁尚未验收。

## GitHub自动化

本仓自动化只在GitHub的main源码上执行；控制台只调用与展示。各目标独立拥有同名的YAML与Node实现，不调用其他仓或其他目标的Workflow。版本、构建、测试、签名、完整产物核验与正式tag/Release均由本仓负责。

- `.github/workflows/release-linux-amd.yml`及同名`.mjs`。
- `.github/workflows/release-linux-arm.yml`及同名`.mjs`。
- `.github/workflows/release-macos.yml`及同名`.mjs`。
- `.github/workflows/release-wasm.yml`及同名`.mjs`。
- `.github/workflows/release-windows.yml`及同名`.mjs`。

每个目标的最后任务使用always读取所有前置结果：全部成功清本仓本目标旧成功，否则清旧失败并失败退出。仅保留最新成功、最新失败各一条；保护本次Run和所有活动任务，另一类结果与其他目标不受影响。删除关联正式Release、tag、Actions产物和Run后回查；任何清理错误都按实际失败报告，不自动重试。

所属回归位于各目标同名mjs，覆盖前置结果、版本边界、平台隔离、活动保护和完整分页；真实GitHub构建与发布验收依任务授权另行执行。

## WASM自动化与专属开发升级

WASM只由本仓.github/workflows/release-wasm.yml和同名mjs执行。GitHub读取本仓CHAIN_URL、CHAIN_GENESIS_HASH变量，以及CHAIN_ID、CHAIN_SECRET机密，按固定只读RPC方法取得同一finalized锚点的版本与创世身份；缺件、错链或请求失败立即失败，不重试。生成的spec_version取本仓源码版本、已成功产物版本和链上版本加一的最大值，写入本次临时构建源码，不回写main。链上未升级时可复用同一协议版本，正式tag包含本次Run和attempt，准确指向本仓源码提交。

实际编译、Clippy、完整WASM集合与节点候选Runtime政策探针均在CitizenChain内执行；三件WASM逐件上传和回读，来源正文保留链目标证明。本仓负责自己的正式Release及同目标同结果历史清理。控制台只调用、跟踪和展示此自动化；其专属开发升级从成功wasm产物消费三件已证明资产，再执行冷签和提交，不生成自动化版本，也不替本仓读取链来阻止派发。配置原件归所属GitHub仓，本次源码修改没有向GitHub写入机密或部署配置。


### 本仓 GitHub 自动化与塔塔门禁目录

`.github/` 仅保留 `workflows/` 与 `tatagate/` 两个目录。`workflows/` 持有本仓自动化；`tatagate/` 仅保留 `tatagate.json` 与 `tatagate.mjs`。前者登记本仓门禁合同，后者保留正式门禁实现与测试报告器，测试代码统一位于正式代码之后。直接运行执行门禁命令，测试运行只执行末尾测试，普通导入不注册测试；本仓测试清单及逐文件成功回执使用同一个门禁文件且仅执行一次。
