# CitizenChain 节点技术文档

公民链本机交付命令为 `node scripts/build.mjs package macos`。它用现有本机 Developer ID 和描述文件签名并验证唯一的 `公民链.app`，再把包本体交付到桌面。塔塔安装器调用的 `node scripts/build.mjs compile macos` 仍交付其现有编译回执。GitHub 的节点编译命令为 `node scripts/build.mjs <平台>`，平台为 macos、windows、linux-arm、linux-amd。WASM 的正式编译命令为 `node scripts/runtime.mjs build`。本机和 GitHub 共用节点编译、资源装配及 macOS 安装包组装实现。

## 文件职责

`scripts/build.mjs` 只负责调用真实编译工具、组装完整运行资源、生成安装包和直接产物检查。它不请求 GitHub、不计算下个版本、不创建 Tag/Release、不执行终态清理、不启动观察进程、不维护任务状态，也不包含冷签、链升级、节点目录、SSH、服务器更新器。必要的动态库处理、Developer ID 签名、公证及安装包检查属于完整产物生成。

`scripts/onchina.mjs` 只实现宿主、所选模块和前端编译、类型检查、资源装配及必要产物检查，直接读取现有Cargo目标和模块--describe；不复制整仓，不启动Node或前端开发服务。集成编译输出属于target/build/<平台>；开发编译直接使用本仓源码，保持target/onchina/<平台>的互斥工作根、整套/单模块输出及指定交付目录。模块业务、VERSION、MODULE_INTERFACE_VERSION、协议、数据库和UI保持。预览由前端package.json直接调用官方vite preview，vite.config.ts只读取产品静态平台声明选择原开发产物路径，原TLS和代理保持。四端Workflow在Node编译后、Artifact记录前调用既有实际模块产物和实际IIFE装载校验。

## 五个平台的完整产物

四个桌面构建的产物目录各只有一个安装包，分别是 macOS 的 `公民链.app`、Windows 的 `公民链.exe` 和两个 Linux 架构各自的 `公民链.deb`。包内包含节点、链上中国宿主、模块、前端和 PostgreSQL 运行资源。Windows 和 Linux 发布时沿用既有版本化附件名，传输同一份包。macOS 在构建机验证签名。GitHub Release 附件只接受普通文件，发布作业遇到应用包目录时明确报出限制。WASM 交付 runtime.wasm、runtime.compact.wasm 和 runtime.compact.compressed.wasm。

编译工具版本以公开声明为准，缺件或错误版本直接失败。Unix PostgreSQL使用官方源码与既定OpenSSL配方，实际从随包路径运行initdb，运行库使用官方平台工具处理。macOS先执行Tauri官方build --no-bundle，再用官方bundle生成应用并装入自包含安装包，核对内外签名。GitHub 构建另外检查内置应用的时间戳与公证。Linux实际解包检查架构、宿主、模块、前端和PostgreSQL。Windows动态OpenSSL依赖及各进程使用Runner现有Visual Studio官方可再分发CRT进行应用本地部署，不能只因Runner已装VC运行库就把缺件安装包当作完整。签名材料仅交付编译入口的签名分段，不进入编译器环境或缓存。正式节点打包不内置WASM。

构建输出位于 `target/build/<平台>/artifact`。每个平台的 GitHub 工作现场彼此独立；Runner管理工具进程及取消，工作流在上传产物后清理该轮编译现场。官方 actions/cache 恢复和保存本平台Cargo中间物、npm下载缓存及macOS准确版本PostgreSQL资源/源码缓存，按平台、系统、架构及原锁隔离，限时且continue-on-error；不缓存安装包、附件、任务状态或机密。缓存失败不改变编译及完整产物的失败结果。

## 配置、编译与GitHub任务职责

scripts/config.mjs 只声明产品、平台、工具、原锁、固定交付名和必要配置读取。Windows 和 Linux 的 Release 传输名称仍由消费者代入版本，WASM 文件名沿用既有声明。

scripts/build.mjs负责Node编译、资源装配、签名和打包；scripts/runtime.mjs build负责WASM编译、临时版本输入及产物检查。编译脚本不管理GitHub Job、Release、Run或本地任务库。QR协议位于crates/protocol独立Cargo工作区，根工作区显式排除它。

五份既有Workflow各自负责工具准备、版本输入、prepare/build/release/cleanup及GitHub产物。Node版本从本平台最新成功正式Release读取后patch加1，首次使用节点配置版本；构建环境中的TAURI_CONFIG同时保留同值version与build配置，不因覆盖构建选项丢失版本；WASM仍消费当前构建的冻结链快照及双版本各加1的输入，不使用Node的X.Y.Z。五份工作流共用.github/release.mjs完成工具准备、版本核对、产物记录和正式发布。平台编译及历史清理仍由各自工作流调度。

节点四个原生平台默认不启用runtime-tests特性，Cargo校验覆盖工作区全部目标，仅排除sc-consensus-grandpa自身的测试目标，其生产依赖仍正常编译。SKIP_WASM_BUILD=1用于跳过SDK和产品WASM生成；该设置不会覆盖已有嵌入输出，因此缓存恢复后先清理缺少wbuild资源的旧生成输出。需要实际WASM的Ethereum整链校验和GRANDPA专属校验由Runtime构建流程负责，复用同一份产品Runtime，不向节点安装包添加WASM。

## 正式Tag、Release

build记录实际产物集合、大小和摘要；release只消费同一Run的Artifact，核对字节后由官方gh创建草稿，并通过官方API核对附件、转正式、回读Tag准确源码。保留既有发布元数据格式和WASM双版本字段，控制台开发升级仍可消费既有正式交付。发布Job不调用编译，不代表已升级链或部署服务器。

## GitHub历史与控制台本地任务

本地任务、观察连接、日志和本地历史保留属于控制台；GitHub Run、Artifact、Tag及Release的维护属于产品Workflow。控制台本地删除不代替GitHub删除，Workflow不访问控制台任务库。

五端cleanup保留本平台最近成功1条、失败1条记录。当前Run按已结束的prepare、build、release作业结果占用对应保留名额：三项全部成功才占成功名额，否则占失败名额；已完成Run使用GitHub真实conclusion。当前和其他活动Run不删除，已完成旧维护Run不占保留槽位。旧交付只按准确Run、SHA和Tag归属删除，并回读Run不存在；删除后再次核对两类各最多1条。各阶段不派发收尾、重编或取消任务，不由控制台接管产品清理。

当前Run最终conclusion仍由GitHub在cleanup结束后确定，不写入或伪造终态。清理错误必须失败，未完成状态如实展示；权限、网络错误或外部强制中断不能宣称清理完成。后续本平台任务继续按相同规则清理真实历史。

## 控制台既有功能边界

Runtime开发升级、节点部署、授权、冷签、安全及安装器属于控制台既有功能。开发升级的正式WASM读取检查归其既有实现，只从产品配置读取数据，不调用产品配置中的流程函数；节点取包直接读取静态artifact模板。升级交易、部署、安全和通用任务流程保持。

## Cloudflare 公共钱包接入

公共钱包域名 https://nrcrpc.crcfrcn.com/ 由 CitizenServe Worker 承接，Cloudflare自动管理公网DNS和边缘证书；Worker内部继续调用现有受Access Service Auth保护的Tunnel。节点RPC仅监听回环，现有cloudflared直接连接节点HTTPS端点；旧18080网关属于当前旧部署的历史状态，不能作为新节点静态交付方案。源节点保留Safe、DenyUnsafe及全局预算。客户端限流在Worker边缘按真实IP执行，节点仍承担回环来源的总预算，不依赖可伪造的转发头。

回源目标证书使用Cloudflare Origin CA，SAN为chain.crcfrcn.com。私钥只在生产服务器生成和保存；只上传CSR，证书公开部分与Origin CA根用于验证。生产CLI节点读取CITIZENCHAIN_RPC_TLS_CERTIFICATE的PEM证书链及CITIZENCHAIN_RPC_TLS_PRIVATE_KEY的PKCS#8 DER私钥；用户桌面节点改用本次App会话生成的回环证书。cloudflared直接连接生产节点HTTPS，originServerName为证书域名chain.crcfrcn.com，httpHostHeader为对应节点127.0.0.1:端口以满足既有HostFilter，caPool明确指定现有Origin CA根文件；禁止noTLSVerify和明文降级。

生产顺序固定为：现有旧Node承载新Runtime升级；新Runtime在正式链升级成功后，创世身份结果必须与真实块0一致，并继续正常出块与最终确认；随后才在Node增加对应创世身份守卫，再更新各节点软件。API源码存在或返回编译常量不代替链上验证。Cloudflare证书、CSR及直接节点HTTPS配置可提前准备，公共RPC激活在上述顺序及完整TLS链路校验之后，避免新Node守卫提前拒绝旧Runtime造成升级死锁。

生产回源证书使用Cloudflare Origin CA，私钥与CSR只在服务器生成，公网上传仅包含CSR。证书安装核对CA签名链、服务器域名与CSR公钥的一致性；材料不完整或不符时拒绝覆盖。信任根到期前须更新证书链，叶证书有效期不能代替CA根有效期。

## 聊天功能的唯一产品归属

聊天客户端归公民App的chat普通本地包，随App源码构建；协议唯一源为公民App的network/proto。聊天服务端归CitizenServe的chat模块。本产品不声明独立聊天客户端产品或分发路线。

CitizenChainNode 涉及聊天时只作为依赖使用方；本条不代表尚未接入聊天的产品已经具备聊天能力。

Total output lines: 12501

## Ethereum接口依赖边界

桌面节点只在同步作用域构造和销毁TaskManager，异步作用域仅等待退出；先回收服务，再关闭宿主Tokio Runtime。首个必要任务错误保留原文，清理panic只能追加。CPU矿工线程归本次TaskManager所有，停机通知外层等待和内层nonce搜索退出并等待线程结束，释放交易池与客户端引用；PoW算法、交易池出块门控和收费规则保持原合同。Ethereum初始化、方法装配、同步与TLS错误带准确阶段并保留原错误链。

桌面服务沿现有EVM账户映射游标自动推进一次性回填，CLI服务器不启动该任务。仅在同步完成、best等于finalized且交易池ready/future均为空时，使用本机powr密钥签发普通V5 System.remark；备注为固定用途加游标原字节的blake2_256。矿工热钱包、奖励绑定和维护备注共用签名锁及官方System交易池nonce查询，保持正常验签、收费、资源上限和禁止空块规则。每次最多一笔维护备注在途，核实最终性、ExtrinsicSuccess及游标推进后才继续；只有游标删除且完成标记为true才认定完成。Retracted只表示原入块被分叉撤回，继续观察同一笔已提交交易的状态流，不重签、不再次调用提交，也不重置同一次最终性等待的一小时预算。Usurped、Dropped、Invalid、FinalityTimeout、游标不动、读取失败、状态流结束或原一小时预算耗尽时停止并记录原因，不自动重发。该任务只提交既有交易，不直接改链数据库、Runtime或创世。

维护任务以evm_mapping动作写入已有security-audit.log，使用桌面已核对的数据根目录，复用同一审计写入锁及既有5MiB、5份备份轮转。记录进程、当前阶段、同步标志、最佳/最终高度及哈希、全池ready/future、已提交交易哈希、交易状态流、最终块/序号、游标摘要和推进结果；不记录密钥、签名或原始交易。相同等待快照只记一次，等待条件再次变化或新一轮等待时继续记录，不按停滞时长判定异常。complete、failed、cancelled和panicked分别表示原函数按完成条件返回、原函数返回具体错误、Future正常取消、Rust panic栈展开，记录退出时所在阶段和最后交易状态。审计JSON转义换行，避免一条错误伪造成多条记录；写入失败只沿原日志通道报错，不改变维护任务结果。审计记录不参与维护判断；等待条件、一小时最终性预算和全部安全校验保持，不新增RPC、配置开关或停滞超时。

Ethereum接入由node/src/ethereum.rs组织既有27个普通RPC和eth_subscribe/eth_unsubscribe，不使用全局metadata解码历史正文。每个区块只读取执行父状态的state_getMetadata与版本，构造独立Subxt上下文；0块使用自身状态。该上下文必须能解码V5交易扩展版本1，禁止复用仅有扩展版本0的V15长驻metadata。最终块推进、重复通知和旧通知回放不得使估算状态退回历史块；默认估算必须与显式指定同一当前块一致。余额、代码、gas和模拟执行通过既有ReviveApi在请求指定块执行，不尝试失败回退。框架公开的ReceiptExtractor、ReceiptProvider、FeeHistoryProvider继续负责Ethereum签名地址恢复、回执、日志、分叉裁剪和费用统计；索引仍是单连接内存SQLite、256块窗口，最佳块头和最终日志各保留16条广播缓冲。启动重建和实时订阅使用同一读取路径，分叉必须找到窗口内已处理祖先；最佳块头在索引完成后发布，最终性跳高逐块核实父链并补齐最终日志。日志查询逐高度核对当前规范链哈希，拒绝泄漏尚未裁剪的旧分支日志。框架SDK源码、版本、提交及缓存不改，不恢复V4提交入口、不重写历史数据。web3_clientVersion标识当前公民链节点版本。metadata缺失、损坏或边界错误继续使必要任务失败，不报告虚假可用。

节点接入层对历史缺方法RPC只接受4003、无data与准确原文或准确框架包装全文；不做包含、后缀匹配和通用前缀剥离。进入Ethereum历史之后、当前最佳块或其它错误仍原样失败。公开组件仍消费准确已保存SDK提交，不能用开发工作树替代锁定来源。

升级状态已导出Revive API但尚未生成Ethereum区块时，仅在返回完整默认对象，且同高Ethereum哈希、原始区块存储均不存在的准确条件下等待首块；不把全零对象写入索引、收据、费用历史或通知。区块查询返回null，eth_blockNumber明确返回尚未生成；后续真实区块同高且非零哈希校验成功后才登记索引边界。不改写升级历史区块，也不以重启或放宽断言替代真实后续区块。本机SQLite为进程内近期索引，正常生命周期重建，不清理链数据库。

节点统一固定消费SDK提交`05e0e9efcd4db73f92be9d0e1f804ee01bda5ac4`，根工作空间、QR工作空间声明与根Cargo.lock使用同一来源。该提交包含升级前历史边界与显式空RPC端点修复；旧版spec0无Revive时先提供原生TLS RPC并继续同步，同步到已升级Runtime后受控重启装配Ethereum。现有NodeGuard继续核验冻结创世身份。

公民链工作空间与QR协议消费同一固定SDK提交，依赖需求以产品Cargo声明与原始锁文件为唯一权威；SDK原件仅从该锁定Git提交物化，不读取开发工作树。传递依赖连接由产品原生Cargo锁固定，禁止消费时自动漂移。Tauri生成文件只允许写入CitizenChain本仓target内的规范绝对OUT_DIR；生成前核对OUT_DIR及已有tauri子目录的真实路径，拒绝源码、外部目录及链接路径。费用制度与原生账本单位保持原状。

正式NodeGuard只接受冻结的GENESIS_HASH与GENESIS_STATE_ROOT作为信任锚。启动自检异常只报告诊断；每次入链预检和导入仍核对客户端创世哈希、块0索引、实际区块头高度、状态根及区块头重算哈希，缺失或读取失败拒绝入链。正式构建没有可替换主网身份的配置或环境开关。

NodeGuard 只执行约定保留的23项：冻结创世身份、非空块、真实区块执行、创世固定声明、正常交易收费与费率、三类发行及总发行差额、质押本金、CID格式与命名空间、身份依赖、创世机构身份与协议账户、机构组成及固定治理阈值、候选Runtime接口策略，以及这些保留项的升级复核和失败关闭。机构岗位权限表、固定岗位与名册模板、基金会创始人岗位身份、开发/运营阶段转换、CID登记吊销历史及换绑修订号不在NodeGuard职责中。

普通执行块的唯一导入状态来自本节点验证通过的重放结果；不接收外来预计算变更作为可信状态，也不保留独立预计算比较器。NodeGuard不再执行独立完整状态业务审计；完整状态仍由最外层宪法守卫检查宪法记录，再由SDK验证状态根和共识证明。宪法守卫全部18项保持。

公民认证发行从永久领取事实与首次身份依据计算资格、人数、奖励档位和总发行计划，防止重复领取；不读取或审计Runtime内部临时工作队列。finalize仅检查合法发行计划对应的总发行差额，不再做逐账户余额、非free字段或新账户形态检查；省储行质押账户保护独立保留。

NodeGuard对普通区块的交易正文和`System::Events`统一使用父块哈希处的Runtime元数据，完整解码并拒绝坏元数据、截断或尾随字节。交易信封按实际V4/V5格式读取，V14/V15使用父Runtime的地址、签名和扩展类型，V16按声明的扩展版本及管线索引解释认证；不使用节点内置Runtime交易类型解释历史区块。付款账户与受保护调用从完整解码结果提取，收费调用缺少付款认证、未知扩展版本、非法账户或管线索引均拒块，不按高度跳过历史交易。升级块事件由升级前Runtime编码，因此不使用节点内置的新版`RuntimeEvent`解释整份事件列表；这使旧链无字段的`System::CodeUpdated`与新版带`hash`的同名事件都走同一条按实际Runtime解码的路径。只从完整解码结果提取准确`FeePaid`与`PaymentSettled`字段，原有固定链上费用、投票费、收款行费率及结算金额判断不变；`:code`变化时仅复核保留的费率存量和候选Runtime接口策略，不执行合成业务交易。未知元数据版本或费用事件字段结构失败即拒块，不根据升级调用跳过费用检查。

节点TLS RPC以最终确定块的实际Runtime元数据决定是否装配Ethereum方法。含Revive时必须成功初始化官方Ethereum客户端与同步任务，否则节点必要任务退出；无Revive仅允许创世旧版`spec_version=0`，此时原生RPC继续经同一TLS和Safe约束提供读链与交易提交，其他版本缺Revive均拒绝启动。正式Runtime升级最终确定后须重启节点，使Ethereum方法按新Runtime重新装配；该阶段不关闭NodeGuard、PoW或最终性，也不把缺Revive错误当作TLS证书失败。

SDK与公民链根工作空间声明同一radium0.7.0固定Git补丁，来源为`https://github.com/paritytech/radium-0.7-fork.git`，提交为`a5da15a15c90fd169d661d206cf0db592487f52b`。Cargo不继承Git依赖仓根patch，消费工作空间各自声明；QR工作空间没有使用该依赖时不添加补丁。

已选定公民链Ethereum ChainId为`2027`（十六进制`0x7eb`），后续RPC必须返回与
Runtime签名验证一致的编号。当前chain-spec中的`ss58Format: 2027`仍只表示SS58
地址格式，不作为已经提供`eth_chainId`的证据；依赖接入不代表MetaMask或兑换服务可用。

### Ethereum RPC与TLS

账户主余额统一读取同块System.Account的free+reserved，包含111分账户保留金，frozen不重复计入总额。Ethereum eth_getBalance在请求指定块解析OriginalAccount后读取同块总额，以既有NativeToEthRatio换算为18位精度；不存在的账户返回零，RPC、映射、解码或溢出错误保留。SDK的evm_balance仍提供执行和估算使用的可支配金额；账户保留金不属于手续费。

严格原生模式由现有Runtime路由单次收取业务费。公民链固定非零兼容价格为1 gwei，估算同时覆盖资源和费用，已签名交易的限额须满足执行资源及费用要求。

普通MetaMask接入的唯一源码为node/frontend/desktop/metamask.mjs，正式页面与生成入口归该源码。直接执行node node/frontend/desktop/metamask.mjs向标准输出生成HTML；普通导入只导出installHtml，不输出页面。原snap目录已删除，源码不保留第二份HTML，费用Snap源码、清单和npm包已取消。用户只需点击添加并切换公民链，分别确认wallet_addEthereumChain和wallet_switchEthereumChain；页面回读eth_chainId为0x7eb后才显示已切换。标准网络名称公民链，链ID2027，公民币GMB、EVM精度18；转账费率永久0.1%，最低0.10GMB，按整数分四舍五入。接入页直接展示上述制度，MetaMask原生估算读取真实eth_estimateGas与gas报价，可能包含钱包缓冲；实际收费以FeePaid及回执gasUsed×effectiveGasPrice核对，不安装插件或另改MetaMask界面。

MetaMask接入页的唯一源码为node/frontend/desktop/metamask.mjs，图标准确使用/icons/gmb.png，PNG真源为icons/gmb.png。节点构建脚本通过产品已交付的NODE执行唯一正式生成入口；远端remoteStep将正在运行该产品入口的Node规范路径交付为NODE，沿用现有官方工具版本，不再从PATH搜索；源码视图按仓库根的直接子项沿用原过滤规则逐项复制，避免Node拒绝根向其子目录复制，target与原有资源目录仍排除；输出仅进入Cargo OUT_DIR并嵌入节点，PNG直接嵌入；节点启动不读取静态目录。现有rpc_tls监听器在同一HTTPS端口交付GET/HEAD根页面与GET/HEAD固定图标，根POST、OPTIONS和WSS沿用现有RPC；未知路径404，不支持的资源方法405，HEAD正文为空。固定资源与RPC共用Host/CORS检查及TLS连接额度，不开放文件系统路径、目录列表或任意代理。根WebSocket升级优先于普通页面GET。CitizenServe仍从既有CHAIN_URL的origin读取两个固定资源，使用既有服务端Access身份、3秒总超时、128KiB实际字节上限及类型校验，不复制上游头、Cookie或错误。

对外链路固定为现有Cloudflare Tunnel→cloudflared→node回环HTTPS RPC。机构的证书、私钥路径、端口、Tunnel路由及HTTP Host均由部署环境提供，不进入通用节点源码。直接节点回源时HTTP Host使用对应127.0.0.1:端口以满足现有白名单，TLS SNI使用可信证书中的域名并严格核对CA，不增加网关静态目录。

公民链图标资源统一位于 icons，原 node/resources 目录已移除。Tauri 桌面图标及 Windows 安装图标直接引用 ../icons，打包资源映射到安装包内的 icons/，本机开发入口使用同一资源目录；Logo 派生器的应用母版为 icons/logo.png，公民 App 的来源清单同步指向此路径。公民币专用图标为 icons/gmb_019473.png；不把此图替换为应用 Logo 母版。

对外网络官网为https://www.crcfrcn.com，公共RPC为https://nrcrpc.crcfrcn.com/。安装页与/icons/gmb.png通过Cloudflare Tunnel和HTTPS交付。网页图标与MetaMask内部网络、原生币图标属于不同显示来源；wallet_addEthereumChain的iconUrls参数不代表钱包已收录图标。

Ethereum最佳块处理先沿父哈希收集并校验缺失祖先，要求父哈希、高度连续，且在256块窗口内到达已索引父块或创世，再正序清理同高分叉并写入收据。每块的旧分支删除、交易位置、日志及块映射使用同一SQLite事务，提交后才替换内存缓存、推进最佳块和发布通知；失败保留旧索引并允许重试。缺失父块、错误父链及超窗均拒绝处理。

节点结构为一个Node、一个原生客户端和交易池、一个回环RPC端点。官方
Ethereum RPC通过进程内RpcModule调用既有RpcHandlers，不连接内部网络，不启动
独立eth-rpc进程或8545端口。Ethereum方法与原生方法合并到原9944端点，只接受
HTTPS/WSS。公开模块和每个请求均显式使用DenyUnsafe::Yes；原生内部模块不对外开放。

设置CITIZENCHAIN_HEADLESS=1可强制节点进入无界面服务模式并绕过GUI；CLI子命令优先进入工具模式，执行结束后退出。

无界面 CLI 由service::new_full读取两项运行环境配置：
`CITIZENCHAIN_RPC_TLS_CERTIFICATE`为可信证书链PEM绝对路径，
`CITIZENCHAIN_RPC_TLS_PRIVATE_KEY`为对应PKCS8私钥DER绝对路径。材料须由获准的
密钥系统提供，不进入Git、文档、日志或链状态。缺失、无效、不匹配或相对路径配置
拒绝启动，不复用P2P证书、不开放明文监听。桌面macOS、Windows、Linux共用同一
Rust启动路径：每次App运行在内存中生成仅含localhost和127.0.0.1身份的本机RPC
自签证书，节点在该次App运行中手动重启时复用同一材料；私钥不写磁盘、不进入
系统信任库或子进程。桌面HTTPS客户端只信任本次证书并核对IP主机名，固定连接
127.0.0.1的当前RPC端口；远端RPC客户端继续使用系统信任根和原HTTPS限制。
Cloudflare Origin CA仅用于生产服务器回源，不交付给桌面用户。

控制台提供macOS本机编译入口，随后执行签名和安装；Windows和Linux由各自原生Runner编译，不在macOS交叉编译。桌面服务后台线程如在启动回执前panic，首页显示保存的具体错误；没有保存原因的异常断开使用通道错误提示。Ethereum RPC/TLS必要任务失败时，首页优先显示该任务的具体错误。Runtime元数据缺少Revive时，仅允许spec_version=0的旧链经TLS与Safe约束提供原生RPC；其他版本缺Revive拒绝启动。升级最终确定后重启节点，必须成功初始化Ethereum RPC，创世身份、NodeGuard、PoW与最终性守卫全程生效。

既有回环端点、请求/响应大小、连接数、订阅数、消息缓冲、batch、CORS与限流配置
继续生效。TLS握手最多10秒，握手连接也计入连接上限；仅信任真实连接IP进行限流，
启用代理头限流配置时明确拒绝启动。HTTP/WSS Host只允许对应回环地址和localhost。明文连接仅可被关闭或收到完整致命TLS告警后断连，不返回HTTP/JSON明文响应。配置的Origin白名单同时在HTTP请求和WSS握手核对，未许可来源返回403；允许来源的浏览器预检可使用JSON content-type。限流额度在端点连接之间共享，重连不能重置计数。
公开RPC只接受用户已经签名的Ethereum交易，Ethereum账户列表为空，不生成、托管
或解锁用户Ethereum密钥。既有本机钱包授权与节点守卫继续独立执行。

近期索引固定保留256块，在启动时按现有链状态重建；链状态已修剪而无法重建时
启动失败，不改变节点修剪或扩充历史存储策略。升级前历史尚无Revive API时，
节点接入层仅跳过窗口开头连续且准确缺`ReviveApi_eth_block`的旧区块，从首个Ethereum区块
建立`FirstEvmBlock`边界；进入Ethereum历史后的同类错误和其它读取错误仍停止启动。
节点通过公开RPC接口接入既有进程内处理器，不另开明文回环监听或网络连接；
公开RPC仍只由节点现有TLS入口承载。
重组删除被替换块及全部已索引后代，
即使通知中间存在缺口也不保留旧分叉。按交易哈希读取回执再次校验规范链。
latest/pending状态读取最佳块，safe/finalized读取GRANDPA最终块；newHeads订阅最佳块，
logs订阅只发布最终块日志。eth_subscribe使用标准newHeads和logs参数，由节点同一接入处理器接受、通知并取消；Node直接绑定官方SubscriptionKind，未知类型及多余参数拒绝，通知由同一必要同步任务发布。pending nonce通过原生system_accountNextIndex读取同一
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
把Ethereum transactionIndex当作原生执行序号；Node公开模块仍仅提供既有Ethereum方法。
gas、gasPrice和effectiveGasPrice是兼容执行资源表达，不能替代FeePaid真实GMB费用，
也不承诺gasUsed乘gasPrice等于实际扣款。新合约存在性存款111分由付款人真实资金转入，属于原生资金转移，与FeePaid手续费分别核验；费用与80/10/10分账保持原实现。

受控缓存固定为 `citizenchain/target/<platform>/<build|ci|release|publish>/`；四个流程目录永久独立，启动不建目录。macOS Build 的三个 Node 工程均在 `build/source-view/` 只读引用源码并把各自 `node_modules` 安装在该视图，Cargo、Tauri、前端输出、日志和候选也只写 `build/`。

WASM 远端自动化的官方 protoc 35.0 来源、SHA-256、解包和执行器核验归 `.github/workflows/wasm.yml`。Build 不准备自动化 protoc，也不读取门禁配置。本机开发必须显式提供已经验真的 `PROTOC` 路径，缺件直接失败。

本文是 CitizenChain 节点唯一技术事实文档，统一收录区块链总览、桌面节点、网络、挖矿、设置与节点安全边界。

## 受控源码归属

`node/grandpa/`、`node/libp2p/` 内经来源清单核实的上游原有结构，必须保留官方加载所需目录、版权、许可证与清单必需文档。该保留边界不得用于第一方改写、新增包装层或可删除的原件与生成物；来源与自有差异必须分别核实，不因目录名自动认定所有权。

节点与OnChina的reqwest版本由产品原始Cargo.lock固定并由Cargo解析。节点HTTPS请求按调用启用blocking、json、query及rustls，query用于查询参数。Polkadot SDK同样由原锁固定Git仓库和提交，依赖从锁定来源物化。

### citizenchain/crates 技术说明

`citizenchain/crates/` 只放依赖 CitizenChain runtime/primitives或链交易结构
的内部 Rust crate。跨 CitizenChain、CitizenApp、CitizenWallet 等产品的共享实现统一放在
仓库根 `shared/`，不得以“Rust crate”为理由塞回本目录。

| crate | 用途 |
|---|---|
| `chain-signing` | Rust host 端链交易签名材料真源，直接依赖 CitizenChain runtime/primitives |

仓库级共享 Rust crate：

| crate | 唯一路径 | 用途 |
|---|---|---|
| `qr-protocol` | `citizenchain/crates/protocol/` | QR_V1 协议、registry、生成器、金标夹具和跨端守卫 |
| `citizen-signer` | `shared/citizen-signer/` | CitizenApp、CitizenWallet 共用 sr25519 派生与签名源码 |

公民服务端 Cloudflare 发布使用独立 QR_V1 动作 `citizenserve_cloudflare_publish=17`。
`config_sha256` 是注入九项公开运行值后的最终 Wrangler 配置 UTF-8 字节摘要，
位于产物摘要之后并由冷钱包展示；共用的 `publish=15` 字段和编码不变。

#### 单安装包(三平台零依赖)

Tauri 打包(dmg/nsis/deb)随包"五件套",装好即用、无外部依赖:

| 件 | 位置 | 运行期 |
|----|------|--------|
| node 矿工端 | 主程序 | 桌面=节点运维台,默认全核挖矿 |
| OnChina 二进制 | `resources/onchina-bin/onchina` | 节点设置页二次确认后由 `main.rs` 中的进程管理入口拉起 |
| PostgreSQL 官方二进制 | `resources/postgres/<os>/`(bin/lib/share) | OnChina **自管**内嵌私有实例 |
| OnChina 前端产物 | `resources/onchina-frontend/dist` | OnChina 同源托管(`ONCHINA_FRONTEND_DIST`) |
| china.sqlite | `resources/china.sqlite` | 行政区只读单源(`ONCHINA_CHINA_DB`) |

打包流程:`citizenchain/scripts/prepack.{sh,ps1}` 组装(build onchina+前端、拷 china.sqlite、把官方 PG 二进制 `CITIZENCHAIN_PG_DIST` 拷进 resources)→ 在 `node/` 跑 `npm run tauri build`。
PG 官方二进制来源:https://www.postgresql.org/download/(解压后含 bin/lib/share)。

- 外部调用方 本机 macOS 入口使用仓库锁定的 Tauri CLI。`run.sh` 必须通过动态 `--config "$tauri_override"` 注入当前 `frontendDist`、节点资源、OnChina 前端和 `china.sqlite`，再按 `build --no-bundle --ci -- --locked`、`bundle --bundles app --ci` 的顺序完成编译与签名封装。

设置页的「引导节点」绑定创世引导节点的 P2P 私钥，「最终性节点」绑定 GRANDPA 投票私钥。「Runtime 版本」和下一行的「交易签名版本」分别显示本机节点同一次 `state_getRuntimeVersion` 响应中的 `specVersion` 和 `transactionVersion`。

#### 进程编排(设置页手动拉起,OnChina 自管 PG/TLS)

- 节点 `desktop` setup 不启动 OnChina；用户在设置页“链上中国平台”行点击“启动”并二次确认后，`start_onchina_platform` 调用 `crate::start_onchina(app)`。
- `main.rs` 中的进程管理入口通过环境变量把资源/数据路径告诉 OnChina(`ONCHINA_PG_BIN_DIR`/`ONCHINA_PG_DATA_DIR`/`ONCHINA_PG_PORT`/`ONCHINA_TLS_DIR`/`ONCHINA_PG_WAL_ARCHIVE_DIR`/`ONCHINA_FRONTEND_DIST`/`ONCHINA_CHINA_DB`/`ONCHAIN_WS_URL`/`ONCHAIN_LOCAL_RPC_CERTIFICATE`/`ONCHINA_EMBEDDED_PG=1`)。桌面父进程只通过后者交付本次RPC证书的公开DER十六进制，不交付私钥。
- OnChina 启动：自动选择并预留局域网监听地址 → 准备公信 HTTPS 证书 → `embedded_pg::ensure_started()`（首启 initdb、启动私有PG并建库）→ schema幂等初始化 → 链投影与模块装载 → HTTPS服务；宿主和同机模块共用证书。
- 退出:node 停子进程信号 → OnChina 收 SIGTERM/Ctrl-C → `embedded_pg::stop()` → 退出。node **不碰 PG**。
- 开发期(无随包 PG)继承 `run.sh` / `clean-run.sh` 注入的外部 PostgreSQL 二进制、数据目录、前端产物和 HTTPS 配置。

#### 内网 TLS + 扫码鉴权

- OnChina 内网 API 入口为 `https://onchina.crcfrcn.com:1988/`。节点通过宿主 `--describe` 读取域名和端口。每次启动宿主优先绑定有线网卡的私有 IPv4，没有可用有线时用 Wi-Fi，两者均不可用时停止启动。宿主通过子进程回执交付实际监听地址，浏览器继续依赖局域网 DNS 将域名解析到该地址。
- 链上中国页、健康检查和清算行查询共用 `node/desktop/src/cid.rs` 读取宿主的公开部署说明。健康检查使用本次子进程的监听地址，清算行查询在本机宿主未提供地址时按局域网 DNS 解析同一域名，正常验证公信HTTPS和域名，不设另一缺省入口或私有CA选项。办证流程、密钥存储、续期与模块协议唯一归 `CitizenChainOnChina.md` 第7节；节点编译、安装和主节点功能不依赖运行期办证授权。
- 身份认证 = 扫码签名(3b 链上 Active 管理员集合鉴权),TLS 只负责传输加密。与 node 的 libp2p WSS 证书相互独立。

#### 大市机房形态(如香港:800万公民/500万公司/百管理员)

- 机房服务器 + RAID/NAS(数十 TB:法人照片、档案材料)+ UPS。
- 数据库两选:① 内嵌私有 PG(`ONCHINA_EMBEDDED_PG=1`,OnChina 自管);② 外部托管 PG(关 `ONCHINA_EMBEDDED_PG`,直接给 `DATABASE_URL`)。

OnChina的数据库地址本机判断保持跨平台一致：TCP仅接受`localhost`（不区分大小写）、`127.0.0.1`和`::1`，多主机配置必须全部为本机；未显式声明主机保持既有默认语义。PostgreSQL的`Host::Unix`枚举分支仅在Unix宿主编译，Windows不引用不存在的枚举分支，也不新增通配放行。Unix socket仍按本机地址处理；远程数据库的既有TLS及显式授权规则不变。
- **WAL 归档**:内嵌 PG 配置 `ONCHINA_PG_WAL_ARCHIVE_DIR` 后持续归档 WAL 到指定目录；本仓不再提供全量备份与 PITR 恢复命令。外部托管 PG 的备份与恢复由数据库运维流程负责。
- 联邦节点**按省管理**:每市自治节点跑自己的 OnChina+PG;联邦注册局按省给市配管理员(链上,3a/3b)。
- **联邦注册局（FRG）每节点单省部署**：本节点所辖省由首次 active admin 的链上省专员岗位任职确定。管理员钱包从 `PublicAdmins::AdminAccounts` 读取，省域从 `PublicManage::InstitutionRoleAssignments` 的 `PROVINCE_COMMISSIONER_<省码>` 读取。
  本地省组投影表、虚拟省组 storage 和 `seed-federal-admins` CLI 均已退役；FRG 节点不要求安装前配置省名，未绑定时由冷钱包管理员登录后确认其有效任职省域。

#### 部署约束

桌面节点采用随包依赖部署；chainspec创世后冻结，协议升级通过链上set_code执行；桌面节点默认启用全核挖矿。
---

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
- 桌面节点软件由 `node/src/application.rs`、`node/<功能包>/src` 与 `node/frontend` 提供本地节点运维、设置和打包入口。

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
- Node本地缓存保存唯一账户。桌面JSON使用官方camelCase（`accountId`、`signerPublicKey`等），固定RPC/QR沿用所属合同；SS58输入在边界解析为账户，展示从账户派生。钱包、奖励账户、机构账户与节点登记人不另存或下发地址副本。
- 挖矿奖励账户私有 RPC 固定为 `reward_bindAccount` / `reward_rebindAccount`，本地非密钥配置固定为 `reward-account.json`。旧 RPC、旧 JSON 和兼容读取均已删除。

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

- 进程模型：OnChina 是公民链内置二进制能力，由节点桌面端设置页“链上中国平台”入口手动拉起为子进程、退出时一并停掉；节点启动后默认不启动 OnChina，避免只挖矿节点承担管理后台服务。OnChina 经节点 RPC 读写链，对内网托管 HTTPS API 与前端，固定入口为 `https://onchina.crcfrcn.com:1988/`。桌面 = 节点运维台，浏览器 = 机构管理员，并存不冲突。
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
  - 后端：`citizenchain/onchina` 后端完成迁移和收敛，平台层切换为内嵌 PostgreSQL + 节点 RPC + 进程内本地限流；省/市 scope 与行政区维度保留。
  - 前端：`citizenchain/onchina/platformweb` 前端完成迁移和收敛，OnChina 后端同源托管 `dist` + SPA 回退；桌面 `node/frontend` 与浏览器 `onchina/platformweb` 两套独立前端并存。

##### 5.1 分层结构
- Native Node 层：负责 CLI、网络、数据库、共识服务编排、RPC 服务、chain spec 加载。
- Runtime 层：负责所有链上状态转换、交易校验、治理规则、发行规则、手续费规则。
- Pallet 层：按治理、发行、交易、其他能力拆分功能模块。
- Desktop UI 层：由 `node/src/application.rs`、`node/<功能包>/src` 与 `node/frontend` 负责本地节点进程生命周期管理、参数设置、状态展示与安装包交付。

##### 5.2 关键共享依赖
- `runtime/primitives/`：提供链常量、机构常量、SS58 参数、发行与人口基础常量。
- `polkadot-sdk`：提供 Substrate / FRAME / client / consensus 依赖。

##### 6.1 职责
- 提供 `BuildSpec`、`ExportBlocks`、`ImportBlocks`、`PurgeChain`、`Benchmark` 等标准节点能力。
- 加载 `CitizenChain` 主网 chain spec。
- 编排 PoW 出块、GRANDPA 最终性、交易池、RPC 服务与数据库。

##### 6.2 当前 chain spec 口径
- `node/src/command.rs` 当前把省略 `--chain`、`citizenchain`、`dev`、`local`、`staging` 统一加载为同一份冻结正式 chainspec。
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
- 创世配置由genesis模块的 `runtime/genesis/src/config.rs` 唯一提供；Node传入 `runtime::RuntimeGenesisConfig` 调用公开构造入口，不复制整链配置实现。
- 公民宪法创世正文唯一文件为 `runtime/public/legislation/src/constitution.scale`；该文件是结构化 `章>节>条>款` SCALE 数据，运行态注入为 `law_id=0`。
- 单一宪法版本的目录编号规则固定为：章号全文全局唯一、节号在所属章内唯一（不同章允许复用）、条号全文全局唯一；只约束唯一性，不要求连续或从 1 开始。
- runtime 在创世构建、修宪提案和投票结果最终写入三处独立校验上述规则；原生 `ConstitutionGuard` 从经冻结状态根和哈希验证的内存创世输入取得基准，并对全部历史版本复校验，`check-constitution-genesis.py` 对 chainspec/RPC 创世状态执行同口径检查，任何一层发现重复编号都拒绝相应创世输入、交易或区块；守卫诊断不终止节点服务。
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
- `grandpakey`
- `destroy`
- `upgrade`
- `engingcore`

##### 9.2 管理员模块（`runtime/admins/`）
- 负责公权机构管理员、私权机构管理员和个人多签管理员；固定治理机构初始管理员由链配置写入，运行期治理归公权管理员模块。
- 机构管理员集合真源归 `admins`，机构管理员是可任职人员。公权、私权机构值结构统一为 `Admin { account_id, cid_number, family_name, given_name }`，非空公民 CID 必须与 `identity` 的 `AccountIdByCid` / `CidByAccountId` 双向真源一致。个人多签虽复用同一 SCALE 结构，但按个人多签规则处理字段完整性。管理员账户本身没有机构业务权限。
- ADR-039 目标授权主体为 `RoleSubject(cid_number, role_code)`。岗位、强类型 `RoleBusinessPermission` 和任职真源归 `entity`；业务动作、指定投票引擎和执行真源归业务模块。个人多签保持独立 `AuthorizationSubject::PersonalMultisig`。
- `publicadmin`、`privateadmin`、`personaladmin` 的 `AdminAccounts` 统一接受四字段 `Admin` SCALE 布局；旧纯账户、旧三字段、旧合并姓名和历史 storage migration 均已删除，不保留兼容或双轨。字段是否必须非空由机构类型、岗位和个人多签规则分别判定。

当前模块：
- `adminbase`
- `publicadmin`
- `privateadmin`
- `personaladmin`

##### 9.3 实体模块（`runtime/entity/`）
- 负责公权机构、私权机构、个人多签账户的创建、关闭、资金与生命周期治理。
- 机构管理已按公权/私权拆分两 pallet(取代旧 `organization-manage`)。
- 公权/私权 entity 已保存岗位、岗位权限、任职、`InstitutionRoleNonce` 和永久 `UsedRoleCodes`，并提供 CID 能力封顶、有效任职和岗位权限的统一查询。所有机构强制存在可空缺的 `LR`；普通机构原子创建将在独立业务模块中同时建立至少一个初始治理岗位、权限、任职和投票规则。
- 动态岗位码由 runtime 使用所属 pallet 的 `MODULE_TAG` 作哈希域生成，调用方不得提交；岗位码及权限不可修改，岗位名可以依法修改。

当前模块：
- `publicentity`（公权机构生命周期,idx30）
- `privateentity`（私权机构生命周期,idx31）
- `personalentity`（个人多签）

##### 9.4 公权业务模块（`runtime/public/`）
- 负责公权机构的业务壳。业务壳只解释业务规则和写回业务真源，不复刻投票流程。
- `legislation` 是立法业务壳；立法表决、计票和公投流程归 `legislationvote`。
- `election` 是公民选举公职人员的业务模块，当前仅占位；后续逐步实现具体选举规则，选举投票、计票和结果快照统一归 `electionvote`。

当前模块：
- `legislation`（idx25）
- `election`（idx32，当前仅占位）

index 32 由 `CitizenElection`（`runtime/public/election`）复用，用于公民选举公职人员；当前仅占位，不提供交易入口、业务存储或事件。

##### 9.5 发行模块（`runtime/issuance/`）
- 负责公民发行、全节点发行、省储行利息、决议发行完整流程。

当前模块：
- `citizenissue`
- `fullnodeissue`
- `resolutionissue`
- `prbbankissue`

##### 9.6 交易模块（`runtime/transaction/`）
- 负责链上交易手续费、链下交易手续费、机构多签交易能力。

当前模块：
- `multisig`
- `offchain`
- `onchain`

##### 9.7 其他模块（`runtime/misc/`）
- 负责链上公民身份、人口统计、PoW 难度调整等基础能力。
- `identity` 是链上投票身份、参选身份和全国、省、市、镇四级有效人口数据的唯一真源；投票引擎按提案需要消费这些人口数据并生成快照。投票身份不在链上算/存年龄，能否投票由 `citizen_status=Normal` + 护照有效期窗口判定；最低年龄门禁只在竞选身份按链上保存的出生日期 `birth_date` 实时计算复核（≥16）。

当前模块：
- `difficulty`
- `identity`

##### 10.1 定位
- `citizenchain/node` 是当前唯一桌面节点产品壳与原生节点实现目录。
- 历史 `node` 与独立 `node` 目录中的桌面职责已经收口到 `citizenchain/node`，旧目录已删除。
- 对最终用户仍然提供“安装即用”的节点软件，而不是要求用户手工管理原生 node 命令。

##### 10.2 当前职责
- `node/src/application.rs` 负责 Tauri 桌面入口与 command 注册。
- `node/<功能包>/src` 负责桌面端 Rust 后端能力，不再保留 `node/desktop/src` 目录层。
- `node/frontend/<功能名>` 负责 React 前端页面与交互。
- `citizenchain/node` 负责启动 / 停止内嵌节点进程，管理 bootnode 地址、奖励地址、GRANDPA 地址、节点名称等本地设置，并展示节点状态、链状态、网络概览、挖矿面板与其他辅助信息。
- 管理员、治理、转账、清算和奖励设置的桌面桥接使用 camelCase：账户为 `accountId` 或对应角色账户字段，签名公钥为 `signerPublicKey`。展示地址从唯一账户派生；钱包、奖励账户、机构账户和节点登记人 DTO 不保存或下发 SS58 副本。固定 QR_V1 和当前 Runtime 接口保留所属拼写。
- Node 端所有公民钱包离线扫码签名 UI 统一由 `node/frontend/desktop/signing.tsx` 中的 CitizenSignatureModal 与 CitizenSignaturePanel 承载：左侧固定“扫码签名”，右侧固定“识别签名”，面板只显示二维码有效期倒计时，不显示内部 request id 或签名账户地址；业务页面只负责构造请求、验签和提交交易。地址扫码填入等非签名二维码不纳入该组件。
- 设置页的“全节点模式”仅归档全节点可选。桌面与CLI使用同一参数合同，实际要求ArchiveAll/KeepAll；普通裁剪模式不可选择。模式选择不等于已经恢复旧库缺失的历史状态，数据库模式不兼容准确报告且不自动修改链库。
- 设置页在“全节点模式”之后提供“链上中国平台”手动启动行，显示 `未开启` / `启动中` / `已开启` 状态标签、固定入口文本 `https://onchina.crcfrcn.com:1988` 和“启动 / 关闭”按钮；点击后必须二次确认，只启动或停止 OnChina 子进程，不自动打开浏览器；只有 `/api/health` 真实健康检查通过后才显示 `已开启`。 状态标记紧邻平台名称，使用专属状态属性隔离通用错误块样式，以11px紧凑胶囊和名称垂直居中；入口文本仅省略宿主根URL末尾斜杠，显示为`https://onchina.crcfrcn.com:1988`，不改变实际请求地址。

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
- `node/src/chain.rs` 变更通常不是“现有链 runtime 升级”，而是 chain spec / bootnodes / properties / 启动配置变更。
- `runtime/genesis/src/config.rs` 变更若影响创世状态，通常对应新链或重建链，不等于自动给已运行链打补丁。

##### 12.1 治理
- `CitizenChainRuntime.md`（runtime/governance/grandpakey 对应模块章节）
- `CitizenChainRuntime.md`（runtime/governance/destroy 对应模块章节）
- `CitizenChainRuntime.md`（runtime/governance/upgrade 对应模块章节）
- `CitizenChainRuntime.md`（runtime/votingengine 对应模块章节）

##### 12.1.1 管理员
- `CitizenChainRuntime.md`（runtime/admins 对应模块章节）

##### 12.1.2 实体（机构/个人生命周期）
- `CitizenChainRuntime.md`（runtime/entity/publicentity 对应模块章节）
- `CitizenChainRuntime.md`（runtime/entity/privateentity 对应模块章节）
- `CitizenChainRuntime.md`（runtime/entity/personalentity 对应模块章节）

##### 12.2 发行
- `CitizenChainRuntime.md`（runtime/issuance/citizenissue 对应模块章节）
- `CitizenChainRuntime.md`（runtime/issuance/fullnodeissue 对应模块章节）
- `CitizenChainRuntime.md`（runtime/issuance/resolutionissue 对应模块章节）
- `CitizenChainRuntime.md`（runtime/issuance/prbbankissue 对应模块章节）

##### 12.3 交易
- `CitizenChainRuntime.md`（runtime/transaction/multisig 对应模块章节）
- `CitizenChainRuntime.md`（机构账户资金操作接口位于runtime/primitives/src/institution_asset.rs）
- `runtime/transaction/offchain/STEP1_TECHNICAL.md`
- `runtime/transaction/offchain/STEP2A_RUNTIME.md`
- `CitizenChainRuntime.md`（runtime/transaction/onchain 对应模块章节）

##### 12.4 其他链上模块
- `CitizenChainRuntime.md`（runtime/misc/difficulty 对应模块章节）
- `CitizenChainRuntime.md`（runtime/misc/identity 对应模块章节）

##### 12.5 桌面节点 UI
- `CitizenChainNode.md` 是节点桌面、普通转账与设置的唯一技术文档入口。

储备机构详情页的普通转账、两和基金转账、安全基金转账和手续费划转共用居中弹窗。点击遮罩保留表单，左下角「取消」回到原机构详情，右下角「签名」校验通过后在同一弹窗进入公民钱包扫码流程。提交成功后刷新原机构的提案列表。

普通转账固定从本机构主账户转出。国家储委会的两和基金和安全基金入口分别绑定各自的协议账户。手续费划转只读显示本机构费用账户和主账户，只需填写金额。界面提示按现行 Runtime 规则显示 1.11 元最低保留额和 80% 的单次本金上限。省储委会入口需要已激活的委员具备链上划转发起权，存量权限随 Runtime 升级生效。

##### 12.6 公民链客户端 SDK

- `../citizensdk/CitizenSDK.md`（独立产品边界、源码来源、钱包、轻节点、交易、

#### 14.1 外部调用准入与费率路由：两处都不设通配分支

`runtime/src/configs.rs` 的两个 `RuntimeCall` 匹配统一采用**逐 pallet 显式归类、无 `_` 兜底**：

- `RuntimeFeeRouter::fee_route` — 历来如此，默认 `FeeRoute::Reject`。
- RuntimeCallFilter::contains 显式列出全部28个RuntimeCall变体：拒绝Balances、Assets、OnchainIssuance和OffchainTransaction，放行其余24个变体。

这样新增 pallet 会触发编译期 non-exhaustive 错误，强制作者显式决定放行还是拒绝。

> 注：只有带 `#[pallet::call]` 的 pallet 才会出现在 `RuntimeCall` 中。`TransactionPayment`、
> `ProvincialBankInterest`、`PowDifficulty`、`GenesisPallet`、`PublicAdmins`、`PrivateAdmins`
> 六个 pallet 不暴露 extrinsic，因此 34 个 pallet 对应 28 个 `RuntimeCall` 变体。

#### 冻结链身份

GENESIS_HASH为`0x18847a5dfd263272f2e7727836fe6582f8c4463ff48609df7b96d5e4d9dd24dd`，GENESIS_STATE_ROOT为`0x1f74a2ca094fc3ebb2143f504d807a6b4f4f9b0a3d13ac808ae84efc7cb12111`。节点、轻节点和链上身份校验使用同一冻结锚点；Runtime升级通过正式链交易执行，不重新生成创世数据。

### 公民身份全链路:建档 → 上链 → 人口统计 → 投票引擎消费

- 事实源:本文是流程导读;字段与编码以代码为准
  - `citizenchain/onchina/registry/`(建档 + 上链准备)
  - `citizenchain/runtime/misc/identity/src/lib.rs`(链上身份 + 人口计数)
  - `citizenchain/runtime/votingengine/legislationvote/src/lib.rs`(快照消费)
  - `CitizenChainOnChina.md`(扫码动作登记)

#### 1. 公民创建(本地建档,不碰链、不碰钱包)

- 注册局管理员登录 OnChina 塔塔控制台(链上 Active 管理员集合鉴权)创建公民档案,
  落节点本地 PG `citizens` 表:CID编号、护照号、`family_name`、`given_name`、性别、出生日期、
  居住地/出生地省市镇码、护照有效期、`citizen_status`、`voting_eligible`。
- 建档不要求钱包:未成年人、无钱包公民都可以先持有本地电子护照档案。
- 公民在 CitizenApp「我的 → 电子护照」选择一个热钱包作为投票账户,
  页面展示钱包地址二维码,供办理现场的操作员扫入。

#### 2. 公民上链(录入钱包 + 双签名,`onchina/registry/citizens/identity.rs`)

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

公民投票主体：资格接口返回 `CitizenSubject { cid_number, account_id }`；
联合公投、立法公投和选举普选均按永久 CID 去重并在票据值中保存完整主体。候选快照和
当选结果保存完整主体，候选人计票以候选 CID 为唯一键；账户只证明当前签名授权，不得恢复
裸账户身份主键。

#### 3. 公民人口数据(identity pallet)

- 每次注册/更新/撤销身份时同步维护四级有效选民计数器:
  `CountryVotingCount` / `ProvinceVotingCount(省)` /
  `CityVotingCount(省,市)` / `TownVotingCount(省,市,镇)`,
  最终只统计状态正常且在当前人口就绪日期护照有效的身份。
- 护照未来生效、到期、身份吊销和迁居必须由 identity 维护有界日期变化计划；
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

#### 4. 投票引擎消费(legislationvote 特别案公投)

- **建案事务内快照**:特别案创建时由 `legislationvote` 先从发起机构
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
  状态正常 + 快照日期护照有效 + 行政区匹配”。四级计数按该口径维护，
  过期或尚未生效的护照不再进入分母。
- 客户端对齐:CitizenApp、CitizenWallet 和 OnChina 不构造、签名或解码独立快照
  交易；扫码只确认业务提案，作用域和快照均由链端按 actor CID 内联确定。

#### 签名域三端一致性纪律

`blake2_256(GMB || 0x10 || payload)` 的构造分别在:

| 端 | 位置 |
|---|---|
| runtime 真源 | `primitives::sign::signing_message` |
| OnChina 验签 | `onchina/registry/citizens/identity.rs` |
| CitizenWallet 签名 | `lib/signer/qr_signer.dart::signingBytesFor` |
| CitizenApp 签名 | `lib/signer/signing.dart::signingMessage`(经 `qr_signer.dart::signingBytesForHex`) |

#### 公民授权主体与快照

当前、竞选和历史资格接口返回完整CitizenSubject { cid_number, account_id }。联合公投与立法公投按永久CID去重并保存完整票据；人口快照冻结作用域、有效总数、资格revision、判定日期和创建区块。

ElectionMeta使用actor_cid_number与role_code定位拟任职机构及岗位；候选快照、普选票据、候选计票和当选结果保存完整CitizenSubject。互选按机构CID、岗位码和账户票据去重。

CitizenApp与边缘服务在同一个finalized区块校验CID Active、CID与账户双向绑定及CID主键身份；缺失、截断、尾随或错配均拒绝。VotingIdentityByCid保存护照有效期、状态、居住省市镇码和更新时间，CID只位于存储键。CandidateIdentityByCid另保存出生地域、姓、名、性别及出生日期。

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
  管理的 `CHAIN_URL / CFACCESS_CLIENTID / CHAIN_SECRET` 单一生产配置；缺失任一项时关闭链读取。
  当前代码不接受公网 HTTP、回环 HTTP、非 `chain.crcfrcn.com` 上游或无 Access 凭据的请求。
- 后续容灾：最多选择少量不同地区的权威引导节点作为 Worker 私有 RPC 备用，每个节点使用独立 Tunnel 和凭证，不连接全部 44 个节点。

CitizenApp 不直接依赖国储会节点 RPC；App 的链上真源是内置轻节点验证的 finalized 链状态。

相关实现参考：
- [`CitizenChainNode.md`](<本仓根>/CitizenChainNode.md)
- [`node/src/command.rs`](<本仓根>/node/src/command.rs)
- [`node/src/service.rs`](<本仓根>/node/src/service.rs)
- [`node/src/chain.rs`](<本仓根>/node/src/chain.rs)

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

不得为 `9944`、`9615`、`1988`、`5433` 创建公网入站规则。没有运维需求时也不得为 `22` 创建公网规则。

##### 10.3 RPC 与 Prometheus 安全边界
- 默认 RPC：`127.0.0.1:9944`
- Prometheus：权威引导节点默认使用 `--no-prometheus` 关闭
- OnChina：节点启动时默认不启动；启用时也只能绑定本机或受控私网

禁止直接暴露以下端口到公网：
- `9944`
- `9615`
- `1988`
- `5433`

##### 10.4 权威引导节点安全边界

国储会和后续 43 个权威引导节点使用相同边界：

- `30333/tcp`：CitizenChain P2P 通信端口。
- `22/tcp`：默认关闭；确需运维时只通过 Oracle Bastion、Cloudflare Access 或固定来源临时放行。

权威节点的 `9944` 必须保持回环监听。后续 Worker 访问国储会本机 RPC 时使用服务器主动出站的 Cloudflare Tunnel，仍不开放任何 RPC 入站端口。

##### 10.5 国储会私有 RPC Tunnel

国储会节点是 44 个权威 bootnode 中的第 1 个，也是首期唯一 Worker 链上游，不另建 Cloudflare 节点或独立 RPC 节点。Cloudflare 只运行边缘 Worker、Access 和 Tunnel 控制面；安装在国储会服务器上的 `cloudflared` connector 主动建立出站连接，并把受控请求转发到同机 `127.0.0.1:9944`。

- 远程管理Tunnel使用chain.crcfrcn.com的Access保护入口；节点RPC继续限制在回环地址。
- Access采用自托管应用和Service Auth策略。Worker的CHAIN_URL、CFACCESS_CLIENTID与CHAIN_SECRET由CitizenServe声明并保管，链仓不重复登记。

本仓 `scripts/config.mjs` 只声明软件产品需求：`rtupg:NRC_PUBLICKEY` 是国储会现任管理员公钥，作为开发升级和节点实例部署批准的同一受保护信任锚，只存控制台 Touch ID Keychain；创世块哈希仍为公开配置。权威与权益节点是使用本产品的部署实例，不作为产品配置需求；控制台节点管理从本仓既有机构目录读取节点身份，按准确节点ID管理服务器地址、节点身份、验证和SSH运维输入，值由控制台安全层保管与授权。本仓Build不声明节点配置字段。

服务器部署顺序：

1. 先在服务器本机确认 CitizenChain 正常运行，`9944` 只监听 `127.0.0.1`，`30333` 监听 `0.0.0.0`。
2. 使用 Cloudflare 塔塔控制台为既有 `nrcgch-rpc` Tunnel 添加 Linux connector；Tunnel token 只在服务器 root 会话中使用，不写入仓库或普通用户配置。
3. 把 `cloudflared` 安装为 systemd 服务并启动，确认服务开机自启、connector 状态为 Healthy。
4. 不携带 Access 服务令牌访问链保护路径必须被拒绝；携带 Worker 专用服务令牌后必须到达固定方法网关。
5. 从公网确认 `30333/TCP` 可连接且 `9944/TCP` 不可连接，再通过 staging Worker 验证固定链读取方法；全部通过前保持 `CHAIN_EXTRINSIC_RELAY_ENABLED=0`。

`cloudflared` 不是区块链节点，不参与共识、P2P、Runtime 或 RocksDB，也不要求 Oracle 开放任何 Cloudflare 入站端口。首期只连接国储会这一台上游；后续容灾节点必须使用独立 Tunnel、独立 Access 凭证和显式故障切换策略，不连接全部 44 个 bootnode。

##### 11.1 创建服务文件

本仓 `node/citizenchain.service` 是权威节点服务的唯一模板，服务器固定安装为 `/etc/systemd/system/citizenchain-node.service`。本节不保留第二份内联模板。

```bash
sudo install -o root -g root -m 0644 node/citizenchain.service /etc/systemd/system/citizenchain-node.service
```

服务以 `citizenchain` 运行，使用 `NoNewPrivileges`、空 `CapabilityBoundingSet`/`AmbientCapabilities`、`PrivateDevices`、`ProtectSystem=strict` 和 `UMask=0077`；只允许写既有 `/opt/citizenchain/data`。维护不修改该目录的数据、权限、节点身份或验证密钥。WASM/JIT仍需动态执行内存，不启用 `MemoryDenyWriteExecute`。

节点通过 `CITIZENCHAIN_RPC_TLS_CERTIFICATE` 和 `CITIZENCHAIN_RPC_TLS_PRIVATE_KEY` 读取TLS材料，服务账户必须具有读取权限。本公开程序及服务模板不定义某一组服务器的SSH、防火墙、审计规则或私钥运维归属。

##### 11.2 重新加载并启动服务

```bash
sudo systemctl daemon-reload
sudo systemctl enable citizenchain-node.service
sudo systemctl start citizenchain-node.service
```

##### 11.3 查看服务状态

```bash
sudo systemctl status citizenchain-node.service
```

##### 11.4 查看实时日志

```bash
journalctl -u citizenchain-node.service -f
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
- 从外部网络连接 `9944`、`9615`、`1988`、`5433` 和未启用运维时的 `22` 必须失败。

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
sudo systemctl status citizenchain-node.service
journalctl -u citizenchain-node.service -n 100 --no-pager
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
sudo install -m 0644 node/citizenchain.service /etc/systemd/system/citizenchain-node.service
sudo systemctl daemon-reload
sudo systemctl enable --now citizenchain-node.service
```

若 `citizenchain` 系统账户已经存在，跳过 `useradd`，不得删除或重建现有账户。

### OnChina 公民/私权/公权机构 链↔注册局本地库一致性 技术方案 (v2)

实现落点:
- 创世回填(§2.5):`domains/genesis_projection.rs`(程伟联邦播种 + 基金会绥阳回填,DO NOTHING 幂等)。
- 投影纯逻辑 + 保正本:`domains/projection.rs::merge_citizen_record / merge_institution_record`。
- M1 indexer 增量:`onchina/platform/chain/runtime.rs::read_chain_citizen_detail` + `indexer/event_parser.rs::collect_entity_projection_cids` + `indexer/worker.rs`(每块投影 + 作用域解析,仅城市节点)。
- M3 联邦 drill-in:`chain_runtime` 两个 scoped 扫描 + `projection::drill_in_project_scope` + `projection::drill_in_project_city`(POST `/api/admin/registry/drill-in-project`,联邦+省级访问控制)。

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

#### 2.5 创世实体投影

创世时直接上链、未走注册局流程的仅两个,按各自 CID 特性分开落地:

- **基金会 `GZ018-SFGYR`(私权,CID 带市码 018=绥阳)** → 落 **绥阳市注册局(城市节点)**。绥阳节点(scope=GZ/绥阳)启动时 `institution_lookup(基金会CID)` → `INSERT subjects(PRIVATE) DO NOTHING`。绥阳看基金会时"法定代表人"字段自带程伟姓名(链上 `legal_representative`)。联邦贵州组管理员进绥阳市时经 M3 也能看到基金会。
- **程伟 `GZ000-CTZN6`(公民,CID 无市码,无 identity,无居住地)** → **直接落联邦注册局本地库**,置于 **province=GZ(贵州)、city=绥阳市码(018,与基金会同市)**。联邦贵州组管理员**进绥阳市→显示程伟**,进贵州其他市→不显示。**不加省级/创世特殊视图**——他就是联邦库里一条普通"省+市"公民记录,现有列表/搜索(`GZ000-CTZN6`)即可见。绥阳城市节点本身不持有程伟(他是联邦的)。姓名/CID/账户取自 `primitives` 创世常量 + 链上 `legal_representative` 权威值;`INSERT citizens + subjects(CITIZEN) DO NOTHING`。
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
- **触发点**:M3 私权投影已接进读取路径
  `onchina/registry/institutions/registration.rs::list_institutions_inner`——联邦(Tier1)管理员按市查私权列表时,
  先 `projection::drill_in_project_private_scope((省,市))` 再查库。链读失败只 warn 不阻断(fail-open)。

> 稳态:各市靠 M1;创世靠 M2;联邦看外市靠 M3。全状态扫描仅极端兜底。

#### 4.5 正本补档：公民编辑与不可变字段

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

| 列 | 正常公民(有 identity) | 待补/联邦办公民 |
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
- 程伟(无 identity,仅基金会法定代表人)→ PENDING 待补档,姓名从 `legal_representative` 取。

#### 6. 字段映射 · 私权机构(subjects kind=PRIVATE + 明细)

- subjects:cid、kind='PRIVATE'、全称/简称、省/市/镇码(**CID 直接给省市**)、机构码、法人标记、`legal_representative_*`、created_at。
- 账户:`InstitutionAccounts` → 私权账户明细。
- 链下(证件照等)由**市补档**,投影不写。

#### 7. 链状态读取接口

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
- 桌面端 Rust 后端模块放在 `node/<功能包>/src`，不再保留 `node/desktop/src` 目录层
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

---

### chainspec 与创世状态冻结规则（铁律）

> 适用范围：全节点 `citizenchain.json`、正式创世审计 `genesis-state/`、CitizenApp
> `assets/chainspec.json`、`assets/light_sync_state.json`，以及任何分发给轻节点 / 钱包 / App 的创世锚点文件。

#### 结论

正式创世冻结plain chainspec、创世状态包及轻节点锚点，各分发端使用同一创世哈希和状态根。

1. **全节点plain chainspec**：`citizenchain/node/citizenchain.json`，保存Runtime WASM、genesis patch、bootNodes、properties和protocolId。
2. **创世状态包**：`target/chainspec/genesis-state/manifest.json`及所属链数据库，记录块0、构建来源与交叉哈希。安装包不携带RocksDB。
3. **轻节点锚点**：`assets/chainspec.json`和`assets/light_sync_state.json`与全节点使用同一冻结创世身份。

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
- **公民建档时序**:

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

- **机构册只读投影**:公权机构唯一真源是链上
  `PublicManage::Institutions` / `InstitutionAccounts`。OnChina 不再按 primitives 或
  `china.sqlite × 模板` 本地物化公权机构,只能从链上读取并写入 PostgreSQL 查询缓存;
  链不可达、创世哈希不匹配或链上目录不可读时 fail-closed。运行期新增机构走占号先行路径落库;
  旧 `sfid` 库删除,全仓零 `sfid_number` 残留。

#### 七、影响

- 公民建档依赖链活性(fail-closed),每单一次用户账户控制签名和一次管理员冷签；链上不保存姓名生日等档案全文。
- CitizenApp/CitizenWallet:公民确认签名(ACTION_CITIZEN_IDENTITY)与扩展尾规则不变;链交易提交动作从钱包端移到 onchina 后端。
- 节点首启复制官方创世状态包并等待 RPC ready;GenesisBuilder 本地物化仅作开发/排障兜底。

#### 九、风险与防护

- **模板/行政区常量漂移**:导出工具幂等 + 构建期逐号 parse 断言。
- **首启构建性能**:正式安装包内置官方创世状态包,用户首启先复制链数据库;GenesisBuilder 本地物化落库仅作为开发/排障兜底,不得作为正式用户默认路径。当前 plain spec 仍会被 Substrate 用于创世块校验,可能产生分钟级 CPU 窗口,以 `chain_getBlockHash(0)` 作为唯一可用标准。
- **占号规模**:公民占号随建档线性增长,`CidRegistry` 条目 ~100B/人,亿级人口 ≈ 数十 GB 级远期 state——链上极简字段已是下限,属注册局链上化的固有账,创世期无感。

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
Envelope、KeyPackage 与附件 + 近场通信。
设备直连只使用公共 STUN 发现候选地址，不经过 TURN、HTTP、R2 或其它云端中继。

本节的 Chat 边界由 ADR-043 进一步固化；与 ADR-043 冲突的历史云端转发口径均以 ADR-043 为准。

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

---

#### 标题

VotingEngine 使用 Track handler、异步业务执行和公平维护预算。

#### 决策

- 核心引擎通过 runtime 注册的递归 Track tuple 派发 timeout、mode 数据清理和 mode 终态副作用，不再匹配每个具体 stage。
- 投票判定只写状态并登记业务执行队列；业务执行、自动终结和清理分别使用条数与 weight 双重预算。
- 清理使用两级 FIFO：固定保留期的延迟 FIFO 到期后进入就绪 FIFO；每次只删除一个有界 chunk，未完成项重新入队，避免大型公投阻塞其他提案。
- `identity` 只提供四级 `PopulationData` 和历史资格判断，是人口数据唯一真源；投票引擎在建案事务中读取该数据并生成自己的提案人口快照。联合公投、立法公投和 Popular 选举只消费投票引擎快照。
- Mutual 互选由业务模块指定目标 `RoleSubject`，投票引擎读取 entity 的岗位有效任职并冻结 `VoterSnapshot`；Track 不复制资格真源。提案创建后的身份或任职变化不得改变既有提案。
- 自动业务执行的回调错误、Ignored、结果应用错误和 Track 后处理错误统一递增 attempts、指数退避并在上限后 dead-letter；终态副作用使用独立队列，禁止重新执行业务动作。
- 宪法阈值继续引用 `primitives::constitution`，Track 只表达流程和派发，不复制宪法数学。
- 具体公权选举业务模块是对应选举规则真源，按业务种类分别放在 `runtime/public/`；无具体规则的通用选举业务壳已决定删除。`electionvote` 继续只负责投票、计票和结果快照。
- 公民投票与竞选主体统一为公民 CID + 签名钱包。`identity` 提供完整主体和规范化人口数据，投票引擎不得继续用裸钱包表达公民票据或候选人。
- 生产权重必须由 FRAME benchmark 生成；upgrade 业务执行仍显式叠加 `SystemWeightInfo::set_code()` 的最重成本。

当前 Runtime 将自动终结、异步执行、历史清理的独立预算分别配置为最大区块权重的 `1/4`、`1/4`、`1/8`。总占比为 62.5%，每条管线在 60 秒最大计算区块内均能容纳至少一个最重任务，因此保持该预算。

#### 影响

- 新增普通投票模式时，只需在 VotingEngine 边界增加 Track 实现并在 runtime 注册，不需要修改核心 timeout/cleanup 分支，也不需要创建新的 pallet。
- 特殊投票仍可由现有 sub-pallet 承载自己的快照、票据和计票，但必须实现统一 Track handler。
- Popular 选举不把完整选区人口塞入 `BoundedVec`；Mutual 互选只冻结 VotePlan 指定岗位的有效任职账户，同一钱包跨岗位形成独立岗位票据，不使用机构全体 admins。
- 清理吞吐按实际 weight 控制，proposal 之间公平推进；任何单项错误或大规模票据都不能饿死其他维护管线。
- 正式冻结后的Runtime升级通过链上setCode执行。

### ADR-037 公民币原生平台订阅与创作者订阅

#### 1. 决策

平台订阅与创作者订阅统一使用 CitizenChain 原生公民币付款，订阅能力并入现有 `SquarePost` pallet。

订阅周期统一由真实公历决定。链上已确认的 `Timestamp.Now` 是 runtime 判断到期和执行扣款的
时间依据；runtime 以确定性的 UTC 整数公历算法计算月、季、年，订阅期限与区块高度、
固定天数和设备时间无关。

#### 2. 平台与创作者付款

- 平台三档为 Freedom、Democracy、Spark，价格真源为链上 `PlatformPrice`。
- 平台收款账户由创世常量公民链基金会 CID 派生；缺失时 fail-closed。
- 创作者必须拥有当前有效的平台订阅。
- 创作者链上套餐只保存付款必需的 `tier_id`、`billing_period`、`price_fen`。
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
- finalized 镜像证明：交易哈希、区块哈希和完整已签名 extrinsic；Worker 必须复核签名钱包、调用参数、finalized 主链包含关系与同一区块 storage。
- 平台调价：统一投票引擎。
- 平台调价入口与提交：OnChina 读取准确机构 CID 和 finalized 链上真源，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫后通过唯一链提交入口广播。

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

#### 9. 后果

- 订阅期限完全脱离区块高度，页面可显示真实日期与时间。
- App 离线不影响续费；停链期间到期的周期在恢复出块后补扣。
- runtime 承担有界调度和公历计算成本，Cloudflare 不承担扣款或日期计算。
- 首次订阅只有一笔签名交易；续费没有用户交易，也没有周期确认交易。
- 所有端必须同步新 SCALE、call、storage 和状态，不允许单端兼容旧协议。

---

### ADR-039：机构 CID 与岗位码权限模型

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
- 创世机构岗位与权限的业务规则由 runtime 实施；NodeGuard 不冻结岗位或权限目录。
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

- admins 人员名册：`runtime/admins`。公权、私权机构统一使用 `Admin { account_id, cid_number, family_name, given_name }`，非空公民 CID 只能引用 `identity` 的 `AccountIdByCid` / `CidByAccountId` 双向绑定。个人多签复用同一 SCALE 结构，但不属于机构管理员并按个人多签规则处理字段完整性。
- 岗位、岗位权限、岗位码 nonce/占用和任职：`runtime/entity`。
- CID 顶层能力、创世固定岗位与权限：runtime 共享常量与创世规范。
- 业务动作权限要求、指定投票引擎、VotePlan 和通过后执行：对应业务模块。
- 机构治理阈值：`runtime/entity/publicentity` / `privateentity`；提案阈值快照、资格快照、票据、计票和终态：`runtime/votingengine`。
- 固定岗位与权限的业务约束由 runtime 负责；NodeGuard 只保留机构组成及固定治理阈值检查。

#### 后果

- 现有“admins 即授权”和“机构投票快照全体 admins”的实现必须删除，不能保留兼容分支。
- 所有受影响 SCALE、storage、QR 和客户端解码必须在同一步骤跨端同步。
- 普通机构创建载荷会发生 breaking change，开发链重新创世，不做历史迁移。
- 每个业务模块都必须显式登记业务动作、可发起/投票岗位主体和唯一投票引擎；未登记则 fail-closed。

### ADR-040：全仓账户标识采用 Substrate 官方模型

#### 背景

仓库历史上曾把同一个由助记词派生、用于签名和授权的链账户写成多组废弃旧名；`wallet_account`、`admin_account`、`owner_account`、`wallet_pubkey`、`admin_pubkey`、`wallet_address` 均禁止恢复。它们混合了链账户、钱包软件、人员角色、公钥和展示地址，导致跨 Rust、Dart、TypeScript、SQL、JSON、QR 和文档的字段不一致，也容易把 SS58 展示字符串误当授权主键。

Polkadot SDK 的运行时身份模型以 `AccountId` 表示账户，以公钥完成签名验证，以 SS58 表示人类可读地址。ADR-022 已固定 `AccountId` 是账户身份锚点、签名算法只是授权方式；本 ADR 在该密码学边界之上统一全仓命名和文本编码。

##### 1. 账户、公钥和地址严格分层

- runtime 账户类型使用 `AccountId`；当前具体值仍是 32 字节账户标识。
- 单一账户字段统一为 `account_id`。
- 一个结构同时出现多个具有独立业务角色的账户时，统一使用 `<role>_account_id`，例如 `actor_account_id`、`voter_account_id`、`creator_account_id`、`sender_account_id`、`recipient_account_id`、`subscriber_account_id`、`registrar_account_id`、`beneficiary_account_id`、`representative_account_id`。
- 签名公钥统一为 `public_key`；当前签名者公钥为 `signer_public_key`；凭证签名者公钥为 `credential_signer_public_key`。
- SS58只用于输入和派生展示，Rust以 `ss58_address` 表达，桌面自有JSON与TypeScript以 `ss58Address` 表达；固定接口保留所属格式。授权、关系和缓存都以唯一账户标识为准。
- `wallet` 是链外软件，不得再用于 runtime 账户字段；`admin`、`owner` 是人员或业务角色，不得在只有一个账户的结构中替代账户本体命名。

##### 2. 文本编码唯一

- `account_id` 和 32 字节公钥的跨端文本表示固定为小写 `0x` 加 64 位十六进制。
- 唯一校验式为 `^0x[0-9a-f]{64}$`。
- 进入系统边界时必须一次规范化并校验；内部不得同时保存无 `0x`、大写、混合大小写或 SS58 形式的同一账户。
- `ss58_address` 必须从账户字节按指定网络格式派生；不得反向成为账户权限或数据库身份真源。

##### 3. 授权与验签

- 签名流程固定为：校验 `signer_public_key` 和签名，按运行时账户识别规则得到 `signer_account_id`，再与业务要求的 `account_id` 或 `<role>_account_id` 比较。
- 机构岗位权限继续由 `cid_number + role_code + account_id` 三者共同成立；管理员账户本身不获得业务权限。
- 公民身份继续由 `cid_number + account_id` 共同校验；identity 是 CID 与账户一一对应的唯一真源。
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

- 不保留旧字段、旧 JSON、旧 SQL 列、双写、双读、fallback 或任何过渡兼容。
- 助记词、seed、私钥、macOS Keychain、iOS Keychain、Android Keystore 和 GitHub Secrets 不属于待删除业务数据，必须保留；同一安全材料按未改变的派生规则得到同一账户。
- CID 生成、签名 payload、SCALE 字段顺序或哈希材料不得因纯命名重构而改变字节。

#### 影响

- 角色字段不会被无差别压平。`source`、`dest` 等框架语义，以及确有多个账户时的 `<role>_account_id`，继续保留其业务区分。
- ADR-022 的账户派生、AccountId 锚点和未来 PQC 授权路线不变；ADR-023、ADR-039 中的旧账户字段名由本 ADR 的目标命名取代，管理员名册、岗位权限和投票边界不变。

#### 跨端签名文本编码

账户与MLS公钥统一为小写0x加64位十六进制，签名为小写0x加128位十六进制；登记动作13和签名域0x1C遵守共享mls_device_bind合同。Node不维护另一套设备认证或私有数据密钥。

### ADR-042：CitizenApp 按业务域隔离本地存储与操作队列

#### 背景

改造前，CitizenApp 把钱包、交易、治理、广场和 Chat 集合放在同一个共享业务数据库中，
并由一条进程内操作队列串行执行全部业务读写。
这个设计把互不相干的业务域放进同一个故障域：任一业务回调永久等待，后续钱包及继续
依赖该库的页面读写都会排在同一条未完成队列后面。

当前 Chat 不得在数据库回调中再次读取同域数据库，也不得在回调中等待钱包或 MLS 能力。
同域再入必须立即失败；跨域公开绑定和协议操作必须在当前数据库回调返回后执行。

同一耦合还使钱包列表读取中的未知状态或读取失败容易被默认空列表掩盖，右上角“＋”据此只
展示“导入冷钱包”，把“尚未知道钱包状态”错误表达成“已确认没有钱包”。

生产正常启动、
数据库打开和日常读写不探测、不读取、不复制、不改写、不删除其它数据格式，也不保留
双读、双写或兼容重载。

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
MLS 协议状态由 公民App的chat包 管理，钱包签名只授权公开账户绑定，不能替代丢失的 MLS 状态。
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
- 关闭/删除后不得由后台写入重新打开数据库。旧任务的 `finally` 不得污染新实例状态。
- App 级擦除按域执行 AppIsar、WalletIsar、ChatIsar、SocialIsar、UserIsar、Chat 文件域、Social 草稿文件域、
  Secure Storage 和 SharedPreferences，单项失败不得阻止其它项，全部尝试后聚合报告。
  Chat/Social 文件域分别只能定位 Documents 下精确的 `chat/` 与 `square_drafts/` 子树，
  不得跟随符号链接或扩大目标。
- 本节只约束用户明确触发的既有 AppLock 安全擦除流程。它不是 schema 打开、
  普通启动或日常读取的一部分，ADR-042 也不会调用它。

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
- 每个本地数据库都要独立打开、关闭和数据清除收口；App 级隐私清除必须分别
  调用 AppIsar、WalletIsar、ChatIsar、SocialIsar 与 UserIsar 的终态接口，单库失败不得触发另一库破坏性重置。
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
   SDP/ICE/连接就绪信令的短暂内存转发。
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

#### CitizenApp交互边界

CitizenApp是交互入口，不承担链、身份或权限信任根职责。真实空数据保持空态。底部导航为“广场 / 公民 / 聊天 / 交易 / 我的”。交易页链状态由轻节点快照及读取错误驱动，流水区分“待确认 / 已确认 / 失败”，仅finalized成功后显示已确认。

## 目录整合与平台输入

Runtime按所属技术文档的包/src/tests结构组织；上游/派生源码保留原结构。自有 `crates/signing/src/lib.rs` 由 Cargo `[lib] path` 显式登记；`blockchain-harness/src/harness.rs`、`qr-protocol/src/export_registry.rs` 由原名称的 `[[bin]]` 登记。Node 二维码生成目标为 `node/frontend/client/qrBodies.g.ts`。

### 完整链根与白皮书来源
节点、Runtime、OnChina和scanner保持同一完整CitizenChain仓。白皮书唯一正文真源为根目录`CitizenChainWhitepaper.md`，两张配图为`icons/whitepaper-node-diagram.svg`和`icons/whitepaper-reserve-architecture.svg`。节点在编译时内置正文并将原始SVG编码为data URL，白皮书tab继续使用既有`LocalDocViewer`、目录和交互，离线可读。官网本机构建读取相邻公民链仓库；GitHub构建检出公民链main，以`CITIZENCHAIN_WHITEPAPER_ROOT`指定输入，并仅在当前构建的target内快照三件源文件。官网不再保存独立正文或配图副本，页面及静态`/whitepaper/content.json`均从同一真源生成。

公民链版本验真产品参数统一citizenchain；节点平台读取node/tauri.conf.json软件版本，wasm平台使用spec-version；WASM回执展示标题使用完整公民链，平台版本与验真边界不变。

## 固定产物产品目录

桌面节点客户端的本机RPC地址来自本次节点会话，不要求用户设置CITIZENCHAIN_RPC_URL；远端查询仍只接受可信HTTPS且禁止重定向。链上中国健康检查访问本次子进程回传的监听地址，正常验证公信链与onchina.crcfrcn.com域名；它向桌面链RPC发起的HTTPS/WSS连接仍单独验证父进程交付的本次公开证书及回环IP，不与浏览器HTTPS证书混用。局域网浏览器入口为1988端口。烘焙/审计/历史导出脚本保留功能但要求显式可信HTTPS，禁止重定向。

公开仓门禁保留 Apache、MIT、Unlicense 三种准确官方行注释引用；字符串、额外路径及其它网址继续拒绝。Runtime 未修改。

白皮书正文与配图由公民链根目录真源编译内置，官网构建共用该真源。

首推还逐字保留十三份原 Substrate benchmark 53.0.0 生成 weights 的编译器声明；固定完整摘要不同、未知文件及新增抑制仍执行原中文理由要求。生成权重和 Runtime 源码未写入。

账户金标按真实 kind 选择完整派生输入：机构普通账户使用 cid，机构命名账户增加账户名，个人多签使用创建者账户及账户名；缺项、未知类型、重复语义键和 account_id 漂移继续拒绝。

GitHub 推送门禁从 `rustup which --toolchain 1.97.1 cargo` 获取登记工具链的 Cargo 普通原件路径；不把 PATH 中指向 rustup 的 shim 交给原件校验。解析失败直接停止。

## 产品介绍与开源许可

根目录 `README.md` 仅提供本产品简明介绍，不承载技术实现细节。独立自有代码采用根 `LICENSE` 的MIT；上游代码、衍生修改、依赖及组合分发遵循各自原许可、版权、例外与附加要求。
Node收编GRANDPA源码保留GPL-3.0-or-later及Classpath例外，Runtime已有Apache源码保留原版权头。

### 组织重构后的钱包与TLS读取合同

Node钱包只接受当前WalletStore字段闭集；旧id/address/activeId格式、未知字段、不完整账户、重复账户、错误SS58或不规范的活动账户标识直接失败。持久化文件仅保存Cold条目，Hot钱包由本机powr密钥事实动态生成；文件中的规范active_account_id允许指向动态Hot，因此不要求它出现在Cold条目中。向前端提供钱包时先合并Hot与Cold，再确认活动账户属于可用钱包，否则选择首个可用账户。读取失败不会重写、迁移或删除原钱包文件。正常钱包读取和保存继续使用现有账户验证。首页转账余额行左侧直接显示当前钱包的名称，右侧显示同一账户余额；未另选钱包时沿用后端默认矿工热钱包，切换后名称与余额跟随活动账户更新。

挖矿页采用受窗口高度约束的纵向布局，网络统计、挖矿收益、“出块记录”标题及表格列标题保持固定，仅标题下方的记录区域独立滚动；记录表格保留横向滚动。此滚动样式仅作用于挖矿页，其它标签页沿用各自布局，数据读取与刷新逻辑不变。

Node开发页和HMR仅允许HTTPS/WSS；开发及预览必须提供完整匹配的证书和私钥路径，静态构建不启动开发服务。OnChina请求仅允许无凭据的HTTPS来源，禁止自动重定向和跳过证书、主机名校验；LAN可显式加载受信CA，缺失或损坏配置必须失败，公网仍使用Cloudflare Edge。

## 三级目录与单一视觉资源

Runtime、node/grandpa与node/libp2p保持原件。其它源码最深三级，第三层只有文件，每个目录至少两个真实直接子项；Rust公开包名、逻辑模块与业务接口保持，只以编译器文件定位调整物理结构。共享库归crates/harness、signing、protocol、scanner；QR金标直接归protocol/tests。节点按所属包src和frontend组织；OnChina按自身技术文档中的成对前后端工程及包根功能目录组织，目录使用完整单词；导入、Cargo本地路径及npm锁定本地来源同步。

根icons是本产品唯一持久静态视觉资源目录。Logo与公民币母版字节保持，重复PNG仅留一份，ICO/ICNS格式保留；六处扫码按钮共用原SVG几何并保持currentColor和尺寸，宪法背景从唯一PNG编码进离线HTML。白皮书配图也归根icons目录，节点编译时内嵌到正文，官网构建从同一配图生成打包资源。

scripts保留build.mjs与config.mjs；GitHub自动化归.github/workflows，门禁归.github/tatagate，文档、图标、维护和Runtime数据工具各归所属功能目录。原Job阶段、独立产品平台流程身份、候选校验、缓存隔离与失败条件保持；同一平台同一流程共享其本文件实现，阶段号仍由既有Workflow固定。

本机输出归target/build或target/test。Windows预打包将资源组装到独立构建现场；npm视图按锁定原件安装，不依赖控制台缓存链接。

本地scanner的file依赖按所属源码的package.json名称、版本与实际路径验真；远端包继续按原锁SRI验真。根工程复制到本仓target内的任务现场时排除target自身，避免递归复制。固定Git原件显式导入锁定提交，按其原有refs/tata引用格式处理，来源与版本保持原锁。

registry与Git的目录源按原锁准确来源隔离，同名同版本不会混合。SDK临时发布视图保留其Git相对依赖身份；上游dev和可选依赖声明均保持原Git提交，只物化消费锁实际需要的包。原锁和上游原件不被改写。

统一扫码SVG以原几何独立资源输出，消费端使用currentColor的CSS遮罩，保留18×18尺寸并在WebKit实际窗口显示；不依赖外部SVG symbol引用。Rust build-std依赖按已验真Rust工具原始Cargo.lock物化，产品Cargo锁和工具版本保持。

## 节点部署离线批准

权威和权益节点实例的部署批准使用本仓独立 `crates/protocol` 的 QR_V1 `node_deploy=16` 链下动作；它不进入 Runtime WASM，不借用软件发布动作或链交易签名。载荷固定包含公民链产品、节点类别与编号、目标服务器 IP、版本、源码提交、产物摘要、控制台任务编号、期限和随机挑战，钱包必须完整解码并显示目标。签名消息使用独立 `GMB\0TATA_NODE_DEPLOY_V1\0` 前缀，控制台只接受受保护的 `rtupg:NRC_PUBLICKEY`；同一个国储会管理员公钥也是开发升级签名者，且只存 Touch ID Keychain，离线私钥不进入产品或控制台。节点部署是使用公民链的实例操作，由控制台节点管理维护，不等同于发布公民链软件。`scripts/build.mjs` 将独立 QR Cargo 锁纳入当前构建的受控资源供给；本机受控供给使用离线原件，原生CI按原锁准备。失败即不编译。协议依赖准备不执行节点部署。

## 独立执行与外部接入边界

依赖由本产品的原始锁和既有独立实现准备、保存及复用；直接执行与控制台调用使用同一完整入口。控制台通过对应tata产品文件传递动作、普通工具路径、已授权输入与取消信号，不登记或供应产品依赖，不要求私有资源通道。显式离线缺件和真实执行错误按本产品接口失败，不改变版本、来源或SDK提交。

GitHub接口使用普通参数和已授权令牌，返回本仓既有版本计划与Run回执；派发不确定不重发。各平台YAML、Run、Release、Tag规则和增量缓存由本仓保有。控制台接入绑定只在其tata/对应产品唯一文件中实现；平台、流程、动作、布局及控制台任务功能由控制台拥有，本仓不操作控制台任务宿主。产品名称由产品提供，并声明选用的控制台平台和流程。

## 原生源码归档的原始修改时间

本仓原生源码工具的TAR解包显式启用preserveMtime。TAR头提供原始修改时间；PAX局部mtime覆盖全局mtime，空的局部或全局值按扩展头删除语义回到对应TAR头时间。时间必须是有效、有限且在时间接口范围内的十进制Unix秒数；保留小数与负值，传给Node时间接口时使用已验证字符串，避免负数数值被解释成当前时间。

解包仍先完成成员、父目录和链接闭包校验，再物化文件及链接。所有成员写入完成后恢复mtime；符号链接使用不跟随目标的lutimes，普通文件及物化后的硬链接副本使用utimes，目录按深度倒序恢复，剥离的归档根目录最后恢复。任一步失败或取消仍清理本次候选。这样保留发行包预生成文件与源码输入的原时间关系，gettext不会因解包写入顺序误触发aclocal等维护步骤。

普通依赖和ZIP解包沿用原合同。XZ与BZIP2由既有已核对BSD tar的归档输入模式读取，仅将成员转换为内存PAX流，二进制内容不进入日志；输出沿用8GiB边界。转换显式保留原路径，不在校验前提取成员，也不让工具改写绝对路径后误过校验。统一解析器先核对成员、路径、父目录、链接闭包与循环及硬链接目标类型，随后才物化；XZ/BZIP2继续恢复TAR/PAX原时间，原生源码GZIP/TAR仍显式启用preserveMtime。工具版本、官方来源与摘要、SDK固定提交、平台缓存和各平台Workflow保持。

自举到本产品Node的子进程继承父租约，只释放自身编译锁，不在存活期间清空共享工作根。真正租约所有者等待子进程及其后代退出并消费结果后统一收尾；活动进程组、原租约身份与未退出守卫仍逐项核对。正常失败原样抛回；主错误与收尾同时失败时以主错误为cause、保留两项异常并报告清场失败，不能把失败伪装成成功。工具转换保留原始stderr；自举日志持续流出，仅在异常中保留64KiB诊断尾部，不把长编译日志计入JSON回执上限。

### 链上中国宿主及模块的退出衔接

设置页既有按钮、确认、文字和浏览器交互保持。节点启动链上中国宿主时持有专用父管道；停止及节点退出通过关闭管道通知，等待宿主排空请求、关闭和回收模块后再确认停止。等待40秒仍未完成时返回原错误通道并保留句柄，禁止报告已停止或继续替换运行文件。Unix启动前旧宿主清理同样使用TERM并等待完整收尾，超时拒绝重启，不以强杀宿主遗留模块。

启动健康等待按真实经过时间包含HTTPS检查；60秒等待结束而宿主仍存活时使用原starting/启动中状态，子进程退出才报告启动失败。模块慢启动不等同于宿主崩溃。不修改节点前端、Runtime、CID、国家和省市编码或宪法守卫。

## 外部调用入口

### Runtime独立GitHub构建入口

当前macOS正式节点在桌面RPC证书初始化成功后，仅将本次公开DER的十六进制原子写入既有正式数据目录的rpc-certificate.hex（本机为~/Library/Application Support/gmb/rpc-certificate.hex）。不输出私钥，不新增配置项、Keychain条目或UI；交付失败只报告诊断，不改变原节点启停。目录0700、文件0600，所有者必须为当前用户；拒绝链接、硬链接、异常类型及附加ACL，祖先目录不得向其他用户授予写入或替换权限。固定/bin/ls -lde以C语言环境只读检查ACL，不经过shell；未知结果拒绝，祖先deny delete不视为写入。每次完整应用启动更新，手动重启沿用同会话证书；旧文件遇到新会话TLS时失败，退出不增加固定文件删除。公开证书不交给GitHub，不改变RPC回环监听、授权或表决。文件权限不能防御同用户恶意程序或root；创世哈希检查也不能替代证书来源校验。其它平台现有流程保持。

交付端先按既有CITIZENCHAIN_DATA_PROFILE及编译模式确认正式命名空间；开发身份不写正式证书。先只读校验候选目录（首次创建则检查父级）再调用既有目录与原子写入工具，不沿异常链接设权，不自动修复既有不安全目录；最终目录必须与节点现有解析结果一致。同文件通过同一交付函数的写入依赖替身验证明确写失败不回报交付成功，不制造真实磁盘故障。桌面夹具从CARGO_MANIFEST_DIR确定产品根target，不随Cargo的包工作目录写入node源码目录，结束只删除各自独立夹具。TLS夹具子进程仅接收公开证书与临时端点，失败保留其错误诊断，不以通用失败或预期拒绝掩盖衔接错误。

Runtime候选WASM编译由scripts/runtime.mjs负责，节点安装包由scripts/build.mjs负责。config.mjs的WASM分支转调Runtime入口，wasm.yml编排GitHub阶段；build.mjs拒绝旧wasm入口。

正式WASM派发由runtime.mjs装载节点公开证书后读取本机既有https://127.0.0.1:9944的冻结链身份及同一finalized块双版本，分别加1，并通过现有输入和当前构建的runtime_base JSON交给GitHub。准备和发布归wasm.yml；编译前只删除目标整数版本同名的旧Release及Tag，不处理其它版本。目标Release不存在时继续。删除目标Tag收到404或422且响应正文的message为`Reference does not exist`时继续，其它删除错误停止。源码编译归runtime.mjs；Runner只核对快照、目标及实际三份产物，不再次连接本机，不套X.Y.Z，也不要求新增RPC地址或证书配置。快照只对应读取时刻，不能保证构建期间链上版本不变；实际升级仍走原流程。缺少节点、公开证书不可用、可信连接失败或字段异常即停止，不猜值、不降级；不修改控制台、公民服务端或Runtime。原仓Node/Runtime源码不被WASM构建回写，节点交付源码修改仅限上述启动入口。临时输入和现场保护遵循CitizenChainRuntime.md。

WASM塔塔自动化的编译实现仅归scripts/runtime.mjs，唯一工作流为.github/workflows/wasm.yml。runtime/build.rs仅嵌入既有WASM或生成空常量；SDK源码编译钩子由runtime.mjs仅在当前构建的临时源码中生成，退出后按原现场规则清理。Cargo依赖声明沿用原锁。

五端历史维护直接归各自Workflow的cleanup；已完成历史与当前Run已结束的前置作业共同决定保留名额，避免排除当前Run而多留第三条。只删除已完成旧Run，不调用配置中的清理计划或包装函数，也不把当前构建的名额判定写成GitHub最终conclusion。

节点四端版本由各自Workflow准备阶段读取本平台最新成功正式Release的版本并patch加1，首次使用节点配置基准版本；候选冲突必须失败，不在配置或控制台复制版本推算函数。WASM保持其当前构建的冻结基线及双版本输入。

原Run最后Job遇到旧来源Run的真实404时，不补造成功记录：先复用既有来源/附件校验并核对旧Tag，若当前构建的没有新的完整正式交付则保留旧产物并明确记录；只有当前构建的真实Release成功、替代交付重新回读完整有效且版本更高时，才删除该已被替代的旧Tag和Release并回读确认。未知错误、提交或附件漂移、不可变Release、删除失败仍明确报错；活动任务和其它平台保护，终态Run淘汰继续独立执行。

macOS原编译Job通过Cargo官方CARGO_PROFILE_RELEASE_BUILD_OVERRIDE_STRIP=none，明确关闭release构建期脚本、过程宏及其依赖的strip，避免锁定工具链在macOS产生LINKEDIT未对齐而无法被rustc加载的宏动态库。该输入由既有编译环境透传，不修改交付目标原profile、工具或依赖锁、Runtime入口及编译脚本，也不添加替代链接器、缓存删除或辅助文件。

OnChina管理动作的JSON反序列化通过Serde官方Deserialize接口委托既有parse_action_type，公开调度与JSON使用同一动作解析规则；两个已撤销的CITY_REGISTRY动作名统一拒绝，合法SUBORDINATE_REGISTRY名称、模块字符串格式及后续实际登记/授权沿用原实现。序列化名称保持既有协议。

OnChina事件交易投影与实体CID收集均使用既有SCALE codec::Decode解码原始Compact<u32>长度前缀，并按实际消费字节计数，禁止把Subxt长度解码错误后的零事件回退当成合法空块。官方空输入与合法零事件向量保持。

两Linux正式编译在原步骤失败后，使用GitHub官方failure()与steps.compile.outcome条件运行独立资源诊断步骤；仅列出当前构建的resources中符号链接的路径和目标，不跟随目录链接、不修改输入、不新建诊断文件。原编译命令及失败退出码、完整产物与发布门禁、既有缓存及终态清理保持；诊断成功不代表编译成功。资源目录尚未生成时不输出链接。

macOS和两Linux工作流在PostgreSQL原make install完成后，以系统cp -RL把Runner临时pg-dist复制为pg-runtime普通文件树，CITIZENCHAIN_PG_DIST只交付该物理副本。原安装原件、PostgreSQL/Node锁定版本和依赖保持；坏链接复制失败仍立即终止。此步骤避免Node25.2.1递归cpSync的dereference兼容性问题把绝对库链接带入资源包。

macOS 使用 Tauri 的 `--bundles app` 生成内置应用，复用同一个 Swift 安装入口组装唯一交付物 `公民链.app`。正式构建核对内置应用的签名身份、团队、时间戳和公证，再检查外层安装包的深度签名。

构建时从 `node/Entitlements.plist` 将公民链的签名团队写入 macOS 安装器。已签名的内置应用作为未压缩数据随安装器主程序交付。用户双击后，安装器将数据写入私有临时目录，再用该团队和公民链的应用标识校验内置应用及待替换的旧版。

### 原生构建精简与能力边界

节点默认Cargo特性仅std；custom-protocol仍可显式启用，正式Tauri CLI按官方实现自动启用tauri/custom-protocol，桌面正式包继续嵌入已构建前端。普通构建不把前端源码及node_modules作为生产资源嵌入。benchmark模块、CLI入口与依赖仅在既有runtime-benchmarks特性下编译；生产Runtime升级安全检查仍使用frame-benchmarking的API标识，原防护不变。

基准CLI沿用同一固定SDK，作为已有非可选依赖参与默认Cargo元数据，确保WASM构建器能够定位所需Runtime；基准命令及Runtime基准能力仍仅由runtime-benchmarks开放。try-runtime显式同步既有pallet-staking的条件接口，避免session与staking的MigrateDisabledValidators定义不一致。普通构建会编译该依赖闭包；节点正式Runtime不注册该pallet，业务功能、权重及冻结创世输入保持。

OnChina按真实Cargo入口用一次cargo build和重复--bin选项编译所选目标，完整构建仍保留宿主及四个模块共五个程序。每个前端入口保留原类型检查、真实声明和产物校验；共享tsconfig.node.json工具配置每次构建仅检查一次，单模块构建也独立检查一次。

本机的 `scripts/build.mjs compile macos` 供塔塔安装器消费当前源码的应用本体和签名回执，现场保留至调用方关闭 stdin。公民链的 `scripts/build.mjs package macos` 复用同一编译过程，在本机签名后交付桌面的 `公民链.app`。

compile macos作为公开CLI保留；模块公开实际宿主编译及资源装配接口，任务派发与观察由配置声明交付给通用调用方。

原生转账提交接口返回交易池接受的交易哈希，界面显示“已提交”。确认按钮和扫码回调共用防重入状态，提交成功后保持至关闭，失败后允许重新操作；签名、nonce、到期与单次会话消费继续独立校验。原生交易表单、钱包管理、机构及管理员账户统一显示finalized System.Account.data.free+reserved总余额。原生转账预检查独立按Preserve/Polite口径计算free-max(111,frozen.saturating_sub(reserved))的饱和差，再校验金额与手续费，链上执行最终裁决。Ethereum保留latest/pending/safe/finalized和显式块参数语义；同块同账户总额与原生一致，不强行将latest替换为finalized。MetaMask最大金额仍由钱包自身计算，展示总余额不代表全部可支出。

## 节点部署冷签公开协议

节点部署是QR_V1动作16的链下授权。唯一编解码归crates/protocol/src/codec.rs，动作及中文字段声明归同crate的registry。载荷仅接受格式2：版本字节2、固定产品citizenchain、节点类别、单字节UTF-8长度前缀的节点编号/IP/平台/安装包实际版本、32字节安装包SHA-256、9位任务编号、8字节小端期限、32字节本次挑战。保留GMB\0TATA_NODE_DEPLOY_V1\0独立签名域；删除节点旧格式、源码摘要及中文Tag准入，不改变Runtime升级和其它发布动作。

crates/protocol/tests/sign_request.json提供格式2公开样例；golden_fixtures.rs逐字节核验真实Rust编解码，控制台读取同一夹具，钱包使用相同样例及签名摘要金标覆盖真实解码、中文审阅与签名。正式Release安装包的业务核验仅为哈希、平台和版本，签名身份、目标绑定、期限和防重放仍须有效。协议生成器export_registry的公开输出继续是钱包中文字段表的来源。

scripts/config.mjs的describeAction只返回repository、workflow、ref、inputs声明。控制台使用同一通用接口派发并观察准确Run；Node的编译和WASM的编译各归本产品既有编译文件，版本、发布和历史维护归既有YAML。已有声明版本和原锁保持。

本产品配置项统一为name、description、accessLevel、kind、format五个必填字段；不适用规则明确填写“不适用”。name严格使用两组大写字母和一个下划线，同名共用控制台唯一值，当前页仅汇总实际声明的使用产品。操作列固定写入/更换、查看、删除；保密查看保留生物识别。现有配置消费者随声明同步改名，官方工具/API字段保持官方名称，不新增旧名兼容或配置副本。

节点钱包持久化只保存名称、真实Hot/Cold签名路由和唯一accountId；冷钱包文件不保存SS58副本或无人读取的创建时间。展示由原accountIdToSs58按2027前缀编码。旧文件含被删除字段时严格拒绝，普通读取不迁移、覆盖或清空真实文件；真实数据处置须单独确认。Runtime、创世、QR_V1和签名合同保持。

节点自有Tauri参数与DTO采用官方camelCase合同，Rust成员保持snake_case，删除同一DTO内改回snake_case的独立字段注解。当前Runtime、官方RPC、固定QR_V1与公开CLI拼写继续遵循其所属合同，不提前改动链上接口。

账户展示DTO也只携带accountId，奖励账户、机构账户和登记人地址由前端现有SS58函数计算；主/费账户按唯一AccountId分类，不再比较展示地址。机构账户只返回真实balanceFen，前端复用原BigInt格式化函数显示元，不同时传一份格式化金额副本。管理员签名匹配类型由治理模块唯一声明，多签只公开重导出同一类型。Runtime参数、原始金额、签名路由与链上编码保持。

节点待结算记录只嵌入链端公开 PaymentIntent；批次项和签名哈希直接消费 offchain::batch_item 的 OffchainBatchItem 与 batch_signing_hash，不再维护节点镜像。SCALE 嵌入结构无额外信封；保留原批次长度与字段顺序金标，并拒绝尾随字节。

节点自有提案所属模块参数为 moduleTagHex（Rust module_tag_hex）；编码格式仍为链端 SCALE 字节。私有签名公钥与宪法版本标签参数使用三词内名称，不改变所属链端键或调用索引。

## 普通转账边界

普通转账继续使用既有amount_yuan IPC参数及Runtime手续费公式。元转分保留原舍入规则，但转换前必须拒绝乘100后的非有限值及超出u128范围的结果；金额、手续费与账户保留金使用checked_add，在读取链余额前拒绝溢出。正常金额与冷签调用SCALE字节保持原金标。

### 节点主入口与交易子模块挂载

`node/src/platform.rs`拥有OnChina子进程启动、状态查询、父管道关闭和退出等待实现，
主包通过nodemultisig、nodeoffchain、nodeonchain三个所属Cargo包公开接口装配。
三模块和四个进程入口显式为pub(crate)，保持原私有聚合模块下的有效访问边界，
不新增对外Rust接口；桌面命令仍通过原注册表公开。
Rust调用统一为`crate::multisig/offchain/onchain`及主入口进程函数；不保留旧聚合模块、
转发函数或兼容别名。桌面命令名、RPC、交易编码、手续费、清算逻辑与启动/停止行为保持。

## 本机编译资源与版本来源

本机compile macos复用环境PROTOC或PATH中已安装的protoc35.0，不再从GitHub下载protoc；缺件或版本不符直接报告所需工具。Cargo、npm按原锁离线执行，Git仅允许本地资源。现有自动化的GitHub身份、版本及正式交付保持，控制台任务号不传入本产品。

GitHub macOS自动化依据product.tools.xcode选择官方Runner内已安装的对应Xcode，解析其实际Developer目录并设置当前构建的DEVELOPER_DIR，再回读、打印和核对版本；Clang与libclang复用同一目录。Runner默认Xcode切换不改变产品声明，缺少指定版本时保持失败，不下载Xcode或修改系统选择。

## 节点源码结构

节点主包是node；下表14个目录各是一个独立功能包，其src与tests平级。源码按模块文件平铺，不保留旧core/governance/admins包装包。GRANDPA覆盖目录为node/grandpa，官方包名sc-consensus-grandpa；libp2p覆盖目录为node/libp2p，官方包名libp2p-websocket。前端只对应15个第一方包。

|Cargo包|精确包路径|源码模块|
|---|---|---|
|node|node|application, benchmarks, bootnodes, chain, cli, command, ethereum, grandpa, initialization, lifecycle, platform, rotation, rpc, runner, service|
|nodeguard|node/guard|amendment, citizen, composition, constitution, fullnode, governance, guard, interest, lifecycle, phase, policy, render|
|nodeinstitution|node/institution|account, activation, admins, balance, chain, cid, codec, commands, council, registry, storage, types, watch|
|nodecontrol|node/control|bootnodes, documents, grandpa, identity, mode, network, password, reward, rpc, state, sync, task, update|
|nodedesktop|node/desktop|address, certificate, cid, constants, keystore, publication, rpc, security, storage, tls, validation|
|nodeclient|node/client|codec, handle, network, query, session, signing, types|
|nodemining|node/mining|cpu, dashboard, gpu, network, pow|
|nodeonchain|node/onchain|commands, transfer, types, wallet|
|nodeoffchain|node/offchain|bootstrap, commands, endpoint, health, keystore, ledger, listener, packer, reserve, rpc, signer, signing, submitter, types, unlock|
|nodeproposal|node/proposal|business, commands, display, query, tally, types, vote|
|nodemultisig|node/multisig|account, commands, proposal, signing, types|
|nodeupgrade|node/upgrade|call, commands, decode, signing, summary, types|
|nodegrandpakey|node/grandpakey|commands, proof, request, storage, types|
|noderesolutionissue|node/resolutionissue|decode, summary, types|
|nodedestroy|node/destroy|decode, summary, types|

主包保留完整服务启动、停止、重启、桌面注册和换钥监控编排。功能包只拥有对应能力，通过公开接口协作；普通依赖无环。正式构建不启用测试特性。

nodedesktop与node共用node/build.rs，按真实Cargo包名分派：前者只在本包OUT_DIR生成唯一RPC接入页，后者生成原CLI/Tauri资源，沿用相同源码/输出路径归属校验。node/frontend/desktop/metamask.mjs正式HTML和CLI保持。

桌面前端保持node/frontend单一npm包。desktop/node.tsx为入口，desktop/client/guard/institution/control/mining/onchain/offchain/proposal/multisig/upgrade/grandpakey为真实功能目录；proposal/resolutionissue.ts和proposal/destroy.ts为独立的仅类型模块，严格匹配原后端返回字段，不新增UI或请求。原JSX、CSS规则、交互和IPC名/参数/结果保持。清算机构、议事机构与管理员属于institution，保留各自实际DTO，无同名合同强行合并。

销毁详情解码严格拒绝尾随字节，控制包hex_to_bytes对非法ASCII和Unicode返回None。

创世JSON、内嵌WASM/patch、GENESIS_HASH、GENESIS_STATE_ROOT及行政区数据不得改变。

### 源码入口与输入校验

前端根层保留.npmrc、.nvmrc、package.json、package-lock.json、index.html、tsconfig.json、vite.config.ts。桌面入口和RPC接入页生成器归desktop，发行与销毁类型归proposal下独立模块。

销毁载荷要求CID后恰好32字节账户及16字节金额，尾随或截断输入返回None；控制包十六进制解析复用hex库，非法ASCII或Unicode返回None。

## 共享包源码归属

chain-signing的src按material、signature、extrinsic、preflight组织，lib登记公开入口。qr-protocol具有独立workspace与锁文件，registry的四份YAML是生产协议声明；scanner通过自身src与npm入口提供实现。组帧助手createQrAssembler只在crates/scanner/src/index.ts公开，由节点和OnChina通过既有@gmb/scanner-react依赖消费；不跨目录读取Runtime前端源码。

共享扫码控制器开始新会话时清理已取流但尚未完成播放的旧会话；过期play完成只退出，不停止新会话。停止/释放的generation规则保持；控制器与ScannerView沿用原设备入口，结果回调以false表示继续、void表示结束，并允许异步等待。扫码间隔、画布到jsQR路径、UI及码型业务职责保持。

节点签名弹层和面板统一归node/frontend/desktop/signing.tsx；扫码设备外壳与地址扫码弹层统一归同目录scanner.tsx。原组件名、Props字段、样式、交互与共享ScannerView保持。开发TLS配置通过本仓scripts/build.mjs公开实现读取，开发证书路径、校验和TLS1.3保持；节点已有工程、创世、Runtime及资源位置保持。

## macOS PostgreSQL资源准备

本机安装和macOS塔塔自动化共用scripts/build.mjs的preparePostgres；工作流通过.github/release.mjs的共用工具准备调用postgres macos并读取当前构建的CITIZENCHAIN_PG_DIST。先验证明确指定的目录，再查PATH对应安装、Homebrew当前主版本安装及本产品target/cache/macos下准确版本缓存；已有完整同版本且目录可独立搬迁的分发直接复用。版本只接受官方完整版本和Homebrew发行后缀，不接受较新或近似版本。显式输入错误、可执行文件失败、缓存损坏均报错，不以下载掩盖。

全部缺件才下载产品声明版本的原官方源码，已有源码包复用；原OpenSSL编译配方不变。缓存独占锁避免并发覆盖，下载完成和编译核验后才分别交付完整源码包和资源目录；当前构建的失败现场清理，已完成缓存保留。完整性检查覆盖postgres、initdb、pg_config、lib、postgres.bki及COPYRIGHT，并核对实际bin、pkglibdir、sharedir均属于分发内部。原件不写入，装配仍只复制资源和处理当前构建的动态库。protoc、Rust、Node版本声明及各自既有准备流程保持。

控制台内置Node无npm时，npmCLI先检查当前解释器及PATH中的完整Node安装，再检查设备NVM的声明版本安装；只有实际Node版本准确匹配才使用其随附npm。安装编译环境加入该声明版本的NVM bin供npm生命周期调用，不能误用其它版本或要求控制台额外复制npm。

本机编译子进程使用上述完整官方Node的node/npm配对；platformCargoEnvironment保留显式NODE，npm、模块前端与Tauri均用同一解释器，避免控制台内置加固Node在重新执行时丢弃DYLD_FALLBACK_LIBRARY_PATH。沿用Xcode已有libclang及原动态库搜索环境，不改变App权限、系统信任或上游依赖。

本机Tauri编译的公开stderr与stdout进入同一任务日志流，确保长编译的最终错误可见；密钥、签名命令和捕获型接口仍使用原独立有界通道。该调整不改变编译退出码和成功判定。

macOS装配从官方原件复制外部动态库时，保留当前构建的内存中的原路径；其@loader_path相邻依赖若尚未随包存在，就从该准确原路径递归复制并改写副本引用。既有依赖仍直接使用随包文件；来源无法确认和同名不同内容继续失败。该路径映射只存在当前构建的内存，不形成资源登记或第二真源。

## 节点更新与白皮书读取

安装包URL按GitHub正式下载路径拼接，先移除基址末尾空段，再追加Tag和文件名；保留Release资产身份、大小、摘要及地址严格匹配。

白皮书命令保持原异步调用及返回格式，直接返回编译内置的根目录正文和配图，不再发起官网请求；前端在本次程序会话中保留成功正文并共享进行中的请求，失败不缓存，保留既有重试交互。切换页面不重复获取已经成功的正文，首次立即显示加载状态。钱包持久化仍仅接受name、signMode、accountId及既有列表根结构。

本机macOS装配完成动态库路径改写后，对随包Mach-O资源临时签名。正式发布仍使用原正式签名，本机交付由塔塔安装器在最终签名后验证整包。两条路径均在交付前以C locale实际执行独占临时initdb并检查PG_VERSION及postgresql.conf；本机不得提前返回而跳过初始化校验。最终App身份签名与安装继续归塔塔安装器。

## 通用本机签名交付

产品编译回执TATA_COMPILE通过signing数组交付待签名目标；path为产物内相对路径，主App为“.”，entitlements为产品权限plist路径或null。Apple产品明确列出嵌套代码及主App，Android交付空数组。产品拥有清单与权限，编译、依赖及资源校验流程不导入控制台实现。控制台按相对路径嵌套深度从内到外签名，不扫描或推定产品结构，不以产品名称选择权限；描述文件授权值及本次身份覆盖对应请求键，产品独有功能权限完整保留。签名结果及错误沿用原任务日志，最终签名与安装仍归通用安装器；独立产品入口保持可执行。

PostgreSQL复用除完整版本和资源外，还通过其pg_config确认bin、pkglibdir、sharedir均在同一分发目录内部。模块目录仍指向外部Homebrew安装的同版本发行不满足随包独立运行要求，不参与复用；使用已存在的同版本可搬迁分发或原官方构建配方。资源准备与该检查仅归公民链scripts/build.mjs，不加入控制台签名器。

搬迁原始macOS分发时，内部dylib可能仍以原绝对安装名相互引用；仅在本分发已有库的otool安装名与依赖引用完全一致时，将其重定位到随包相对路径。名称不同或来源不明确仍失败；外部依赖仍从当前构建的声明工具的真实原件复制，禁止把旧安装包中的已签名依赖当原始分发混入。

链上中国本机WSS客户端沿用固定jsonrpsee0.24接口：连接预算包含初次连接，设为1，仅连接原127.0.0.1地址，不跟随重定向；父进程公开证书与主机名校验保持。私有PostgreSQL归档命令对Shell路径与配置字符串分层转义，支持Application Support等含空格路径。启动时幂等维护所属四项配置并对已运行的私有集群重载，其余配置及持久数据库不重建。

### 设置页正式版下载与安装更新

双击 macOS 或 Windows 的「公民链」安装包时直接打开安装进度窗口。窗口标题栏显示「公民链安装程序」，窗口中间显示「公民链」及目标版本。右上角叉号请求取消，已开始写入时等待当前步骤结束再关窗。安装成功自动打开公民链，失败或取消时自动关窗。Linux 首次安装由系统的 DEB 包管理器处理，应用内更新使用公民链的进度窗口。

节点程序版本行由产品后端从GitHub仓库crcfrcn/citizenchain的Release列表选择当前平台最新成功正式版，按published_at排序，排除草稿、预发布和WASM，不使用Actions Artifact。沿用Release中的自动化记录确认本平台成功Run及原完整安装包。用户首次点击安装更新仅下载，后台按实际已收/总字节通过Tauri Channel交付进度，进度条在按钮左侧，按钮显示下载中且不可重复点击。完整下载并落盘后显示100%和安装更新；再次点击才执行安装。

下载及安装状态归 node/desktop/installer/update.rs。前端不提交任意 URL、文件路径或命令，后端仅跟随 GitHub HTTPS 附件跳转。完整包可在重开应用后恢复。安装进程等待旧 PID 退出，沿用 application.rs 的退出清理释放节点数据库锁，保留链数据、密钥和设置。macos.swift 为新的自包含安装包提供双击入口，并为已经发布的 macOS 更新附件保留安装入口。Windows 应用内更新使用同一个 NSIS EXE，Linux 应用内更新由 linux.c 安装同一个 DEB。系统按需请求管理员授权。

### 各平台节点产品版本唯一传递

macOS、Windows、LinuxARM、LinuxAMD各自独立发行，工作流从本平台最新成功正式Release读取版本，下一轮候选仅补丁位加1；失败不增加正式版本，重试沿用同一候选号。不再从历史Artifact推算。既有node/tauri.conf.json的version只提供首次发行初始值；正式交付以当前构建的CITIZENCHAIN_RELEASE_VERSION为唯一输入，同值注入Rust节点公开版本常量NODE_VERSION和Tauri配置。设置页nodeVersion与命令行impl_version读取同一常量，移除旧Cargo固定版本读取和SDK拼接提交号的产品版本生成。本机塔塔安装器沿用本平台最新成功正式版，不增加版本，不再从Cargo内部包元数据确定软件版号。更新检查继续比较已安装版本与同平台最新正式版，内部Cargo/NPM包元数据不作为产品版本入口。上述仅统一节点软件版本，不改变其它版本体系。

注册绑定核验唯一只读RPC为identity_getBinding([cid_number,account_id,expected_genesis])。节点一次读取client.info，核对创世链并固定其finalized_hash为H，使用本链真实存储类型在H读取CidRegistry、AccountIdByCid、CidByAccountId及BindingRevisionByCid；仅有效、未吊销、双向一致且修订号为正时返回block、cid_number、account_id、bind_rev四字段。block是同一个H，不是best块哈希。不存在返回null；错误链、坏编码、缺失映射、无效修订或撤销均拒绝。一次服务端RPC，节点最多四次本地存储读取；不调用其它网络RPC、不改标准state_queryStorageAt、不新增Runtime API、端口、表或缓存。该结果只供服务端保存公开绑定事实，不授予登录或设备权限。

公开配置入口describeConfiguration只返回schema、product_id、configuration三字段；manifest.runtime继续保留既有证书运行期声明。配置读取不交付runtime字段，沿用控制台既定闭集及受保护NRC_PUBLICKEY授权，不变更密钥、节点二进制、交易或Runtime；缺失运行期证书授权不阻塞独立的配置读取和节点软件部署。

提案编号：提案ID内部使用u128，年份u32，年内序号u64；Tauri/HTTP/JSON中的提案ID及年内序号统一传十进制字符串。分页运算使用BigInt，不经JavaScript number；提案键/调用ID槽为16字节，展示号为12字节。节点组成守卫按完整长度及哈希校验旧8字节、新16字节ID键，并以同一编码配对提案与阈值，覆盖升级前后状态。

CID年份u32：CID生成器调用中的year同步为u32，组成和生命周期守卫继续执行原有职责。生命周期守卫对新年份号码仍核对地域、机构码、盈利属性与原始文本校验和，不增加年份或随机号段的独立政策。既有CID和派生账户无迁移；runtime-benchmarks工厂仅同步年份字面量类型。

### ML-DSA-65 原生账户交易

核心统一为 `runtime/transaction/mldsa`：开发期 sr 账户用 sr 签名、PQ 账户用 PQ 签名。sr AccountId 为 sr 公钥32字节，PQ AccountId 为 `blake2_256("GMB/account/ml-dsa-65/v1" || public_key)`；现有 AccountId 不变，无跨算法绑定或转换。
原生交易统一为V5 General、认证管线1，认证扩展为AccountAuth；证明仅有sr25519、ML首次公钥、ML已登记公钥三种分支。sr账户仍是原公钥32字节，ML账户派生及PqPublicKey存储保持；不转换账户、不迁移余额。两种算法共用账户绑定的签名约束：blake2_256(TRANSACTION_DOMAIN || AccountId || SCALE(ImplicationParts))；ImplicationParts覆盖管线版本、call及全部后续扩展的explicit/implicit，审阅端保留完整业务字段。nonce、权限、费用、质押保护、资源检查及上限保持；运行阶段仍拒绝sr。
Mldsa pallet index36仍仅保存PqPublicKey。ML首次在正常交易校验通过后写公钥一次；Stored缺公钥拒绝。SDK要求的FRAME交易构造合同同样只生成V5；runtime在检查入口拒绝外层签名格式及未注册原生管线。Ethereum专用交易和固有交易沿用SDK检查。
`node scripts/runtime.mjs build-operation` 在原候选构建流程启用 operation 特性；一次 runtime 升级的 hook 把既有 Phase 切至 Operation 并关闭 DeveloperUpgradeEnabled，不扫描或改写账户。运行期在 sr 密码学验签前拒绝 sr 交易。
钱包与SDK通过主清单直接引用同一核心，源码构建优先复用同级公民链，缺少时仅在本轮临时目录浅克隆源码并直接引用；默认sr、提供PQ选项，今后 operation 客户端构建只保留PQ派生。首次认证随正常交易自动完成。
认证权重目前是保守预算，尚非目标节点硬件的 FRAME 生成基准。

### 统一V5的交付边界

节点热签、冷签、投票、节点内协议升级和链上中国提交均使用同一完整审阅材料与账户绑定摘要。节点守卫从V5认证证明读取实际付款账户，继续核对原手续费公式；旧高度区块按其父Runtime执行，不重写历史。默认Metadata_metadata公开完整V16认证管线。冻结chainspec、创世WASM、块0、创世写入源码及哈希常量逐字节不变，不重新创世；新代码仅通过另行获准的既有链升级生效。已签原升级交易不改字节、不换WASM、不重签或重发。

扫码结果处理只在明确返回false时继续同一摄像头；void结束，异步处理串行等待。单码仍只交付一次；多帧完整后才交付业务页面。帧数、大小、超时和Rust核心校验保持；取消、错误及旧会话迟到结果不能重启或停止新会话设备，也不再触发已关闭页面的扫码提交。

机构管理员列表、清算行管理员列表与提案详情共用InstitutionAssignmentCard：保留原方形数字序号，首行按“序号、岗位名称 · 管理员姓名、原操作控件”排列，分隔点为居中几何圆点；横线下第1行左来源右任期，第2行账户余额，第3行完整管理员账户。任期按UTC显示xxxx/xx/xx-xxxx/xx/xx，无固定任期及原有空态保持。三个入口只统一布局，不改变查询、权限、激活、投票及字段取值。

提案详情顶部返回与状态同一行居中对齐，两端沿用页面相同边距；投票分类、展示号与机构全称移入业务详情卡片标题区，取代原“xx详情”。分类与展示号沿用对应原详情标题字号，机构全称字号与颜色保持；金额、查询和投票交互不变。

国家储委会、省储委会、省储行、清算行四类入口下的返回按钮统一沿用提案详情顶栏的按钮样式与顶部位置，独立按钮使用相同的16px后续间距；带返回按钮的机构详情取消原-4px顶部偏移，无返回按钮的国家储委会首页保持现状。仅限定上述入口，不改变返回行为及其他Tab。


## 守卫启动与归档合同

节点守卫、宪法守卫的启动自检仅报告诊断，不以panic或构造错误终止节点服务。守卫使用由冻结chainspec在内存构造、按原状态版本核对完整状态根及创世哈希的唯一基准，不从数据库块0取基准，不双读、不回退；冻结输入不写入链库。基准或创世身份不能验证时，入链校验拒绝委派，普通区块、挖矿、预计算变更、完整状态和候选runtime仍须通过对应原生规则。构造阶段继续运行不授予非法状态入链权限。

桌面、CLI及服务装配共同要求state_pruning=ArchiveAll和blocks_pruning=KeepAll，裁剪参数拒绝接受。归档模式从初始化开始保留完整历史；已存在的constrained库与archive不兼容，增大窗口、补参数或修改模式标记不能恢复被裁剪状态。旧库恢复另行确认，不自动清库、切库、重建或重新创世，也不重新回填账户。真实隔离回归必须经过产品参数入口，覆盖超过256个最终块后的早期状态读取及重启，不以测试夹具手写ArchiveAll代替实际默认配置。
