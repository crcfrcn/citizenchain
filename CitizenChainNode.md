## 平台编译现场

本产品编译任务使用本仓 `target/build/<平台>` 独立临时目录，平台键为 `macos`、`windows`、`linux-arm`、`linux-amd`、`wasm`。不同平台同时领取并执行；同平台已有活跃任务时立即拒绝再次领取。资源准备、工程副本、缓存和编译输出只写本平台现场；确认进程及后代退出、结果被调用方消费后，删除整个平台目录。`target/build` 仅是父目录，`target/test` 仍用于独立测试。独立执行和控制台调度调用同一本仓编译入口与清理接口。

## 工具与依赖的声明和供给职责（2026-10-08）

本仓Build结果按节点能力确认：macOS交付与自身启动声明对应的唯一节点App；Windows、Linux和WASM本机目标只确认编译，不宣布正式节点安装包或Runtime发布完成。桌面启动在准备数据库前确认本仓macOS App和真实节点可执行文件，拒绝借用其它产品声明。

本产品完全独立管理全部流程所需的工具、依赖及其它资源需求。需求唯一依据为本仓源码、公开声明、锁文件及本产品拥有的准备配方，包括准确版本、平台、官方来源、摘要或固定提交、闭包、验真方式和失败条件；塔塔控制台按当前产品声明提供资源，不维护另一份产品需求或替产品决定版本、来源与流程步骤。

本产品必须能在没有塔塔控制台时完全独立执行全部已实现流程。独立执行时，本产品自行完成可信引导、资源获取、验真、保存、复用及任务工作视图准备，不依赖控制台源码、私有资料、安装位置或资源库。

通过塔塔控制台执行本产品流程时，本产品向控制台声明所需资源并使用其已准备好的供给。控制台先核对并复用已有的匹配工具与依赖；没有的由控制台按本产品声明下载、准备、验真并保存到控制台工具库或依赖库，再交付本产品复用。本产品负责核验交付与自身需求一致并使用资源，不因控制台缺件或供给失败改为自行下载，也不另建同一资源的永久副本；可写包管理器视图与流程过程数据仍归本产品当前任务工作目录。

两种执行方式使用本产品同一声明、锁和流程实现，仅资源供给职责随执行方式改变。该职责适用于本产品全部平台与已实现流程；控制台本身作为产品同样适用。独立模式下资源缺失由产品处理；控制台模式下资源缺失由控制台处理。显式离线缺件、交付失败、损坏、错误摘要、来源漂移或越界必须据实失败，不自动升级、覆盖可疑原件或切换执行方式。

以上为当前职责规范；本次只更新文档，不代表现有资源协议与运行代码已完成接入或通过真实流程验收。历史记录中的“可选供给”或“产品负责缺件获取”仅描述当时实现，不作为当前职责依据。

本仓现行入口以`scripts/build.mjs describe`及产品公开scripts实现为准；本文按日期保留的历史验收只描述当时结果，不作为当前工具、私有调用者或已撤销Publish实现的运行条件。独立塔塔门禁候选的职责和未验收状态见文末。

## 当前工作目录归属

Node、Runtime、OnChina 的全部测试、编译临时数据和产物使用完整 CitizenChain 仓的同一工作边界；固定工作目录和收尾合同统一见本文“本机固定执行目录”。平台只进入执行身份、授权、日志和状态，不建立平台工作目录层；独立入口与控制台调用使用本仓同一流程实现。

第8、9步完成目录与路径实现、根文档迁移及测试源码维护，未运行测试、门禁、编译或安装。本文唯一原件位于<本仓根>/CitizenChainNode.md；产品接口及流程直接以本仓实际代码和声明为准，业务字典库与其检查已撤销，不另建登记副本。历史验收事实不表示本轮改造已经通过验收，统一测试在第10步进行。根技术文档由本仓门禁按原文、JSON解码值及既有补丁快照扫描机密，仅报告路径；文档迁出不减少资料安全检查。


## Cloudflare 公共钱包接入的生产前置条件（2026年10月7日）

公共钱包域名 https://nrcrpc.crcfrcn.com/ 由 CitizenServe Worker 承接，Cloudflare自动管理公网DNS和边缘证书；Worker内部继续调用现有受Access Service Auth保护的Tunnel。节点RPC仅监听回环，现有cloudflared直接连接节点HTTPS端点；旧18080网关属于当前旧部署的历史状态，不能作为新节点静态交付方案。源节点保留Safe、DenyUnsafe及全局预算。客户端限流在Worker边缘按真实IP执行，节点仍承担回环来源的总预算，不依赖可伪造的转发头。

回源目标证书使用Cloudflare Origin CA，SAN为chain.crcfrcn.com。私钥只在生产服务器生成和保存；只上传CSR，证书公开部分与Origin CA根用于验证。节点实际读取CITIZENCHAIN_RPC_TLS_CERTIFICATE的PEM证书链及CITIZENCHAIN_RPC_TLS_PRIVATE_KEY的PKCS#8 DER私钥。cloudflared直接连接节点HTTPS，originServerName为证书域名chain.crcfrcn.com，httpHostHeader为对应节点127.0.0.1:端口以满足既有HostFilter，caPool明确指定现有Origin CA根文件；禁止noTLSVerify和明文降级。

本轮实际SSH只读确认国储会节点、Nginx、cloudflared运行正常；运行二进制为citizenchain 1.0.1-babef11d9a7，现有网关/etc/nginx/sites-available/guo-rpc转发至HTTP回环9944，真实TLS握手失败。该部署二进制未检出Ethereum RPC及新TLS配置标识，实际链上Ethereum接口与链ID尚未验收。不能仅添加Cloudflare域名或证书后把旧节点视为当前以太坊兼容实现；具备已验收接口的准确Node/Runtime部署须先单独落实，禁止用临时测试链冒充正式公民链。

生产顺序固定为：现有旧Node承载新Runtime升级；新Runtime在正式链升级成功后，创世身份结果必须与真实块0一致，并继续正常出块与最终确认；随后才在Node增加对应创世身份守卫，再更新各节点软件。API源码存在或返回编译常量不代替链上验证。Cloudflare证书、CSR及直接节点HTTPS配置可提前准备，公共RPC激活在上述顺序及完整TLS链路验收之后，避免新Node守卫提前拒绝旧Runtime造成升级死锁。

生产证书准备已推进：服务器/opt/citizenchain/tls中已生成RSA3072私钥及公开CSR，PEM私钥root:root 0600、同密钥PKCS8 DER root:citizenchain 0640、CSR root:root 0444；仅公开CSR提交Cloudflare。chain.crcfrcn.com的Origin CA证书已签发，CA签名链、服务器域名及CSR公钥匹配均已验证；公开叶证书与官方RSA CA根已安装于/opt/citizenchain/tls，均为root:root 0444；生产OpenSSL3.0.13实际验证CA签名链、chain.crcfrcn.com域名及CSR公钥一致性通过。重复安装先只读核对已有两份普通非链接文件的权限、完整内容与证书链，一致则成功退出；材料不完整或不符时明确报告阶段并拒绝覆盖，首次安装仍要求root及服务器内私钥一致性验证。真实TLS链路仍待切换与验收。叶证书到期为2041年10月3日，当前官方RSA Origin CA根到期为2029年8月15日，必须在信任根实际到期前更新并重新验收，不能把叶证书有效期当作CA根有效期。

## 聊天功能的唯一产品归属

**聊天客户端的逻辑功能只能在 TataChatSDK 中实现；聊天服务端的逻辑功能只能在 CitizenServe.tatachat 中实现。公民、途遇及其他产品只依赖使用。**

CitizenChainNode 涉及聊天时只作为依赖使用方；本条不代表尚未接入聊天的产品已经具备聊天能力。

- 消息、会话、群组、加密、协议、传输、同步、重试、聊天存储、附件、通话及聊天界面行为，按客户端与服务端职责分别归 TataChatSDK 和 CitizenServe.tatachat；新增功能、缺陷修复和平台差异也必须在所属产品内完成。
- 消费产品只提供产品入口、身份与业务权益结果、服务地址及授权、主题和公开接口要求的平台配置；只通过公开接口接入，禁止复制、重写、包装成另一套聊天内核或维护产品专属聊天实现。CitizenServe、TuyuServe 的产品身份与权益授权不包含聊天数据面的实现职责。
- 本机开发直接依赖仓库路径；公民、途遇等产品的正式版本依赖塔塔聊天正式 Release；第三方市场分发使用公开市场版本。依赖使用不以公开市场发布为前置条件，也不改变实现归属。

Total output lines: 12501

# CitizenChain 节点技术文档

## Ethereum接口依赖边界

公民链工作空间与QR协议消费同一固定SDK提交，依赖需求以产品Cargo声明与原始锁文件为唯一权威；SDK原件仅从该锁定Git提交物化，不读取开发工作树。传递依赖连接由产品原生Cargo锁固定，禁止消费时自动漂移。Tauri生成文件只允许写入CitizenChain本仓target内的规范绝对OUT_DIR；生成前核对OUT_DIR及已有tauri子目录的真实路径，拒绝源码、外部目录及链接路径。费用制度与原生账本单位保持原状。

正式NodeGuard启动入口只接受冻结的GENESIS_HASH与GENESIS_STATE_ROOT；客户端创世哈希、块0索引、实际区块头高度、状态根及区块头重算哈希必须全部一致，缺失或读取失败均拒绝启动。临时测试链通过仅cfg(test)编译的显式入口装配，信任身份从声明的创世storage独立计算，且代码必须逐字匹配本次内置WASM；不从数据库返回值反向生成信任值。测试与正式入口共用完整身份核验及后续宪法、NodeGuard、PoW导入规则，正式构建没有可替换主网身份的配置、环境开关或测试入口。

公民链工作空间新增`pallet-revive`编译依赖，与现有SDK固定为同一Git来源和提交
`ac4a17f99d39e6e67b47a9e809351a763fe789f0`；Node继续通过现有Runtime路径依赖
消费其feature。第6步获批源码已新增官方`pallet-revive-eth-rpc`依赖和节点内接入；
固定消费与本机离线开发验收已完成：完整Runtime、真实WASM、330项默认Node回归及默认二进制构建均通过；生产证书部署、正式链升级及MetaMask实际连接归第8步。

第6步正式依赖补充已在SDK与公民链根工作空间声明同一radium0.7.0固定Git补丁，来源为`https://github.com/paritytech/radium-0.7-fork.git`，提交为`a5da15a15c90fd169d661d206cf0db592487f52b`。Cargo不继承Git依赖仓根patch，实际消费工作空间各自声明。公民链已按准确差异授权写入新固定SDK消费及完整RPC闭包，根锁1844包、SDK328包；第一方futures-timer由3.0.4统一锁为3.0.3，仍符合原version="3.0.2"范围，使用已登记的完整WASM依赖，不补下载。iana-time-zone0.1.65继续连接windows-core0.61.2，num_enum_derive0.7.6继续连接proc-macro-crate2.0.0，未接受额外重选。QR当前实际锁没有radium或SDK包，不添加未使用的补丁。根、QR及真实WASM夹具的原生锁验证已通过；本轮Runtime64项与原生收费/签名112项库测试及5项金标集成测试全部通过，benchmark和try-runtime检查通过。真实WASM执行4项已通过，覆盖实际导出上的标准预编译/非法输入/签名单次费用及既有部署/回滚；完整默认Node330项回归、默认二进制构建及真实HTTPS/WSS/WASM网络验收均已通过，包含分叉、最终链与同库重启；本轮只完成本机离线开发验收，生产证书部署、正式链升级及MetaMask实际连接归第8步。

已验收SDK经本机独立保存为`ac4a17f99d39e6e67b47a9e809351a763fe789f0`，源码与通过的验收快照完全一致。产品根与QR已统一正式消费该40位提交，正式锁及两声明回读摘要与获准候选一致。离线原件只由已保存提交物化，唯一依赖库中的旧SDK来源/174坐标已替换为新来源/328坐标；17仓83份正式Cargo文件无旧提交消费、索引零引用及新bundle包含旧提交历史均核对后，仅删除旧SDK原件。产品原生锁验证、完整Runtime64项、真实WASM执行4项及完整默认Node330项均通过；默认Node二进制构建和真实HTTPS/WSS分叉、最终链及同库重启验收通过。结果限定为本机离线开发验收，不代表远端CI、正式链升级或生产部署。

已选定公民链Ethereum ChainId为`2027`（十六进制`0x7eb`），后续RPC必须返回与
Runtime签名验证一致的编号。当前chain-spec中的`ss58Format: 2027`仍只表示SS58
地址格式，不作为已经提供`eth_chainId`的证据；依赖接入不代表MetaMask或兑换服务可用。

### Ethereum RPC与TLS

MetaMask本机联调使用node/src/core/ethereum_rpc.rs的cfg(test)三节点会话，不给正式节点增加可替换冻结创世身份的启动参数。会话以当前源码WASM和独立RocksDB运行；签名交易先经HTTPS进入同一交易池，测试出块只提取ready交易、生成合法PoW seal并经过现有NodeGuard/ConstitutionGuard，TLS P2P同步后由官方GRANDPA voter/observer完成最终确认。空交易池不产生空块。

显式会话参数CITIZENCHAIN_TEST_SESSION_ROOT必须是本产品根target内的全新、规范生成目录，按本产品真实平台和流程隔离，父目录不得经过符号链接；调用方负责工作目录及TMPDIR，默认会话也只使用已交付到该边界内的TMPDIR。显式会话必须交付CITIZENCHAIN_TEST_RPC_PORT，端口占用即失败；普通自动化和其余节点各用回环随机端口。普通夹具的测试RPC证书私钥仅存内存，只导出公开DER证书；服务器显式会话原位使用交付的可信证书及DER私钥，不更改系统或浏览器信任。隔离页面通过完整内容核对后，其准确HTTPS Origin成为该会话唯一网页白名单。公开端继续复用Safe与DenyUnsafe::Yes处理。两个公开secp256k1测试向量由官方Account::substrate_account映射，各在测试创世分配10000 GMB（1000000原生分）；Ethereum余额仍以18位兼容表达、按10^16比例换算，原生精度和费用制度不变。准备过程的双向测试转账真实执行并核对一次FeePaid、余额、nonce、回执及三节点最终状态，输出ready.json记录操作后的实际余额，不能当作尚未扣费的创世余额。

会话同步直接读取每个节点System账户的原生总余额，并经HTTPS读取Ethereum可用余额；官方evm_balance不包含111分账户保留金。每个账户创世总余额10000 GMB，对应初始Ethereum可用余额9998.89 GMB。ready.json的balance_fen记录原生总余额，ethereum_balance_fen记录扣除保留金后的可用余额，existential_deposit_fen明确保留金额；两者都按真实转账和FeePaid回读，不能把保留金当成交易费用。

同一会话从源码WASM执行一次创世构造，核对storage的:code逐字节等于本轮内置WASM，再让三个独立数据库导入相同的不可变创世storage；每个节点继续独立计算预期创世身份并执行WASM与导入守卫。Ethereum区块哈希与原生头哈希分别处理：先验证回执高度与同高度Ethereum区块哈希，再通过原生HeaderBackend读取对应区块费用事件。ready.json的finalized_hash记录原生最终头，ethereum_finalized_hash记录同高度Ethereum最终块，禁止互换用于查询。

CITIZENCHAIN_TEST_SESSION_SECONDS限定就绪后保留1至1800秒，默认自动化回归真实执行会话后保留1秒，没有新增忽略测试。调用方可在会话目录创建普通stop文件请求结束；符号链接停止文件拒绝。正常结束关闭出块、RPC及三节点任务，并验证原RPC端口可重新绑定；失败退出同样取消持有的RPC和出块任务。本机开发结束由调用方停止全部实际进程、确认数据库已释放后清理本轮临时产物。三节点业务逻辑已真实通过：签名交易池、PoW、三节点GRANDPA、双向转账及每笔单次10分扣费、HTTPS证书/主机名、余额/nonce/回执和受控停机均验证。工作目录规则更新后的target约束及node/build.rs生成目录检查已同步；本次目录调整按用户指令只核对准确差异，不重复编译或三节点完整验收，已有运行态结果及其源码归属保留在任务卡。浏览器信任、MetaMask实际操作和正式链仍待验收。

严格原生模式继续由现有 Runtime 路由单次收取业务费。SDK费用实现已准确保存，当前三份清单与锁统一固定消费 134a87024fbd9edb5fd5fc22d7ae2ba71090fb46；此前 eth_gasPrice 为零且回执乘积与 FeePaid 不一致的真实结果只属于旧消费 add12c738a8510cb2253e4a98393e8955286e2b6。固定非零兼容价格（公民链 1 gwei）、同时覆盖资源和费用的估算、签名限额校验及精确回执乘积已通过 SDK 完整 Revive 与 Ethereum RPC 库测试。旧固定提交 4fbac6450231e8fe4bf660a3a8f6eb2b4e28c306 的历史验收：2026年10月7日完整 Runtime 库66项全部通过，真实源码 WASM 的 Node Ethereum RPC 模块5项全部通过，均零失败、零忽略；节点仅使用 std 服务特性，327项其他节点测试未在本轮运行，不代表默认桌面包验收。MetaMask 实际连接、发起转账、确认页和完成后费用显示仍须独立真实验收，准确证据记在唯一任务卡。

上一轮实际钱包连接恢复后，公开测试账户的四笔交易全部失败，链端未入块、未扣费。新固定消费本轮完整Runtime库72项全部通过；新Node三节点基础四档费用验收通过，本轮固定消费完整服务模块5项已实际全部通过，零失败/零忽略；327项无关节点测试未运行。Cloudflare临时HTTPS回源严格校验证书，RPC/页面/图标200、OPTIONS204及单一CORS通过，生产三服务原PID/启动时间不变。本轮MetaMask先发1 GMB已成功：真实type2签名nonce2、gas101600004、maxFee与maxPriority均1 gwei，三个节点最终确认到6号块，单次FeePaid10分、回执gasUsed100000000×effectiveGasPrice1 gwei=0.10 GMB，发送可用余额10996.57 GMB。其余104.99/105/1000三档与确认窗口显示、钱包native图标仍待验收，不能据第一笔宣称全部功能完成。
真实报价已复现旧固定 SDK 拒绝任何非零优先费；没有取得钱包底层错误，尚不能断言为唯一原因。
费用参数修复后的三节点用例必须以相同非零优先费完成真实报价和签名，Legacy 使用价格缓冲，
继续核对原生 FeePaid 单次收费、余额、nonce、回执精确乘积与三节点最终一致性。
当前 SDK 统一消费上述新固定提交；SDK 源码验收通过，固定消费后的完整 Runtime、真实三节点、实际钱包重试及失败边界通过后，
才能确认转账功能完成。此改动不改变生产 Runtime 升级与 Node 身份守卫的既定顺序。

既有三节点会话测试使用真实 eth_gasPrice、eth_maxPriorityFeePerGas 和 eth_estimateGas 构造 Legacy/EIP-1559 签名，真实完成 1、104.99、105、1000 GMB 转账，业务费分别为 0.10、0.10、0.11、1.00 GMB。gas 缓冲及价格上限未改变该费用；逐笔核对单次 FeePaid、付款者、转账余额、nonce、区块基价与精确回执乘积，三个独立数据库由官方 GRANDPA 最终确认到第5块，费用历史零优先费和无效金额/价格报价拒绝均通过。就绪记录保存四笔交易、每账户两次 nonce，累计收费分别为21分和110分；受控停机及端口释放通过。另一服务回归同时通过真实合约、分叉日志/回执、迟到第三节点、最终链和同库重启。完整5项测试耗时956.81秒，不表示 MetaMask 页面已经通过。

普通MetaMask接入的唯一源码为node/frontend/metamask.mjs，正式页面定义位于文件前半部，测试位于后半部。直接执行node node/frontend/metamask.mjs向标准输出生成HTML；普通导入只导出installHtml，不输出页面或登记测试；node --test node/frontend/metamask.mjs执行同文件测试。原snap目录已删除，源码不保留第二份HTML，费用Snap源码、清单和npm包已取消。用户只需点击添加并切换公民链，分别确认wallet_addEthereumChain和wallet_switchEthereumChain；页面回读eth_chainId为0x7eb后才显示已切换。标准网络名称公民链，链ID2027，公民币GMB、EVM精度18；转账费率永久0.1%，最低0.10GMB，按整数分四舍五入。接入页直接展示上述制度，MetaMask原生估算读取真实eth_estimateGas与gas报价，可能包含钱包缓冲；实际收费以FeePaid及回执gasUsed×effectiveGasPrice核对，不安装插件或另改MetaMask界面。

桌面浏览器扩展与手机App共用同一接入页及添加/切换合同。桌面使用已安装MetaMask扩展的浏览器，手机从MetaMask App的“探索”内置浏览器打开同一HTTPS页面；两端需要分别添加网络。页面优先发现EIP-6963中rdns为io.metamask的钱包，也接受MetaMask注入的window.ethereum；每次点击读取当前提供者，允许加载后注入。未发现有效钱包时展示两端使用引导，不请求账户、网络或签名权限；取消添加、取消切换、无效回执及错误链号保留真实结果。当前25项页面测试实际通过，包括原有19项钱包合同、HTML生成与导入隔离，以及隔离视图绑定、双向域拒绝、模板漂移拒绝和受控输出。隔离HTML仅由后置测试入口生成，将三个准确RPC绑定点同时替换为https://metamask-test.crcfrcn.com并标明“隔离测试链”；正式CLI与导入仍固定永久RPC。CITIZENCHAIN_TEST_PAGE_OUTPUT仅在直接node --test入口有效，准确输出必须是当前调用方持有的本产品target/build或target/test中的metamask/install.html，核对PRODUCT_WORK_LEASE、活跃所有者及规范目录后以排他创建写入。正常测试不生成文件，实际生成、回读和调用方清场已验证；这些测试使用钱包替身，不能代替桌面与手机版余额、四档转账及费用的真实验收。钱包两端实机验收及图标官方收录保留为未执行事项；用户已明确本步仅以本机测试成功验收，两端实机和远端临时入口不作为本步完成条件。

MetaMask接入页的唯一源码为node/frontend/metamask.mjs，图标准确使用/icons/gmb.png，PNG真源为icons/gmb.png。节点构建脚本通过产品已交付的NODE执行唯一正式生成入口；远端remoteStep将正在运行该产品入口的Node规范路径交付为NODE，沿用现有官方工具版本，不再从PATH搜索；源码视图按仓库根的直接子项沿用原过滤规则逐项复制，避免Node拒绝根向其子目录复制，target与原有资源目录仍排除；输出仅进入Cargo OUT_DIR并嵌入节点，PNG直接嵌入；节点启动不读取静态目录。现有rpc_tls监听器在同一HTTPS端口交付GET/HEAD根页面与GET/HEAD固定图标，根POST、OPTIONS和WSS沿用现有RPC；未知路径404，不支持的资源方法405，HEAD正文为空。固定资源与RPC共用Host/CORS检查及TLS连接额度，不开放文件系统路径、目录列表或任意代理。根WebSocket升级优先于普通页面GET。CitizenServe仍从既有CHAIN_URL的origin读取两个固定资源，使用既有服务端Access身份、3秒总超时、128KiB实际字节上限及类型校验，不复制上游头、Cookie或错误；本轮不改变已验收的Worker静态交付代码。

对外链路固定为现有Cloudflare Tunnel→cloudflared→node回环HTTPS RPC。机构的证书、私钥路径、端口、Tunnel路由及HTTP Host均由部署环境提供，不进入通用节点源码。直接节点回源时HTTP Host使用对应127.0.0.1:端口以满足现有白名单，TLS SNI使用可信证书中的域名并严格核对CA，不增加网关静态目录。本轮源码WASM与Node编译、完整Node服务回归333项、页面25项、HTTPS专项3项及流程后置10项均实际通过；HTTPS专项包含在完整Node结果内，不重复计算用例数。本机显式18081三节点会话和外部HTTPS探针覆盖页面/PNG逐字节交付、POST/OPTIONS及Host/Origin边界，已正常停机释放端口。固定SDK的BasePath::new_temp_dir在同进程复用路径，本轮经准确确认仅在cfg(test)辅助函数用进程号/原子序号排他创建独立目录及BasePath::new，现有坏块回归核对两份配置路径互异；3项同进程并发服务用例及相同4线程完整333项已通过，保留KnownBad与不委派导入断言。用户最新明确本步以本机测试成功验收，服务器临时部署及两端钱包实机不作为本步完成条件；本机结果限定std服务构建，正式链升级及部署仍归后续独立确认步骤。本轮工具退出、结果消费后由所属产品生命周期清空target/build和target/test，根本身保留为空。

显式隔离三节点会话仍只存在于ethereum_rpc.rs的测试模块。CITIZENCHAIN_TEST_RPC_PORT设置1至65535的规范端口，普通自动化继续随机端口；CITIZENCHAIN_TEST_RPC_SERVER_NAME及CITIZENCHAIN_TEST_RPC_CA与既有RPC TLS证书/DER私钥变量一起交付服务器可信TLS，客户端只解析到回环并核对CA和证书名，HTTP Host使用回环地址。CITIZENCHAIN_TEST_WORK_ROOT仅允许部署方规范target/test，session限定metamask/session；CITIZENCHAIN_TEST_PAGE_INPUT只接受同一现场install.html并核对隔离标记及完整业务字节，仅允许该视图的准确HTTPS Origin。正式节点没有这些测试页面注入入口。每端钱包分别启动全新数据库会话，最长1800秒，最终签名由用户完成。

公民链图标资源统一位于 icons，原 node/resources 目录已移除。Tauri 桌面图标及 Windows 安装图标直接引用 ../icons，打包资源映射到安装包内的 icons/，本机开发入口使用同一资源目录；Logo 派生器的应用母版为 icons/logo.png，公民 App 的来源清单同步指向此路径。公民币专用图标由用户指定为 icons/gmb_019473.png，原图1254×1254、970817字节，移动后逐字节保持一致；不把此图替换为应用 Logo 母版。

对外网络资料官网固定为https://www.crcfrcn.com，永久公共RPC固定为https://nrcrpc.crcfrcn.com/，隧道重构不改变对外地址。2027资料已于2026-10-08T11:38:48Z合并至ethereum-lists/chains，PR为https://github.com/ethereum-lists/chains/pull/8828，合并提交1673f362779c138e407be2bdca90cf478d12f28f；原登记为incubating、空rpc及省略icon；本轮登记更新只将rpc补为["https://nrcrpc.crcfrcn.com/"]，保持incubating及其余链资料。提交、维护者合并与公开数据刷新分别回读，未合并前不记为登记更新完成；RPC真实可用及转账验收继续按既定生产顺序完成。图标为可选登记字段，省略该字段不需要IPFS；本方案不使用IPFS、Filebase或费用Snap。既有安装页与/icons/gmb.png统一通过现有Cloudflare Tunnel及HTTPS交付，不另建网站。网页图标不表示MetaMask内部原生币/网络图标已收录，需向钱包官方提供指定PNG及2027/GMB资料，按扩展端与手机端的实际取图来源分别核对官方收录和已安装版本显示；wallet_addEthereumChain的iconUrls参数不能当作已收录证据。当前生产节点仍是旧版，公共RPC尚未激活；实际钱包已完成添加/切换及首笔1GMB，剩余三档转账、确认费用和原生币图标仍待验收，2027登记已合并，MetaMask扩展及手机网络/GMB原生币图标申请https://github.com/MetaMask/metamask-extension/issues/46932仍为open、零评论，收录及用户已安装版本显示待验收。生产顺序固定为旧Node承载Runtime升级、链上实际创世身份与持续出块/最终性通过后，再补Node守卫并更新节点，最后完成全程Cloudflare TLS与公共域名激活；候选开发与本地测试不改变该顺序。 此前候选接入页15项、服务端164项及合成服务的真实HTTPS传输5项全部通过，零失败/忽略，TypeScript无诊断；这些结果不代表生产Tunnel、Nginx实际加载或MetaMask钱包已经验收。

第8步三节点合约同步验收在既有 node/src/core/ethereum_rpc.rs 服务级测试内扩展：三个独立 RocksDB、真实 TLS P2P、生产导入队列与源码 WASM；通过各自 HTTPS 核对代码、存储、余额、nonce、回执和日志，覆盖更重分叉、迟到节点追块、断开后同库重启及重放单次收费。此前实际服务回归2项通过、1项失败；更重分支同步后SDK遗漏祖先通知，日志查询保留旧分支记录，用例中途停止，第三节点追块及断开重启尚未执行。2026年10月6日用户确认执行本次方案后，SDK正式main的Revive RPC修复候选已写入：最佳块处理先沿父哈希收集并校验缺失祖先，确认父哈希、高度连续且在256块窗口内到达已索引父块或创世，再正序执行同高分叉清理与收据写入；全部成功后才推进最佳块并发布通知。每块旧分支删除、交易位置、日志及块映射使用同一SQLite事务，提交成功后才替换内存缓存；失败保留旧索引并允许重试。缺失父块、错误父链及超窗拒绝处理；测试覆盖跳过分叉祖先、重复通知、缺失及错误父链、窗口边界、创世、真实合约事件及删除/插入故障回滚。Node夹具只在cfg(test)创世配置使用公开Alice GRANDPA测试权威，先完成未最终化分叉，再启动官方voter/observer；分别等待各独立客户端的最终块哈希，并经HTTPS核对safe/finalized，继续覆盖迟到及同库重启节点。已移除手动finalize证明；正式Runtime、创世及Node生产实现不由该夹具改动。SDK修复源码已直接离线编译并通过完整RPC库52项测试，包含祖先边界、真实SQLite故障回滚及3条非空合约事件的完整元数据/重复通知断言；合约夹具及开发Runtime WASM均实际构建。Node新增GRANDPA夹具使用当前SDK的KeystoreContainer::keystore取得测试内存密钥库，继续使用公开Alice权威。节点服务验收使用显式std特性，custom-protocol仅涉及桌面资产嵌入，其默认桌面包准备尚未通过，不将服务测试代替桌面验收。首轮服务测试因遗漏WASM_BUILD_FROM_SOURCE在启动前失败，补齐原build.rs要求的环境并使用产品原config.toml后，源码WASM及节点服务重编译通过。最终真实服务回归2项通过、1项失败，耗时427.26秒：两个独立节点经TLS P2P导入更重分叉，HTTPS的eth_getLogs逐项比较发现跟随节点保留旧分支0x2日志，源节点只返回规范0x3日志；用例在此停止，GRANDPA启动、迟到第三节点及同库重启未执行。上轮失败测试的产品消费为ac4a17f99d39e6e67b47a9e809351a763fe789f0，旧BestBlocks实现只处理通知尖端，尚未包含本次已通过52项回归的SDK修复。SDK修复现已本地保存为add12c738a8510cb2253e4a98393e8955286e2b6，保存内容与已通过52项回归源码逐字一致，尚未推送。用户第二次准确确认后，产品三份声明/锁已落实add12c738a8510cb2253e4a98393e8955286e2b6，唯一依赖库SDK原件和328包来源同步替换，版本、checksum及非SDK连接不变。新固定消费源码WASM/Node服务编译通过，真实Ethereum服务3项全部通过，耗时943.77秒、退出0：跨节点分叉日志/回执、官方GRANDPA最终性、safe/finalized、迟到第三节点、原库重启及真实单次收费均完成。结果仅覆盖这3项服务测试，默认桌面包、移动正式页面、MetaMask及正式链升级仍待验收；SDK本地保存未推送。
第8步升级预检固定核对 SDK bundle 内 chain/manifest.json、chainspec.json 与 light_sync_state.json：9项摘要、创世身份、状态根、协议和币种属性回读一致；该 light-sync 检查点仍为创世块。冻结链内嵌旧 WASM 与开发候选均声明 spec_version/transaction_version 为0，但二者内容不同，这些静态值不能替代已部署 finalized RuntimeVersion。正式升级只能通过既有受保护 HTTPS 读取器及操作绑定的 Keychain/Touch ID，按 finalized spec_version+1 构建已验真 Release；公网 bootstrap 明确不公开 RPC，MetaMask需另行确认测试端点与受信证书。


第6步唯一结构为一个Node、一个原生客户端和交易池、一个回环RPC端点。官方
Ethereum RPC通过进程内RpcModule调用既有RpcHandlers，不连接内部网络，不启动
独立eth-rpc进程或8545端口。Ethereum方法与原生方法合并到原9944端点，只接受
HTTPS/WSS。公开模块和每个请求均显式使用DenyUnsafe::Yes；原生内部模块不对外开放。

CLI和桌面均由service::new_full读取两项运行环境配置：
`CITIZENCHAIN_RPC_TLS_CERTIFICATE`为可信证书链PEM绝对路径，
`CITIZENCHAIN_RPC_TLS_PRIVATE_KEY`为对应PKCS8私钥DER绝对路径。材料须由获准的
密钥系统提供，不进入Git、文档、日志或链状态。缺失、无效、不匹配或相对路径配置
拒绝启动，不生成RPC自签证书、不复用P2P证书、不开放明文监听。HTTPS/WSS客户端
仍必须信任签发CA并校验主机名；生产证书部署和正式发布归后续升级步骤。

既有回环端点、请求/响应大小、连接数、订阅数、消息缓冲、batch、CORS与限流配置
继续生效。TLS握手最多10秒，握手连接也计入连接上限；仅信任真实连接IP进行限流，
启用代理头限流配置时明确拒绝启动。HTTP/WSS Host只允许对应回环地址和localhost。明文连接仅可被关闭或收到完整致命TLS告警后断连，验收拒绝任何HTTP/JSON明文响应。配置的Origin白名单同时在HTTP请求和WSS握手核对，未许可来源返回403；允许来源的浏览器预检可使用JSON content-type。限流额度在端点连接之间共享，重连不能重置计数。
公开RPC只接受用户已经签名的Ethereum交易，Ethereum账户列表为空，不生成、托管
或解锁用户Ethereum密钥。既有本机钱包授权与节点守卫继续独立执行。

近期索引固定保留256块，在启动时按现有链状态重建；链状态已修剪而无法重建时
启动失败，不改变节点修剪或扩充历史存储策略。重组删除被替换块及全部已索引后代，
即使通知中间存在缺口也不保留旧分叉。按交易哈希读取回执再次校验规范链。
latest/pending状态读取最佳块，safe/finalized读取GRANDPA最终块；newHeads订阅最佳块，
logs订阅只发布最终块日志。eth_subscribe使用标准newHeads和logs参数，由同一官方EthRpcServerImpl处理器接受、通知并取消；Node直接绑定官方SubscriptionKind，未知类型及多余参数拒绝，不另建发布任务。pending nonce通过原生system_accountNextIndex读取同一
System nonce及交易池依赖标签，不另建Ethereum nonce账本。pending地址先通过进程内state_getStorage读取官方Revive OriginalAccount（Identity键）的已有映射；仅存储为空时使用官方H160派生账户，损坏编码或读取失败必须拒绝。宿主线程不调用需要Externalities的Runtime存储函数。

进程内传输直接采用官方JSON-RPC Response逐字段解析，保留null、原始大数及错误data，
缺失结果、重复字段及result/error同时出现均拒绝。订阅退出会注销原生服务端订阅。

Ethereum transactionIndex从0连续计算Ethereum交易；logIndex从0连续计算块内成功
Ethereum日志；cumulativeGasUsed累计本块Ethereum执行资源，失败部署contractAddress
为null。单笔回执按Ethereum序号重建整个块以保留累计值。日志按块、交易、日志序号
排序；超出保留范围或超过10000项明确报错，不能返回伪装完整的部分结果。
裁剪后只给toBlock的隐含完整历史查询拒绝，须明确提供仍被保留的fromBlock；完整
历史包含创世块时继续支持原有查询。按blockHash查询须确认哈希仍在近期索引中，
已裁剪或未知哈希明确失败；被索引但没有日志的块正常返回空数组。
SDK内权重查询和执行追踪按已签名payload哈希重新绑定原生extrinsic位置，不能直接
把Ethereum transactionIndex当作原生执行序号；Node公开模块仍仅合并官方Ethereum方法。
gas、gasPrice和effectiveGasPrice是兼容执行资源表达，不能替代FeePaid真实GMB费用，
也不承诺gasUsed乘gasPrice等于实际扣款。新合约存在性存款111分由付款人真实资金转入，属于原生资金转移，与FeePaid手续费分别核验；费用与80/10/10分账保持原实现。

第6步本机离线开发验收已完成。SDK最终完整RPC45项、执行器155项、完整workspace、Revive668项（1项上游既有忽略）及no_std检查通过；公民链完整Runtime64项、原生收费/签名117项和真实WASM执行4项全部通过，benchmark与try-runtime检查通过。完整默认Node330项全部通过、0失败/忽略/过滤，默认二进制构建通过。真实HTTPS/WSS/WASM验证签名部署/调用/模拟回滚、共享pending nonce、真实ED与单次FeePaid、成功/失败回执、混合交易序号和累计资源、同高重组清理、标准newHeads与最终日志、safe/finalized、重复交易拒绝及释放数据库后的同库重建；TLS证书、Origin、Safe、Host及明文拒绝边界均通过。完整回归保留实际WASM策略、发行、NodeGuard、PoW及真实P2P坏块测试。

宿主映射上下文、TLS错误所有权、致命TLS告警断连及标准订阅参数问题已在获准Node接入与测试范围修复；正式Runtime费用、固定SDK消费及四份冻结流程未扩大。旧SDK零引用原件、旧检出和失败夹具数据库已清理，本轮生成的linker目标缓存也已清理；必要编译产物与成功/失败验收证据保留。本轮没有推送、正式链升级或生产部署，MetaMask真实连接和多节点验收仍归第8步。

2026年10月1日依赖接入验收：Runtime源码WASM与原生回归通过，Node默认构建的
受控前端准备在TypeScript检查处失败，准确位置为`node/frontend/vite.config.ts:8`。
现有配置函数返回对象的`publicDir: false`被推断为`boolean`，不满足Vite的`UserConfig`；
经扩大范围二次确认，已补充配置函数的`UserConfig`返回类型，保留现有构建选项。
修复后前端类型检查、生产构建和本地文档测试通过；默认Node release构建通过。
Node核心测试的首次构建还因`libclang.dylib`加载失败停止；已核对本机Xcode为登记版本
`27.0`，测试显式引用其现有动态库后编译通过。150项核心测试首轮149项通过、1项
因RocksDB目录锁失败，该失败项单独串行复验通过，不能把首轮结果记为全套通过。
当前WASM策略探测、创世状态检查和真实P2P坏块拒绝均实际执行并通过，没有跳过。
重试只调整当前测试进程环境，不修改正式流程，也不替代默认Node构建或正式安装包验收。

扩大范围修复后的最新一轮使用默认Node feature和本轮源码WASM串行执行，150项核心
回归全部通过、0失败、0忽略；上一轮目录锁错误未重现。当前WASM策略、创世状态、
发行与费用守卫、真实P2P坏块拒绝均执行通过。此轮默认构建已通过，但benchmark
WASM此前被Runtime SquarePost基准缺失no_std导入阻碍。用户后续明确修复指令后，两项
基准问题均已修复：生产与benchmark源码WASM、两种Runtime配置回归通过；benchmark
Node默认feature构建通过，当前源码fresh spec导出成功，5条身份基准CLI实际执行通过。
CLI采用2 steps / 1 repeat验证执行路径，生成文件只写任务缓存，不改正式权重。
本轮生产WASM另通过节点真实候选Runtime策略探测；该测试没有跳过。依赖接入第一步
已完成开发验收，第二步仅进行金额适配只读设计；生产候选与基准候选分别保存。
正式安装、生产启动、升级和发布没有执行，不将隔离测试记作生产操作。

受控缓存固定为 `citizenchain/target/<platform>/<build|ci|release|publish>/`；四个流程目录永久独立，启动不建目录。macOS Build 的三个 Node 工程均在 `build/source-view/` 只读引用源码并把各自 `node_modules` 安装在该视图，Cargo、Tauri、前端输出、日志和候选也只写 `build/`。

远端自动化的官方 protoc 35.0 来源、SHA-256、解包和执行器核验只归 `.github/workflows/release-wasm.mjs`；各平台 Workflow 在自己的任务现场调用该自动化入口。Build 不准备自动化 protoc，也不读取门禁配置。本机开发必须显式提供已经验真的 `PROTOC` 路径，缺件直接失败。

本文是 CitizenChain 节点唯一技术事实文档，统一收录区块链总览、桌面节点、网络、挖矿、设置与节点安全边界。

## 受控源码归属

`node/vendor/`、`node/libp2p/` 内经来源清单核实的上游原有结构，必须保留官方加载所需目录、版权、许可证与清单必需文档。该保留边界不得用于第一方改写、新增包装层或可删除的原件与生成物；来源与自有差异必须分别核实，不因目录名自动认定所有权。

节点与OnChina的reqwest使用受控依赖库唯一固定版本，共用原始`citizenchain/Cargo.lock`，由Cargo解析更新，不手工改锁。节点的HTTPS请求按实际调用启用blocking、json、query、rustls；query是查询参数调用所需的显式功能。锁维护只写该锁及已批准的消费者清单，源码Runtime不参与写入。Polkadot SDK仍使用原锁固定Git仓库与提交，取源由受控唯一原件提供；锁解析通过不代表节点编译或网络运行验收通过。

节点入口、工具准备与运行失败条件由本仓scripts及声明拥有；对应回归归本仓`scripts/flow.test.mjs`和既有真实测试。门禁只使用本仓准确提交、测试清单和所属技术文档，不读取外部规则、证据区间或任务库；文档同步不等于真实运行验收通过。

### citizenchain/crates 技术说明

`citizenchain/crates/` 只放依赖 CitizenChain runtime/primitives、链交易结构或链测试环境
的内部 Rust crate。跨 CitizenChain、CitizenApp、CitizenWallet 等产品的共享实现统一放在
仓库根 `shared/`，不得以“Rust crate”为理由塞回本目录。

| crate | 用途 |
|---|---|
| `blockchain-harness` | 链行为夹具与篡改用例（导出块、构造异常 state root 供守卫测试） |
| `chain-signing` | Rust host 端链交易签名材料真源，直接依赖 CitizenChain runtime/primitives |

仓库级共享 Rust crate：

| crate | 唯一路径 | 用途 |
|---|---|---|
| `qr-protocol` | `citizenchain/crates/protocol/` | QR_V1 协议、registry、生成器、金标夹具和跨端守卫 |
| `citizen-signer` | `shared/citizen-signer/` | CitizenApp、CitizenWallet 共用 sr25519 派生与签名源码 |

#### 格式与静态检查

两个内部 crate 与 runtime/node/onchina 同属 CitizenChain workspace。两个仓库级共享 crate
属于根 Rust workspace。CI 的「公民链全工程验证」必须分别对两个 workspace 执行格式、
Clippy 和测试，禁止只跑 `citizenchain --workspace` 后误报共享模块已通过。

- 两个 workspace 提交前都必须执行 `cargo fmt --all -- --check`，不能只格式化本次文件：
  局部格式化会把其它文件的既有偏差留到下一次提交，届时 CI 报的是「与本次改动无关的文件」，
  排查方向容易被带偏。
- 两个 workspace 的 `cargo clippy --workspace --all-targets --locked -- -D warnings` 都必须通过。

#### 金标夹具

`citizenchain/crates/protocol/tests/golden_fixtures.rs` 与 `repo_guard.rs` 是 QR 协议的跨端锁步夹具；
改动 QR 协议字段序时，它们与四端（onchina、citizenapp、citizenwallet、node）必须同改，
另有 `.github/scripts/repository/ci-repository.mjs golden-vectors` 在 CI 侧校验真源与各端镜像一致。

#### 仓库守卫的错误处理与 Clippy

CI 用 `cargo clippy --workspace --all-targets --locked -- -D warnings`，`expect_used` /
`unwrap_used` 在测试目标里同样是错误。`repo_guard.rs` 的仓库根解析和六个测试返回
`Result`：文件读取与 JSON 解析错误保留中文上下文并向测试框架传播，路径或归档缺项
用 `ok_or` 返回错误。既有合同断言全部保留，读取失败不会变为测试成功。
该文件不增加 Clippy 豁免；验证必须包含原严格 Clippy 命令和真实仓库守卫测试。
仓库扫描以 Git 跟踪文件为范围，Git 清单或跟踪源码读取失败必须使测试失败，本机被忽略的
Pods、Flutter 等生成物不作为产品源码。QR 生成物一致性继续验证钱包及 Node/OnChina；
CitizenApp 已消费 SDK 公共接口，改为验证公共 QR 导出、调用入口及旧宿主实现不存在。

---

#### 单安装包(三平台零依赖)

Tauri 打包(dmg/nsis/deb)随包"五件套",装好即用、无外部依赖:

| 件 | 位置 | 运行期 |
|----|------|--------|
| node 矿工端 | 主程序 | 桌面=节点运维台,默认全核挖矿 |
| OnChina 二进制 | `resources/onchina-bin/onchina` | 节点设置页二次确认后由 `onchina_proc` 拉起 |
| PostgreSQL 官方二进制 | `resources/postgres/<os>/`(bin/lib/share) | OnChina **自管**内嵌私有实例 |
| OnChina 前端产物 | `resources/onchina-frontend/dist` | OnChina 同源托管(`ONCHINA_FRONTEND_DIST`) |
| china.sqlite | `resources/china.sqlite` | 行政区只读单源(`ONCHINA_CHINA_DB`) |

打包流程:`citizenchain/scripts/prepack.{sh,ps1}` 组装(build onchina+前端、拷 china.sqlite、把官方 PG 二进制 `CITIZENCHAIN_PG_DIST` 拷进 resources)→ 在 `node/` 跑 `npm run tauri build`。
PG 官方二进制来源:https://www.postgresql.org/download/(解压后含 bin/lib/share)。

- 外部调用方 本机 macOS 入口使用仓库锁定的 Tauri CLI。`run.sh` 必须通过动态 `--config "$tauri_override"` 注入当前 `frontendDist`、节点资源、OnChina 前端和 `china.sqlite`，再按 `build --no-bundle --ci -- --locked`、`bundle --bundles app --ci` 的顺序完成编译与签名封装；仓库依赖门禁和扫码权限测试共同锁定这一完整命令，禁止退回不含资源覆盖的短命令。

#### 进程编排(设置页手动拉起,OnChina 自管 PG/TLS)

- 节点 `desktop` setup 不启动 OnChina；用户在设置页“链上中国平台”行点击“启动”并二次确认后，`start_onchina_platform` 调用 `onchina_proc::start_onchina(app)`。
- `onchina_proc` 用 env 把资源/数据路径告诉 OnChina(`ONCHINA_PG_BIN_DIR`/`ONCHINA_PG_DATA_DIR`/`ONCHINA_PG_PORT`/`ONCHINA_TLS_DIR`/`ONCHINA_PG_WAL_ARCHIVE_DIR`/`ONCHINA_FRONTEND_DIST`/`ONCHINA_CHINA_DB`/`ONCHAIN_WS_URL`/`ONCHINA_EMBEDDED_PG=1`/`ONCHINA_ENABLE_TLS=1`)。
- OnChina 启动:`embedded_pg::ensure_started()`(首启 initdb→起 postgres@127.0.0.1:私有端口→建 onchina 库→自拼 DATABASE_URL)→ schema 幂等建 → `tls`(机构私有 CA 签发 HTTPS,主机 `onchina.local`)→ 服务。
- 退出:node 停子进程信号 → OnChina 收 SIGTERM/Ctrl-C → `embedded_pg::stop()` → 退出。node **不碰 PG**。
- 开发期(无随包 PG)继承 `run.sh` / `clean-run.sh` 注入的外部 PostgreSQL 二进制、数据目录、前端产物和 HTTPS 配置。

#### 内网 TLS + 扫码鉴权

- OnChina 内网 API 固定入口为 `https://onchina.local:8964`，服务监听 `0.0.0.0:8964` 并通过 mDNS 广告 `onchina.local`。
- OnChina 内网 API 走 HTTPS（机构私有 CA 签发，证书持久化 `ONCHINA_TLS_DIR`）。根 CA 只允许在证书和私钥同时不存在的首次启动生成，有效期到 2036-01-01；重启、升级、配置标识变化均不得覆盖根 CA。根证书或根私钥缺失、损坏、不匹配、用途错误或失效时必须失败关闭，禁止自动补建。`onchina.local` 服务证书有效期不超过 397 天，有效时直接复用，仅在缺失、损坏、域名或签发关系不符、进入到期前 30 天时使用现有根 CA 重签。
- 身份认证 = 扫码签名(3b 链上 Active 管理员集合鉴权),TLS 只负责传输加密。与 node 的 libp2p WSS 证书相互独立。

#### 大市机房形态(如香港:800万公民/500万公司/百管理员)

- 机房服务器 + RAID/NAS(数十 TB:法人照片、档案材料)+ UPS。
- 数据库两选:① 内嵌私有 PG(`ONCHINA_EMBEDDED_PG=1`,OnChina 自管);② 外部托管 PG(关 `ONCHINA_EMBEDDED_PG`,直接给 `DATABASE_URL`)。
- **WAL 归档**:内嵌 PG 配置 `ONCHINA_PG_WAL_ARCHIVE_DIR` 后持续归档 WAL 到指定目录；本仓不再提供全量备份与 PITR 恢复命令。外部托管 PG 的备份与恢复由数据库运维流程负责。
- 联邦节点**按省管理**:每市自治节点跑自己的 OnChina+PG;联邦注册局按省给市配管理员(链上,3a/3b)。
- **联邦注册局（FRG）每节点单省部署**：本节点所辖省由首次 active admin 的链上省专员岗位任职确定。管理员钱包从 `PublicAdmins::AdminAccounts` 读取，省域从 `PublicManage::InstitutionRoleAssignments` 的 `PROVINCE_COMMISSIONER_<省码>` 读取。
  本地省组投影表、虚拟省组 storage 和 `seed-federal-admins` CLI 均已退役；FRG 节点不要求安装前配置省名，未绑定时由冷钱包管理员登录后确认其有效任职省域。

#### 约束(已遵守)

- 三平台桌面端零依赖([[project_installer_zero_dep_2026_05_05]]);chainspec 创世后冻结、升级走 setCode([[feedback_chainspec_frozen]]);桌面=矿工端全核挖矿不动([[feedback_desktop_is_miner]])。

---

#### 1. 文档目的
- 固化 `citizenchain` 当前产品级技术基线，作为开发、联调、测试、运维、打包发布的统一参考。
- 说明 `citizenchain` 完整独立仓的定位，以及与 `CID`、`citizenapp` 的边界。
- 建立产品技术文档与模块技术文档之间的映射关系，避免后续只维护模块文档、不维护产品全局口径。

##### 2.1 技术文档三层结构
- 仓库技术文档：`<本仓根>/CitizenChainOnChina.md`
- 产品技术文档：`<本仓根>/CitizenChainNode.md`
- 模块技术文档：位于 `docs/citizenchain/`，描述单模块需求与实现细节。

##### 2.2 本文范围内
- `node/`：区块链节点原生程序、桌面节点 UI、内嵌节点管理与打包入口。
- `runtime/`：链上运行时与统一状态机。
- `runtime/governance/`：治理类 pallet。
- `runtime/admins/`：管理员类 pallet。
- `runtime/private/`：私权类 pallet。
- `runtime/issuance/`：发行类 pallet。
- `runtime/transaction/`：交易与手续费类 pallet。
- `runtime/misc/`：其他链上基础能力 pallet。
- `runtime/primitives/`：运行时共享常量、基础类型与制度数据。

##### 2.3 本文范围外
- `CID` 的链外网站、签名服务与数据库内部实现。
- `citizenapp` 的移动端 UI、钱包与登录实现细节。
- `citizensdk` 的轻节点、热钱包、移动硬件金库与 Dart/Flutter 公共接口实现细节；其独立
  产品文档为 `../citizensdk/CitizenSDK.md`。
- 仓库级 CI/CD、安装器流水线、工具库、白皮书与宣传性文档。

##### 3.1 产品定位
- `citizenchain` 是 `crcfrcn/citizenchain` 完整仓的主权区块链产品，负责链上状态、共识、治理、发行、交易结算与节点运行。
- 原生链名称为 `CitizenChain`，原生数字货币为 `GMB`。
- 产品作为一个安装包交付，包含三类核心能力：
  - 区块链节点程序：`node/src/service.rs`、`node/src/command.rs` 等原生节点模块。
  - 链上状态机：`runtime/` 编译出的 wasm 与所有 pallet。
  - 链上中国平台：`onchina/` 多机构工作台，由节点桌面端按需拉起。
- 桌面节点软件由 `node/src/desktop.rs`、`node/src/<功能名>` 与 `node/frontend` 提供本地节点运维、设置和打包入口。

##### 3.2 对外协作边界
- 对 `CID`：提供绑定、资格校验、人口快照、投票凭证等链侧接口承载能力。
- 对 `citizenapp`：提供链上账户、交易、治理、节点状态、奖励与网络可观测能力；CitizenApp 默认通过内置 smoldot 轻节点连接 P2P 网络并验证 finalized 链状态，不把公网 HTTP API 当作链上真源。
- 对 `citizensdk`：提供相同 CitizenChain runtime、chain spec、P2P 和交易协议。SDK 在设备
  内运行收编的 smoldot PoW 轻节点，供其它 App 使用公民链钱包和交易；它不是第二条链，
  也不改变 CitizenChain 节点、runtime 或现有 CitizenApp。
- 对 Cloudflare 边缘层：首期由国储会权威引导节点通过本机 RPC + 受控 Tunnel 提供链事件投影和已签名交易广播能力；Cloudflare 不运行 Substrate 节点，不保存用户私钥，也不获得公网 RPC。

##### 3.3 账户标识目标契约

- runtime 账户类型统一为 `AccountId`；单一账户字段统一为 `account_id`，具有业务角色的第二个及后续账户使用 `<role>_account_id`。
- `AccountId` 是链上账户身份和权限比较值；签名公钥使用 `public_key` / `signer_public_key`；SS58 仅以 `ss58_address` 作为派生展示值。
- 机构岗位授权必须同时验证 `cid_number + role_code + account_id`；公民身份必须同时验证 `cid_number + account_id`。命名统一不改变管理员名册、岗位、CID 或投票引擎职责边界。
- 跨 RPC/JSON 的 32 字节账户和公钥统一编码为小写 `0x` 加 64 位十六进制；runtime 内部继续使用强类型和原始 32 字节值。
- Node 与桌面前端的账户输入边界严格执行 `^0x[0-9a-f]{64}$`，不接受无前缀、大写或混合大小写文本；签名路径先校验 `signer_public_key`，再派生并比较 `signer_account_id`。
- Node 本地缓存、桌面命令和私有 RPC 只保存、传递 `account_id`；SS58 输入在边界解析为账户 ID，输出时可另行派生 `ss58_address`，不得把 SS58 当作授权或缓存主键。
- 挖矿奖励账户私有 RPC 固定为 `reward_bindAccount` / `reward_rebindAccount`，本地非密钥配置固定为 `reward-account.json`。旧 RPC、旧 JSON 和兼容读取均已删除。
- 完整目标与无兼容实施顺序见 ADR-040 和任务卡 `账户官方统一.md`。当前旧字段只描述实施前代码，不得用于新增实现。

#### 4. 当前目录结构

```text
citizenchain/
├── node/            # 原生节点、桌面端 Rust 后端、React 前端与 Tauri 打包入口
├── onchina/         # 链上中国平台:多机构工作台、注册局业务、行政区、机构登记、管理后台和链侧凭证
├── runtime/         # 运行时 wasm 与 runtime API
│   ├── governance/  # 治理 pallet 与治理文档
│   ├── admins/      # 管理员 pallet 与管理员文档
│   ├── private/     # 私权 pallet 与私权文档
│   ├── issuance/    # 发行 pallet 与发行文档
│   ├── transaction/ # 交易 pallet 与手续费文档
│   ├── misc/ # 其他链上基础能力 pallet
│   └── primitives/  # 运行时共享常量、基础类型与制度数据
└── scripts/         # 本产品脚本
```

##### 4.1 OnChina 多机构工作台

`citizenchain/onchina` 是公民链 workspace 成员 crate，承接链上中国平台、多机构工作台、注册局业务、行政区、机构登记、管理后台和链侧凭证能力。任意机构可在办公室服务器安装节点后手动启动 OnChina；首次管理员冷钱包登录后可由链上 admins 关系确定人员所属机构候选，但具体业务能力必须继续按有效 `RoleSubject` 解析。

- 进程模型：OnChina 是公民链内置二进制能力，由节点桌面端设置页“链上中国平台”入口手动拉起为子进程、退出时一并停掉；节点启动后默认不启动 OnChina，避免只挖矿节点承担管理后台服务。OnChina 经节点 RPC 读写链，对内网托管 HTTPS API 与前端，固定入口为 `https://onchina.local:8964`。桌面 = 节点运维台，浏览器 = 机构管理员，并存不冲突。
- 工作台模型：登录账户属于哪个链上机构 admins 人员集合，就显示哪个机构候选；同一账户属于多个机构时先选择机构。工作台操作列表和每次链写都必须按该账户的有效岗位任职与 `RoleBusinessPermission` 过滤，不能因登录成功获得机构业务权限。注册局是 `workspace` 的一类，司法院、立法院、学校、公司、公益组织等机构按自己的工作台 UI 进入“操作 / 显示 / 记录”页面。
- 数据两层：链上最小身份 + 承诺哈希(选择性/绑定触发上链)；链下明细存本市内嵌 PostgreSQL + 本地/NAS 文件仓库(文件哈希上链验真)。
- 创世机构精确为 49,593 个公权机构 + 1 个私权非营利法人“公民链技术发展基金会”，
  合计 49,594 个机构；公权协议账户为 99,232 个，基金会另有主、费 2 个协议账户，
  全创世合计 99,234 个机构协议账户。管理员个人钱包不计入机构协议账户。公民链基金会
  使用 `PrivateManage/PrivateAdmins`，其 CID、协议账户、一名程伟管理员、同一账户的
  三项固定岗位任职、机构阈值 2 和法定代表人引用来自 runtime primitives 单一常量源；
  OnChina 运行期读取链上机构、admins、岗位权限和有效任职，并用本地投影补齐展示字段，
  不生成第二套机构授权真源。
- 公民档案先本地建档并发电子护照,不要求链账户;注册局推送链上投票身份时才录入 `account_id`、要求目标公民签名,并由注册局管理员提交 `CitizenIdentity.register_voting_identity`。
- 当前进度：
  - Step0：crate 骨架 + node 拉起子进程的最小贯通（已完成）。
  - Step1：`citizenchain/onchina/src` 后端完成迁移和收敛，平台层切换为内嵌 PostgreSQL + 节点 RPC + 进程内本地限流；省/市 scope 与行政区维度保留。
  - Step2：`citizenchain/onchina/frontend` 前端完成迁移和收敛，OnChina 后端同源托管 `dist` + SPA 回退；桌面 `node/frontend` 与浏览器 `onchina/frontend` 两套独立前端并存。
  - 后续：链上管理员供给与扫码登录、公民护照直接录入收口、打包部署均按 OnChina 当前任务卡推进，不再引用旧注册局迁移任务口径。

##### 5.1 分层结构
- Native Node 层：负责 CLI、网络、数据库、共识服务编排、RPC 服务、chain spec 加载。
- Runtime 层：负责所有链上状态转换、交易校验、治理规则、发行规则、手续费规则。
- Pallet 层：按治理、发行、交易、其他能力拆分功能模块。
- Desktop UI 层：由 `node/src/desktop.rs`、`node/src/<功能名>` 与 `node/frontend` 负责本地节点进程生命周期管理、参数设置、状态展示与安装包交付。

##### 5.2 关键共享依赖
- `runtime/primitives/`：提供链常量、机构常量、SS58 参数、发行与人口基础常量。
- `polkadot-sdk`：提供 Substrate / FRAME / client / consensus 依赖。

##### 6.1 职责
- 提供 `BuildSpec`、`ExportBlocks`、`ImportBlocks`、`PurgeChain`、`Benchmark` 等标准节点能力。
- 加载 `CitizenChain` 主网 chain spec。
- 编排 PoW 出块、GRANDPA 最终性、交易池、RPC 服务与数据库。

##### 6.2 当前 chain spec 口径
- `node/src/core/command.rs` 当前把省略 `--chain`、`citizenchain`、`dev`、`local`、`staging` 统一加载为同一份冻结正式 chainspec。
- `citizenchain-fresh` 只允许用于重新生成冻结 chainspec 的本机 bake 流程。
- 生产部署必须显式使用 `--chain citizenchain`；`mainnet` 不是内置链标识，会被当成文件路径解析。

##### 6.3 当前运行形态
- 数据库存储：RocksDB
- 网络层：`libp2p` / `litep2p`
- 默认本地 RPC：`127.0.0.1:9944`
- 默认本地 Prometheus：`127.0.0.1:9615`

##### 6.4 云节点角色与公民端接入

冻结 chainspec 固定包含 44 个权威引导节点：第 1 个是国储会权威节点，其余 43 个后续逐步部署。权威节点和公开 bootnode 是同一台安装 CitizenChain 软件的服务器，不拆成两种节点；其公网职责与私有职责按端口隔离：

- 公网 P2P：每个权威引导节点开放 `30333/TCP` 的 WSS/libp2p 入口，供 CitizenApp 轻节点、普通全节点和其他权威节点连接。
- 本机 RPC：`9944/TCP` 只监听回环地址，不配置 `--rpc-external`，不下发给 CitizenApp。
- 监控和管理：Prometheus、OnChina、数据库和 SSH 默认不向公网开放；没有运维需求时不创建对应公网入口。
- Cloudflare 链连接：首期 Worker 只通过 Access + 独立 Tunnel 访问国储会节点的本机 RPC；Worker 使用远端 Secret 保存 HTTPS URL 与 Access 服务令牌，只允许内部固定的 `state_getStorage`、`author_submitExtrinsic`，不提供通用 JSON-RPC 代理。后续最多选择少量权威引导节点作为私有 RPC 备用，不连接全部 44 个节点。

CitizenApp P2P 暂时不可用时，聊天和广场不依赖链节点 RPC，继续走 Cloudflare；链上关键状态必须等待轻节点恢复或通过 Worker 受控接口完成已签名交易广播后，再由 finalized 链状态确认。

##### 7.1 定位
- `citizenchain` 是统一链上状态机。
- 账户体系、交易扩展、链上 pallet 装配、runtime API、创世配置都由这里统一编译到 wasm。

##### 7.2 当前实现特征
- `AccountId` 与公钥等价，链上账户体系直接以公钥签名身份为主。
- 交易扩展中显式拒绝 `stake` 账户作为发送方。
- runtime 当前直接依赖本产品的治理、发行、交易、其他 pallet。
- 创世配置由 `runtime/src/genesis_config_presets.rs` 提供。
- 公民宪法创世正文唯一文件为 `runtime/public/legislation-yuan/src/constitution.scale`；该文件是结构化 `章>节>条>款` SCALE 数据，运行态注入为 `law_id=0`，修改必须经 runtime 二次确认并通过 `legislation-yuan` 解码/创世测试。
- 单一宪法版本的目录编号规则固定为：章号全文全局唯一、节号在所属章内唯一（不同章允许复用）、条号全文全局唯一；只约束唯一性，不要求连续或从 1 开始。
- runtime 在创世构建、修宪提案和投票结果最终写入三处独立校验上述规则；原生 `ConstitutionGuard` 对 block#0 与全部历史版本复校验，`check-constitution-genesis.py` 对 chainspec/RPC 创世状态执行同口径检查，任何一层发现重复编号都失败关闭。
- 创世法律版本标签唯一常量在 `runtime/primitives/src/genesis.rs`：`GENESIS_LAW_VERSION_LABELS` 目前固定写入 `(law_id=0, version=1) -> 创世版 / Genesis Edition`；runtime 创世构建把该常量写入 `LegislationYuan.LawVersionLabels`，显示端不得本地推断 `v1=创世版`。

##### 7.3 Runtime 升级边界
- 改动 `runtime/` 内部逻辑，通常属于 runtime 变更。
- 改动被 runtime 直接依赖的 pallet，也属于 runtime 变更。
- 改动 genesis patch / chain spec，不一定是“现有链 runtime 升级”，很多情况下更接近“新链配置”或“重发 chain spec”。

##### 8.1 出块
- 当前新区块生产采用 PoW。
- 节点使用独立 `powr` key type 生成 / 管理本地 PoW 作者身份。
- 首次启动若不存在 `powr` 密钥，节点会自动生成。
- 普通节点清库或首次安装后，必须先从现网导入区块，未接入网络或仍处于主同步阶段时禁止本地先出块，避免节点自发形成离线分叉。

##### 8.2 最终性
- 最终性使用 GRANDPA。
- GRANDPA 最终性密钥治理能力由治理模块承接，而不是硬编码在 UI 或脚本层。
- GRANDPA 正常更换由目标 NRC/PRC 的单个委员按 `CID + 委员岗位码 + account_id`
  授权，并由旧、新 GRANDPA 私钥共同签名，不进入投票；旧私钥丢失时才由目标机构
  自己的委员内部投票紧急恢复，NRC 为 `13/19`、每个 PRC 为 `6/9`，不是联合投票。
- 两条更换路径都延迟生效。节点在提交前同时保存旧、新私钥；只有 finalized 状态
  确认新 authority 已生效且旧 authority 已移除后，才自动删除旧私钥。
- 最终性是否推进取决于 GRANDPA authority 是否按当前链配置正确上线并参与投票。
- 节点刚安装完成时默认是普通同步节点；只有在本地导入 GRANDPA 私钥且该公钥匹配当前 authority set 后，节点才会切换为 GRANDPA 节点。
- 所有节点统一注册 GRANDPA 网络协议并挂载 warp proof provider；只有本地持有且匹配当前 authority set 私钥的节点启动 `grandpa-voter` 参与最终性投票。
- 普通节点启动 `grandpa-observer` 消费最终性通知，不参与投票；该 observer 同时避免协议接收端提前关闭触发 `EssentialTaskClosed`。
- GRANDPA 持久化仅保留恢复与 proof 所需的覆盖写状态；按轮次追加的 `concluded_rounds` 已在本地 vendored `sc-consensus-grandpa` 中停用，用于限制多节点长期运行时的 AUX 膨胀。
- 新安装 CitizenApp 的快速接入依赖 GRANDPA warp：客户端从签名安装包内置 finalized 锚点验证 authority set 交接 proof，再下载近头 runtime/state proof。公开权威引导节点必须维持归档状态和 finalized 正典历史，不得只提供 peer discovery。

##### 8.3 链身份
- 地址显示格式使用自定义 `SS58 = 2027`。
- 链名、链 ID、Token 显示属性统一来自 `runtime/primitives` 与 chain spec 配置。

##### 9.1 治理模块（`runtime/governance/`）
- 投票引擎负责内部投票、联合投票、立法投票和选举投票的资格快照、票据、阈值、计票及终态；最终性密钥、运行时升级、销毁、决议发行等具体业务仍归各自业务模块。
- ADR-039 目标中，每个业务模块先按完整 `RoleSubject(cid_number, role_code)` 校验发起权限，在代码中静态指定唯一投票引擎并绑定 VotePlan；投票引擎不得由调用方选择，也不得执行具体业务。

当前模块：
- `grandpakey-change`
- `resolution-destro`
- `runtime-upgrade`
- `votingengine`

##### 9.2 管理员模块（`runtime/admins/`）
- 负责公权机构管理员、私权机构管理员和个人多签管理员；固定治理机构初始管理员由链配置写入，运行期治理归公权管理员模块。
- 机构管理员集合真源归 `admins`，机构管理员是可任职人员。公权、私权机构值结构统一为 `Admin { account_id, cid_number, family_name, given_name }`，非空公民 CID 必须与 `citizen-identity` 的 `AccountIdByCid` / `CidByAccountId` 双向真源一致。个人多签虽复用同一 SCALE 结构，但按个人多签规则处理字段完整性。管理员账户本身没有机构业务权限。
- ADR-039 目标授权主体为 `RoleSubject(cid_number, role_code)`。岗位、强类型 `RoleBusinessPermission` 和任职真源归 `entity`；业务动作、指定投票引擎和执行真源归业务模块。个人多签保持独立 `AuthorizationSubject::PersonalMultisig`。
- `public-admins`、`private-admins`、`personal-admins` 的 `AdminAccounts` 统一接受四字段 `Admin` SCALE 布局；旧纯账户、旧三字段、旧合并姓名和历史 storage migration 均已删除，不保留兼容或双轨。字段是否必须非空由机构类型、岗位和个人多签规则分别判定。

当前模块：
- `admin-primitives`
- `public-admins`
- `private-admins`
- `personal-admins`

##### 9.3 实体模块（`runtime/entity/`）
- 负责公权机构、私权机构、个人多签账户的创建、关闭、资金与生命周期治理。
- 机构管理已按公权/私权拆分两 pallet(取代旧 `organization-manage`)。
- 公权/私权 entity 已保存岗位、岗位权限、任职、`InstitutionRoleNonce` 和永久 `UsedRoleCodes`，并提供 CID 能力封顶、有效任职和岗位权限的统一查询。所有机构强制存在可空缺的 `LR`；普通机构原子创建将在独立业务模块中同时建立至少一个初始治理岗位、权限、任职和投票规则。
- 动态岗位码由 runtime 使用所属 pallet 的 `MODULE_TAG` 作哈希域生成，调用方不得提交；岗位码及权限不可修改，岗位名可以依法修改。

当前模块：
- `public-manage`（公权机构生命周期,idx30）
- `private-manage`（私权机构生命周期,idx31）
- `personal-manage`（个人多签）

##### 9.4 公权业务模块（`runtime/public/`）
- 负责公权机构的业务壳。业务壳只解释业务规则和写回业务真源，不复刻投票流程。
- `legislation-yuan` 是立法业务壳；立法表决、计票和公投流程归 `legislation-vote`。
- `citizen-election` 是公民选举公职人员的业务模块，当前仅占位；后续逐步实现具体选举规则，选举投票、计票和结果快照统一归 `election-vote`。

当前模块：
- `legislation-yuan`（idx25）
- `citizen-election`（idx32，当前仅占位）

index 32 由 `CitizenElection`（`runtime/public/citizen-election`）复用，用于公民选举公职人员；当前仅占位，不提供交易入口、业务存储或事件。

##### 9.5 发行模块（`runtime/issuance/`）
- 负责公民发行、全节点发行、省储行利息、决议发行完整流程。

当前模块：
- `citizen-issuance`
- `fullnode-issuance`
- `resolution-issuance`
- `provincialbank-interest`

##### 9.6 交易模块（`runtime/transaction/`）
- 负责链上交易手续费、链下交易手续费、机构多签交易能力。

当前模块：
- `multisig`
- `offchain`
- `onchain`

##### 9.7 其他模块（`runtime/misc/`）
- 负责链上公民身份、人口统计、PoW 难度调整等基础能力。
- `citizen-identity` 是链上投票身份、参选身份和全国、省、市、镇四级有效人口数据的唯一真源；投票引擎按提案需要消费这些人口数据并生成快照。投票身份不在链上算/存年龄，能否投票由 `citizen_status=Normal` + 护照有效期窗口判定；最低年龄门禁只在竞选身份按链上保存的出生日期 `birth_date` 实时计算复核（≥16）。

当前模块：
- `pow-difficulty`
- `citizen-identity`

##### 10.1 定位
- `citizenchain/node` 是当前唯一桌面节点产品壳与原生节点实现目录。
- 历史 `node` 与独立 `node` 目录中的桌面职责已经收口到 `citizenchain/node`，旧目录已删除。
- 对最终用户仍然提供“安装即用”的节点软件，而不是要求用户手工管理原生 node 命令。

##### 10.2 当前职责
- `node/src/desktop.rs` 负责 Tauri 桌面入口与 command 注册。
- `node/src/<功能名>` 负责桌面端 Rust 后端能力，不再保留 `node/src/ui` 目录层。
- `node/frontend/<功能名>` 负责 React 前端页面与交互。
- `citizenchain/node` 负责启动 / 停止内嵌节点进程，管理 bootnode 地址、奖励地址、GRANDPA 地址、节点名称等本地设置，并展示节点状态、链状态、网络概览、挖矿面板与其他辅助信息。
- 管理员、治理、转账、清算和奖励设置等桌面桥接统一把账户字段输出为 `account_id` / `<role>_account_id`，把签名公钥输出为 `signer_public_key`，把展示地址输出为 `ss58_address`；前端不得重新创造同义字段。
- Node 端所有公民钱包离线扫码签名 UI 统一由 `node/frontend/protocol/CitizenSignaturePanel.tsx` 和 `CitizenSignatureModal.tsx` 承载：左侧固定“扫码签名”，右侧固定“识别签名”，面板只显示二维码有效期倒计时，不显示内部 request id 或签名账户地址；业务页面只负责构造请求、验签和提交交易。地址扫码填入等非签名二维码不纳入该组件。
- 设置页的“全节点模式”当前展示归档全节点和普通全节点：默认归档全节点；普通全节点置灰不可选择；在底层剪裁能力完成前，节点实际仍按归档全节点运行。
- 设置页在“全节点模式”之后提供“链上中国平台”手动启动行，显示 `未开启` / `启动中` / `已开启` 状态标签、固定入口 `https://onchina.local:8964` 和“启动 / 关闭”按钮；点击后必须二次确认，只启动或停止 OnChina 子进程，不自动打开浏览器；只有 `/api/health` 真实健康检查通过后才显示 `已开启`。

##### 10.3 打包边界
- 桌面端与原生节点在同一个 `node` crate 中构建，Tauri 打包从 `node/frontend/dist` 读取前端产物。
- 对用户交付形态始终保持单个桌面安装包；对工程实现来说仍是“UI 壳 + 内嵌 node 二进制”。

#### 11. 变更与发布边界

正式创世前，CitizenChain runtime 的 `authoring/spec/impl/transaction/system` 五项版本全部为 `0`，所有项目 pallet `StorageVersion` 为 `0`，workspace/Node/runtime 本地程序包版本为 `0.0.0`。当前结构调整直接落到创世终态，不编写 migration 或兼容分支；第三方依赖和 Substrate runtime API trait 协议版本不属于项目版本归零范围。

##### 11.1 需要 runtime 升级的改动
- `runtime/` 中的状态机、类型、交易校验、runtime API。
- `runtime/governance/`、`runtime/admins/`、`runtime/private/`、`runtime/issuance/`、`runtime/transaction/`、`runtime/misc/` 中被 runtime 直接引用的链上逻辑。
- `runtime/primitives/` 中被 runtime 直接使用、并影响链上行为的常量 / 类型 /编码结构。

##### 11.2 不需要 runtime 升级的改动
- `node/` 中的 CLI、RPC、服务编排、网络与本地运行逻辑。
- `node/` 的桌面 UI、设置页、Tauri 命令与安装包逻辑。
- 构建脚本、CI/CD、前端界面、说明文档。

##### 11.4 特殊情况
- `node/src/chain_spec.rs` 变更通常不是“现有链 runtime 升级”，而是 chain spec / bootnodes / properties / 启动配置变更。
- `runtime/src/genesis_config_presets.rs` 变更若影响创世状态，通常对应新链或重建链，不等于自动给已运行链打补丁。

##### 12.1 治理
- `runtime/governance/grandpakey-change/GRANDPAKEYCHANGE_TECHNICAL.md`
- `runtime/governance/resolution-destro/RESOLUTIONDESTRO_TECHNICAL.md`
- `runtime/governance/runtime-upgrade/RUNTIMEUPGRADE_TECHNICAL.md`
- `runtime/votingengine/VOTINGENGINE_TECHNICAL.md`

##### 12.1.1 管理员
- `runtime/admins/ADMINS_TECHNICAL.md`

##### 12.1.2 实体（机构/个人生命周期）
- `runtime/entity/public-manage/PUBLIC_MANAGE_TECHNICAL.md`
- `runtime/entity/private-manage/PRIVATE_MANAGE_TECHNICAL.md`
- `runtime/entity/personal-manage/PERSONAL_MANAGE_TECHNICAL.md`

##### 12.2 发行
- `runtime/issuance/citizen-issuance/CITIZENISS_TECHNICAL.md`
- `runtime/issuance/fullnode-issuance/FULLNODE_TECHNICAL.md`
- `runtime/issuance/resolution-issuance/RESOLUTIONISSUANCE_TECHNICAL.md`
- `runtime/issuance/provincialbank-interest/PROVINCIALBANK_TECHNICAL.md`

##### 12.3 交易
- `runtime/transaction/multisig-transfer/MULTISIG_TRANSFER_TECHNICAL.md`
- `runtime/transaction/institution-asset/INSTITUTION_ASSET_TECHNICAL.md`
- `runtime/transaction/offchain-transaction/STEP1_TECHNICAL.md`
- `runtime/transaction/offchain-transaction/STEP2A_RUNTIME.md`
- `runtime/transaction/onchain-transaction/ONCHAIN_TECHNICAL.md`

##### 12.4 其他链上模块
- `runtime/misc/pow-difficulty/POW_DIFFICULTY_TECHNICAL.md`
- `runtime/misc/citizen-identity/CITIZEN_IDENTITY_TECHNICAL.md`

##### 12.5 桌面节点 UI
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`
- `CitizenChainNode.md`（第 9 节记录全节点模式设置边界）

##### 12.6 公民链客户端 SDK

- `../citizensdk/CitizenSDK.md`（独立产品边界、源码来源、钱包、轻节点、交易、

#### 13. 维护要求
- `citizenchain` 发生架构级、边界级、发布级改动时，必须同步更新本文档。
- 模块行为变更时，必须同时更新对应模块技术文档。
- 若产品级口径与模块级口径冲突，以代码实现为准，并应在本次改动中同时修正文档。

#### 14. 本地 fresh Node 验收口径

- macOS 节点二进制默认进入桌面模式；需要命令行指定 `--chain citizenchain-fresh --tmp` 做隔离验收时，必须同时设置 `CITIZENCHAIN_HEADLESS=1`，否则命令行 chain 参数不会代表实际启动的桌面节点状态。
- fresh 验收必须读取 RPC 的 block 0、health、六项项目 Runtime 版本、metadata、genesis hash 与 state root，并在结束后停止节点。与既有 bootnode 的 genesis 不一致只说明正式 chainspec 尚未统一，不得通过削弱 NodeGuard 或复用旧链数据规避。
- 2026-07-22 最终验收：block #0/genesis hash `0x4bd7e3f65f5ad4788e6ac8917abce9b0683f0c93d286766a7512854084ff0dd9`，state root `0xd15b1a20d972f0cc5f64aa9a08a09f6793fe51886f9445c6dc953c0f9d438f7b`，`peers=0`、`isSyncing=false`，六项项目 Runtime 版本均为 `0`，metadata 二进制 220,247 字节；验收节点已停止，未生成正式 chainspec。

#### 14.1 外部调用准入与费率路由：两处都不设通配分支

`runtime/src/configs.rs` 的两个 `RuntimeCall` 匹配统一采用**逐 pallet 显式归类、无 `_` 兜底**：

- `RuntimeFeeRouter::fee_route` — 历来如此，默认 `FeeRoute::Reject`。
- `RuntimeCallFilter::contains` — 2026-07-30 创世前审计整改。整改前是 `_ => true`
  （默认放行），新增 pallet 的 extrinsic 会自动对外暴露且不触发任何编译期提醒；
  现改为把 28 个 `RuntimeCall` 变体全部列出（拒绝 4 个：`Balances` / `Assets` /
  `OnchainIssuance` / `OffchainTransaction`，放行 24 个）。

这样新增 pallet 会触发编译期 non-exhaustive 错误，强制作者显式决定放行还是拒绝。
编译器只能拦住「删掉某个 pallet 分支」，拦不住有人把 `_ => true` 加回来，故另配源码级
回归测试 `runtime/src/tests/cases.rs::runtime_call_filter_has_no_wildcard_arm` 钉死这一点。

> 注：只有带 `#[pallet::call]` 的 pallet 才会出现在 `RuntimeCall` 中。`TransactionPayment`、
> `ProvincialBankInterest`、`PowDifficulty`、`GenesisPallet`、`PublicAdmins`、`PrivateAdmins`
> 六个 pallet 不暴露 extrinsic，因此 34 个 pallet 对应 28 个 `RuntimeCall` 变体。

#### 15. 正式创世前全仓静态与测试门禁

- 2026-07-25 已通过
  `cargo clippy --workspace --all-targets -- -D warnings`，范围包含全部 production、test、
  example、Node、OnChina、runtime、协议 crate 和固定 GRANDPA vendor target，不按目录
  跳过告警。
- 同一源码已通过 `cargo test --workspace --all-targets`。现有 `square-post` 真实日历、
  自动续费、取消、暂停恢复与价格重确认测试均通过，本轮没有修改订阅业务逻辑。
- FRAME 宏生成的既定 extrinsic 参数 ABI、固定上游 vendor 和测试断言只允许使用最窄、
  带中文原因的 lint 范围；不得以此扩展为 production 全局静音。
- **2026-07-30 创世前审计复核：上条「2026-07-25 已通过」当时已不成立，门禁实际是红的**，
  共 10 条错误（`citizen-identity` 参数超阈值 1 条来自当日提交 `51148c76`；
  `node/vendor/finality_proof.rs` 冗余借用 1 条从 2026-03-26 起就红着；
  `node/admins/management/activation.rs` 冗余借用 2 条；
  `node/transaction/offchain/rpc.rs` `return Err(..)?` 冗余 6 条）。全部已修，
  当前 `cargo clippy --workspace --all-targets -- -D warnings` **exit=0**。
- `clippy::too_many_arguments` 的放宽范围按「够得着的最窄一层」选，不是一律 crate 级：
  - 手写辅助函数**一律不放宽**，按设计把长参数收成描述结构体
    （本轮把 `prepare_legislation_sign`、`build_chain_sign_output` 各收成 3 参数）。
  - FRAME `#[pallet::call]` 中单个 extrinsic 超阈值 → per-fn `#[allow]`
    （`citizen-identity::admin_rebind_cid_account_id`、`public-manage`、`private-manage`）。
  - 宏在 pallet **模块层**生成的 Call 分发代码超阈值 → per-fn 与 call 块级 `#[allow]` 都够不到，
    只能用模块内部属性 `#![allow(...)]`（`address-registry`、`legislation-yuan`）；
    这仍窄于原先的 crate 级，模块外辅助函数的参数过多依然会被抓出。
- 该门禁此前**只靠手工跑、没有进 CI**，这正是它红了几个月无人发现的原因；
  修完必须补 CI job，否则还会再漂。
- 本轮只完成正式创世前代码质量验收；没有烘焙 chainspec、切换节点数据、触发 GitHub CI、
  部署节点或转入资金。
- 第8.1步最终冻结源码提交为
  `ac6de21b2432f52f45f1767f88f4e6833a2c79d0`。CitizenChain WASM run
  `30190068925` 已成功并上传 artifact `8628330093`；正式烘焙必须使用该 run 的
  `citizenchain.compact.compressed.wasm`，其 SHA-256 为
  `a838dd763c1c7003aca1edf177738d85b64936bbc1ba98dda7da348cc57d0d1a`。
  任何 HEAD SHA、run id 或 WASM 哈希不一致都必须阻止第8.2步。
- 第8.2步已使用上述唯一 CI 产物完成正式 `--finalize`：
  `genesis_hash=0xe8f4067de2323dc27b2a2c409fa4b3ab882e4e88dfa6f4a81355f51f8cf8eb45`、
  `state_root=0xbdc2593a538b7010717ac475b0b59973dd57c77d35683c4e7d9b8058b9ae18f9`、
  `chainspec_hash=3e79942fabad332fee5e8692b503c393005730bc5b2d85b9d38694833fada652`。
  这三个值是 2026-07-26 那次 `--finalize` 的历史记录，已被 2026-07-31 重新创世替代。
  创世状态包、CitizenApp 链资产、公权机构包和 Cloudflare 链身份锚点均由同一候选生成并
  交叉校验；本步没有切换正式数据、部署节点或转入资金。

#### 16. 正式创世运行基线（2026-08-01）

- 第8.3B步已经完成本机旧链数据切换；CitizenChain CI run `30211805231` 四个平台任务
  成功，本机安装 CI macOS 应用后从内嵌冻结 plain chainspec 物化正式链数据。
- 当前唯一正式创世哈希为
  `0x157558224b682de0384fd50dea0735aff55795f6d145993233c901cf1258671d`，
  状态根为
  `0x363d9c4836875a1a8270940caef743524350a6341199ec75966c3b25065bbe80`；
  runtime 源提交为 `9c2ec97b91b3236c6268ddd3057a4700a4591cd2`，冻结资产提交为
  `7cea3885783064b5c02850e23d48e41e1fce7065`。
  2026-07-26 的
  `0xe8f4067de2323dc27b2a2c409fa4b3ab882e4e88dfa6f4a81355f51f8cf8eb45`
  与 2026-07-31 的
  `0x278e68bced2dabf9690701188272da22d216fdaa2c617e7dcbe100df3e8bcbfa`
  只保留为历史冻结记录。
- 本机正式链、固定远端 RPC、CitizenApp 轻节点资产和 Cloudflare 链身份使用同一创世锚点。
  第8.3D步只读审计时本机 best/finalized 均为 block #6、`isSyncing=false`。
- 正式创世已经完成。第 14、15 节中的 fresh/创世前证据只作为历史验收记录；后续 runtime
  升级只能走正式链交易，不得重新运行正式 `--finalize` 或替换创世数据。

---

### 公民身份全链路:建档 → 上链 → 人口统计 → 投票引擎消费

- 更新日期:2026-07-22
- 事实源:本文是流程导读;字段与编码以代码为准
  - `citizenchain/onchina/src/citizens/`(建档 + 上链准备)
  - `citizenchain/runtime/misc/citizen-identity/src/lib.rs`(链上身份 + 人口计数)
  - `citizenchain/runtime/votingengine/legislation-vote/src/lib.rs`(快照消费)
  - `CitizenChainOnChina.md`(扫码动作登记)

#### 1. 公民创建(本地建档,不碰链、不碰钱包)

- 注册局管理员登录 OnChina 塔塔控制台(链上 Active 管理员集合鉴权)创建公民档案,
  落节点本地 PG `citizens` 表:CID编号、护照号、`family_name`、`given_name`、性别、出生日期、
  居住地/出生地省市镇码、护照有效期、`citizen_status`、`voting_eligible`。
- 建档不要求钱包:未成年人、无钱包公民都可以先持有本地电子护照档案。
- 公民在 CitizenApp「我的 → 电子护照」选择一个热钱包作为投票账户,
  页面展示钱包地址二维码,供办理现场的操作员扫入。

#### 2. 公民上链(录入钱包 + 双签名,`chain_identity.rs`)

前置:档案状态 NORMAL、`voting_eligible=true`、满 16 周岁、档案在本注册局辖区。

注册局上链操作一律最严档(`CITIZEN_ONCHAIN_PUSH` → PasskeyColdSign):
prepare 与 complete 前各需一次 WebAuthn passkey 断言 + 管理员冷钱包扫码签名,
换取绑定 `{cid_number, account_id}` 的一次性安全 grant,无 grant 一律 403。

1. **prepare**:操作员录入/扫描公民账户 → 后端组
   `VotingIdentityPayload` SCALE 字节(8 字段:cid_number、account_id、
   passport_valid_from/until、citizen_status、居住地省/市/镇码;不含年龄,
   投票身份不在链上算/存年龄)→ 打包 QR_V1 `k=1 a=2` 签名请求(180 秒有效)。
2. **公民签名(第一重签名)**:公民用 CitizenWallet 离线签名页或
   CitizenApp 电子护照扫码签名页扫码 → 两色识别独立解码载荷并展示中文字段,
   解不开一律拒签 → 本人确认后对
   `blake2_256(GMB || 0x10 || payload)`(`OP_SIGN_CITIZEN_IDENTITY`)
   做 sr25519 签名 → 展示 sign_response 二维码。
3. **complete**:操作员扫回执 → 后端验公钥一致 + 同域验签 → 通过后绑定账户
   落库,并构造 `register_voting_identity(actor_cid_number, actor_role_code,
   payload, citizen_signature)` call data,生成第二张二维码(链交易动作码 `0x0a00`)。
4. **管理员签名(第二重签名)**:注册局管理员用自己的钱包扫码冷签并提交
   extrinsic(标准 Substrate 交易签名)。
5. **链端执行**(pallet CitizenIdentity idx 10):
   - `ensure_signed` 管理员 origin;
   - 载荷合法性(年龄 ≥16 等);
   - `can_manage_voting_identity`:签名账户必须是 `actor_cid_number + actor_role_code`
     对应注册局岗位的有效任职管理员,FRG 省专员管本省、CREG 专员只管本市;
   - `ensure_citizen_signature`:链上对 `payload.encode()` 再验一次公民
     0x10 域签名——公民本人同意在链上可验证;
   - CID 永久唯一；`VotingIdentityByCid` 以 CID 保存身份，`AccountIdByCid` 与
     `CidByAccountId` 保存当前唯一签名账户的双向绑定；
   - 写入上述 CID 主键身份与当前账户绑定，触发人口计数增量与
     `CitizenIssuance` 首次注册发行钩子(按档位/全局上限/CID+账户双重防重,
     把公民币 `deposit_creating` **直接铸入公民账户**,无需领取动作),
     发 `VotingIdentityRegistered` 事件。
- 候选人身份 `upgrade_to_candidate_identity` 同构,链上公开档案多出生地三级码、
  `family_name`、`given_name`、性别(`citizen_sex`)和出生日期。姓名结构不得再拼接或另造同义字段。
- 链上身份字段定稿:投票公民 = CID号(身份存储键)+ 当前绑定 `account_id` + 护照有效期起止 +
  身份状态 + 居住地省/市/镇码;参选公民另加出生地省/市/镇码 + `family_name` +
  `given_name` + 性别 + 出生日期。
  年龄只作注册门槛(≥16)校验,不进链上状态。

2026-07-30 已完成投票引擎公民主体收口：资格接口返回 `CitizenSubject { cid_number, account_id }`；
联合公投、立法公投和选举普选均按永久 CID 去重并在票据值中保存完整主体。候选快照和
当选结果保存完整主体，候选人计票以候选 CID 为唯一键；账户只证明当前签名授权，不得恢复
裸账户身份主键。

#### 3. 公民人口数据(citizen-identity pallet)

- 每次注册/更新/撤销身份时同步维护四级有效选民计数器:
  `CountryVotingCount` / `ProvinceVotingCount(省)` /
  `CityVotingCount(省,市)` / `TownVotingCount(省,市,镇)`,
  最终只统计状态正常且在当前人口就绪日期护照有效的身份。
- 护照未来生效、到期、身份吊销和迁居必须由 citizen-identity 维护有界日期变化计划；
  当天人口变化尚未处理完成时，新提案人口数据 fail-closed，不得用不完整分母建案。
- `PopulationReadyDate` 保存四级人口完整推进至的 UTC+8 日期；护照生效和到期转换按
  `(date, index)` 独立存储并携带永久 CID、身份 revision 和转换类型，不复制账户或身份全文。
  `on_idle` 每块最多推进 366 个日期、处理 2,048 个转换项，且最多使用区块权重的 1/8；
  实际处理量继续受剩余权重限制。
- 身份写入只在 `PopulationReadyDate == 当前日期` 且没有人口维护故障时执行。某日转换
  分块处理中，内部计数即使已经部分改变也不会通过 provider 对外发布，身份变更同样暂停，
  直到整日转换全部完成。
- 取数入口 `CitizenIdentityProvider`:
  - `citizen_subject(who)`:先由 `CidByAccountId` 取得永久 CID，再校验 `AccountIdByCid`、CID 主键身份、身份状态和 CID 状态后返回完整公民主体，任何错配均 fail-closed;
  - `population_data(scope)`:仅在当前日期人口完整就绪时返回 `Some(PopulationData)`，O(1)
    读取对应作用域计数、当前资格 revision 和判定日期；未就绪或维护故障返回 `None`；
  - `voting_subject(who, scope)`:当前身份、护照、作用域和 CID↔账户全部有效时返回完整投票公民主体;
  - `candidate_subject(who, scope)`:voting_subject 有效且持有候选人身份时返回完整竞选公民主体;
  - `voting_subject_at(who, population_data)`:由当前签名账户解析永久 CID，按该 CID 的不可变身份历史校验快照时资格并返回完整主体。
- 每次身份注册、资料更新、迁居或撤销都会递增全局 revision,关闭同一永久 CID 的旧版本并写入
  `VotingEligibilityVersions`;同一区块多次写入也有确定顺序。CID 不得修改、替换、删除或复用。

#### 4. 投票引擎消费(legislation-vote 特别案公投)

- **建案事务内快照**:特别案创建时由 `legislation-vote` 先从发起机构
  `actor_cid_number` 的唯一 CID 解析国家/省/市作用域，再在同一存储事务内调用
  `VotingEngine::create_population_snapshot(proposal_id, scope)`。投票引擎只从
  `CitizenIdentityReader::population_data(scope)` 取得人口数据，再写入自身
  `ProposalPopulationSnapshots[proposal_id]`。人口为零或后续建案失败时整笔事务回滚，
  不存在公开准备交易、调用者缓存、独立 snapshot_id 或待消费中转存储；普通案和重大案不创建人口快照。
- **分母与成员资格同源冻结**:`cast_referendum_vote` 对每张票把该提案人口快照传给
  `voting_subject_at(who, population_data)`；提案创建后的新增、迁居、资料更新或撤销不改变已有提案。
- **消费端全量校验**(`voting_subject_at`):由当前账户绑定解析永久 CID，按 revision 定位该 CID 在创建时的身份版本 + 状态 NORMAL + **护照有效期窗口内**(链上时间戳按 UTC+8
  冻结 YYYYMMDD,过期或未生效即拒,时间戳缺失 fail-closed)+ 居住地在作用域内;
  账户签名由投票 extrinsic 本身在交易层强制。
- **分母口径约束**:人口分母与单人资格同为“永久 CID Active + CID↔账户绑定完整 +
  状态正常 + 快照日期护照有效 + 行政区匹配”。2026-07-22 起四级计数已经按该口径维护，
  过期或尚未生效的护照不再进入分母。
- 客户端对齐:CitizenApp、CitizenWallet 和 OnChina 不构造、签名或解码独立快照
  交易；扫码只确认业务提案，作用域和快照均由链端按 actor CID 内联确定。

#### 签名域三端一致性纪律

`blake2_256(GMB || 0x10 || payload)` 的构造分别在:

| 端 | 位置 |
|---|---|
| runtime 真源 | `primitives::sign::signing_message` |
| OnChina 验签 | `domains/citizens/chain_identity.rs` |
| CitizenWallet 签名 | `lib/signer/qr_signer.dart::signingBytesFor` |
| CitizenApp 签名 | `lib/signer/signing.dart::signingMessage`(经 `qr_signer.dart::signingBytesForHex`) |

任何一端改动必须四处同步,并刷新对应测试
(`citizenwallet/test/signer/qr_signer_test.dart`、
`citizenapp/test/signer/qr_signer_test.dart` 0x10 用例、
`citizenapp/test/my/myid/voting_identity_payload_test.dart`)。
现场设备必须使用 2026-06-30 提交 5c8374185 之后的构建,
旧构建对 payload 直签会被后端域验签拒绝。

#### 2026-07-22 第 5 步验收

- 当前、竞选和历史人口快照资格接口全部返回 `CitizenSubject { cid_number, account_id }`，不再用 bool 或裸账户表达公民授权主体。
- 联合公投与立法公投按永久 CID 去重并保存完整票据；人口快照仍只冻结作用域、有效总数、资格 revision、判定日期和创建区块。
- 五个投票 crate、runtime 46 项及受影响业务模块测试，全 workspace 测试目标，`no_std`、WASM、benchmark/try-runtime 和 release Node 构建均通过。
- `citizenchain-fresh --tmp` 真实节点 block #0 为 `0x69b4a0025356d050004cff3ef176167a6520b59c9086c9ac6b9a45c4b9e9c0e6`，state root 为 `0x0b066c3567ed25c15cfa96b7d249b6235df4746a253144db21c87dfd2ed2333e`，metadata 二进制 220,197 字节，runtime 六项项目版本均为 `0`；验收节点已停止。

#### 2026-07-22 第 6 步验收

- 选举元数据只保留唯一 `actor_cid_number + role_code`，发起机构就是拟任职机构；`target_cid_number`、`office_code`、`rule_id` 已从 `election-vote` 清零。
- 候选快照、普选票据、候选计票和当选结果全部保存完整 `CitizenSubject`；普选按永久 CID 去重，互选按机构 CID + 岗位码 + 账户票据去重。
- `election-vote` 17 项、votingengine 4 项、runtime 46 项、全 workspace 测试目标、`no_std`、WASM、benchmark/try-runtime 和 release Node 构建通过。
- `citizenchain-fresh --tmp` 真实节点 block #0 为 `0x285ca7f4ab0f24771baff6a6fc10141ee281fbbd6ce1a8f9dcd1d7676501a41b`，state root 为 `0x27ecdc5b73ce195df4bdfe6c05fe68ef0b682c58751f5f145868a69a1f4672bd`，metadata 二进制 220,398 字节，runtime 六项项目版本均为 `0`；验收节点已停止。

#### 2026-07-22 第 8 步客户端协议验收

- CitizenApp 与 Cloudflare 读取公民身份时，先由账户取得永久 CID，再在同一个 finalized 区块校验 CID Active、CID↔账户双向绑定和 CID 主键身份；任何缺失、截断、尾随或错配都不承认身份。
- `VotingIdentityByCid` 值只承载护照有效期、状态、居住省/市/镇码和更新时间；CID 只在 storage key。`CandidateIdentityByCid` 只增加出生省/市/镇码、`family_name`、`given_name`、性别、出生日期和更新时间。
- OnChina PostgreSQL 最终 schema 已用真实临时数据库初始化并核验；数据库不再在启动期读取、回填或删除旧合并姓名、旧有效期、旧竞选范围和重复居住区字段。注册局办理身份的双签业务边界没有改变。
- 第 8 步没有修改 runtime；全端完整 fresh Node 与链上调用验收统一留在第 9 步。当前发现的护宪守卫 `LawDecodeFailed` 必须从源码嵌入 WASM 与节点解码结构查明，不得通过关闭或绕过守卫处理。

---

#### 1. 文档目的
- 固化 `citizenchain` 在 Oracle Cloud 云服务器上的标准部署流程。
- 统一“从空白服务器到节点启动”的操作口径，便于后续重复部署、迁移和运维。
- 明确 `citizenchain` 在 Oracle Cloud 上的依赖、端口、安全边界与常见排障方式。

#### 2. 适用范围
- 适用于将完整公民链仓的 [`citizenchain`](/Users/rhett/citizenchain) 部署到 Oracle Cloud Linux 云服务器。
- 适用于普通全节点、参与 PoW 挖矿的节点，以及后续扩展为 `systemd` 常驻服务的节点。
- 默认服务器系统口径为 `Ubuntu`。

##### 3.1 当前部署对象
- 部署对象为 `citizenchain/node` 原生链节点程序。
- 该节点为 Rust/Substrate 风格原生程序，不是通过 `apt` 或 Docker 直接安装的现成链客户端。
- 当前项目依赖自有维护仓 `polkadot-sdk` 的固定提交，必须通过源码编译获得节点二进制。

##### 3.2 当前链运行口径
- 正式链标识：`citizenchain`
- 共识机制：`PoW + GRANDPA`
- 默认本地 RPC：`127.0.0.1:9944`
- 默认本地 Prometheus：`127.0.0.1:9615`
- 默认 P2P 端口：`30333`
- 首次启动若不存在 `powr` 密钥，节点会自动生成本地 PoW 作者密钥

##### 3.3 生产节点角色口径

冻结 chainspec 固定包含 44 个权威引导节点。国储会节点是第 1 个权威 bootnode，其余 43 个后续逐步部署；权威节点和公开 bootnode 是同一台安装 CitizenChain 软件的服务器，不拆成两种节点。

- 公开入口：每个权威引导节点只开放 `30333/TCP` 的 WSS/libp2p，服务 CitizenApp 轻节点、普通全节点和其他权威节点。
- 本机入口：RPC `9944` 只监听 `127.0.0.1`；Prometheus 默认关闭；OnChina、数据库和管理端口不向公网开放。
- Cloudflare 链连接：首期 Worker 和 外部调用方 协议行「Release」只通过 Access + 独立
  Tunnel 访问国储会节点的本机 RPC，不需要另建独立 RPC 节点。两者复用 外部调用方
  管理的 `CHAIN_URL / CHAIN_ID / CHAIN_SECRET` 单一生产配置；缺失任一项时关闭链读取。
  当前代码不接受公网 HTTP、回环 HTTP、非 `chain.crcfrcn.com` 上游或无 Access 凭据的请求。
- 后续容灾：最多选择少量不同地区的权威引导节点作为 Worker 私有 RPC 备用，每个节点使用独立 Tunnel 和凭证，不连接全部 44 个节点。

CitizenApp 不直接依赖国储会节点 RPC；App 的链上真源是内置轻节点验证的 finalized 链状态。

相关实现参考：
- [`CitizenChainNode.md`](<本仓根>/CitizenChainNode.md)
- [`node/src/core/command.rs`](<本仓根>/node/src/core/command.rs)
- [`node/src/core/service.rs`](<本仓根>/node/src/core/service.rs)
- [`node/src/core/chain_spec.rs`](<本仓根>/node/src/core/chain_spec.rs)

#### 4. 总体部署步骤
- 第 1 步：准备 Oracle Cloud 云服务器
- 第 2 步：安装系统依赖与 Rust 工具链
- 第 3 步：获取 `GMB` 源码
- 第 4 步：编译 `citizenchain` 节点
- 第 5 步：手工启动并验证节点
- 第 6 步：配置防火墙与 Oracle Cloud 入站规则
- 第 7 步：配置 `systemd` 常驻运行
- 第 8 步：运行检查与常见排障

##### 5.1 建议实例规格
- CPU：至少 2 核，建议 4 核及以上
- 内存：至少 4GB，建议 8GB 及以上
- 系统盘：至少 80GB，建议更大
- 网络：具备固定公网 IP，仅为 `30333/TCP` P2P 提供公网入口

##### 5.2 登录服务器
只有临时运维窗口已经通过 Oracle Bastion 或固定来源 IP 放行 SSH 时，才直接登录：

```bash
ssh -i your-key.pem ubuntu@<ORACLE_SERVER_IP>
```

说明：
- 某些 Oracle Cloud 镜像用户名可能不是 `ubuntu`，也可能是 `opc`。
- 若首次连接，系统会要求确认主机指纹。
- 没有公网运维需求时不得开放 `22/TCP`；应保留 Oracle 塔塔控制台或 Bastion 作为恢复通道。

##### 6.1 更新系统

```bash
sudo apt update
sudo apt upgrade -y
```

##### 6.2 安装基础依赖

```bash
sudo apt install -y build-essential clang cmake pkg-config libssl-dev git curl
```

说明：
- `build-essential`、`clang`、`cmake`、`pkg-config` 用于编译 Rust 和 Substrate 相关依赖。
- `libssl-dev` 用于 TLS/加密相关编译依赖。
- `protoc` 不从 APT 或系统 PATH 取得；克隆产品源码后必须通过
  `.github/workflows/release-wasm.mjs` 在正式自动化中按唯一锁定声明准备官方 35.0；本机编译须显式供给已验真的入口。

##### 6.3 安装 Rust

```bash
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable
```

##### 6.4 验证 Rust 环境

```bash
rustc --version
cargo --version
```

##### 7.1 克隆仓库

```bash
git clone <YOUR_GMB_REPOSITORY_URL>
```

##### 7.2 进入 CitizenChain 目录

```bash
cd citizenchain
```

##### 7.3 源码依赖说明
- 当前工作区依赖 `https://github.com/crcfrcn/polkadot-sdk.git`
- 依赖固定提交为 `1aa4447575d446ab393e89b86cd8ec0a8fca100d`，不消费浮动分支
- 因此服务器必须具备访问 GitHub 的能力，否则 `cargo build` 无法完成依赖拉取

相关参考：
- [`Cargo.toml`](<本仓根>/Cargo.toml)
- [`node/Cargo.toml`](<本仓根>/node/Cargo.toml)

##### 8.1 执行编译

```bash
export CARGO_TARGET_DIR="$(node scripts/build.mjs temporary-root '' macos)/cargo-target"
cargo build --release -p node
```

说明：
- `-p node` 表示只编译节点程序。
- 首次编译时间可能较长，因为需要下载并构建 Substrate 相关依赖。

##### 8.2 编译输出位置
编译成功后，节点二进制位于：

```text
<product-root>/target/<host-platform>/tmp/cargo-target/release/citizenchain
```

##### 8.3 验证节点帮助信息

```bash
"$CARGO_TARGET_DIR/release/citizenchain" --help
```

若帮助信息正常输出，说明节点二进制已可运行。

##### 9.1 准备数据目录

```bash
mkdir -p /home/ubuntu/citizenchain-data
```

如果当前登录用户不是 `ubuntu`，请改成实际用户目录。

##### 9.2 启动权威引导节点

```bash
"$CARGO_TARGET_DIR/release/citizenchain" \
  --chain citizenchain \
  --name oracle-citizenchain-01 \
  --base-path /home/ubuntu/citizenchain-data \
  --listen-addr /ip4/0.0.0.0/tcp/30333/wss \
  --rpc-port 9944 \
  --rpc-methods Safe \
  --no-prometheus \
  --in-peers 32 \
  --in-peers-light 100 \
  --out-peers 8 \
  --max-parallel-downloads 5 \
  --no-mdns \
  --mining-threads 2
```

参数说明：
- `--chain citizenchain`：显式加载冻结正式 chainspec
- `--name`：设置节点名称，便于识别
- `--base-path`：指定数据库、网络密钥、keystore 等本地数据目录
- `--listen-addr`：只把 `30333/TCP` 作为公网 WSS/libp2p 入口
- `--rpc-methods Safe`：RPC 保持回环监听并限制为安全方法；禁止追加 `--rpc-external`
- peer 数量参数：固定当前 SDK 默认安全基线，扩容前必须压测
- `--mining-threads 2`：启用 2 个 CPU 挖矿线程

##### 9.3 关于链规格说明
- 生产部署必须使用 `citizenchain` 或省略 `--chain`。
- `dev`、`local`、`staging` 当前也会加载同一份冻结 chainspec，不会创建另一条临时链。
- `citizenchain-fresh` 只供本机 bake 流程使用；`mainnet` 不是内置链标识。

##### 9.4 启动后预期行为
- 节点开始连接 chain spec 中内置的 bootnodes
- 首次启动若本地无 `powr` 密钥，会自动生成
- 节点开始同步区块，并在满足条件时参与 PoW 出块

##### 9.5 验证节点是否正常运行
可观察塔塔控制台日志中是否出现以下类型信息：
- 已启动网络服务
- 已连接到其他 peers
- 正在同步或导入区块
- 已启动 RPC 服务

如需停止前台运行，使用：

```bash
Ctrl+C
```

##### 10.1 服务器本机防火墙

```bash
sudo ufw default deny incoming
sudo ufw default allow outgoing
sudo ufw allow 30333/tcp
sudo ufw enable
```

说明：
- `30333/tcp`：CitizenChain P2P 通信端口
- 没有运维需求时不开放 `22/tcp`；临时 SSH 必须限定固定来源 CIDR，并在操作完成后删除规则

##### 10.2 Oracle Cloud 塔塔控制台安全规则
Oracle Cloud Network Security Group 只创建一条固定公网业务入站规则：

- 来源：`0.0.0.0/0`
- 协议：TCP
- 目标端口：`30333`

不得为 `9944`、`9615`、`8964`、`5433` 创建公网入站规则。没有运维需求时也不得为 `22` 创建公网规则。

##### 10.3 RPC 与 Prometheus 安全边界
- 默认 RPC：`127.0.0.1:9944`
- Prometheus：权威引导节点默认使用 `--no-prometheus` 关闭
- OnChina：节点启动时默认不启动；启用时也只能绑定本机或受控私网

禁止直接暴露以下端口到公网：
- `9944`
- `9615`
- `8964`
- `5433`

##### 10.4 权威引导节点安全边界

国储会和后续 43 个权威引导节点使用相同边界：

- `30333/tcp`：CitizenChain P2P 通信端口。
- `22/tcp`：默认关闭；确需运维时只通过 Oracle Bastion、Cloudflare Access 或固定来源临时放行。

权威节点的 `9944` 必须保持回环监听。后续 Worker 访问国储会本机 RPC 时使用服务器主动出站的 Cloudflare Tunnel，仍不开放任何 RPC 入站端口。

##### 10.5 国储会私有 RPC Tunnel

国储会节点是 44 个权威 bootnode 中的第 1 个，也是首期唯一 Worker 链上游，不另建 Cloudflare 节点或独立 RPC 节点。Cloudflare 只运行边缘 Worker、Access 和 Tunnel 控制面；安装在国储会服务器上的 `cloudflared` connector 主动建立出站连接，并把受控请求转发到同机 `127.0.0.1:9944`。

2026-07-12 实查 Cloudflare 控制面后的当前状态：

- 远程管理 Tunnel `nrcgch-rpc` 健康，运行 1 个 connector。
- 唯一链入口为 `chain.crcfrcn.com` 的 Access 保护路径，Tunnel 转发到 `127.0.0.1:18080` 固定方法网关，网关再连接本机 `127.0.0.1:9944`。
- Access 使用 `chain` 自托管应用、`CitizenChain` Service Auth 策略和唯一链服务令牌；Worker 的 `CHAIN_URL`、`CHAIN_ID`、`CHAIN_SECRET` 只保存在远端 Secret。
- 外部调用方 协议行的「Release」复用本机 Keychain 中同名三项生产配置，经固定方法网关
  读取 `chain_getFinalizedHead`、`chain_getBlockHash(0)` 和 finalized 头对应的
  `state_getRuntimeVersion`；不再依赖本机 `NODE_WS`，也不把 P2P `30333` 当作 RPC。
- 不增加 `Everyone`、交互式 `Allow` 或 `Bypass`，也不把令牌、Tunnel token 或完整私有 URL 写入仓库、安装包、日志或命令文档。

服务器部署顺序：

1. 先在服务器本机确认 CitizenChain 正常运行，`9944` 只监听 `127.0.0.1`，`30333` 监听 `0.0.0.0`。
2. 使用 Cloudflare 塔塔控制台为既有 `nrcgch-rpc` Tunnel 添加 Linux connector；Tunnel token 只在服务器 root 会话中使用，不写入仓库或普通用户配置。
3. 把 `cloudflared` 安装为 systemd 服务并启动，确认服务开机自启、connector 状态为 Healthy。
4. 不携带 Access 服务令牌访问链保护路径必须被拒绝；携带 Worker 专用服务令牌后必须到达固定方法网关。
5. 从公网确认 `30333/TCP` 可连接且 `9944/TCP` 不可连接，再通过 staging Worker 验证固定链读取方法；全部通过前保持 `CHAIN_EXTRINSIC_RELAY_ENABLED=0`。

`cloudflared` 不是区块链节点，不参与共识、P2P、Runtime 或 RocksDB，也不要求 Oracle 开放任何 Cloudflare 入站端口。首期只连接国储会这一台上游；后续容灾节点必须使用独立 Tunnel、独立 Access 凭证和显式故障切换策略，不连接全部 44 个 bootnode。

##### 11.1 创建服务文件

```bash
sudo nano /etc/systemd/system/citizenchain.service
```

写入：

```ini
[Unit]
Description=CitizenChain Node
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=citizenchain
### 权威引导节点只公开 P2P；RPC 保持回环监听。
ExecStart=/usr/bin/citizenchain \
  --chain citizenchain \
  --base-path /opt/citizenchain/data \
  --node-key-file /opt/citizenchain/data/node-key/secret_ed25519 \
  --rpc-port 9944 \
  --rpc-methods Safe \
  --no-prometheus \
  --state-pruning archive \
  --trie-cache-size 268435456 \
  --listen-addr /ip4/0.0.0.0/tcp/30333/wss \
  --in-peers 32 \
  --in-peers-light 100 \
  --out-peers 8 \
  --max-parallel-downloads 5 \
  --no-mdns
Restart=always
RestartSec=5
LimitNOFILE=65536
UMask=0077
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/opt/citizenchain/data
ProtectKernelTunables=true
ProtectKernelModules=true
ProtectControlGroups=true
RestrictSUIDSGID=true

[Install]
WantedBy=multi-user.target
```

说明：
- 安装前创建 `citizenchain` 专用系统账户和 `/opt/citizenchain/data` 数据目录。
- 把编译产物安装为 `/usr/bin/citizenchain`；节点只允许写入自己的数据目录。
- 禁止在 `ExecStart` 中恢复 `--rpc-external`、`--unsafe-rpc-external` 或 `--rpc-cors all`。
- 仓库中的标准模板为 `citizenchain/node/citizenchain.service`，服务器配置必须与其保持一致。

##### 11.2 重新加载并启动服务

```bash
sudo systemctl daemon-reload
sudo systemctl enable citizenchain
sudo systemctl start citizenchain
```

##### 11.3 查看服务状态

```bash
sudo systemctl status citizenchain
```

##### 11.4 查看实时日志

```bash
journalctl -u citizenchain -f
```

##### 12.1 检查进程是否存活

```bash
ps aux | grep citizenchain
```

##### 12.2 检查端口监听

```bash
ss -lntp | grep 30333
ss -lntp | grep 9944
ss -lntp | grep 9615
```

预期结果：
- `30333` 监听 `0.0.0.0`，可从公网完成 TCP/WSS 连接。
- `9944` 只能监听 `127.0.0.1` 或 `::1`，不得出现 `0.0.0.0:9944`。
- `9615` 不应存在监听。
- 从外部网络连接 `9944`、`9615`、`8964`、`5433` 和未启用运维时的 `22` 必须失败。

##### 12.3 常见问题一：编译失败
可能原因：
- 系统依赖缺失
- Rust 工具链未正确安装
- 无法访问 GitHub 拉取依赖
- 机器内存不足导致编译过程中断

优先检查：

```bash
rustc --version
cargo --version
free -h
df -h
```

##### 12.4 常见问题二：节点无法连入网络
可能原因：
- Oracle Cloud 安全组未放行 `30333/TCP`
- 本机 `ufw` 未放行 `30333/TCP`
- 节点无公网出口，无法访问 bootnodes
- DNS 解析异常，导致 `/dns4/...` bootnode 地址无法解析

优先检查：

```bash
ping github.com
nslookup nrcgch.crcfrcn.com
ss -lntp | grep 30333
```

##### 12.5 常见问题三：节点启动后马上退出
可能原因：
- `ExecStart` 路径错误
- `WorkingDirectory` 不正确
- 数据目录权限错误
- 端口被占用

优先检查：

```bash
sudo systemctl status citizenchain
journalctl -u citizenchain -n 100 --no-pager
```

##### 12.6 常见问题四：同步慢或资源占用高
可能原因：
- CPU 核数不足
- 内存不足
- 磁盘性能较差
- 同时开启的 `mining-threads` 过多

建议：
- 先把 `--mining-threads` 设置为 `1` 或 `2`
- 确保至少 4GB 内存，最好 8GB 及以上
- 尽量使用性能更好的块存储

#### 13. 生产部署建议
- 使用固定 `--base-path`，不要用 `--tmp`
- 使用 `systemd` 进行常驻托管
- 国储会和后续 43 个节点都是权威 bootnode，统一只开放公网 `30333/TCP`
- 不把任何权威节点 RPC `9944`、Prometheus、OnChina、数据库或管理端口暴露到公网
- Worker 首期只通过 Access + 独立 Tunnel 访问国储会节点的本机 RPC，不另建独立 RPC 节点
- 定期检查磁盘使用量、日志和同步状态
- 在升级节点版本前，先保留数据目录和服务配置备份

#### 14. 最小可执行部署清单
如果只需要最短路径完成部署，可按以下顺序执行：

```bash
sudo apt update
sudo apt upgrade -y
sudo apt install -y build-essential clang cmake pkg-config libssl-dev git curl
curl https://sh.rustup.rs -sSf | sh -s -- -y
source "$HOME/.cargo/env"
rustup default stable
git clone <YOUR_GMB_REPOSITORY_URL>
cd citizenchain
: "${PROTOC:?请先交付已验真的绝对 protoc 35.0 可执行路径}"
test -x "$PROTOC"
export CARGO_TARGET_DIR="$(node scripts/build.mjs temporary-root '' macos)/cargo-target"
cargo build --release -p node
sudo useradd --system --user-group --home-dir /opt/citizenchain --shell /usr/sbin/nologin citizenchain
sudo install -m 0755 "$CARGO_TARGET_DIR/release/citizenchain" /usr/bin/citizenchain
sudo install -d -o citizenchain -g citizenchain -m 0700 /opt/citizenchain/data
sudo install -m 0644 node/citizenchain.service /etc/systemd/system/citizenchain.service
sudo systemctl daemon-reload
sudo systemctl enable --now citizenchain
```

若 `citizenchain` 系统账户已经存在，跳过 `useradd`，不得删除或重建现有账户。

#### 15. 文档维护边界
- 若 `node` CLI 启动参数发生变化，应同步更新本文。
- 若 chain spec、默认端口、bootnodes 或运行模式发生变化，应同步更新本文。
- 若后续引入 Docker、安装器、自动化部署脚本，应新增专门部署文档，不直接覆盖当前源码编译部署口径。

---

### OnChina 公民/私权/公权机构 链↔注册局本地库一致性 技术方案 (v2)

状态:设计定稿 v2 + **已实现并测试**(2026-07-24)。任务卡 08-tasks/链服务私有链.md。
关联:[[onchina-chain-projection-asymmetry-citizen-gap]]。

实现落点:
- 创世回填(§2.5):`domains/genesis_projection.rs`(程伟联邦播种 + 基金会绥阳回填,DO NOTHING 幂等)。
- 投影纯逻辑 + 保正本:`domains/projection.rs::merge_citizen_record / merge_institution_record`(9 单测,含两类保正本)。
- M1 indexer 增量:`core/chain_runtime.rs::read_chain_citizen_detail` + `indexer/event_parser.rs::collect_entity_projection_cids` + `indexer/worker.rs`(每块投影 + 作用域解析,仅城市节点)。
- M3 联邦 drill-in:`chain_runtime` 两个 scoped 扫描 + `projection::drill_in_project_scope` + `projection::drill_in_project_city`(POST `/api/admin/registry/drill-in-project`,联邦+省级访问控制)。
- 验证:onchina `cargo test` 138 全绿(含 9 投影);链读/事件/HTTP 为编译级验证(无活链运行时),保正本/作用域/映射核心逻辑离线单测覆盖。
- 未做(前瞻/前端):待补档 PENDING 推链门(创世方案已绕开,不需要);M3 前端触发按钮(Step5 前端)。

#### 0. 角色

- **市/县注册局(CREG)**:每市一节点一本地库,`is_tier1_registry`=false,作用域=本(省,市)。
- **联邦注册局(Tier1/FRG)**:**全国仅一节点一本地库**;43 个省组管理员,每人 `scope_province_name` 锁定一省,只能进本省下辖的市,直接在某市管理公民/私权/公权。
- **正本 vs 副本**:某注册局采集/创建的链下资料=该局**正本**;链上数据=各局共享**副本**。

#### 1. 两类实体的根本差异(CID 结构决定,已核实)

CID 的 r5 段 = 省码(2)+ 市码(3)(number.rs:88-92)。

| | CID 市码段 | 真实市来自 | 属主判定 |
|---|---|---|---|
| **公权/私权机构** | 真实市码(如 `GZ018`、`ZS001`) | **CID 本身** | CID 自带市 → 市是唯一属主 |
| **公民** | `000`(省级占位,如 `GZ000-CTZN6`) | **链上 `residence_city_code`**(占号 `occupy_cid` 时写入 `CidRegistry`) | 无 CID 强制属主;正本随**采集/注册的注册局** |

##### 机构(公权/私权)
- 联邦或市创建 → **市注册局补链下档案(待补档)**;联邦只建链上,市补链下;两边**对齐链上数据**。
- 链下正本永远在**市**(CID 属主)。

##### 公民
- **档案真源在"注册该公民的注册局"**:本市办的 → 正本在本市;联邦办的 → 正本留联邦。
- 各市注册局**按作用域(链上 residence)各自维护自己的公民**,拿到的是**链副本**(姓名/居住地/护照窗口/状态,足够列表与资格判断);不强制向市补档。
- 联邦办的公民:联邦持链下正本 + 占号上链(residence=该市)→ 该市按 residence 回填链副本。

#### 2.5 创世实体特例落地(本次立刻要修的两个,已定死)

创世时直接上链、未走注册局流程的仅两个,按各自 CID 特性分开落地(去掉早前"程伟塞进某市 + 待补档 + PENDING"的复杂做法):

- **基金会 `GZ018-SFGYR`(私权,CID 带市码 018=绥阳)** → 落 **绥阳市注册局(城市节点)**。绥阳节点(scope=GZ/绥阳)启动时 `institution_lookup(基金会CID)` → `INSERT subjects(PRIVATE) DO NOTHING`。绥阳看基金会时"法定代表人"字段自带程伟姓名(链上 `legal_representative`)。联邦贵州组管理员进绥阳市时经 M3 也能看到基金会。
- **程伟 `GZ000-CTZN6`(公民,CID 无市码,无 citizen-identity,无居住地)** → **直接落联邦注册局本地库**,置于 **province=GZ(贵州)、city=绥阳市码(018,与基金会同市)**。联邦贵州组管理员**进绥阳市→显示程伟**,进贵州其他市→不显示。**不加省级/创世特殊视图**——他就是联邦库里一条普通"省+市"公民记录,现有列表/搜索(`GZ000-CTZN6`)即可见。绥阳城市节点本身不持有程伟(他是联邦的)。姓名/CID/账户取自 `primitives` 创世常量 + 链上 `legal_representative` 权威值;`INSERT citizens + subjects(CITIZEN) DO NOTHING`。
- 两条均**纯新增、DO NOTHING、不动现有写入器与推链门**;程伟不用 PENDING/待补档,故**不改** `canPushOnchain`。
- M1/M3 仍是普通实体的通用机制,不受这两个特例影响。

#### 3. 发现机制(市侧如何知道"我市有这个实体")

- **机构**:按 **CID 市码**。联邦一上链,市按 CID 市码即可发现。
- **公民**:按 **链上 `residence_city_code`**(占号写入 `CidRegistry`)。**公民只有上链(至少占号)后,市才可发现**;纯本地未上链的联邦公民不属于任何市、也不该被发现。
  - 规则:联邦新建、应归某市的公民,**必须占号上链(带 residence)**才落到该市。

##### M1 · indexer 事件增量(各市注册局稳态主力)
- 每个 CREG 节点 indexer 订阅 finalized 区块;对本作用域事件 upsert:
  - 公民:`CidOccupied`/`VotingIdentityRegistered`/`VotingIdentityUpdated`/`CandidateIdentityUpgraded`/`CandidateIdentityUpdated`/`CitizenIdentityRevoked`/`CidRevoked` → 读该 CID 的 `CidRegistry`+`VotingIdentityByCid` 取 residence,`residence_city`==本市则 upsert。
  - 私权机构:私权创建/更新事件 → 读 `PrivateManage::Institutions[cid]`,CID 市码==本市则 upsert。
- 断点续读(记 last_processed_block):重启只补处理停机期间的新区块,**非全表扫**。
- 联邦省组管理员在某市新建的公民 X:X 上链的区块 finalized → 该市 indexer 近实时拿到 X 链副本。

##### M2 · 启动创世一次性读(创世实体不在事件流,在 block0 状态)
- 节点启动,对 `primitives::cid::china` 已知创世 CID(基金会 GZ018-SFGYR;程伟 GZ000-CTZN6),做**单点状态读**:
  - CID(机构)/residence(公民)∈本作用域 且本地无 → 回填。
  - 只有属主市(绥阳)命中并插入;别的市对 2 个已知 CID 做一次廉价 scope 判断即跳过。**不是全国全扫。**
- 幂等:PK 存在跳过。

##### M3 · 联邦 drill-in 按市投影(联邦无 indexer,按需)
- 联邦单节点不跑全国 indexer;省组管理员进入本省某市 → 对该市做**按作用域链读**投影进联邦本地库:
  - 公民:扫 `CidRegistry` 过滤 `residence`==该市 → 逐 CID 读明细 upsert。
  - 私权:扫 `PrivateManage::Institutions` 过滤 CID 市码==该市 → upsert + 账户;含 legal-rep-only 公民(如程伟)。
  - 公权:已有 `sync_gov_chain_projection`(全国),按市过滤展示即可,无需新读。
- 幂等:(省,市)anchor 记 finalized head,未变跳过;可手动刷新。
- 访问控制:目标省==管理员 `scope_province_name`,市属该省,越省 403。
- **触发点(as-built 2026-07-24)**:M3 私权投影已接进读取路径
  `institution/subjects/registration.rs::list_institutions_inner`——联邦(Tier1)管理员按市查私权列表时,
  先 `projection::drill_in_project_private_scope((省,市))` 再查库。链读失败只 warn 不阻断(fail-open)。
  → 解决"联邦贵州组管理员进绥阳市看不到基金会"报障:基金会正本在绥阳市注册局,联邦按需 drill-in 才可见,非播种。

> 稳态:各市靠 M1;创世靠 M2;联邦看外市靠 M3。全状态扫描仅极端兜底。

#### 4.5 正本补档:公民编辑(不可变字段锁定,as-built 2026-07-24)

创世/待补公民(如程伟)投影落库后正本残缺(护照有效期空 → `computed_identity_status` 判注销),
需注册局补齐正本才能推链。为此加 `POST /api/admin/citizens/:cid/edit`
(`admin_entry::admin_update_citizen`):

字段可变性按"现实是否可变"定死:
- **可变**:姓、名、居住市、居住镇、voting_eligible(人会改名、会搬家)。姓名不进锁定集,
  handler 直接取 input(必填非空);居住市/镇限本省内改(按 existing.province_code 校验归属)。
- **不可变**(现实不可变,初始化后永久锁定):性别、出生日期、出生地(省市镇)、护照号。
  `lock_immutable`——现存非空即锁定拒改,空则允许初始化,存成功后永久锁定。
- **护照号/有效期**:不接受前端直填,服务端确定性签发(`allocate_passport_no` + 出生日期派生年限,
  与建档同源,命名空间用最终居住省市);现存已签发则锁定原样保留。保护护照号唯一性与有效期口径。
- **固定不动**:居住省 province_code(= CID 省 = 分区键)、公民 CID(主键)。
  **跨省居住迁移属"跨地区"(分区迁移 + 注册局交接),本入口不做,后续单独处理**。
- 链投影字段、账户、状态、onchain_*、创建人、created_at 一律保留(仍守保正本铁律,§10)。
- 前端 `citizens/EditCitizenModal.tsx`:姓名/居住市镇可编辑;性别/出生日期/出生地按现存值锁定
  (非空只读、空放开级联录入);居住省只读;居住市为固定省下级联 Select。

#### 5. 字段映射 · 公民(citizens)

| 列 | 正常公民(有 citizen-identity) | 待补/联邦办公民 |
|---|---|---|
| cid_number | CID | CID |
| province_code | CID 省 / residence 省 | 同 |
| city_code | **链上 residence_city_code** | 同 |
| town_code | VotingIdentity.residence_town | '' |
| account_id | AccountIdByCid | AccountIdByCid / legal_rep.account_id |
| citizen_status | VotingIdentity(NORMAL/REVOKED) | PENDING(仅程伟这类无 identity 者) |
| voting_eligible | 派生(NORMAL+护照窗口) | false |
| passport_valid_* | VotingIdentity | '' |
| family/given/sex/birth_* | CandidateIdentity(有则) | legal_rep 姓名(程/伟)其余空 |
| passport_no/archive_hash/documents | **链下正本,投影不写** | '' |
| creator_account_id | 公民自身账户(来源锚点) | 同 |

- 市侧拿到的是**链副本**;链下正本(passport_no/证件)在**采集局**(本市或联邦),不跨节点同步。
- 程伟(无 citizen-identity,仅基金会法定代表人)→ PENDING 待补档,姓名从 `legal_representative` 取。

#### 6. 字段映射 · 私权机构(subjects kind=PRIVATE + 明细)

- subjects:cid、kind='PRIVATE'、全称/简称、省/市/镇码(**CID 直接给省市**)、机构码、法人标记、`legal_representative_*`、created_at。
- 账户:`InstitutionAccounts` → 私权账户明细。
- 链下(证件照等)由**市补档**,投影不写。

#### 7. chain_runtime 新增

- `for_each_chain_private_institution(cb)` + `read_chain_private_institution(cid)`(镜像公权,码校验 `is_private_legal_code`)。
- `for_each_chain_citizen(scope, cb)`(扫 `CidRegistry` 过滤 residence)+ `read_chain_citizen_detail(cid)`(Voting/Account/Candidate 单点)。
- `read_foundation_legal_representative(inst_cid)`(取程伟)。
- 复用 `for_each_chain_institution_account`(已支持 PrivateManage)。

#### 8. indexer 扩展

- `event_parser.rs`/`worker.rs`:识别公民/私权/私权机构相关事件 → 以 CID 为线索**读状态 + scope 过滤 + upsert**;记 `last_processed_block` checkpoint,重启续读。
- 联邦节点不启用该 scope indexer(按 M3 drill-in)。

#### 9. 本地库 schema

- 复用 subjects/citizens/私权明细。
- 新增 `indexer_checkpoint(node_scope, last_block, updated_at)`(M1 续读)。
- 新增 `projection_anchor(domain, province_code, city_code, genesis_hash, block_hash, block_number, synced_at, PK)`(M3 幂等)。
- `citizen_status` 允许 `PENDING`(现 DDL 无 CHECK);前端列表加"待补档"徽标,不隐藏。

#### 10. 保正本 UPSERT(铁律)

```
INSERT ... ON CONFLICT (province_code, cid_number) DO UPDATE SET
  <仅链上来源列> = EXCLUDED.<...>
-- 链下正本列(passport_no/archive_hash/documents/证件/市补录) 不进 UPDATE SET → 保留
-- 姓名:EXCLUDED 为空则保留本地(NULLIF/COALESCE)
```
机构同理。任何"整行写"都会抹掉链下正本 —— 最易埋 bug 处。

#### 11. 访问控制

联邦 Tier1:省组管理员 `scope_province_name` 锁省;M3 目标 city 必属该省;跨省 403(复用 `get_visible_scope`/`includes_province`)。

#### 12. 分步实现(一次交付,内部)

1. 私权机构链读 + subjects(PRIVATE)/明细 upsert(基金会可见,市补档)。
2. 公民创世一次性读 M2 + 待补档态(程伟可见)。
3. indexer 扩展 M1(各市稳态自维护,含联邦新建公民近实时到市)。
4. 联邦 drill-in M3 端点 + anchor + 省级访问控制。
5. 前端:待补档徽标;补档完成放行 PENDING 推链;联邦选市触发 M3。

#### 13. 测试

- 单测:字段映射;PENDING 待补档;**保正本 UPSERT(补录后再 upsert 不覆盖)**;residence/CID 市码 scope 过滤;越省 403;indexer 事件→scope→upsert;checkpoint 续读。
- 集成(离线 fake chain):创世回填→程伟+基金会可见;联邦某市新建公民→该市 indexer 拿到链副本;机构联邦创建→市待补档→补齐→对齐链上;M3 drill-in 投影一市全量。

#### 14. 规模 / 回滚

- 稳态零全表扫(M1 事件增量 + checkpoint);M3 按市 on-demand + anchor 缓存;M2 仅少数已知创世 CID。
- 回滚:投影只写本地库,清表/清 checkpoint/anchor 即回滚,不触链。

---

#### 1. 设计目标

CitizenChain 必须被视为一个完整的软件产品，而不是若干松散目录的拼接。

对外发布时：

- 节点程序与节点 UI 为同一个桌面安装包
- 用户只下载一个安装包
- 节点核心和节点界面版本必须保持一致

#### 2. 顶层目标结构

```text
citizenchain/
  node/
  runtime/
    governance/
    issuance/
    misc/
    transaction/
    primitives/
  packaging/
  docs/
```

##### `node/`

- 节点核心程序
- 链连接、RPC、节点服务能力
- 与 runtime 编译产物的集成
- 当前唯一桌面节点 UI 与桌面壳
- 负责节点启停、本地设置、状态展示与桌面打包
- 对外发布的桌面节点产品入口
- 桌面端 Rust 后端模块放在 `node/src/<功能名>`，不再保留 `node/src/ui` 目录层
- 历史 `node/` 与独立 `node/` 能力已统一收口到此目录

##### `runtime/`

`runtime/` 是链上规则和运行时能力的统一目录，内部固定为：

```text
runtime/
  governance/
  issuance/
  misc/
  transaction/
  primitives/
```

其中：

- `governance/`：治理相关 pallet
- `issuance/`：发行相关 pallet
- `misc/`：其他链上功能模块
- `transaction/`：交易相关 pallet
- `primitives/`：runtime 内部常量、基础类型、运行时组织层

#### 4. 关于 `runtime/primitives/` 的说明

`citizenchain/runtime/primitives/` 已经承载原先仓库根目录 `primitives/` 的 Rust crate 与数据文件。

因此当前约束为：

- 运行时共享常量、基础类型、制度保留地址等统一放在 `citizenchain/runtime/primitives/`
- 其他 crate 如需依赖该能力，统一引用 `citizenchain/runtime/primitives`
- 后续新增 runtime 常量与基础类型，不再回到仓库根目录新增独立 `primitives/`

#### 5. 迁移原则

- 先定目标结构，再迁移代码
- 先补文档和规则，再做目录移动
- 迁移时保持构建可验证
- 迁移时不得影响单安装包发布目标
- 旧桌面目录只有在功能、脚本和文档全部收口后才能删除

---

### chainspec 与创世状态冻结规则（铁律）

> 适用范围：全节点 `citizenchain.json`、正式创世审计 `genesis-state/`、CitizenApp
> `assets/chainspec.json`、`assets/light_sync_state.json`，以及任何分发给轻节点 / 钱包 / App 的创世锚点文件。

#### 结论

主网创世那一刻必须由同一次 release bake 同时冻结六组资产：

1. **全节点 plain SSOT**：`citizenchain/node/citizenchain.json`。
   它只保存 runtime WASM、genesis patch、bootNodes、properties 和 protocolId。
2. **正式创世审计状态包**：`target/chainspec/genesis-state/manifest.json` 与
   `target/chainspec/genesis-state/chains/citizenchain/db/**`。它记录 release 块 0、CI
   provenance 和交叉哈希，只保存在忽略产物目录；四平台安装包不携带 RocksDB。
3. **CitizenApp 轻形态 chainspec**：`ci…170007 tokens truncated…世快照缓存 + 链投影增量更新”。
- 重新创世部署(6 节点 mesh);创世后重跑 CitizenApp 公权机构快照包生成器。
- 旧全量镇级创世资产已废弃。2026-07-16 的
  `genesis_hash=0x840d5b12c541a010783e54069c9168a13d102ba63cd8f3a00263440c1803aad9`
  只保留为历史冻结记录，已被 2026-07-26 正式创世替代。
- 当前 49,593 个公权机构的唯一正式创世基线由 runtime 源提交
  `9c2ec97b91b3236c6268ddd3057a4700a4591cd2`、GitHub `CitizenChain WASM` run
  `30721127038` 和冻结资产提交 `7cea3885783064b5c02850e23d48e41e1fce7065` 生成：
  `genesis_hash=0x157558224b682de0384fd50dea0735aff55795f6d145993233c901cf1258671d`、
  `state_root=0x363d9c4836875a1a8270940caef743524350a6341199ec75966c3b25065bbe80`、
  `runtime_wasm_hash=2b329862be596f8844457452c37f0beac89c80fec22b101132b35e1b04324a36`、
  `chainspec_hash=b239671c5ed930d39ed69aea9fcc09bfaacc299f3456d19af0a3ed61ab2f3e9c`、
  `public_institution_root=c21f99f5bd40bc3c9fcee9439de9f6902c98212b2510dd7440c9630284ab939f`。
  2026-07-26 的 `genesis_hash=0xe8f4067de2323dc27b2a2c409fa4b3ab882e4e88dfa6f4a81355f51f8cf8eb45`
  和 2026-07-31 的 `genesis_hash=0x278e68bced2dabf9690701188272da22d216fdaa2c617e7dcbe100df3e8bcbfa`
  都只保留为历史冻结记录，已被 2026-08-01 正式创世替代。

##### 4.4 规模账(终态)

| 层 | 构成 | 数量 |
|---|---|---|
| 常量直铸 | 国家单体+国家参众议会+联邦部委署局+新增宪法国家级机构+省核心治理 6 类×43 | 296 |
| 模板派生 | 省级部门 11 类 × 43 省 | 473 |
| 模板派生 | 市级 17 类 × 2,872 市 | 48,824 |
| 非创世运行期注册 | 镇级 14 类 × 39,087 镇 | 547,218 |
| **创世合计** | **国家/省/市创世,零交易零手续费** | **49,593** |

#### 五、onchina 端

- **组装+提交通路**:OnChina 以实时 nonce 重建并本地验签，`system_dryRun` 通过后用
  submit-and-watch 跟踪目标 extrinsic；只有 finalized 且该 extrinsic 出现
  `System.ExtrinsicSuccess` 才进入回写。提交前持久化 tx hash/锚点，恢复扫描每批最多
  128 个 finalized 块并保存游标，禁止无界回扫与盲目重放。
- **事件回写闭环**:目标 extrinsic 成功后，在同一 finalized block 读取并严格解码
  `CidRegistry`、`AccountIdByCid`、`BindingRevisionByCid`；成功标记必须先于本地
  `citizens/subjects` 投影保存。投影失败保留会话，下一次只补投影。
- **公民建档时序(卡2)**:

```text
录入档案 → 生成号(种子 + nonce 0..999 重试,治愈同名同生日碰撞死局) → [可选 RPC 预查]
→ 用户钱包签完整首次绑定授权(创世/CID/revision=0/过期时间/自选账户)
→ 管理员冷签 occupy_cid(单笔)
→ OnChina 组装+dry-run+submit-and-watch → finalized + ExtrinsicSuccess
→ 同 finalized 块核对 Active+目标账户+revision=1+办理注册局+blake2_256(account_id)
→ 先保存 finalized 标记，再写 citizens 投影(含 tx_hash/块高)
→ 断线按持久化游标分批恢复；投影失败只补投影，不重提
清档 → revoke_cid(PasskeyColdSign)→ 链上墓碑 + 本地清档
```

- **机构册只读投影(卡3,2026-07-04 修订)**:公权机构唯一真源是链上
  `PublicManage::Institutions` / `InstitutionAccounts`。OnChina 不再按 primitives 或
  `china.sqlite × 模板` 本地物化公权机构,只能从链上读取并写入 PostgreSQL 查询缓存;
  链不可达、创世哈希不匹配或链上目录不可读时 fail-closed。运行期新增机构走占号先行路径落库;
  旧 `sfid` 库删除,全仓零 `sfid_number` 残留。

#### 六、执行顺序

1. **卡2**(`公民身份流程.md`):3.1 + 3.2 + 3.3 + onchina 提交通路/回写/建档流程——与卡1 同一 runtime 版本;
2. **卡3**(`公民身份创世.md`):4.1-4.3 国家/省/市直铸 + 部署形态改造 → 重新创世 → CitizenApp 公权机构快照包重跑 → onchina 链投影同步;
3. 终态对账:链上创世 Institutions = 49,593(genesis 测试断言与推导值一致),onchina 本地库 ↔ 链上两方一致。

#### 七、影响

- runtime breaking,重新创世,零兼容零残留。
- 公民建档依赖链活性(fail-closed),每单一次用户账户控制签名和一次管理员冷签；链上不保存姓名生日等档案全文。
- CitizenApp/CitizenWallet:公民确认签名(ACTION_CITIZEN_IDENTITY)与扩展尾规则不变;链交易提交动作从钱包端移到 onchina 后端。
- 节点首启复制官方创世状态包并等待 RPC ready;GenesisBuilder 本地物化仅作开发/排障兜底。

#### 八、备选方案(均已否)

- **建档只做链下查询防重**:TOCTOU 竞态防不住并发,查询不是仲裁。
- **清档删除链上号**:删除即可复用,污染历史指认;墓碑成本极小。
- **存量走批量交易上链**(v1 方案):数十笔批量交易+冷签+迁移窗口,全是多余复杂度;"直铸不可行"的旧判断只对 raw chainspec 形态成立,改 plain spec + 官方创世状态包后直铸零交易更简。
- **占号交易由节点热键自动签**:违背「鉴权真源=链上 Active 管理员集合+冷签 origin」安全边界。

#### 九、风险与防护

- **模板/行政区常量漂移**:导出工具幂等 + genesis 测试断言数量/名称与推导值一致 + 构建期逐号 parse 断言。
- **smoldot 轻端 chainspec 形态**:stateRootHash 形态需在 CitizenApp 真机验证(卡3 验收项)。
- **首启构建性能**:正式安装包内置官方创世状态包,用户首启先复制链数据库;GenesisBuilder 本地物化落库仅作为开发/排障兜底,不得作为正式用户默认路径。当前 plain spec 仍会被 Substrate 用于创世块校验,可能产生分钟级 CPU 窗口,以 `chain_getBlockHash(0)` 作为唯一可用标准。
- **占号规模**:公民占号随建档线性增长,`CidRegistry` 条目 ~100B/人,亿级人口 ≈ 数十 GB 级远期 state——链上极简字段已是下限,属注册局链上化的固有账,创世期无感。

#### 状态

- 2026-07-03:**机构信息可维护补齐**(卡 20260703-institution-info-update-and-add-account)——链是机构信息唯一真源(公权/私权统一),私权名改为上链;entity 两 pallet 加 `update_institution_info`(改全称/简称,机构码/CID/省市码物理编码在 CID 里不可改)+ `add_institution_account`(存量机构新增账户,派生地址上链),注册局授权;创世只铸初始版本,今后改名/加账户/新增机构走交易。public 38+private 37 测试绿。剩 onchina 冷签流程/App reconcile/internal-vote 自治路径。

- 2026-07-03:**卡3 代码全部完成**——plain spec 部署形态、smoldot 轻形态、onchina 启动抽样对账+audit-chain-catalog 全量比对、同源年份钉死、runtime 全量断言(抓修 193 常量漏铸)。旧扩大创世口径已被 v3 废弃,需按 49,593 重新验证。

- 2026-07-04:**部署口径更新**——正式节点不再要求每台机器首启全量 GenesisBuilder 物化;当时的冻结创世生成流程形成 plain spec、CitizenApp `stateRootHash` 轻形态和 `genesis-state/` 链数据库包;节点安装包内置该包,首启复制本地 DB 后等待 RPC ready;OnChina 启动前必须等 `chain_getBlockHash(0)` 成功。

- 2026-07-02:初版定稿(批量交易方案);同日完成嵌入式库旧机构清理。
- 2026-07-03:Q1-Q5 已决;卡1 完成归档;命名规则统一并验证;v2 曾定为扩大创世范围。
- 2026-07-03:**卡2 链端完成**(§3.1 CidRegistry+occupy/batch/revoke、§3.2 机构 Closed 墓碑+register 缺口封堵、§3.3 费类 Free)——citizen-identity 21/21、entity 34+34、citizen-issuance 12+5、runtime 30/30 全绿;全 runtime benchmarks 编译过(顺修 4 处既有断链)。
- 2026-07-03:**卡2 完工(onchina 侧 D6/D7/D8 完成,归档 done/)**——`core/chain_submit.rs`(组装+dry-run+提交+等进块+区块回查,QR 只签不提交)、`domains/citizens/occupy.rs`(两阶段占号 prepare/submit,nonce 碰撞重试+承诺哈希幂等续用+吊销墓碑,`chain_sign_sessions` 会话表)、D8 提交路径同步回写 onchain_* + `cid_registry_lookup` 链上预查、chain_identity complete 切 D7 会话、前端 useChainSign+两阶段 api+建档/吊销 UI;onchina 134 测试全绿、前端 tsc+build 过、node 不受影响。
- 2026-07-04:**卡3 口径更新为 v3**——`official_derive` 创世枚举只含省/市模板,直铸 296+49,297=**49,593**;镇级模板保留给注册局运行期注册,并通过 `town_code` 入链。12 个宪法国家级机构进入 `CHINA_ZF`;国家 NSN/NRP 进入 `CHINA_LF`;`china_zb` 制度保留地址同步增至 637。

---

#### 状态

Accepted（2026-07-08）

#### 背景

CitizenApp 是手机软件，需要在移动网络下快速、稳定地连接 CitizenChain 网络，同时快速、稳定地使用聊天和广场功能。国储会等核心区块链节点可能部署在 Oracle Cloud 等云服务器上，但这些核心节点必须降低公网暴露面，避免成为公民端流量和恶意请求的直接入口。

本 ADR 同时固定以下边界：

- CitizenWallet 不参与本架构讨论；CitizenWallet 只做离线冷钱包和扫码签名。
- CitizenChain 是一个整体安装包，由 `node`、`runtime`、`onchina` 组成。
- CitizenApp 必须保留内置轻节点能力，不能改成只访问 HTTP API 的中心化客户端。
- Cloudflare 只承接边缘入口、缓存、广场媒体、Chat 无内容唤醒与 WebRTC 建连信令、启动清单
  和受控转发，不运行 Substrate 节点，不保存用户私钥，不成为链上状态真源。

##### 1. 总体分层

目标架构采用四层协作：

```text
CitizenApp
  ↓
Cloudflare 边缘层
  ↓
Citizen API / OnChina 投影能力
  ↓
CitizenChain 云节点网络
```

各层职责固定如下：

- CitizenApp：内置 smoldot 轻节点，连接 CitizenChain P2P 网络；端上私钥只在本机签名；链上关键判断以 finalized 链状态为准。
- Cloudflare 边缘层：提供 DNS/WAF/限流、Worker API Gateway、广场 R2 媒体存储、D1/KV/DO
  边缘数据、Chat 无内容唤醒和 WebRTC SDP/ICE 信令、广场 feed、轻节点启动清单和服务健康信息。
- Citizen API / OnChina 投影能力：提供非链上查询、公开目录、业务聚合、受控链事件投影和已签名交易广播；不托管私钥，不替用户签名。
- CitizenChain 云节点网络：运行 `citizenchain/node + runtime + onchina` 安装包，承担 bootnode/full node/archive/indexer/RPC service node 等角色。

##### 2. 链连接真源

CitizenApp 的链上真源仍是内置轻节点通过 P2P 获取并验证的 finalized 链状态。

Cloudflare 启动清单可以帮助 App 更快进入可用状态，例如提供：

- 当前推荐 bootnodes。
- lightSyncState/checkpoint。
- Worker/OnChina 投影服务健康状态。
- 可选的受控交易广播入口。

这些信息只用于启动加速、服务发现和降级提示，不替代轻节点验证。

##### 3. P2P 失败时的产品状态

P2P 连接失败不等于整个 App 不可用。CitizenApp 运行态拆成三种：

- 正常：轻节点已连接 P2P，能够推进 best/finalized，链读写走轻节点。
- 降级：轻节点暂时无法连接 P2P，聊天、广场、公开目录和本地缓存继续可用；链上余额、投票资格、提案状态等关键状态标记为等待链同步或只显示最近 finalized 快照。
- 离线：网络不可用；只展示本地缓存和离线可完成的签名准备，不承诺链上状态。

已签名交易在降级状态下可以通过受控 API 广播到服务节点 RPC，但广播成功只表示节点已收到交易，不表示链上成功。链上成功仍必须以 finalized runtime storage 或事件确认为准。

##### 4. 云节点安全边界

国储会核心节点不得作为 CitizenApp 的公共 RPC 入口。

生产节点角色必须拆分：

- 核心/权威节点：持有必要出块、最终性或机构运行能力；只开放必要 P2P；RPC/Prometheus 只允许本机或内网访问。
- 公开 bootnode：只承担 P2P 发现和连接引导，尽量不持有关键业务私钥。
- RPC service node：供 Citizen API、Indexer、Worker 后端侧受控访问，必须经过反向代理、白名单、限流和审计。
- Archive/Indexer：服务历史查询、广场发布确认、公开投影和运维观测，不参与用户私钥保管。

##### 5. 聊天和广场

聊天采用 OpenMLS + Cloudflare 无内容唤醒/SDP/ICE 信令 + WebRTC DataChannel 设备直连
Envelope、KeyPackage 与附件 + 近场通信。消息、会话、MLS 密文、Envelope、KeyPackage 和
附件只存在于设备；Cloudflare D1 只保存操作系统推送端点，Durable Object 不保存 Chat 状态。
设备直连只使用公共 STUN 发现候选地址，不经过 TURN、HTTP、R2 或其它云端中继。

本节的 Chat 边界由 ADR-043 进一步固化；与 ADR-043 冲突的历史云端转发口径均以 ADR-043 为准。

广场继续采用当前正式路线：媒体文件存 Cloudflare R2，CDN 分发；CitizenChain 只保存发布所需的链上元数据、哈希、索引和费用结果；Worker/D1/KV/DO 承接登录、会员、上传、feed、推荐信号和发布确认。

当前 ADR 不引入 Matrix，不恢复区块链节点聊天，不把聊天消息写入链上。

##### 6. OnChina 边界

OnChina 是 CitizenChain 安装包内置能力，不是第五个产品。OnChina 可以提供局域网机构工作台、链上投影、公开目录和受控服务端聚合能力，但不得成为 CitizenApp 链上状态真源，也不得恢复旧独立后端结构。

#### 禁止项

- 禁止把 CitizenApp 改成所有链读写都依赖 `api.onchina.org` 的 API-only 客户端。
- 禁止让 Cloudflare Worker 持有或接触用户私钥。
- 禁止把国储会核心节点 RPC `9944` 或 Prometheus `9615` 直接暴露到公网。
- 禁止把 API 广播成功显示成链上交易成功。
- 禁止把 CitizenWallet 写入在线链连接、聊天或广场架构。
- 禁止把 Matrix 写成当前聊天目标路线。
- 禁止恢复区块链节点聊天、云端聊天内容存储或节点配对流程。

#### 影响

后续实施必须分步确认：

1. Cloudflare 启动清单 API。
2. CitizenApp 链连接状态机。
3. 已签名交易受控广播兜底。
4. Oracle Cloud 节点角色拆分与防火墙口径。
5. 聊天和广场生产硬化。
6. 安全、审计、限流和可观测性。
7. 真实运行态验收。

Runtime 修改授权统一见 `CitizenChainRuntime.md` 的“Runtime 修改与链协议边界”。

---

#### 标题

VotingEngine 使用 Track handler、异步业务执行和公平维护预算；后续岗位票据改造沿用同一 Track 边界。

#### 背景

VotingEngine 已统一内部、联合、立法和选举投票，但核心仍按具体 stage 维护多份派发分支；生产文件超过 800 行，清理状态机固定读取 `StorageMap::iter().next()`，真实 FRAME benchmark 也尚未覆盖五个投票 crate。项目将在改造完成后重新创世，因此继续保留开发期旧布局迁移会制造无用复杂度和错误入口。

#### 决策

- 先做不改变行为和 metadata 的物理拆分，再改存储和调度，最后基于稳定代码生成 benchmark。
- 核心引擎通过 runtime 注册的递归 Track tuple 派发 timeout、mode 数据清理和 mode 终态副作用，不再匹配每个具体 stage。
- 投票判定只写状态并登记业务执行队列；业务执行、自动终结和清理分别使用条数与 weight 双重预算。
- 清理使用两级 FIFO：固定保留期的延迟 FIFO 到期后进入就绪 FIFO；每次只删除一个有界 chunk，未完成项重新入队，避免大型公投阻塞其他提案。
- `citizen-identity` 只提供四级 `PopulationData` 和历史资格判断，是人口数据唯一真源；投票引擎在建案事务中读取该数据并生成自己的提案人口快照。联合公投、立法公投和 Popular 选举只消费投票引擎快照。
- Mutual 互选由业务模块指定目标 `RoleSubject`，投票引擎读取 entity 的岗位有效任职并冻结 `VoterSnapshot`；Track 不复制资格真源。提案创建后的身份或任职变化不得改变既有提案。
- 自动业务执行的回调错误、Ignored、结果应用错误和 Track 后处理错误统一递增 attempts、指数退避并在上限后 dead-letter；终态副作用使用独立队列，禁止重新执行业务动作。
- 宪法阈值继续引用 `primitives::constitution`，Track 只表达流程和派发，不复制宪法数学。
- 当前正式链尚未创世，全部 runtime 与 pallet storage version 在创世前统一归零；删除旧 storage alias、升级翻译和兼容测试，不保留双读或影子流程。正式创世后的首次 runtime、节点程序或 storage 升级才允许从零递增。
- 具体公权选举业务模块是对应选举规则真源，按业务种类分别放在 `runtime/public/`；无具体规则的通用选举业务壳已决定删除。`election-vote` 继续只负责投票、计票和结果快照。
- 公民投票与竞选主体统一为公民 CID + 签名钱包。`citizen-identity` 提供完整主体和规范化人口数据，投票引擎不得继续用裸钱包表达公民票据或候选人。
- 生产权重必须由 FRAME benchmark 生成；runtime-upgrade 业务执行仍显式叠加 `SystemWeightInfo::set_code()` 的最重成本。

当前 Runtime 将自动终结、异步执行、历史清理的独立预算分别配置为最大区块权重的 `1/4`、`1/4`、`1/8`。正式 benchmark 后复核总占比为 62.5%，每条管线在 60 秒最大计算区块内均能容纳至少一个最重任务，因此保持该预算。

#### 影响

- 新增普通投票模式时，只需在 VotingEngine 边界增加 Track 实现并在 runtime 注册，不需要修改核心 timeout/cleanup 分支，也不需要创建新的 pallet。
- 特殊投票仍可由现有 sub-pallet 承载自己的快照、票据和计票，但必须实现统一 Track handler。
- Popular 选举不把完整选区人口塞入 `BoundedVec`；Mutual 互选只冻结 VotePlan 指定岗位的有效任职账户，同一钱包跨岗位形成独立岗位票据，不使用机构全体 admins。
- 清理吞吐按实际 weight 控制，proposal 之间公平推进；任何单项错误或大规模票据都不能饿死其他维护管线。
- 重新创世会生成新的 genesis hash；正式冻结后升级仍必须走链上 `setCode`，本 ADR 不改变创世冻结规则。

#### 备选方案

- 保留 stage 巨型 `match`：新增模式仍需修改核心多处分支，拒绝。
- 把所有投票强行统一成同一计票算法：会破坏选举排名、立法会签和联合公投的业务边界，拒绝。
- 继续用 `StorageMap::iter().next()` 清理：大型提案可以长期头部阻塞，拒绝。
- 为开发期旧链保留多轮历史 migration：正式链尚未创世，违反目标态一次性改造，拒绝。

#### 落地与验收

1. 四个超限生产文件已物理拆分，生产文件均不超过 800 行，空模块已删除或承载真实职责。
2. 公平清理队列、递归 Track handler、异步执行和三条独立 weight 预算已落地。
3. 五个投票 crate 共 19 个 FRAME benchmark 已按 `steps=50/repeat=20/WASM compiled` 生成生产权重；Track timeout 与 cleanup 改为动态计账。
4. 原生 LLVM 可执行业务代码行覆盖率为 81.80%，workspace 全量测试、`no_std`、`runtime-benchmarks`、`try-runtime` 和 release WASM 构建通过。
5. 最终源码的 `citizenchain-fresh` 全新创世真实启动通过，genesis hash 为 `0xd81962210c603a4a0f078b2cc022bac3daab344cd7dce8c6fc3501973d1552ab`，metadata RPC 响应 418,806 字节，runtime spec、system、state 与受影响 StorageVersion 均为 1。
6. 正式创世发布仍须统一烘焙冻结 chainspec、替换 bootnode 所在网络的 genesis，并按发布门禁验收客户端签名版本；本任务未推送或部署。

以上第 5 项是 2026-07-14 开发期验收事实，不再代表正式创世目标。2026-07-21 最终决策要求正式创世前把项目自身 runtime、pallet storage 和 workspace/Node 程序版本全部归零。

---

### ADR-037 公民币原生平台订阅与创作者订阅

- 状态：Accepted
- 初始日期：2026-07-16
- 统一修订：2026-07-18
- 关联：ADR-038、ADR-018、`20260716-citizen-coin-subscription`

#### 1. 决策

平台订阅与创作者订阅统一使用 CitizenChain 原生公民币付款，订阅能力并入现有 `SquarePost` pallet。2026-07-21 已确认当前正式链尚未创世，直接使用终态 storage 和 `StorageVersion = 0`，不执行开发期原地迁移、不保留旧状态或兼容双轨；正式创世后的真实升级才另行设计迁移。

订阅周期统一由真实公历决定。链上已确认的 `Timestamp.Now` 是 runtime 判断到期和执行扣款的
时间依据；runtime 以确定性的 UTC 整数公历算法计算月、季、年，订阅期限与区块高度、
固定天数和设备时间无关。

#### 2. 平台与创作者付款

- 平台三档为 Freedom、Democracy、Spark，价格真源为链上 `PlatformPrice`。
- 平台收款账户由创世常量公民链基金会 CID 派生；缺失时 fail-closed。
- 创作者必须拥有当前有效的平台订阅。
- 创作者链上套餐只保存付款必需的 `tier_id`、`billing_period`、`price_fen`。
- 名称、说明、权益文案和媒体只保存在 Cloudflare/D1。
- 每次真实扣款读取当时的链上当前价格；当前已付周期不补差价。
- 创作者订阅款按 `creator_cid_number` 解析当前双向绑定账户并全额进入该账户。

#### 3. runtime 自动周期流程

1. 用户在 CitizenApp 对订阅签名；runtime 立即按最新链上价格扣款。
2. runtime 以当前区块共识时间戳记录首次扣款，并计算下一个真实公历到期时间。
3. Active 订阅进入有界到期索引；每个区块在 `on_initialize` 使用上一块已确认时间处理
   已到期项目，正常出块时最多延后一个区块且绝不提前扣款。
4. 到期时无需再次签名，runtime 由 `subscriber_cid_number` 解析当前双向绑定账户，
   从当前账户向平台费用账户或创作者当前绑定账户转账，并读取最新链上价格。
5. 停链期间不能写状态；恢复出块后按到期顺序补扣全部已到期周期，直到追上当前共识时间或余额失败。
6. 余额不足写 `Suspended(InsufficientBalance)`；CID 当前绑定不可用写
   `Suspended(IdentityBindingUnavailable)`；目标套餐失效写
   `Suspended(NeedReconsent)`；均禁止回退扣历史账户。

CitizenApp 只负责订阅、取消、换套餐签名和链上时间戳展示。设备、App 和 Cloudflare 是否在线均不影响自动续费。

#### 4. 状态与换套餐

- `Active`：当前链时间戳早于 `paid_until` 时权益有效。
- `Cancelled`：停止后续续费，已付权益保留至 `paid_until`。
- `Suspended`：余额不足、改价待再授权或当前身份绑定不可用，停止自动续扣。
- `CreatorPaused`：创作者平台会员暂停，保留调度等待恢复。
- `Terminated`：仅用于不可恢复的明确终止。
- 换套餐立即按剩余权益折算，不存在延迟生效套餐字段。
- 已取消后换套餐作为新的签名授权，立即按目标计划当前价格处理。
- 不退款、不补差价、不按日折算。

#### 5. Call index

`SquarePost` pallet index 固定为 `34 / 0x22`。

| index | 调用 |
|---:|---|
| `0` | `publish_post` |
| `1` | `subscribe` |
| `2` | `cancel` |
| `3` | `set_creator_plans` |
| `4` | `change_subscription_plan` |
| `5` | `propose_set_platform_price` |
旧 keeper、外部续费调用、周期确认调用和旧 SCALE 布局全部废弃，不保留兼容入口。

#### 6. 真源与资源边界

- 平台价格、创作者付款套餐、扣款事实、订阅状态和权益截止时间：CitizenChain。
- 真实公历计算、到期调度与自动扣款：CitizenChain runtime。
- 订阅、取消、换套餐签名和日期展示：CitizenApp。
- 创作者展示资料：Cloudflare/D1。
- D1 订阅记录：finalized 链状态的可重建镜像。
- finalized 镜像证明：交易哈希、区块哈希和完整已签名 extrinsic；Worker 必须复核签名钱包、调用参数、finalized 主链包含关系与同一区块 storage。
- 平台调价：统一投票引擎。
- 平台调价入口与提交：OnChina 读取准确机构 CID 和 finalized 链上真源，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫后通过唯一链提交入口广播。
- 平台订阅、创作者身份、套餐和订阅关系的链上及 D1 业务键全部是 CID；
  `account_id` 只表示当前交易签名者、实际付款/收款账户或不可变审计事实。

Cloudflare 只承担低频镜像、展示与门禁加速，不保存第二份未来扣款价格，不计算日期，不持有扣款能力，不进行高频全链扫描。镜像中的最近扣款价格只用于审计和用量预算，不决定下一次扣款。

#### 7. 安全与信任边界

- 订阅者签名订阅即建立持续自动扣款授权，直到该订阅者签名取消。
- 续费、周期推进、换档生效和失败处理均由 runtime 执行，不接受任何外部账户提交。
- 自动续费必须在 `on_initialize` 完成，使付款与收款余额变化进入 NodeGuard 的 finalize
  前状态；禁止移回 `on_finalize` 或在 finalize 后阶段产生业务转账。
- 换绑不迁移订阅 storage：CID 键天然不变；续费只解析当前绑定账户，旧账户永不再扣。
- UTC 公历算法只使用确定性整数运算，所有节点对同一时间戳得出相同日期。
- 到期索引按时间顺序处理并设置单块权重上限；积压在后续区块继续，不能静默跳过。
- Cloudflare 只镜像 finalized 状态，无法延长权益或触发扣款。
- 同一 finalized 交易只能首次绑定一个钱包、动作和规范化请求；完全相同的重试幂等，改写请求内容 fail-closed。
- Cloudflare 门禁只接受未陈旧的 finalized 链时钟；`Active` 与未到期的 `Cancelled` 可用，`Terminated`、过期、未知或陈旧镜像拒绝。
- OnChina 平台模块只授权链上 `PlatformCidNumber` 对应的准确机构实例；机构码、前端显示和本地数据库都不能代替 CID 与链上 `admins` 真源。
- OnChina 的请求二维码、CitizenWallet 一次签名响应二维码和 OnChina 回扫提交是所有管理员链交易的统一流程；禁止调价业务自建直接钱包提交、第二次签名或第二套提交接口。

#### 8. 创世决策

当前正式链尚未创世，SquarePost 直接以 CID 终态键重新创世：

1. 不提供 storage migration、旧键读取、旧枚举或双轨兼容。
2. `Subscriptions`、续费索引、`CreatorPlans` 和帖子发布计数全部直接使用 CID。
3. Worker D1 与 KV 随创世部署同批重建，镜像由新链 finalized 状态产生。
4. 真实验收必须覆盖订阅者和创作者换绑，证明关系不变且只使用当前账户付款/收款。

#### 9. 后果

- 订阅期限完全脱离区块高度，页面可显示真实日期与时间。
- App 离线不影响续费；停链期间到期的周期在恢复出块后补扣。
- runtime 承担有界调度和公历计算成本，Cloudflare 不承担扣款或日期计算。
- 首次订阅只有一笔签名交易；续费没有用户交易，也没有周期确认交易。
- 第三步只接入订阅、取消、换套餐和 finalized 状态展示。
- 第四步只做 finalized 证明镜像、到期候选对账和 Cloudflare 资源门禁，不增加账户签名、设备签名或链上交易。
- 第五步只增加 OnChina 平台调价入口、准确 CID 工作台隔离和 CitizenWallet 严格识别；不修改 runtime，不在业务模块实现投票，也不增加第二次签名。
- 所有端必须同步新 SCALE、call、storage 和状态，不允许单端兼容旧协议。

---

### ADR-038 链上税务体系：收入台账 + 申报期结算 + 征税权两级治理（非逐笔预扣）

- 状态：**Proposed（草案·仅机制架构；税率/征收规则由税务机构后期运行期设定，架构不预设；本轮不改链码）**
- 决议日期：2026-07-16（草案，含用户关键纠正：申报期结算 ≠ 逐笔预扣）
- 关联：ADR-037（订阅侧全额到账、不预扣，本 ADR 为其后置税务）、ADR-027（立法院授征税权）、ADR-030（onchina）、ADR-025（CID 机构码单源）、`primitives::cid`/`account_derive`。

#### 标题

订阅等收入**全额到账**，税走**独立的申报期结算子系统**：链上按纳税主体（CID）记收入台账，税务机构在申报期按其设定的税率/成本/征收规则计算应纳税额并征收——**不逐笔预扣、不逐笔分账**。税务管辖由 CID 决定；税率与征收方式由有征税权的税务机构运行期设定，架构只提供机制容器。

#### 背景

- 用户 2026-07-16 纠正：交税是**申报期统一交税**（同现实），不是每收一笔就扣；收入要按税率、成本在申报期结算。故原"扣款时点原子扣缴(withholding)"模型作废，改**申报期结算(periodic assessment)**。
- 税务管辖：**CID 号决定归属哪个地方的税务机构征税**（CID 内含地域）；无 CID 用户由用户自行申报。
- 税率/征收方式**不是架构现在定死的**——是税务机构获征税权后运行期设定的可配置项。架构只保证"能记账、能设规则、能按期结算征收"。
- 探源：财政机构"中华民族联邦共和国财政与税务部"等已创世公权法人；机构类型单源＝CID 机构码（ADR-025）；立法院《税法》当前纯文本、无机器可读授权字段。

#### 决策（只定机制，不定值）

**1. 独立税务 pallet，与订阅正交；订阅侧全额到账不预扣**
- `subscription`（ADR-037）扣订阅款后**全额转收款方**（平台→公民链基金会费用账户；创作者→创作者钱包），**不扣税、不分账**。
- 税务是收款方作为纳税主体的**后置周期义务**，与订阅资金流解耦。

**2. 纳税主体 = CID；税务管辖由 CID 决定**
- **收款方标识是钱包账户**（订阅侧 `IssuerKey::Creator=AccountId`，创作者无 CID 限制、任意钱包账户可开、收款不受限）；税务侧从收款账户**唯一解析 CID**（个人 citizen-identity / 机构 CID，防伪造）：有 CID → 按 CID 记台账、链上申报结算；**无 CID → 自行申报、不进链上台账、不阻塞收款**。机构类型由 CID 反查 `institution_code`（ADR-025 单源，不建第二套分类）。
- **CID 决定归属税务机构**（CID 内含地域 → 地方税务机构管辖）。
- **无 CID 主体**：不纳入链上自动结算，由用户**自行申报**（链下/例外路径，架构留口不强制）。

**3. 收入台账（税基原始数据，按 CID × 申报期聚合）**
```
IncomeLedger: StorageMap<(taxpayer_cid, tax_period), accrued_income /*分*/>
```
- 收入发生时（如 `subscription` 扣款成功）向台账**记账累加**（记账 ≠ 扣钱）；台账按 CID × 申报期聚合，非逐笔存储。
- 成本/抵扣项由申报提供或税务机构规则定义（架构不预设是否允许成本扣除）。

**4. 税率与征收规则 = 税务机构运行期治理设定（架构只给容器）**
- 税率表、成本/抵扣规则、申报期长度、征收方式、落账账户——全部是**有征税权的税务机构在运行期通过治理写入的可配置 storage**。
- 架构**不预设**任何税率值、分档粒度、落账层级、征收周期——这些"税务机构怎么设置就怎么征"。
- primitives 侧只放**类型定义与硬顶护栏**（如 `Perbill` 上限、单位），不放可调值。

**5. 征税权两级治理**
- **第一级·立法院授权**：税务机构须经 `legislation-yuan` 通过《税法》获征税权。新增机器可读授权存储 `TaxAuthorization`（授予某税务机构对某范围/某类主体的征税权 + 上限区间 + 有效期，《税法》表决终态回调写入，effect 落税务 pallet 非法条文本）。
- **第二级·税务机构设规则**：获授权的税务机构 admin 在 onchina 经 `internal-vote` 设税率/征收规则（机构级参数强制内部投票、无单管理员直改）；写入前 + 结算执行时复核 `TaxAuthorization` 区间 + `caller_cid ∩ scope` 双向校验（能力位绑**唯一税务机构 CID**，非类别码，防同类别/别省机构越权）。

**6. 申报期结算与征收（周期性，非逐笔）**
- 申报期到，按税务机构设定规则对 `IncomeLedger` 计算应纳税额（收入 − 成本 × 税率），从纳税主体账户征收到该管辖税务机构的财政账户。
- 结算/征收时点联合校验 `TaxAuthorization` 有效期与范围命中（防授权过期后续征 = reverse fail-open）。

**7. fail 语义（分层，非坍缩）**
- 税务子系统**未启用/无征税机构/该主体无适用税率** = **不征**（等税务机构后期设定，承接 ADR-037 全额到账）。
- 主体**有 CID** = 有管辖归属，纳入台账待其税务机构设规则后结算。
- 主体**无 CID** = 不自动结算，自行申报（不阻塞其收款）。

**8. 无逐笔预扣带来的简化**
- 订阅资金流干净全额到账；无逐笔二分账 → 原"扣款时点原子分账/净额<ED 烧毁"等问题消失。
- 税务与订阅唯一接触点 = 收款成功时向 `IncomeLedger` **记账**（不动钱）。

#### 边界

- 本 ADR 只定税务**机制架构**（台账/授权/结算容器 + 两级治理路径）；**具体税率、分档、成本规则、申报期、落账、征收方式一律税务机构运行期设定，不在架构、不在本轮**。
- 串行依赖：税务机构补管理员 → 《税法》立法授征税权 → 税务机构设规则 → 申报期结算启用。会员两期（ADR-037）不被税务阻塞。
- 财政账户支出通道（税款如何花出）非本 ADR，另立卡。
- 若《税法》需输出结构化授权数值，立法院机器可读参数可拆 **ADR-039**。

#### 影响

- **链（重大，重新创世无 migration）**：新税务 pallet（`IncomeLedger`/`TaxAuthorization`/税率与规则可配置 storage/申报期结算执行）；`RuntimeTaxSubjectQuery`（钉稳定 CID 反查，不跑地址 union）；`legislation-yuan` 回调写 `TaxAuthorization`；`InternalVoteResultCallback` 扩 executor；`subscription` 收款成功记 `IncomeLedger` 的接线。
- **onchina**：`domains/fiscal/` 税务机构设规则入口（`internal-vote` + 冷签 + 能力位绑唯一税务机构 CID）；申报/结算相关 workspace。
- **Worker/App**：税在链上；创作者/公民链基金会看应税收入台账与申报状态读链。

#### 备选方案

- 逐笔预扣分账(withholding)：**否**（用户纠正：申报期结算，非逐笔）。
- 架构预设税率/分档/落账层级：**否**（税务机构后期运行期设定，架构只给容器）。
- 纳税主体按地址 union 判定：否（钉稳定 CID，CID 决定管辖）。
- 能力位按机构类型码授权：否（绑唯一税务机构 CID + scope）。

#### 后续动作

- 具体税率/征收规则待税务机构运行期设定，不入本架构。
- 落地后同步 `docs`、runtime 索引、`unified-protocols.md`。

---

### ADR-039：机构 CID 与岗位码权限模型

状态：Accepted（2026-07-20；公私权管理员分型、机构阈值解耦、公民链基金会重新创世、岗位席位记票和 ADR-040 runtime 账户字段统一均已实施）。

#### 背景

一个机构可以有大量工作人员、多个岗位，并且不同岗位可发起、投票或执行不同业务。管理员只是人员名册；若以管理员账户直接授权或把全体管理员自动作为投票人，会把机构人员、岗位职责、业务权限和投票规则混成同一层，无法表达现实机构结构。

##### 1. 授权主体

机构业务授权主体唯一表示为：

```text
RoleSubject = (cid_number, role_code)
```

- CID 决定机构能够拥有的顶层业务能力。
- `RoleSubject` 决定该机构内某岗位能够执行的具体业务动作。
- 管理员账户没有固有业务权限；有效权限来自“账户属于 admins + 账户对 RoleSubject 有有效任职 + RoleSubject 拥有目标业务动作权限”。
- 同一账户可在多个机构和多个岗位任职，各 `RoleSubject` 完全隔离。

##### 2. 岗位权限

- 稳定业务动作标识统一使用 `BusinessActionId`，岗位权限记录统一使用 `RoleBusinessPermission`。
- 权限操作至少区分 `Propose` 与 `Vote`；不得用“管理员能做什么”或泛化字符串权限替代。
- 岗位权限必须属于该 CID 的顶层能力范围。
- 岗位权限与岗位码绑定且不可修改；改变权限必须删除旧动态岗位并创建新岗位码。

##### 3. 业务模块与投票引擎

- 每个状态变更由对应业务模块前置校验发起人的 `RoleSubject` 与 `Propose` 权限。
- 每个业务动作在业务模块代码中静态指定唯一投票引擎；管理员、岗位、客户端和交易参数均不得选择或覆盖。
- 业务模块构造并绑定 `VotePlan`：业务动作、业务对象摘要、提案岗位主体、参与投票岗位主体及引擎所需规则。
- 投票引擎只负责合格任职账户快照、投票资格、阈值、计票、通过/否决、终态和维护。
- 投票引擎不读取岗位权限来决定业务是否合法，不解释业务正文，也不执行转账、发行、升级、任免等具体业务。
- 投票通过后，由已绑定业务模块执行确定性回调；回调不得再次建立投票流程。
- 机构和个人多签对自身的内部治理状态变更必须经过指定投票引擎。只读、创世写入、投票引擎内部维护和已通过提案回调除外。注册局为公民或其他机构办理登记属对外行政业务，依其业务规则校验岗位与必要签名，不能误改为注册局机构内部投票。

##### 4. 联合投票

联合业务可以绑定多个 `RoleSubject`。协议升级与决议发行采用相同参与结构：NRC/43 个 PRC 的 `COMMITTEE_MEMBER` 可发起和投票，43 个 PRB 的正式 `DIRECTOR / 董事` 只投票。参与资格按 VotePlan 的完整岗位主体解析，不把参与机构的全部 admins 自动纳入。

##### 5. 岗位生命周期

- 所有机构必须永久存在唯一 `LR / 法定代表人` 岗位；岗位允许空缺，但岗位码和岗位名不可修改或删除。
- `LR` 岗位任职人数只能为 0 或 1；法定代表人姓名、个人 CID、账户三字段必须与 `LR` 任职在同一治理结果中一起设置或一起清空。
- 所有创世固定岗位的岗位码、岗位名和岗位权限永久固定。非营利法人“公民链技术发展基金会”固定包含 `LR`、`GENESIS_PRODUCT_MANAGER`、`GENESIS_PROGRAMMER`，并允许同一账户分别任职三岗。
- 创世机构可以依法增加、改名和删除普通动态岗位；NodeGuard 只保护固定岗位，不禁止额外动态岗位。
- 动态岗位码由 runtime 生成，机构内唯一、不可修改，删除后永不复用；调用方不得指定岗位码。
- 动态岗位码格式：`R_<32 位大写十六进制>`。
- 生成材料：`blake2_256(SCALE(MODULE_TAG, cid_number, institution_role_nonce, proposal_id))`，取前 16 字节并转为大写十六进制。
- `InstitutionRoleNonce[cid_number]` 单调递增；`UsedRoleCodes[(cid_number, role_code)] = true` 永久保留，删除岗位不删除占用记录。
- `role_name` 在机构内唯一；同名多人必须表达为同一岗位码下的多个任职席位，不能复制成多个同名岗位。动态岗位名可以依法修改；岗位码、岗位权限及固定属性不可修改。
- 同一管理员可以在同一机构担任多个不同岗位；任职去重边界是“同一岗位内同一账户不得重复占席”，不是“一个账户在机构内只能有一个岗位”。
- 岗位只定义席位与权限，不保存岗位阈值；投票阈值属于机构/具体投票计划。例如 NRC 是一个 `COMMITTEE_MEMBER` 岗位、19 个任职席位、机构阈值 13。
- 机构阈值由 public/private entity 按 CID 独立保存，不得从 `admins` 账户数推导。投票引擎只在建案时消费该阈值并冻结提案快照。
- 机构投票票据唯一键是 `InstitutionVoteTicket { role_subject, voter_account_id }`。同一账户兼任多个有效投票岗位时，每个岗位各有一张票；同一岗位和账户组合只能投一次。个人多签仍使用独立的账户票据。
- `InstitutionTicketCountSnapshot[(proposal_id, cid_number)]` 冻结该机构在本提案中的岗位席位票据总数，只用于可达票数和阈值判定，不建立岗位阈值，也不按账户去重。

##### 6. 机构创建与个人多签

- 普通机构创建必须原子建立 admins、强制 LR、至少一个初始治理岗位及其权限、初始任职和初始投票规则。
- 不存在临时管理员权限、超级管理员或先创建再补岗位的授权窗口。
- 个人多签使用 `AuthorizationSubject::PersonalMultisig`，继续按个人多签管理员集合治理，不复用机构 `RoleSubject`。

#### 唯一真源与边界

- admins 人员名册：`runtime/admins`。公权、私权机构统一使用 `Admin { account_id, cid_number, family_name, given_name }`，非空公民 CID 只能引用 `citizen-identity` 的 `AccountIdByCid` / `CidByAccountId` 双向绑定。个人多签复用同一 SCALE 结构，但不属于机构管理员并按个人多签规则处理字段完整性。
- 岗位、岗位权限、岗位码 nonce/占用和任职：`runtime/entity`。
- CID 顶层能力、创世固定岗位与权限：runtime 共享常量与创世规范。
- 业务动作权限要求、指定投票引擎、VotePlan 和通过后执行：对应业务模块。
- 机构治理阈值：`runtime/entity/public-manage` / `private-manage`；提案阈值快照、资格快照、票据、计票和终态：`runtime/votingengine`。
- 永久固定岗位保护：NodeGuard；不得扩大为一般机构业务授权真源。

#### 后果

- 现有“admins 即授权”和“机构投票快照全体 admins”的实现必须删除，不能保留兼容分支。
- 所有受影响 SCALE、storage、QR 和客户端解码必须在同一步骤跨端同步。
- 普通机构创建载荷会发生 breaking change，开发链重新创世，不做历史迁移。
- 每个业务模块都必须显式登记业务动作、可发起/投票岗位主体和唯一投票引擎；未登记则 fail-closed。

#### 第 4A 固定权限盘点

完整动作目录、逐岗位 `Propose/Vote` 固定矩阵、实现记录和验收记录在任务卡 `历史实施记录《机构岗位权限与投票职责统一》（卡已删除，规范以本文为准）` 的“第 4A 步盘点结果与固定矩阵”和“第 4B 步完成记录”。

固定矩阵已经确认并实施：协议升级与决议发行均由 NRC/PRC `COMMITTEE_MEMBER` 发起和投票，两个业务中的 PRB `DIRECTOR` 均只投票；FRG 按准确省专员岗位隔离；公民链基金会平台调价由 `GENESIS_PRODUCT_MANAGER` 发起、三个固定岗位投票。没有明确固定职责的转账、普通资产发行等能力不授予固定岗位，后续由动态岗位承接。

#### 实施与验收

实施顺序及完整验收以 `历史实施记录《机构岗位权限与投票职责统一》（卡已删除，规范以本文为准）` 为准。2026-07-19 已完成共享授权类型、跨端 SCALE 契约、public/private entity 岗位权限生命周期、创世固定权限、准确 CID 顶层能力和 NodeGuard 固定权限保护。旧机构直接创建 call 5 已永久关闭。

联合、内部、立法和互选 Track 均按每个 `RoleSubject` 建立 `VoterSnapshot`，机构票据按 `RoleSubject + voter_account_id` 分别保存，已删除旧的按 CID 合并账户快照。同一账户仍只需一把私钥，但可依法分别行使其每个有效岗位席位；机构阈值未改变。普选和立法公投继续使用 citizen-identity 人口数据生成的提案人口快照，个人多签继续按管理员账户票据记票。

不得恢复 admins 直接授权、机构全体 admins 快照、按账户合并岗位票权或旧账户票 storage。跨端调用必须显式携带岗位码，并以提案冻结的 VotePlan 和 VoterSnapshot 筛选可投岗位。

---

### ADR-040：全仓账户标识采用 Substrate 官方模型

状态：Accepted（2026-07-22 冻结目标契约；2026-07-26 已随正式创世完成实施收口）。

#### 背景

仓库历史上曾把同一个由助记词派生、用于签名和授权的链账户写成多组废弃旧名；`wallet_account`、`admin_account`、`owner_account`、`wallet_pubkey`、`admin_pubkey`、`wallet_address` 均禁止恢复。它们混合了链账户、钱包软件、人员角色、公钥和展示地址，导致跨 Rust、Dart、TypeScript、SQL、JSON、QR 和文档的字段不一致，也容易把 SS58 展示字符串误当授权主键。

Polkadot SDK 的运行时身份模型以 `AccountId` 表示账户，以公钥完成签名验证，以 SS58 表示人类可读地址。ADR-022 已固定 `AccountId` 是账户身份锚点、签名算法只是授权方式；本 ADR 在该密码学边界之上统一全仓命名和文本编码。

##### 1. 账户、公钥和地址严格分层

- runtime 账户类型使用 `AccountId`；当前具体值仍是 32 字节账户标识。
- 单一账户字段统一为 `account_id`。
- 一个结构同时出现多个具有独立业务角色的账户时，统一使用 `<role>_account_id`，例如 `actor_account_id`、`voter_account_id`、`creator_account_id`、`sender_account_id`、`recipient_account_id`、`subscriber_account_id`、`registrar_account_id`、`beneficiary_account_id`、`representative_account_id`。
- 签名公钥统一为 `public_key`；当前签名者公钥为 `signer_public_key`；凭证签名者公钥为 `credential_signer_public_key`。
- SS58 只允许命名为 `ss58_address`，只用于输入、输出和界面展示。授权、关系索引、数据库主键、缓存 key 和链上真源必须使用 `account_id`。
- `wallet` 是链外软件，不得再用于 runtime 账户字段；`admin`、`owner` 是人员或业务角色，不得在只有一个账户的结构中替代账户本体命名。

##### 2. 文本编码唯一

- `account_id` 和 32 字节公钥的跨端文本表示固定为小写 `0x` 加 64 位十六进制。
- 唯一校验式为 `^0x[0-9a-f]{64}$`。
- 进入系统边界时必须一次规范化并校验；内部不得同时保存无 `0x`、大写、混合大小写或 SS58 形式的同一账户。
- `ss58_address` 必须从账户字节按指定网络格式派生；不得反向成为账户权限或数据库身份真源。

##### 3. 授权与验签

- 签名流程固定为：校验 `signer_public_key` 和签名，按运行时账户识别规则得到 `signer_account_id`，再与业务要求的 `account_id` 或 `<role>_account_id` 比较。
- 机构岗位权限继续由 `cid_number + role_code + account_id` 三者共同成立；管理员账户本身不获得业务权限。
- 公民身份继续由 `cid_number + account_id` 共同校验；citizen-identity 是 CID 与账户一一对应的唯一真源。
- 机构 CID、公民 CID、岗位码、账户 ID、公钥和 SS58 地址分别表达不同语义，禁止互相替代。

##### 4. 目标结构与存储

```text
Admin { account_id, cid_number, family_name, given_name }
CitizenSubject { cid_number, account_id }
VotingIdentityPayload { cid_number, account_id, ... }
InstitutionAdminAssignment { cid_number, account_id, role_code, ... }
InstitutionVoteTicket { role_subject, voter_account_id }

AccountIdByCid[cid_number] -> AccountId
CidByAccountId[account_id] -> cid_number
```

结构中存在第二个账户时必须根据业务角色命名，不得为避免重名恢复 `wallet_account`、`admin_account`、`owner_account` 等旧称。

##### 5. 数据与兼容策略

- 本 ADR 实施时尚未正式创世，因此 runtime 和 pallet StorageVersion 保持 `0`，直接采用
  最终结构且不写 migration；该开发期条件已于 2026-07-26 正式创世结束。
- PostgreSQL、Isar、Cloudflare D1/R2 旧业务数据已在正式创世切换前全部删除并按最终
  schema 重建；正式创世后的现行数据不得再次按本条开发期策略删除。
- 不保留旧字段、旧 JSON、旧 SQL 列、双写、双读、fallback 或任何过渡兼容。
- 助记词、seed、私钥、macOS Keychain、iOS Keychain、Android Keystore 和 GitHub Secrets 不属于待删除业务数据，必须保留；同一安全材料按未改变的派生规则得到同一账户。
- CID 生成、签名 payload、SCALE 字段顺序或哈希材料不得因纯命名重构而改变字节。实施中若发现必须改变协议字节，必须停止并单独取得确认。

#### 影响

- 这是全仓 breaking rename，影响 runtime、Node、OnChina、CitizenApp、CitizenWallet、Cloudflare、QR registry、SQL、JSON、SCALE、测试、生成物和文档。
- 旧名称只有在任务卡实施完成前用于准确描述当前代码时才可出现；不得新增使用，完成对应步骤时必须连同代码、注释、文档和数据一起删除。
- 角色字段不会被无差别压平。`source`、`dest` 等框架语义，以及确有多个账户时的 `<role>_account_id`，继续保留其业务区分。
- ADR-022 的账户派生、AccountId 锚点和未来 PQC 授权路线不变；ADR-023、ADR-039 中的旧账户字段名由本 ADR 的目标命名取代，管理员名册、岗位权限和投票边界不变。

#### 实施状态

- 第 1 步已冻结全仓命名、格式和无兼容原则。
- 第 2 步已完成 runtime 结构、存储、事件、权限入口及其直接 SCALE 消费者统一；正式创世前版本与 StorageVersion 保持 `0`。
- 第 3 步已完成 Node、桌面直接消费者和 `chain-signing` 共享 crate 统一：账户、公钥和 SS58 已分层，跨进程账户与公钥严格使用小写 `0x` 加 64 位十六进制，签名流程先验公钥再比较账户，奖励账户 RPC 与本地非密钥缓存已按最终命名重建且没有兼容入口。
- 第 4 步已完成 OnChina 后端、前端、PostgreSQL 最终 schema、HTTP/JSON、登录与授权上下文统一，并删除重建本地 PostgreSQL 业务数据库。经单独二次确认，已使用当前 Runtime WASM 启动隔离 fresh chain，真实完成 PostgreSQL、HTTPS OnChina、链投影、账户格式、登录验签和管理员链上门禁验收；验收后数据库再次清空重建且全部服务已停止。
- 第 5—7 步已完成 QR registry 与生成物、CitizenApp/Isar、CitizenWallet/Isar 的账户、公钥、SS58 分层和严格边界统一；旧数据库按最终 schema 重建，不保留兼容读取。
- 第 8 步已完成 Cloudflare Worker、D1、KV、R2、Queues 和边缘协议统一，staging/production 旧业务数据按用户确认删除重建，并以真实 HTTP 和运行状态验收。
- 第 9 步已完成当时的 外部调用方 协议升级公钥命名、严格校验、WASM 版本职责隔离和验收；
  只有本机状态、依赖、日志、产物和机密继续忽略。旧 Keychain target 已经单独确认后删除，
  新 `NRC_SIGNER_PUBLIC_KEY` 由用户另行配置。
- 第 10 步已完成正式创世运行态总验收：节点、CitizenApp、Cloudflare 使用同一创世哈希
  和状态根，账户标识继续按本 ADR 执行；本次无资金回归没有产生交易或业务数据。

#### 备选方案

- 保留 `wallet_account`：拒绝。钱包是软件载体，不是 runtime 身份类型。
- 全部统一为 `public_key`：拒绝。账户标识、公钥和 SS58 地址是不同层次，未来签名算法变化后尤其不能混用。
- 全部统一为 `account_id` 而删除角色：拒绝。同一结构有多个账户时会失去发送方、接收方、投票人等业务语义。
- 保留旧字段兼容：拒绝。正式创世前直接清理已经完成；正式创世后恢复旧字段同样会永久
  制造双轨。

#### 后续动作

- 后续变更继续遵守本 ADR；Runtime 修改授权统一见 `CitizenChainRuntime.md` 的“Runtime 修改与链协议边界”。
- 正式创世已经完成，不再执行“最终重新创世”。后续 runtime 变更只能通过正式链升级流程，
  并对既有状态执行必要的原子迁移，不得恢复旧命名或兼容分支。

---

#### 跨端签名文本编码

账户与MLS公钥统一为小写0x加64位十六进制，签名为小写0x加128位十六进制；登记动作13和签名域0x1C遵守共享mls_device_bind合同。Node不维护另一套设备认证或私有数据密钥。

### ADR-042：CitizenApp 按业务域隔离本地存储与操作队列

状态：Accepted（2026-08-16 决策生效；源码与冻结快照自动化验收完成；
签名 Release 真实运行验收单独记录）。

#### 背景

改造前，CitizenApp 把钱包、交易、治理、广场和 Chat 集合放在同一个共享业务数据库中，
并由一条进程内操作队列串行执行全部业务读写。
这个设计把互不相干的业务域放进同一个故障域：任一业务回调永久等待，后续钱包及继续
依赖该库的页面读写都会排在同一条未完成队列后面。

当前 Chat 不得在数据库回调中再次读取同域数据库，也不得在回调中等待钱包或 MLS 能力。
同域再入必须立即失败；跨域公开绑定和协议操作必须在当前数据库回调返回后执行。

同一耦合还使钱包列表读取中的未知状态或读取失败容易被默认空列表掩盖，右上角“＋”据此只
展示“导入冷钱包”，把“尚未知道钱包状态”错误表达成“已确认没有钱包”。

当前没有外部用户数据需要迁移，因此本决策直接实现目标 schema：生产正常启动、
数据库打开和日常读写不探测、不读取、不复制、不改写、不删除其它数据格式，也不保留
双读、双写或兼容重载。本次实施未连接手机，未构建、安装、迁移或擦除手机数据。

##### 1. `5+2` 目标与分步物理隔离

CitizenApp 最终采用五个 Isar 业务边界：`isar_app` 通用业务、`isar_user` 身份/资料/联系人/
设置/App 状态、`isar_social` 广场、`isar_chat` 聊天、`isar_wallet` 钱包；设备机密分别进入
iOS Secure Enclave/Keychain 与 Android StrongBox/Keystore。页面不是存储边界，“我的”中的
钱包和身份必须分别归属 Wallet/User 两个域。

五个业务数据库现已全部落地：

- `AppIsar(citizenapp_app)` 只保存行政区、公权机构公开目录和通用目录版本状态；
  禁止承载用户、广场、聊天、钱包或链上业务事实。
- `WalletIsar(citizenapp_wallet)` 保存钱包、账户、余额/交易记录、机构与个人多签、提案、
  投票/立法、管理员激活、会员与创作者等全部区块链相关本机事实。
- Chat 新建专属 `ChatIsar`，使用不同于钱包库的数据库名称、数据库文件、Isar 实例、打开
  生命周期和操作队列。Chat 的排队、busy 重试、打开失败和永久 pending 只能影响 Chat。
- Social 新建专属 `SocialIsar(citizenapp_social)`；帖子、草稿、同步检查点和文件清理事实
  只进入 4 个类型化 collection，不得借用其它数据库。
- User 新建专属 `UserIsar(citizenapp_user)`；用户资料、公开资料缓存、身份徽章快照、
  通讯录密文/交接状态、通用用户设置与机构关注只进入 5 个类型化 collection；会员、
  创作者和管理员激活属于链上功能，固定进入 WalletIsar。
- 五个数据库不得共享 `_operationTail`、事务、Isar 实例或 collection。页面和源码目录不决定
  数据归属；业务事实本身决定唯一数据库。
- 各域可以通过窄接口读取不可变的公开身份绑定，或在事务外请求钱包签名及所属 SDK 的 MLS
  协议操作；共享能力不等于共享数据库或共享操作队列。
- `AppIsar`、`WalletIsar`、`ChatIsar`、`SocialIsar` 与 `UserIsar` 都必须对“当前域操作回调再次进入当前域”做 Zone
  fail-fast；禁止让同域再入排到自身未完成的队尾。跨域能力只能在当前数据库回调返回后调用。

##### 2. Chat schema 归 Chat 域所有

以下九个既有 collection 从 `WalletIsar` schema 和 `lib/isar/app_isar.dart` 中彻底移除，
改由 `ChatIsar` 独占：

- `ChatConversationEntity`
- `ChatMessageEntity`
- `ChatOutboundQueueEntity`
- `ChatOutgoingMediaEntity`
- `ChatPendingInboundEntity`
- `ChatRouteCacheEntity`
- `ChatGroupEntity`
- `ChatGroupMemberEntity`
- `ChatGroupPendingCommitEntity`

Chat 换绑暂存由 Chat 域专属 `ChatAccountHandoverEntity` 承载；清单只保存来源与目标公开绑定、
会话或消息稳定键、本地 Isar 行 ID 及内容摘要。清单与消息记录由操作系统保护，不派生额外应用密钥。
MLS 协议状态由 TataChatSDK 管理，钱包签名只授权公开账户绑定，不能替代丢失的 MLS 状态。
清单的唯一范围由 `cid_number + target binding_revision + target account_id` 确定，暂存、提交和丢弃
全部在 `ChatIsar` 内完成；钱包私钥与 MLS 私有协议状态不得进入清单。

`ChatBindingFenceEntity` 是同一 Chat 域的持久转换栅栏；它以
`genesis_hash + cid_number + binding_revision + account_id + generation` 约束敏感写和动作读取，
pending 后普通 source/target token 必须失败关闭。生产代码不再存在跨域通用 KV，
也不存在 Chat 交接旧前缀的读取、删除或兼容分支。

##### 3. 数据库回调只做数据库工作

各业务 Isar 的 `read` / `writeTxn` 回调内只允许执行目标数据库查询、复制值对象
和原子写入。回调内禁止等待：

- 另一个 Isar 域或同一队列的再次进入；
- `WalletManager`、平台通道、Secure Storage、Keychain、Keystore 或硬件金库；
- 密钥派生、加密、解密、HMAC 搜索 token 生成或 OpenMLS；
- HTTP、WebSocket、链 RPC、文件 IO、图片/视频处理或任何其它外部 Future。

Chat 读取固定为以下顺序：

##### 4. 钱包页面必须表达未知状态

钱包列表至少区分四种状态：

- 首次读取中：钱包事实未知，显示局部读取状态；
- 首次读取失败：钱包事实仍未知，显示错误和重试；
- 读取成功：只有此状态下 `wallets.isEmpty` 才表示已确认没有钱包；
- 刷新失败：保留上一次成功的钱包列表，并显示非破坏性错误，不能用空列表覆盖。

右上角“＋”菜单只能依据最近一次成功读取的数据生成。首次读取中或首次失败时不得按空钱包
渲染，也不得只剩“导入冷钱包”；应禁用依赖钱包事实的动作或先要求重试。Chat、广场、链 RPC
或任何其它模块的错误不得改写钱包成功快照。

钱包、热账户和默认账户必须组成一次一致本地快照：读取前后比较同一
`WalletManager.walletsRevision`，版本变化则整体丢弃并有界重读，不允许提交混代数据。每次
主动加载领取独立 generation，晚到的旧成功、旧失败和后续副作用均不得提交。余额 RPC 另用
owner + generation 管理不可取消请求，只允许当前 owner 写余额、结束刷新或把所有权原子移交给
最新等待代次。删除钱包或账户后，不论调用是否抛出安全存储清理异常，都必须重新读取一致快照，
以事实行是否仍存在决定“已删除”“未删除”或“无法确认”，异常对象不能取代事实判断。

##### 5. App 级擦除必须是终态操作

- `AppIsar`、`WalletIsar`、`ChatIsar`、`SocialIsar` 与 `UserIsar` 各自维护 `active → closing → closed` 生命周期与 generation。
  擦除意图必须在返回 Future 前同步生效，使未开始操作、晚到打开结果和旧 generation 返回值
  全部失败；短暂有界排空后即使队列永久 pending 也继续强制关闭并删除本域数据库。
- 关闭/删除后不得由后台写入重新打开数据库。测试只能在前一次关闭删除真实完成后，通过
  `resetForTest()` 显式建立新 generation 的空实例；旧任务的 `finally` 不得污染新实例状态。
- App 级擦除按域执行 AppIsar、WalletIsar、ChatIsar、SocialIsar、UserIsar、Chat 文件域、Social 草稿文件域、
  Secure Storage 和 SharedPreferences，单项失败不得阻止其它项，全部尝试后聚合报告。
  Chat/Social 文件域分别只能定位 Documents 下精确的 `chat/` 与 `square_drafts/` 子树，
  不得跟随符号链接或扩大目标。
- 本节只约束用户明确触发的既有 AppLock 安全擦除流程。它不是 schema 打开、
  普通启动或日常读取的一部分，ADR-042 也不会调用它。本次实施未触发该路径。

启动预检与数据擦除必须严格分开：预检先持有 Documents 直属 startup barrier，并在注册
后台入口、构造 ChatRuntime 或打开业务库之前处理协调 artifact。CitizenApp 的 Android
manifest 不声明 `android:process`，iOS 也没有运行 Chat writer 的 App Extension；因此合法
owner 的 process generation 不属于当前启动时，该 CID lease 是已经退出的上一应用进程
artifact，必须二次验真并通过原子重命名立即退役。普通运行态仍保留30秒心跳判旧保护；启动
阶段不得套用该门槛形成“8秒等待永远早于30秒过期”的失败关闭。当前 generation 活锁、损坏
owner、异常实体和不可读目录仍然失败关闭，且任何预检失败都不得删除业务数据。

#### 影响

- 任一数据库损坏、打开失败或永久 pending 不得占用其它四个数据库队列。WalletIsar 只
  串行化钱包与区块链事实，AppIsar 只串行化通用目录事实，禁止重新扩大为跨域共享队列。
- 每个本地数据库都要独立打开、关闭、测试复位和数据清除收口；App 级隐私清除必须分别
  调用 AppIsar、WalletIsar、ChatIsar、SocialIsar 与 UserIsar 的终态接口，单库失败不得触发另一库破坏性重置。
- 本决策不包含任何手机旧数据处理动作；源码只定义新建/新打开时的当前目标结构。
- 现有 Cloudflare Chat 控制面、OpenMLS 状态目录、媒体文件目录和网络协议不因 Isar 拆分自动
  改变；涉及清理时仍必须按各自明确生命周期执行。

#### 备选方案

- 只给现有全局队列增加超时：拒绝。超时不能解除正在等待自己的闭环，也不能恢复已被占用的
  单一故障域。
- 在同一个 Isar 文件上建立第二条 Dart 队列：拒绝。两个调度器仍操作同一 MDBX 文件，
  无法建立物理故障隔离，还会重新制造并发 busy。
- 保留 Chat 在 WalletIsar，只把当前解密移出回调：拒绝。虽能修复已知自锁，却继续允许未来
  Chat 慢操作或库损坏拖住钱包，不满足业务域隔离目标。
- 把其它数据结构复制到 ChatIsar：拒绝。本任务无用户数据迁移需求，迁移会额外引入
  双库读取、部分成功、回滚和密钥处理风险。
- 清除整个 App 数据后重建两库：拒绝。会破坏正式钱包、账户 child 和硬件密钥，越过本 ADR
  的授权范围。

#### 实施与验收状态

- 已收口：ChatIsar 独立物理库/队列与 11 个目标 collection、两域对称重入
  fail-fast、生命周期/generation、持久 binding fence、事务外密码学、认证交接清单、
  文件域 `staged → committing` receipt、跨 isolate CID lease/startup barrier 和 Runtime 终态。
- 已收口：钱包 facts mutation gate/revision/settle、一致快照、四态菜单、余额三元组 CAS、
  默认账户完整事实 CAS、完整删除 cleanup plan、硬件删除回读和监控集合 generation/drain。
- 已收口：SocialIsar 独立物理库/队列与 4 个类型化 collection；帖子、草稿和同步检查点
  不再进入其它数据库；草稿纯读、可重试文件清理事实和 Social 文件终态已覆盖。
- 已收口：UserIsar 独立物理库/队列与 5 个类型化 collection；只保存公开资料缓存、身份展示、
  通讯录、用户设置和机构关注。
- 已收口：AppIsar 独立物理库/队列与 5 个通用 collection；WalletIsar 独立物理库/队列与
  21 个钱包/区块链 collection。管理员激活、会员、创作者、交易、多签、提案、投票与
  立法状态全部进入 WalletIsar；生产和测试代码中跨域通用 KV、旧共享 `citizenapp` 实例、
  User 链状态实体均为零。
- 已收口：App 壳层启动不构造 ChatRuntime；ChatRuntime 只在首次进入 Chat Tab 时惰性创建。
  广场推送打开由 App 壳层直接路由，聊天唤醒只落无内容提示，不再借 ChatRuntime 承担 App
  导航。User 与 Wallet 全局门禁对永久 pending 使用有界等待、可见错误和重试，同时保持
  fail-closed，未知事实不会被伪装成空数据或成功状态。
- 锁定 Flutter 3.44.4 / Dart 3.12.2 的最终命令、通过数与残留扫描记录在
  `公民钱包存储隔离.md`；正式数字只以该卡最终验收段为准。
- 正式签名 Release 是独立验收步骤，只能在用户确认后经 外部调用方 执行；禁止手工脚本、
  手工签名、`adb install`、卸载或清除 App 数据。第 1 步没有操作手机；第 2 步在源码、文档、
  注释、测试和残留清理完成后按用户确认对已连接 Google 手机执行覆盖安装与验收。

#### 预计修改目录

- `citizenapp/lib/chat/storage/`：ChatStore、ChatCrypto、认证换绑清单编排与事务外密码学；
  该业务目录不保存数据库定义或生成物。
- `citizenapp/lib/chat/`（除 `storage/`）：ChatRuntime 文件树终态擦除与运行态关闭门闩；
  涉及代码、中文注释和旧进程内复活路径清理。
- `citizenapp/lib/isar/`：五个数据库定义、五个同目录生成物、独立生命周期与公共 Isar
  启动能力的唯一目录；涉及代码、生成物、中文注释和分散旧路径清理。
- `citizenapp/lib/wallet/`、`citizenapp/lib/transaction/`、`citizenapp/lib/citizen/`：钱包、
  交易、多签、提案、投票、立法和链上管理员等调用切换 WalletIsar；涉及代码、测试和旧 KV 清理。
- `citizenapp/lib/security/`：各业务库、Chat/Social 文件域和平台存储独立终态擦除与错误聚合；涉及代码、
  中文注释和旧单库清理口径清理。
- `citizenapp/test/`：增加五库隔离、typed collection、未知态、生产密钥、目标 schema 和
  显式安全生命周期验收；涉及测试代码，
  不创建兼容测试。
- `docs/`、`tasks/`：更新 ADR、总架构、Chat 和钱包技术文档；只涉及文档与冲突旧口径清理。

---

#### 状态

Accepted（2026-08-18；代码与本地自动化验收完成，Release 双真机验收待另行授权）。

#### 背景

ADR-020 的既有实现允许 Cloudflare Worker 瞬时转发 OpenMLS `ChatEnvelope`，并允许大媒体以
密文形式通过 R2 中继。该边界虽然不保存聊天明文，但聊天密文、Envelope 和附件字节仍会进入
云端基础设施，也使消息发送依赖设备登记、云端 KeyPackage 库存和实时转发连接。

产品目标已经明确调整为：所有聊天内容只允许存在于参与聊天的用户设备。Cloudflare、APNs、
FCM 不得接收、转发或保存文字、表情、贴纸、语音、图片、视频、文件、OpenMLS 密文、
`ChatEnvelope` 或附件字节。

#### 决策

1. `cid_number` 是唯一用户聊天身份。`account_id` 及其当前绑定证明只用于鉴权；推送 Token
   只是操作系统唤醒端点，不形成第二聊天身份或设备数量门槛。
2. 文字、表情、贴纸和 OpenMLS 控制消息通过 WebRTC 可靠有序控制通道直接传输。
3. 语音、图片、视频和文件通过独立的 WebRTC 可靠有序媒体通道分片直传，支持背压、断点
   续传、完整性校验和幂等接收。
4. Cloudflare 只允许完成：当前 CID 会话鉴权、最小推送端点维护、无内容 `chat_wake`、
   SDP/ICE/连接就绪信令的短暂内存转发。信令正文不得进入 D1、KV、R2 或业务日志。
5. 删除云端 Envelope 转发、云端 KeyPackage 库存、聊天绑定 nonce、R2 聊天中继、TURN
   聊天中继和累计设备数量限制，不保留旧接口兼容分支。
6. OpenMLS `ChatKeyPackage` 在双方建立直连后直接交换；MLS 状态、Welcome、Commit 和应用
   密文只在端侧保存。
7. 发送操作先把本地消息和待发事实原子写入受操作系统保护的 `ChatIsar` 并立即显示本地气泡；MLS 加密、唤醒、建连、
   发送和确认均异步执行。内部持久化确认只用于补发，不向用户显示已读或送达状态。
8. iOS 与 Android 的聊天数据库、附件仓和 MLS 状态必须排除系统云备份，避免聊天内容通过
   iCloud Backup 或 Android Auto Backup 离开设备。

#### 后果

- 聊天内容不会进入应用后端、对象存储或消息队列，云端泄露面显著缩小。
- 接收方暂时不能建立网络路径时，消息继续保存在发送方本地；发送方重新获得执行和网络条件
  后自动补发。
- 不使用 TURN 时，部分对称 NAT、企业网络或运营商网络可能无法建立纯直连；该限制不能通过
  云端消息中继规避。
- iOS 后台通知由系统调度且不保证每次启动应用。系统未授予后台执行时，只能显示无内容通用
  通知，并在用户重新打开应用后建立直连。
- 大群消息由发送端分别向成员设备直传，上行流量和待发送状态由发送端承担。

#### 取代关系

本 ADR 取代 ADR-020 中“Cloudflare 瞬时转发 MLS Envelope”和“R2 瞬时中继大媒体”的决策。
ADR-020 中 OpenMLS、本机加密存储、消息幂等和 WebRTC 端到端传输原则继续有效。

#### CitizenApp 实施与验收约束

- CitizenApp 只是交互入口，不承担链、身份或权限信任根职责；Isar 结构、认证流程和关键交互
  变化必须先确认边界。
- UI 设计、实现和评审必须核对目标页面代码、`lib/ui/app_theme.dart`、实际图标资产和已确认
  稿；真实空数据保持空态，不得写入演示数据贴图。
- 底部导航固定为“广场 / 公民 / 聊天 / 交易 / 我的”，未经当前任务明确授权不得改变顺序、
  标签或既有图标。
- 交易页链状态只允许“公民链 已更新 / 公民链 更新中 / 公民链 连接失败”，由真实轻节点
  快照和读取错误驱动；流水只允许“待确认 / 已确认 / 失败”，成功 finalized 后才是已确认。
- 关键 Flutter 交互和本地存储逻辑必须补中文注释；完成前必须同步对应技术文档、清理残留，
  并以真实运行页面完成验收。

#### 节点前置验证与 Runtime 候选字段合同

- CitizenChain Runtime 只有产品级 spec_version 状态。WASM 保留为 UI、Workflow 和 Tag 的端标识，但不得进入版本候选 platform 字段。
#### Release 双层登记合同与文档同步

- 产品、平台、流程的登记真源位于控制台 `test/<product>/`；17个完整产品仓
  的唯一登记入口均为本仓 `.github/workflows/<产品>-<平台>-<流程>.yml`。外部调用方必须按完整身份选择入口，
  受控执行合同和仓库隔离均由测试校验。
- CitizenChain 节点 Release 的前置验证与正式构建统一使用从当前最新 `main` 隔离拉取的 Release 工具，禁止回退到 CI 产物中的旧脚本。
- 所有 12 个独立 Release 动作统一传递 `--latest false`。GitHub 仓库级 Latest 不参与产品版本判断；每个产品、端、动作的版本由本产品正式标签与GitHub记录独立确定。

#### CitizenServe D1 最终结构发布合同（2026-08-27）

- 唯一数据库结构真源为 CitizenServe 正式 Release 内本产品根相对的 `schema/citizenserve.sql`，发布器不得维护表名或索引名副本。

#### 完整产品软件流程终态保留合同

17个完整产品统一以“产品 + 平台 + 流程”作为唯一流程分组，仓库只用于源码定位和准确授权。CI 和 Release
各自最多保留一条成功 Run 与一条失败 Run；新任务成功只删除同分组旧成功 Run 及全部
Artifact，新任务失败只删除同分组旧失败 Run 及全部 Artifact，任何清理都不得影响相反终态。
正式 Release 只允许成功事务存在：新成功 Release 固化后删除同端旧成功 Release、Tag 与全部
资产，只保留当前唯一正式事务；失败 Release 只保留一条失败 Run，并清除本次失败形成的草稿、
实现，禁止产品例外或第二套计数逻辑。公共清理的保留数量只统计删除后实际留下的正式事务，
不得把已经成功删除的旧事务继续计入终态数量并误报任务失败。
- 全部业务表和显式索引使用可重复执行定义；同一份最终结构重复执行后，数据库对象集合与定义必须保持不变。
- 外部调用方 在一次扫码签名和一次生物识别授权范围内，先调用 Cloudflare 官方 D1 Query API 执行整份最终结构，再读取 `sqlite_master` 逐表逐索引比对。
- 只有数据库对象集合和定义完全匹配，才允许上传本次 Worker；执行失败、查询失败、对象缺失、对象多余或定义不同均直接判定发布失败。
- Worker 的周期清理、身份投影和会员投影独立结算；任一清理失败不得阻断其余工作启动，但本次 Cron 仍以失败结束并报告失败项名称。
- 正式发布不增加第二套数据库脚本、结构历史或旧表专用处理。

## Release 全量构建（第 7.4 步）

正式 Release 固定从干净源码执行全量构建，显式关闭 Rust 增量编译及工具链内置缓存，不读取CI作业缓存且不复用本机编译中间物。版本、签名、校验、产物和发布流程保持原有产品合同。

## 双仓统一流程最终收口（第 7.5 步）

本产品执行统一流程规则：本机编译中间物只进入本轮塔塔缓存库的build目录并按终态规则清理；GitHub CI 的作业过程数据只进入该次Runner任务空间；正式Release从干净编译状态执行。源码不进入塔塔缓存库、塔塔依赖库或塔塔产物库。

## 平台与 Rust target triple 分域（GMB 第 2.4 步，2026-09-02）

- `node/src/settings/device_password.rs` 的 PAM 响应清零注释已使用官方 Rust target triple
  `aarch64-unknown-linux-gnu`、`x86_64-unknown-linux-gnu` 表达 `c_char` 的机器类型差异；
  `LinuxARM`、`LinuxAMD` 仍是对应的公开交付身份，二者不得互相替代。
- 本次只修改源码注释、仓库守卫与技术记录，不改变指针类型、清零顺序、PAM 调用、认证行为
  或任何运行时代码。
- GMB `repo_guard` 已正向锁定两个 target triple 与 `u8`、`i8` 的准确映射，并反向拒绝旧的
  架构拼接式平台表述回流到设备密码源码。
- `rustfmt --edition 2021 --check` 通过；使用受控隔离目录执行
  `cargo test -p qr-protocol --test repo_guard`，最终 11/11 通过。
- 本轮 112MB 受控测试目录已移至系统废纸篓
  `/Users/rhett/.Trash/gmb-platform-naming-step2-4-20260902`，活动受控目录和 GMB 源码树均无本轮测试产物残留。
- 本步骤不表示 CitizenChain 其它公开 Release、资产或历史文档命名已经迁移完成；其余候选
  继续由全仓命名任务卡分步处理。

## macOS updater 公开路由分域（GMB 第 2.5 步，2026-09-02）

- Tauri updater 源码配置中的唯一公网入口已迁移为 `/download/citizenchain/macOS/updater`：
  `macOS` 是独立平台路径段，`updater` 是独立交付用途路径段；旧架构拼接路由已从源码合同
  删除且不保留兼容壳。本步没有部署，因此不把源码完成冒充生产入口已经切换。
- CitizenServe 路由、限流白名单与 外部调用方 发布后/回滚验收同步使用同一新路径；GMB
  `repo_guard` 正向锁定四端合同并反向拒绝旧路由和 `macOS-updater` 复合名称。
- 本步不迁移 GitHub updater manifest、安装包、Release Tag 或生产 D1 平台值；当前 manifest
  `citizenchain-node-latest-macos.json` 仍属于待后续原子迁移的 Release 资产合同。
- Tauri JSON 解析通过；CitizenServe 两个定向文件 16/16、GMB `repo_guard` 12/12、
  外部调用方 原生发布合同 2/2 通过。
- 首次无签名 Xcode 构建在编译前因正式打包阶段的 外部宿主专用 运行时资源尚未暂存而
  中止；随后在不触碰共享工作目录的前提下，仅对本次编译命令排除这两个外部复制项，同一工程
  完成 macOS 平台 `arm64-apple-macos14.0` target 的 Swift 编译和链接。编译产物含新路由
  字节且不含被排除的运行时资源，不能冒充完整可运行包、正式 Release 或安装验收。
- 本轮 268MB 受控测试与编译目录已移至系统废纸篓
  `/Users/rhett/.Trash/gmb-platform-naming-step2-5-20260902`；没有启动、停止、安装或重启 外部调用方。

## 四端安装下载公开路由统一（GMB 第 2.6 步，2026-09-02）

- CitizenChain 四端安装下载的源码公开路径已统一为
  `/download/citizenchain/macOS`、`/download/citizenchain/Windows`、
  `/download/citizenchain/LinuxARM`、`/download/citizenchain/LinuxAMD`；macOS updater 继续使用
  `/download/citizenchain/macOS/updater`。公开路径不再拼接处理器架构、位数或小写别名。
- CitizenWeb 只生成上述标准公开路径；CitizenServe 在类型化边界内把它们分别映射到既有内部
  发布指针 `macos`、`windows`、`linux-arm`、`linux-amd`；外部调用方 发布后验收和回滚验收
  使用相同公开路径。内部指针不是公开平台名，本步没有修改其数据库值。
- 旧安装路径 `macos`、`windows-x86_64`、`linux-arm64`、`linux-amd64`、`linux-arm`、
  `linux-amd` 已从生效源码删除，不保留兼容入口；只允许作为负向测试和守卫的禁止值存在。
- 本步不修改 Release Tag、GitHub 资产名、manifest 文件名、生产 D1、Tauri updater 资产合同或
  外部调用方 流程定义，也没有部署，因此不能把源码合同完成表述成生产下载入口已经切换。
- 本机定向验证结果：CitizenServe 17/17、CitizenWeb 9/9、外部调用方 静态合同 1/1、GMB
  `repo_guard` 13/13、外部调用方 macOS 原生下载合同 XCTest 1/1，全部通过；没有启动、停止、
  安装或重启正在运行的 外部调用方。
- 独立跨端终检确认上述公开路径迁移正确，同时发现两处早于本步存在、但现有分层测试未覆盖的
  发布互操作阻断：CitizenServe publication wire 使用 `version_tag`，外部调用方 发布器却使用
  `release_tag`；当时 CitizenServe 302 仍使用旧仓库账户，已退役聚合仓remote 与塔塔统一流程
  使用 `crcfrcn/citizenchain`。因此本步只可判定“路径合同完成”，不能判定真实发布事务可用；必须
  先完成单独确认的第 2.6.1 步互操作修复，再迁移 Release 资产名。

## 下载发布互操作修复（GMB 第 2.6.1 步，2026-09-02）

- 外部调用方 动作/QR_V1 输入的 `release_tag` 与 CitizenServe publication/D1 的
  `version_tag` 已明确分域。Swift 发布器只在 publication 组装边界显式映射；快照、
  回滚、比较和验收不再错读动作字段。
- CitizenServe 302 与 外部调用方 精确验收统一使用权威仓库
  `crcfrcn/citizenchain`，不再使用旧所有者。
- 唯一跨端 golden 位于
  D1 路由测试与 Swift 真实 codec XCTest 同时执行；Node/Rust 门禁锁定消费接线、
  字段闭集和仓库身份。
- 本机验证为 CitizenServe 18/18、GMB 守卫 14/14、外部调用方 Node 1/1、
  Swift XCTest 2/2；282MB 受控输出已移入
  `/Users/rhett/.Trash/gmb-platform-naming-step2-6-1-20260902-1219`。
- 本步不改链协议、节点运行时、D1 schema/数据、Release Tag、安装包、updater 或
  manifest，也没有部署、正式 Release 或 Git 操作。当前只能声明源码合同与
  本机验证完成，不能声明线上已切换。

## 四端正式 Release 资产公开命名（GMB 第 2.7 步，2026-09-02）

CitizenChain 正式 Release 自定义资产名现固定为以下闭集；`<VERSION>` 是不含 `v` 的版本值：

| 公开平台 | 安装资产 | 更新资产 | manifest |
|---|---|---|---|
| LinuxARM | `citizenchain-node-LinuxARM-v<VERSION>.deb` | `citizenchain-node-LinuxARM-v<VERSION>.AppImage` | `citizenchain-node-latest-LinuxARM.json` |
| LinuxAMD | `citizenchain-node-LinuxAMD-v<VERSION>.deb` | `citizenchain-node-LinuxAMD-v<VERSION>.AppImage` | `citizenchain-node-latest-LinuxAMD.json` |
| macOS | `citizenchain-node-macOS-v<VERSION>.dmg` | `citizenchain-node-macOS-v<VERSION>.app.tar.gz` | `citizenchain-node-latest-macOS.json` |
| Windows | `citizenchain-node-Windows-v<VERSION>.exe` | 与安装资产共用同一 `.exe` | `citizenchain-node-latest-Windows.json` |

  仍是统一流程中的独立动作，不增设第二套发布实现。
- Release Tag 前缀继续为既有 `citizenchain-linux-arm-v`、
  `citizenchain-linux-amd-v`、`citizenchain-macos-v`、
  `citizenchain-windows-v`；内部 D1 发布键、Tauri updater target、Runner、Rust target
  triple、Debian 架构与操作系统工具链值也保持不变。公开资产名迁移不得机械改写这些类型化值。
- CitizenServe 和 外部调用方 共同校验新资产；旧 `linux-arm64`、`linux-amd64`、
  `macos`、`windows-x86_64` 自定义资产只允许存在于负向测试或历史记录，不能重新进入
  当前 Release matrix、发布指针或下载 Location。
- macOS 互操作 golden 的新 snapshot anchor 为
  `ccdp:macos:1:c7665f9bf103e67517f8c56665db3553ec3a764fc935a0d0ac5337d9bd67b042`，
  由 Swift、CitizenServe、Node 与 Rust 共同消费。
- 本机验证结果为 CitizenServe 18/18、GMB 守卫 15/15、外部调用方 Node 49/49、Swift
  XCTest 4/4，共 86 项；Node 的 49 项由 github 26 项、matrix 8 项和 native-ui 15 项组成，
  native-ui 第 15 项通过 `testTuyuMerchantMobileUI` 别名注册。静态格式、语法、JSON 和 golden
  哈希检查均通过。
- 284MB 受控验证目录已整体移至
  `/Users/rhett/.Trash/gmb-platform-naming-step2-7-20260902-1245`。独立终审另发现 Vitest 覆写了
  源码树既有 `.vite` 目录中的 3450-byte `results.json`；该精确文件已移至
  `/Users/rhett/.Trash/gmb-platform-naming-step2-7-vitest-results-20260902-1255.json`，预存空目录
  保持不动，活动源码树无本轮缓存文件。本步没有运行 Git、远程 CI、正式 Release、发布或部署，
  也没有启动、停止、安装或重启 外部调用方。
- 生产 D1 可能仍保存旧资产指针。正式上线必须把新 Release 资产、CitizenServe 严格身份校验与
  publication 指针作为同一维护事务切换并验证；源码迁移完成不能冒充线上已完成。
### 产品流程物理归属





Node 的每个实际平台×CI或Release只有下列一个本仓顶层 Workflow，均有且仅有一个主 `flow` Job；必要辅助 Job 只服务该身份。Workflow 只调用本仓 `scripts`，不执行保存、拉取、推送、Start 或 Publish，不读取 TataConsole 私有源码和资料。CI 验证源码，Release 生成正式产物；Publish 是否已接入以本仓当前声明及实际入口为准，不由本文新增。

## 目录整合与平台输入

Runtime 与上游/派生目录冻结。自有 `crates/signing/lib.rs` 由 Cargo `[lib] path` 显式登记；`blockchain-harness/src/harness.rs`、`qr-protocol/src/export_registry.rs` 由原名称的 `[[bin]]` 登记。Node 二维码生成目标为 `node/frontend/protocol/qrBodies.g.ts`；P2P 坏块测试位于 `node/src/core/p2p_bad_block_tests.rs`，仍由 service 原测试模块引入。移除空的 postgres `.gitkeep`，不清理实际数据库。

### 完整链根与白皮书来源
节点、Runtime、OnChina和scanner保持同一完整CitizenChain仓。节点run/clean-run/prepare-toolchain不再使用旧聚合父根。内置白皮书仍由官网src/whitepaper.md及原图片生成；产品dependencies.json只声明crcfrcn/citizenweb的main，准备器每轮先捕获main真实SHA，再取得该不可变提交并验真detached、origin及干净状态。生成器退出时清理自己创建的输入目录；正文、图片嵌入、摘要和节点功能合同保持不变。首轮门禁须在官网真实main推送后执行相关链编译。
## 完整产品组织与执行合同

所有者：`citizenchain`，正式源码根 `<本仓根>`；本说明属于该完整产品内的Node组件资料。组件不会拆成独立仓库或目录产品。所有执行身份统一为 `产品.平台.流程`；工作目录合同由本文“本机固定执行目录”唯一承载，不建立平台工作目录层。

真实平台目标：`macos`、`windows`、`linux-arm`、`linux-amd`、`wasm`。

仓库推送仅上传本仓已经保存的main提交。控制台推送的唯一实现为console/tuisong.mjs，每仓一次生物识别，授权成功后建立独立任务，任务栏记录Git进度、准确SHA、取消及成功/失败终态。只执行Git与GitHub main只读回查，不执行源码、依赖、注释、文档、测试、签名或资源门禁；不派发产品Workflow、不运行hooks、不续签或重复认证、不自动重试、合并或强推。

本仓已移除GitHub main推送门禁触发器；main上传后不自动运行产品自动化。自动化由用户单独发起，产品仍拥有自己的Workflow、声明、资源、测试和产物实现；产品不导入控制台源码，不依赖控制台工具库、私有规则或其它仓库工作树。控制台只是可选Git客户端。各仓可独立使用公开Git接口完成仓库操作，公开SDK依赖不构成流程耦合。


技术文档由所属完整产品仓根唯一持有；私有规则和任务库由控制台私仓持有，公开产品不读取它们。公开门禁不依赖私仓资料、安装包源码、其它本机产品或个人账号；必要链真源只读本仓明确固定的公开40位SHA，不在门禁中跟随main。本机开发跨产品验收仍比较三仓已保存快照与各端真实镜像。

## 组织重构验收边界修正

公民链版本验真产品参数统一citizenchain；节点平台读取node/tauri.conf.json软件版本，wasm平台使用spec-version；WASM回执展示标题使用完整公民链，平台版本与验真边界不变。

### 门禁与开发审查职责

准确中文注释按开发阶段逐项复核，不以保留源码每文件包含汉字作为仓库门禁的开发凭证。初始完整内容、生成文件和上游原件保持原文；真实第一方临时注释、机密、源码输出、Workflow、依赖和适用测试仍由本仓同提交门禁验真。公民门禁只把scripts中的Node命令行结果报告识别为CLI输出；本仓实际执行测试的准确协议拒绝断言不属于新运行协议，字符串、注释、模板和未登记测试中的同文不豁免。保存及推送仍逐仓独立授权，并以本机门禁和同SHA的GitHub门禁双成功为唯一终态。

### 固定产物产品目录


节点客户端 RPC 必须显式配置 CITIZENCHAIN_RPC_URL 可信 HTTPS 地址；公网统一 Cloudflare Edge，局域网仅可信 TLS，缺失配置失败，不伪造本机 HTTPS 服务。远端查询均 HTTPS 且禁止重定向。链上中国健康检查只信任本机构现有 CA，以 onchina.local 校验主机名并固定连接本机，移除证书跳过与明文回退。烘焙/审计/历史导出脚本保留功能但要求显式可信 HTTPS，禁止重定向；上游 Apache 法律注释原件保持，Runtime 无任何修改。

公开仓门禁保留 Apache、MIT、Unlicense 三种准确官方行注释引用；字符串、额外路径及其它网址继续拒绝。Runtime 未修改。


公民链门禁准确辨认既有 rustls API、GRANDPA 数据库键、官方 protoc 35.0 来源、QR 守卫测试名和 WASM 旧标签拒绝夹具；未知版本继续拒绝。本机白皮书生成文件仅 JSON Markdown 正文属于文献，其它代码及网络安全检查完整执行。

八个原准确字面量以原顺序通过 Node stdin 批量去除，减少长生成正文的 Bash 复制成本；新增超过 320 KiB 重复标识的真实门禁性能回归，未知版本和网络安全检查不变。

首推还逐字保留十三份原 Substrate benchmark 53.0.0 生成 weights 的编译器声明；固定完整摘要不同、未知文件及新增抑制仍执行原中文理由要求。生成权重和 Runtime 源码未写入。

QR registry 回归只读取本仓 Node/OnChina 输出；Dart 两份生成器的完整字节金标固定于组织重构保留的 Wallet 原件摘要。App 宿主 API 全部原断言转移到 App 同名真实测试，SDK 消费仍来自其声明锁定的公开提交。避免新独立仓第一次推送与未发布消费者相互等待，不读取本机邻仓。

账户金标按真实 kind 选择完整派生输入：机构普通账户使用 cid，机构命名账户增加账户名，个人多签使用创建者账户及账户名；缺项、未知类型、重复语义键和 account_id 漂移继续拒绝。

GitHub 推送门禁从 `rustup which --toolchain 1.97.1 cargo` 获取登记工具链的 Cargo 普通原件路径；不把 PATH 中指向 rustup 的 shim 交给原件校验。解析失败直接停止，原有准确 Cargo 版本、QR 格式/Clippy/全套测试检查保留。

## 产品介绍与开源许可

根目录 `README.md` 仅提供本产品简明介绍，不承载技术方案、任务记录或验收结论。独立自有代码采用根 `LICENSE` 的MIT；上游代码、衍生修改、依赖及组合分发遵循各自原许可、版权、例外与附加要求。
Node收编GRANDPA源码保留GPL-3.0-or-later及Classpath例外，Runtime已有Apache源码保留原版权头；根目录补齐两份上游许可全文，Runtime源码不因许可补充而改写。

### 组织重构后的钱包与TLS读取合同

Node钱包只接受当前WalletStore字段闭集；旧id/address/activeId格式、未知字段、不完整账户、重复账户、错误SS58或不规范的活动账户标识直接失败。持久化文件仅保存Cold条目，Hot钱包由本机powr密钥事实动态生成；文件中的规范active_account_id允许指向动态Hot，因此不要求它出现在Cold条目中。向前端提供钱包时先合并Hot与Cold，再确认活动账户属于可用钱包，否则选择首个可用账户。读取失败不会重写、迁移或删除原钱包文件。正常钱包读取和保存继续使用现有账户验证。

Node开发页和HMR仅允许HTTPS/WSS；开发及预览必须提供完整匹配的证书和私钥路径，静态构建不启动开发服务。OnChina请求仅允许无凭据的HTTPS来源，禁止自动重定向和跳过证书、主机名校验；LAN可显式加载受信CA，缺失或损坏配置必须失败，公网仍使用Cloudflare Edge。

### 本机Build代码所有权


本次依赖统一同时覆盖归档差分测试的第一方smoldot C ABI适配及hex/parking_lot直接声明；对应Cargo锁与SDK冻结摘要原子同步。上游PoW与libp2p内部闭包仍按来源保留，不把第一方适配当成上游例外。全17仓直接声明回归按准确源码归属检查Cargo、Pub与npm，不只比较依赖库索引。

## 本机编辑器依赖解析

源码工程直接读取各自package.json及原始package-lock.json。OnChina直接使用的图标包与dayjs分别固定5.6.1与1.11.19，不依赖其它包的间接声明。依赖归档仍按锁定完整性进入唯一依赖库，安装树归源码外工作目录；正式源码的node_modules只保留Git忽略的本机解析链接，TypeScript从正式源码检查实际业务类型。源码与Runtime不复制、不移动，依赖解析恢复不启动应用、TLS服务或链编译。

Node和OnChina的本仓file依赖使用npm锁文件原生link条目，并登记../../crates/scanner的准确包元数据；禁止用缺少本仓目标条目的打包归档条目代替本仓链接，默认npm ci按原锁离线安装成功。

本仓扫码链接包与宿主通过Vite resolve.dedupe统一react、react-dom的实际实例；依赖版本仍由原锁固定，不建立React别名或第二套版本。

SDK维护源码唯一入口为`/Users/rhett/polkadot-sdk`，远端为`crcfrcn/polkadot-sdk`的main，官方上游为`paritytech/polkadot-sdk`。独立维护仓不作为公民链构建的本地路径依赖；Node和Runtime由公民链统一声明和锁消费固定提交。官方版本更新须核对PoW验证与出块、GRANDPA最终性、非保留节点交易传播、SS58及WebSocket/Relay/DCUtR等实际自有差异，完成节点真实回归后才能更换产品锁。保留旧历史引用，不合并旧分支或把全部官方历史认定为自有补丁。第2a步仅完善维护入口和同提交门禁，公民链单位、发行、收费、账户与最终性不改变。

公民链工作区及二维码协议包的第一方作者元数据使用同一准确作者署名；作者字段独立于依赖仓库来源，固定组织及消费提交保持不变。

第5步获批源码已将NodeGuard读取交易的方式适配官方Ethereum包装器：签名来源仍读内部preamble，调用使用ExtrinsicCall::call；Timestamp bare夹具通过generic交易.into()构造。原有原生费用和候选Runtime策略断言保留；本轮未开放Ethereum RPC、端口或生产升级，默认Node构建、完整节点回归及真实源码WASM执行验收均已通过；现有费用及候选Runtime策略没有弱化。


### 产品独立资源与编译入口


本产品平台闭集为`macos`、`windows`、`linux-arm`、`linux-amd`、`wasm`。调用格式为`node scripts/build.mjs <requirements|prepare|build> <platform> --work <绝对工作目录>`；requirements只读并输出唯一JSON，prepare/build从标准输入读取schema=1的资源回执。调用方交付准确工具执行器、锁定依赖目录、Git来源和归档后先prepare，再读取展开来源新增的需求，完整交付后执行build。准备、展开和编译属于同一调用工作根，各平台互不共享可写状态。独立调用方按本仓声明准备资源即可运行，无需读取其他产品工作树或私有资料。



## 2026-10-06 产品自主资源阶段（第2步）

本仓`scripts/build.mjs`拥有工具准确来源/版本/配方、递归锁解析、缺失获取、验真、复用和本轮依赖准备；`scripts/build.mjs resources <platform> --work <绝对外部工作根>`调用同一实现，独立入口为`resources.mjs <platform> --work <工作根> [--offline]`。前者从stdin读取公开身份回执；后者允许空请求。最小宿主必须使用本仓声明的官方Node25.2.1绝对入口，本机配方限定macOS ARM；资源阶段使用本仓声明的Node入口，不能从PATH取同名程序。工作根预先存在、位于源码外且不经过链接。

现存`PRODUCT_TOOL_ROOT`与`PRODUCT_DEPENDENCY_ROOT`是工具和依赖的只读路径输入，本身不能完成控制台缺件准备与交付。当前供给职责按本文“工具与依赖的声明和供给职责”执行：经控制台运行由控制台准备、保存与供给，独立运行由产品自行处理；源码外`~/.local/share/product-resources`仅描述现存独立资源存储，本轮可写状态仅在work。GNU Bash/grep/sed纳入自身需求；发行件旧Shell仅用于声明中的首次GNU构建，不进入正式PATH。下载/源码工具编译不持全局锁，最终不可变对象提交使用短锁，取消传递到工具进程组。错误摘要、损坏、未锁来源、路径越界和显式离线缺失失败并保留可疑原件。

Pub/npm/Cargo按原始锁准备；Git按固定HTTPS提交检出，Git Cargo目录源展开workspace继承并锁定相对包版本；CocoaPods按准确锁摘要恢复验真快照，缺失spec校验规范摘要，未锁源码来源拒绝取得。Android固定包与修订归产品；额外平台仅消费官方固定发行来源与发行树摘要，不借宿主历史SDK目录。Maven供给只读验真后复制到独占Gradle缓存，由产品准备现有配置，消费仍离线；全库坐标导入与旧目录清理留到第5步。

`PRODUCT_WORK_DIR`、`PRODUCT_BASH_BIN`、`PRODUCT_RSYNC_BIN`及`PRODUCT_SOURCE_DIR`是公开工作/工具/工程入口；Flutter修订不读取调用方私有变量，也不回退系统rsync。旧Flutter补丁对象与当前配方不符时拒绝复用，真实替换须按准确资源操作另行授权。本步不改变编译、签名、安装及回读顺序，不修改产品UI，也未执行真实工具下载/安装。受控资源测试不能代替官方首次取得、正式编译或最终真实运行验收；第4至7步仍待逐步确认实施。

资源原件按完整内容验真；可选依赖供给读取`objects/<SHA256>.blob`。锁解析器与Git bundle仅在本任务现场物化和复用，不提交共享派生目录；源码工具依赖与官方有序补丁仍按各自声明取得。Pod spec每次按锁中的规范checksum回验，Git tag只核对发行声明并消费本产品预锁提交；HTTP发行件消费固定SHA256，首次源码准备命令来自该已验真spec并由GNU Bash执行。spec和准备后源码仅进入本任务CocoaPods视图，离线缺少原件直接失败；供给索引不决定产品版本。正式PATH排除旧POSIX Shell，`sh`对应已验真的GNU Bash。

独立缺省资源目录内`tools`保存工具发行件及工具编译输入，`rely`保存产品依赖的归档原件；Git bundle、Pod spec和源码的派生视图只放本轮工作区。根据用户最新要求，分步骤先完成实现与用例，整项解耦任务完成后统一测试；本步实施记录不等于真实工具首次取得、完整Build或安装验收通过。


### 第3步：产品完整Build入口（2026-10-06）

本产品的正式完整入口为已锁定Node的绝对路径调用`<本仓根>/scripts/build.mjs execute <platform> --work <已存在绝对工作根>`，可选`--offline`。输入stdin可为空；调用方可传schema/product_id/platform/work及真实run_id/program_digest，禁止私有变量或执行命令。入口内部完成需求→资源→准备→再次需求/资源闭包→编译→适用签名/安装/回读；独立与控制台调用同一实现。最小引导Node只启动本产品的资源引导器，产品按自己的官方Node声明准备并重入，控制台运行Node不决定产品Node版本。

标准输出只有唯一有界JSON：schema、product_id、platform、work、completion、files及可选真实run_id。completion沿用固定平台的device-install/macos-artifact/compile-only；files按本产品flows.json登记路径和SHA256。编译日志使用stderr进入现有任务日志，不新增资源任务或任务状态。完整结果只在各阶段成功、源码/锁不漂移、工具进程确认退出后落入本轮build-result.json；同根并发或复用旧结果拒绝，取消/失联/错误身份/损坏候选不得成功。

控制台每次Build直接读取本产品当前flows.json入口，调用一次execute；控制台只跟踪真实任务、核验公开结果和保存产物，不解释产品工具、依赖、编译参数或设备规则。当前控制台静态菜单、其它产品流程/安装器与程序摘要的历史耦合仍归第4步解除，本步不能当作整项解耦已完成。

本步同步完整入口、失败/取消/并发、结果/路径/摘要及适用移动端用例，但未运行测试、语法检查、编译、签名、安装或工具下载/替换；全部实现步骤完成后统一验收。源码交付与用例存在不代表真实Build已经通过。


### 第4步实施中：远端路由当前声明


本次同步路线读取、热更新和失败边界用例，未运行测试、语法检查、编译、签名、安装或下载。第4步仍在开发中：Publish执行器、聊天安装器、Start、固定菜单声明与完整程序摘要的其余实际耦合尚未解除，不能报告该步或整项任务完成。


节点前端本仓扫码包按既有install-links=true复制安装：node/frontend/package-lock.json直接登记@gmb/scanner-react的file来源、1.0.0版本、jsqr1.4.0依赖、React19.2.4 peer及Node25.2.1引擎，不保留外部目标的link条目及扫码包开发依赖副本。根声明、registry版本/来源/SRI及默认std+custom-protocol不变；原build-local.test.mjs校验复制模型、peer和声明/锁一致性。本次验收须在源码外完成离线安装、TypeScript/Vite资产及默认Node真实回归，通过结果另行回读；编译验收不等于签名封装、UI运行、移动端、MetaMask或正式升级完成。


公民链桌面启动由本仓`scripts/build.mjs`实现，读取本仓macos平台的启动声明，检查节点App和可执行入口，保留先验签、读取入口、检查已有节点的顺序。已有节点只激活窗口；首次启动由公民链准备PostgreSQL后传入原有节点参数。CLI领取本仓target/test作为临时现场，完成后自行收尾。`confirmNodeStartup`独立确认本次节点的产品、平台、产物路径及started终态；确认后才通过PRODUCT_RESULT_FD=3输出回执，失败退出不输出成功结果。该确认不导入控制台、商家或厂家的校验。

### 产品远端完整入口



相关正常、失败、身份、版本来源、独立远端跟踪、候选重试和真实控制管道边界用例位于本仓`scripts/flow.test.mjs`；当前只完善源码，尚未运行用例或远端操作。

本机固定菜单不再登记Start源码与产物，也不保存Build产物文件名或验真相对路径副本。执行时读取所属产品当前Start声明及Build的files、verificationPath；产品修改自己的App名称或可执行文件路径无需重编译菜单，原有平台完成方式及产物摘要收口保持。相关用例已同步，尚未运行。


2026年10月6日节点前端复制模型锁修复已直接离线验收：TypeScript/Vite通过，本地文档2项、扫码组件7项及原Build回归3项全部通过。默认std+custom-protocol的生产节点和测试二进制均构建成功，真实源码WASM和锁定add12 SDK实际使用；完整Node回归330项全部通过、0失败/忽略/过滤，耗时2930.33秒、退出0，包含完整真实三节点Ethereum、官方GRANDPA、分叉日志/回执、迟到追块、同库重启和单次真实收费。三个WASM摘要与此前服务验收一致，Runtime无差异。默认可执行文件/资产编译及完整回归范围明确，正式签名封装、设备UI、远端CI、移动页面、MetaMask和正式升级仍待各自验收。版本读取须使用已有CITIZENCHAIN_HEADLESS=1显式无头入口，单独--version在有显示环境会进入桌面分支；本次误探针已停止，只追加既有开发审计，正式链数据未改。


### 产品软件记录与正式版本恢复



资源工具取消、超时、输出超限和异常收尾均等待主进程与整个后代组退出；无法确认退出时保留工作根和候选，禁止删除输入或改为可写。真实取消退出顺序用例仅写入resources.test.mjs，尚未执行。


### 机构岗位权限的既定职责矩阵

下表为2026-07-19既有职责登记，保留原固定权限事实；当前启用条件及链上有效任职仍以本文授权规范和实际runtime为准，不表示本次解耦已执行业务验收。


表中 `P` = `Propose`，`V` = `Vote`；未列出的动作一律无权限。

| 固定机构/岗位 | 推荐固定权限 |
|---|---|
| NRC `COMMITTEE_MEMBER` | `pub-mgmt/3 P+V`；`rt-upg/0 P+V`；`res-iss/0 P+V`；`res-dst/0 P+V`；`gra-key/0 P+V`；`multisig/0,1,2 P+V`；`onc-iss/10..14 P+V` |
| 每个 PRC `COMMITTEE_MEMBER` | `pub-mgmt/3 P+V`；`rt-upg/0 P+V`；`res-iss/0 P+V`；`res-dst/0 P+V`；`gra-key/0 P+V`；`multisig/0 P+V` |
| 每个 PRB `DIRECTOR` | `pub-mgmt/3 P+V`；`rt-upg/0 V`；`res-iss/0 V`；`res-dst/0 P+V`；`multisig/0,2 P+V` |
| NJD `CHIEF_JUSTICE` | `pub-mgmt/3 P+V`；只由首席大法官发起司法院本机构治理 |
| NJD `DEPUTY_CHIEF_JUSTICE`、`JUSTICE` | `pub-mgmt/3 V` |
| NJD `CONSTITUTION_GUARD` | `pub-mgmt/3 V`；`leg-yuan/1 V`，后者只用于修宪护宪终审 |
| PRS/NLG/NRP/NSN/NED、PGV/PLG/PRP/PSN、CGOV 的 `LR` | `leg-yuan/0,1,2 V`，仅承担行政签署或国家/省级三人会签，不得发起法律提案 |
| FRG 每个 `PROVINCE_COMMISSIONER_<省码>` | `pub-mgmt/3 P+V`（仅本省岗位任职治理）；`cit-id/0..4,6..8 P+V`；`addr-reg/0..4 P+V`；当前创世还含历史 `ins-reg/0 P+V` 预留，但本任务不建设或接入机构登记模块；实际省级业务必须按目标省码绑定同一个省岗位 |
| 公民链基金会 `LR` | `pri-mgmt/3 P+V`；`sqr-sub/5 V` |
| 公民链基金会 `GENESIS_PRODUCT_MANAGER` | `pri-mgmt/3 P+V`；`sqr-sub/5 P+V` |
| 公民链基金会 `GENESIS_PROGRAMMER` | `pri-mgmt/3 P+V`；`sqr-sub/5 V` |
| 其他公权创世机构的 `LR` | 空权限；岗位永久存在且允许空缺，但不继承委员、董事、司法或注册权限 |


### 产品独立资源与唯一依赖供给

本产品的scripts/build.mjs独立拥有需求解析、准备配方、来源与摘要验证、可写视图和失败条件。独立执行时由产品获取、保存与复用缺件；经控制台执行时由控制台按产品声明准备、保存并供给，产品核验并使用。PRODUCT_DEPENDENCY_ROOT仅是现存只读路径输入，缺少路径或原件不得在控制台执行模式下触发产品自行下载；实际供给接入仍需代码改造与验收。依赖索引读取仅接受schema_version=2及packages、git_sources、pods，不恢复旧目录或整锁快照。

Maven的具体JAR、AAR、POM、module及分类器文件统一由packages的group:artifact、version、准确上游URL、SHA256和SRI定位objects中的原件。产品在本轮work/dependencies/maven按上游分区复制独占文件；不复制Gradle二进制元数据、锁和下载状态。产品生成本轮GRADLE_USER_HOME/init.d初始化脚本，只在自身已声明的同源仓库之前加入本轮原件视图，缺件仍按产品原仓库解析，明确离线则失败。Gradle解析、工程状态和后续编译都属于同一产品任务。

Pod由pods中的name、version、checksum匹配当前Podfile.lock；spec保存官方CDN地址和原件摘要，source保存官方podspec来源，files保存发布树相对路径、文件内容摘要与权限或安全内部链接。只物化本产品所需的单个发布坐标；其它Pod、整锁、平台或宿主变化不要求复制全树。产品仍按CocoaPods官方规范回验SPEC CHECKSUMS，再验证本产品预锁定Git提交或HTTP发行摘要与源码回执。可写缓存和工具VERSION仅在本轮work产生，不能写回共享原件。

错来源、摘要、重复同源内容、生成状态、硬链接、内部链接越界或循环、取消及任务副本漂移均据实失败。独立与控制台调用使用同一实现；控制台只提供可选原件并跟踪原有任务，UI、功能、按钮、平台与操作顺序保持。用例源码已同步，执行留待整项实现结束后的统一测试。


### 独立入口回归验真边界

资源回归使用自带固定提交、源码字节和spec的合成Pod，不借用产品真实Pod清单提供测试输入；无真实Pod需求的平台也验证来源、摘要、链接、循环、取消和物化失败。测试现场仍位于本产品target的准确平台，不写源码或其它产品目录。资源声明与生产依赖坐标不因测试夹具改变。

Start只接受当前产品声明所对应平台target内的真实App目录；拒绝源码、其它平台和链接候选。产物验签、声明及可执行文件回读、前后摘要、取消处理和原启动顺序保持。

资源取消对同一真实进程组每轮只发送一次信号；组不存在或Windows时才发送给主进程。仍等待主进程和后代实际退出，8秒未退出才强杀，12秒仍未确认则保留现场并失败；取消不能成为成功。


本产品scripts/build.mjs的模块初始化与CLI执行分离：私有异步runCLI承载原命令主体，仅在直接执行文件时启动，拒绝时输出错误并以退出码1失败。模块求值先完成，scripts/build.mjs可反向导入同一checkWork、requirements和平台校验，不复制实现或增加启动入口；普通import不启动CLI。现有公开参数、JSON请求、--offline、必要重入、资源/准备/编译/适用签名安装回读步骤以及取消与结果合同保持。离线缺件和非法输入必须真实失败，禁止以未完成顶层await退出替代完整结果。对应真实CLI回归只在自有target测试现场替换资源供给边界，验证反向导入、参数与错误传播，不据此声称实际产品编译通过。


本仓平台命名门禁仍扫描完整Git跟踪路径和正文，仅在内存副本识别scripts/build.mjs中唯一规范的toolDefinitions与flutterPatch声明。规范JSON回读及唯一工具身份阻断重复键、转义、歧义和重复声明；使用Flutter时核验准确官方来源、版本对应归档和本仓补丁来源与全文摘要，未使用Flutter时只接受已核实固定来源与全文SHA-256的共同原补丁。仅处理官方native_assets_host.dart中与准确文件头、行号、lipoDylibs签名及紧邻调用同时闭合的一行原上下文注释，其它新增、删除、上下文、源码和路径的旧平台名称继续拒绝；实际资源源码、补丁、版本、锁和原件不变。目录边界回归以unlinkSync删除自身合成目录符号链接，继续完整验证根target普通目录可用、嵌套target/目录链接/普通文件拒绝；生产目录边界规则不变。回归使用本仓真实门禁与完整Git跟踪合成文件，只在本产品准确target测试现场运行，不将扫描夹具作为真实产品编译或发布证据。

本仓门禁的测试子进程白名单仅保留已有PRODUCT_GIT_BIN准确执行器路径，供完整Git索引夹具使用；缺少该准确入口时回归失败，不查询PATH、不回退系统Git、不传凭据或其它产品材料。不新增工具版本、声明字段、公开参数或生产资源获取步骤。


## 只读塔塔门禁与功能验收边界（2026-10-10）

`.github/tatagate/tatagate.mjs` 只读核对本仓主检出、HTTPS 来源、目录闭集、流程调用方向、Node 语法与本仓 QR 金标和 Pallet 注册表。门禁不准备资源、不执行产品编译或功能测试，也不调用 `scripts/build.mjs`。功能测试由所属 Build 或各平台自动化执行；旧门禁资源准备函数和配方已从 Build 清除。当前改动只完成静态检查，真实编译和正式门禁尚未验收。

## 本机固定执行目录

生成工作边界仅为 `<本仓根>/target`，直属只允许build、test两个固定目录；build用于本机编译，test用于测试，不建立平台、ci、release、publish或tmp固定目录。整个target必须忽略，并从源码复制、快照、摘要、资料门禁及打包输入中排除；源码中不得保留其它编译目录、工具缓存或生成视图。永久工具与依赖原件保留在源码外既有资源边界。

每个任务仍绑定完整 `citizenchain.<platform>.<flow>` 身份。使用同一固定现场的任务必须串行领取；首个文件步骤先核对身份、规范真实路径、无链接父路径和活跃任务保护，取得准确现场短锁，清空旧现场并回读为空，失败立即终止。不同任务不得互清；Start和仓库操作不得借用编译或测试现场。

本机Build由本仓完整execute入口完成：macOS只编译并验真产物，禁止自动安装；windows、linux-arm、linux-amd和wasm只编译。Build不得启动节点、执行链上升级或部署；任一步失败即任务失败。Runtime源码修改授权见 `CitizenChainRuntime.md`，节点部署和Runtime开发升级的控制台协调合同归 `TataConsole.md`，不得并入普通CI或Release。

历史验收路径保留原记录；本节为当前职责规范，不证明既有实现或真实流程已经通过本次验收。


## 三级目录与单一视觉资源（2026-10-09）

Runtime、node/vendor与node/libp2p保持原件。其它源码最深三级，第三层只有文件，每个目录至少两个真实直接子项；Rust公开包名、逻辑模块与业务接口保持，只以编译器文件定位调整物理结构。共享库归crates/harness、signing、protocol、scanner；QR金标直接归protocol/fixtures。节点与OnChina业务模块提升到各自src和frontend直属，目录使用完整单词；导入、Cargo本地路径及npm锁定本地来源同步。

根icons是本产品唯一持久静态视觉资源目录。Logo与公民币母版字节保持，重复PNG仅留一份，ICO/ICNS格式保留；六处扫码按钮共用原SVG几何并保持currentColor和尺寸，宪法背景从唯一PNG编码进离线HTML。官网白皮书图片按本轮Git输入读取，只进入该任务工程的icons视图并由Vite导入打包；生成模块不进入源码。

scripts仅保留build.mjs与publish.mjs；GitHub自动化归.github/workflows，门禁归.github/tatagate，文档、图标、维护和Runtime数据工具各归所属功能目录。内嵌测试位于对应正式实现之后，仅直接node --test时注册；导入无执行副作用。原Job阶段、独立产品平台流程身份、候选校验、缓存隔离与失败条件保持；同一平台同一流程共享其本文件实现，阶段号仍由既有Workflow固定。

本机输出只归target/build或target/test。非macOS检查通过同一显式工具入口，Windows预打包将原资源组装到本轮现场；本轮npm视图按锁定原件复制安装，不依赖旧控制台缓存链接。金标镜像读取公民App当前test/citizen/shared路径；未执行任何Runtime生成或刷新。实际验收及待确认状态只记录于唯一任务卡，本文目录合同不冒充完整发布或运行验收。

本地scanner的file依赖按所属源码的package.json名称、版本与实际路径验真；远端包继续按原锁SRI验真。根工程复制到本仓target内的任务现场时排除target自身，避免递归复制。固定Git原件显式导入锁定提交，按其原有refs/tata引用格式处理，来源与版本保持原锁。

registry与Git的目录源按原锁准确来源隔离，同名同版本不会混合。SDK临时发布视图保留其Git相对依赖身份；上游dev和可选依赖声明均保持原Git提交，只物化消费锁实际需要的包。原锁和上游原件不被改写。

统一扫码SVG以原几何独立资源输出，消费端使用currentColor的CSS遮罩，保留18×18尺寸并在WebKit实际窗口显示；不依赖外部SVG symbol引用。Rust build-std依赖按已验真Rust工具原始Cargo.lock物化，产品Cargo锁和工具版本保持。

本仓scripts测试正文统一位于所属正式实现末尾；固定目录回归执行node --test scripts/build.mjs，构建夹具支持随build.mjs内嵌回归保存。正常导入与正式执行不注册测试，门禁直接登记所属实现文件，不保留独立测试或夹具模块。

## 本机编译入口

scripts/build.mjs是本产品唯一完整本机编译实现。独立执行由本产品按自己的声明和锁准备资源；控制台调用同一入口，资源由控制台tools/toolchain.mjs供给，并在控制台console/build.mjs施加完成约束。provided模式资源失败不得改为独立准备。本产品在独立执行和控制台调度时均拥有自己的编译与清理实现；调度方消费结果后调用本产品公开收尾接口。

原独立本机编译脚本正文已归入本文件，生产执行不生成第二份编译脚本。公开SDK依赖按本仓锁消费，不读取兄弟仓本机检出或调度兄弟仓任务。GitHub实现不属于本次修改范围。

## GitHub自动化

本仓自动化只在GitHub的main源码上执行；控制台只调用与展示。各目标独立拥有同名的YAML与Node实现，不调用其他仓或其他目标的Workflow。版本、构建、测试、签名、完整产物核验与正式tag/Release均由本仓负责。

- `.github/workflows/release-linux-amd.yml`及同名`.mjs`。
- `.github/workflows/release-linux-arm.yml`及同名`.mjs`。
- `.github/workflows/release-macos.yml`及同名`.mjs`。
- `.github/workflows/release-wasm.yml`及同名`.mjs`。
- `.github/workflows/release-windows.yml`及同名`.mjs`。

每个目标的最后任务使用always读取所有前置结果：全部成功清本仓本目标旧成功，否则清旧失败并失败退出。仅保留最新成功、最新失败各一条；保护本次Run和所有活动任务，另一类结果与其他目标不受影响。删除关联正式Release、tag、Actions产物和Run后回查；任何清理错误都按实际失败报告，不自动重试。

四个节点Release目标的`node-version`动作由各自同名`.mjs`经`action node-version apply`和`action node-version lock`调用；内嵌版本脚本只接受`apply`或`lock`作为首个参数。目标各自验证并同步本次软件版本，调用方不得重复传入动作名称。

WASM Release的`prepare`由`.github/workflows/release-wasm.yml`从GitHub Actions Variables交付`CHAIN_URL`、`CHAIN_GENESIS_HASH`，从Secrets交付`CHAIN_ID`、`CHAIN_SECRET`。`.github/workflows/release-wasm.mjs`只通过受保护的`https://chain.crcfrcn.com`地址与Access服务令牌调用`chain_getFinalizedHead`、`chain_getBlockHash(0)`和finalized块的`state_getRuntimeVersion`；回读块0哈希须等于独立配置的创世哈希，版本至少高于当前finalized的`spec_version`。四项配置不写入Git源码、任务卡或日志；缺失时Release准备直接失败。

所属回归位于各目标同名mjs，覆盖前置结果、版本边界、平台隔离、活动保护和完整分页；真实GitHub构建与发布验收依任务授权另行执行。

本机编译现场由本产品领取和收尾。调度任务编号随本产品领取记录保存；本轮结果消费后，只允许匹配该编号的收尾请求。产品确认自身进程及资源供给后代全部退出后才清场；异常、编号不符或退出未确认时保留现场。控制台只持有调度锁、调用本产品入口并供给资源，不实现产品清理。

本机节点编译目标macOS、Windows、LinuxARM、LinuxAMD都由本仓资源声明交付官方protoc35.0；入口逐次核对`libprotoc 35.0`后才启动Cargo，缺件或错版直接失败。WASM目标单独按其现有源码构建合同执行。Apple工具由所选Xcode给出；包内符号链接的入口和最终普通文件均须留在同一Xcode内，运行时保留`clang++`等原工具名称。控制台调度的资源通道由同一`scripts/build.mjs`接收，不另建产品编译实现。

本机节点Cargo子进程的`LIBCLANG_PATH`和`DYLD_LIBRARY_PATH`仅从本轮已交付、已校验的Xcode Clang入口推导到同一工具链`usr/lib`；该目录和`libclang.dylib`均须为规范真实路径，库文件须为独占普通文件。两个变量只交给当前节点编译子进程，不作为工具来源、全局环境配置或生产节点运行参数。

当前锁定Rust1.97.1在macOS27加载被裁剪的宿主过程宏时可能生成dyld拒绝的LINKEDIT字符串池；本机节点Build只对Cargo的release构建脚本和过程宏设置`CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none`。节点自身仍使用原有release优化、源码和锁；此设置不进入WASM构建、远端Workflow或生产节点运行环境。

macOS App的独立Build明确准备`crates/scanner`、`node/frontend`、`onchina/frontend`三份原始npm锁，再逐个离线安装其工程副本；内嵌Shell只调用资源回执交付的绝对`PYTHON`，复制函数在Bash严格变量模式下先赋参数再计算目标路径。Tauri封装所需`xattr`固定为已验真的`/usr/bin/xattr`，由本轮Apple工具投影交付到受控PATH；App二进制构建、封装、描述文件与签名回读均属于同一完整Build结果。

独立Build在可选依赖供给缺少原锁固定Git提交且允许联网时，只对该提交的GitHub HTTPS `fetch`设置30分钟上限；本地bundle导入、身份复验及其它Git命令仍为10分钟。取得后必须核对`FETCH_HEAD`为原锁提交，再物化本任务bundle；离线缺件继续直接失败。

软件版本计算使用本目标GitHub运行序号作为单调下界，并与本仓已成功版本比较；失败或历史清理不使版本返回源码初值。版本只在GitHub本次运行内产生，同一Run重试保持运行序号，Tag另绑定准确attempt。


### 当前自动化最后处理

本仓每个自动化目标仅由自身release-<平台>.yml与同名mjs执行，最后处理依赖全部前置任务。清理只接受该目标准确Workflow路径、main和手动事件，不根据已删除文件或旧入口名称猜测归属。前置失败时，本次产物撤销与旧失败清理分别尝试并汇总错误；任何一项未确认均失败。固定依赖仍由本仓声明和原锁管理，不参加自产历史结果分类。

桌面更新检查由node/src/settings/desktop_update.rs按当前系统平台读取本仓唯一成功Run、正式Release及Tag证明，使用该Release的更新清单交给Tauri签名更新器。安装包不嵌入自身版本Tag，也不通过其它产品仓的自动化寻找更新。Linux正式集合同时包含DEB、AppImage、本体签名与平台清单。


### 本仓 GitHub 自动化与塔塔门禁目录

`.github/` 仅保留 `workflows/` 与 `tatagate/` 两个目录。`workflows/` 持有本仓自动化；`tatagate/` 仅保留 `tatagate.json` 与 `tatagate.mjs`。前者登记本仓门禁合同，后者保留正式门禁实现与测试报告器，测试代码统一位于正式代码之后。直接运行执行门禁命令，测试运行只执行末尾测试，普通导入不注册测试；本仓测试清单及逐文件成功回执使用同一个门禁文件且仅执行一次。

### 编译启动、GitHub自动化与独立分发准备

`scripts/` 只保留 `build.mjs` 和 `publish.mjs`。`build.mjs describe` 只读交付本仓唯一编译声明；同文件独占编译、启动、资源需求与固定工作根，控制台只调用公开入口。文档生成位于 `node/frontend/docs.mjs`，图标派生位于 `icons/generate.mjs` 且只处理本仓图标；行政区及账户派生脚本归 `runtime/primitives/`。节点与OnChina维护脚本及外部 PostgreSQL 调优样例已删除。旧权重benchmark脚本与模板已删除，已有权重实现保持。内嵌旧链规脚本及创世检查已删除，不再由Build直接修改邻仓资源；重新生成冻结链规须另行确定所属流程。

五组 `release-<平台>.yml` 和同名mjs仍由 `.github/workflows/` 独立拥有，继续产生版本、构建并逐件验收资产、创建GitHub Release和执行本目标历史清理。自动化不调用 `scripts/publish.mjs`。`publish.mjs inspect <平台> <准确Tag>` 是独立只读入口：通过公开GitHub Release、Tag和成功Run读取已完成产物的身份、来源、大小及公开摘要，输出待分发清单；它不派发或重跑自动化，也不修改Release。商店等分发目标未声明时，该入口不执行任何外部渠道上传。


## GitHub塔塔门禁与同类记录清理

本仓保留自己的.github/tatagate门禁实现和合同。main的push只触发本仓.github/workflows/tatagate.yml，gate与cleanup在这一个文件内执行；检出准确GITHUB_SHA并验证本仓GitHub事件、main引用和HTTPS origin，门禁继续执行本仓现有检查。gate成功时删除本仓该门禁旧成功Run；gate失败时删除旧失败Run；另一类最近记录和活动Run保留。清理前重新验真Run、Attempt和结论，删除后回查；清理错误如实记录并由后续运行补清，不影响gate检查结论。塔塔控制台通过塔塔鹿鹿的一次生物识别保存、推送本仓，并按准确SHA与Run ID追踪独立门禁任务；门禁结果不影响已确认的推送。
