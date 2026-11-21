# CitizenChain Runtime 技术文档

自助换绑call9的SelfRebindAuthorization只含genesis_hash、cid_number、current_account_id、new_account_id、expected_binding_revision；删除自助路径expires_at参数及链上时间检查。原账户仍使用GMB+0x11域，目标账户仍使用原交易CheckNonce、签名和finalized执行。成功后绑定版本递增，旧授权失效；二维码300秒只限制扫码会话，已签出的授权在版本变化前不会因扫码超时自动撤销。注册局CidRebindAuthorization与0x1f期限协议独立保留。

## Runtime 修改与链协议边界

Runtime 账户类型必须使用 `AccountId`；账户、公钥和 SS58 的字段与文本合同统一见 `CitizenChainNode.md` 的“账户标识目标契约”。国家名称的基础字段仅为 `country_full_name`、`country_short_name`；既有语言字段由 `runtime/primitives/src/cid/code.rs` 的真实接口定义，不另建字段真源。行政区、机构名称和管理员集合字段的消费边界见 `CitizenChainOnChina.md` 的“行政区数据”和“CID 号”。

业务凭证的哈希域签名仅使用 `runtime/primitives/src/sign.rs` 的 `primitives::sign::signing_message(op_tag, scale_payload)`，结果为 `blake2_256(GMB ‖ op_tag ‖ SCALE)`；既有 `0x18/0x19` 二进制前缀域按同文件定义执行，不改为哈希域。Pallet 的非签名业务哈希必须使用所属 Pallet 的 `MODULE_TAG` 作域；提案数据归属按本文“MODULE_TAG 注册表”核验。

## Runtime 工程名称

Runtime 所属33个 crate 的目录末级名、Cargo包名、Rust crate名和依赖键一致；名称在本工作空间内唯一。主运行时包名为 `runtime`，产品、节点程序和链身份仍为 `citizenchain`。管理员共用模块为 `adminbase`，实体共用模块为 `entitybase`；各管理员模块使用 `personaladmin/privateadmin/publicadmin`，实体生命周期模块使用 `personalentity/privateentity/publicentity`。

发行模块为 `citizenissue/fullnodeissue/onchainissue/prbbankissue/resolutionissue`；投票子模块为 `electionvote/internalvote/jointvote/legislationvote`。adminbase/privateadmin/publicadmin/election的单源码入口统一为模块根lib.rs，其余当前入口及权重组织见本文“六模块源码组织”。primitives仍是单一crate，china与cid物理平级；其中 `china::citizenchain` 是基金会名称与常量源码模块，不是主运行时crate。

整链创世配置和预设唯一归属 `genesis` crate 的 `runtime/genesis/src/config.rs`，通过 `genesis::config::genesis_config::<RuntimeGenesisConfig>()` 与泛型 `get_preset` 公开入口消费。主Runtime保留FRAME生成的实际汇总配置类型并负责Runtime API装配，Node直接依赖genesis；genesis不反向依赖主Runtime，不复制配置类型或构造实现，不保留旧源码模块、兼容转发或重导出。

工程改名只同步源码身份和引用，保留已部署pallet名称及索引、存储前缀、MODULE_TAG、调用/签名协议、SCALE字段顺序与类型、Runtime版本和冻结创世输入。权重文件中的历史 `--pallet` 参数对应既有链上pallet选择器，不是Cargo包名。主运行时内部WASM输出为 `wbuild/runtime/runtime.wasm`、`runtime.compact.wasm` 和 `runtime.compact.compressed.wasm`；产品附件显示名称仍按原产品规范生成。冻结创世JSON不得用新编译WASM替换。

## Runtime 创世身份 API

ChainIdentityApi::genesis_hash保持API版本1、原方法名和32字节SCALE返回编码。调用时先检查System::BlockHash(0)实际存在，再读取并拒绝零值，最后与冻结的GENESIS_HASH比较；成功返回当前链状态保存的实际哈希，缺失、零值或不一致使API调用明确失败。该API只读，不写入storage，不新增extrinsic、覆盖参数或链身份缓存，也不改变区块执行和既有升级入口。

固定消费SDK的System::finalize在区块哈希裁剪时保留块0，因此超过BlockHashCount后仍核对同一历史身份。GENESIS_STATE_ROOT用于真实创世区块头核验，不与当前运行状态根比较。

## 合约执行依赖与Ethereum网络标识

SDK识别升级前历史缺少方法的准确框架错误，并区分尚未生成首个Ethereum区块的状态。默认全零区块不得写入索引，区块查询与高度接口明确表达尚未生成；不为升级前区块补造Ethereum数据。

Node统一固定消费SDK提交`05e0e9efcd4db73f92be9d0e1f804ee01bda5ac4`，根与QR声明及根锁使用同一固定Git来源。节点近期历史索引与进程内RPC复用现有Runtime及原收费实现。

SDK维护仓为crcfrcn/polkadot-sdk，官方上游为paritytech/polkadot-sdk。产品Cargo声明与原始锁文件固定精确Git提交；只读依赖原件、构建缓存和SDK维护仓分别承担消费、缓存与开发职责，不自动跟随官方分支。

费用兼容实现位于SDK Revive的严格原生分支。固定钱包价格为 NativeToEthRatio / 10^7（至少 1 wei），并在完整性检查要求能整除原生单位；公民链为 1 gwei。估算取资源需求与既有 native_quote 换算的费用 gas 两者最大值；已签 gas 必须同时覆盖资源和实际费，钱包参数修复接受 EIP-1559 上限至少为固定价格且优先费不超过该上限，Legacy/EIP-2930 价格至少为固定价格；合法非零优先费不作为框架 tip 收取。

SDK构建工具固定Node25.2.1、Rust1.97.1（同一对象包含rust-src及wasm32-unknown-unknown目标组件）、Git2.54.0和actionlint1.7.12。公开Workflow独立维护远端输入：官方Ubuntu24.04 x64 Runner已有Clang18.1.3、libclang及CMake3.31.6须实际回读匹配；solc0.8.30、resolc1.0.0和protoc35.0从固定官方原件逐项验真。Git保留HTTPS，libcurl及expat仅安装两份已列明版本/摘要的官方开发包，系统对应运行库及zlib开发输入不匹配即失败，不联网解析其他包。一次准备在源码外本仓独占临时根内，以同对象Cargo读取准确SDK锁及Rust标准库锁，核对官方原件、完整展开清单、逐文件摘要及两份锁文件后交付独立离线目录源。

Git使用官方NO_INSTALL_HARDLINKS安装选项并回读主程序链接数为1；独占普通文件校验保持不变，拒绝时报告准确路径。远端门禁的源码验证区间固定为本仓合同initial_sha至当前push的准确head，覆盖初始化后的全部自有改动；上一轮失败后的工具补修也必须重新验证金额边界相关crate和宿主接口。本机提交区间及远端push事件身份检查保持原规则。

远端Linux x64原生目标固定交付CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS=--cfg=rustix_use_libc，选择原锁rustix已有的官方libc后端，避免其linux_raw内部属性与Rust1.97.1冲突。remoteTools拒绝目标值缺失、误值及全局RUSTFLAGS/CARGO_ENCODED_RUSTFLAGS覆盖；WASM目标仍按既有真实构建路径验证。libc0.2.172和errno0.3.10均在原锁及同一联合闭包内，不改锁或原件、不增加依赖版本。

WASM子Cargo设置自身RUSTFLAGS后，清除父进程CARGO_ENCODED_RUSTFLAGS及CARGO_CFG_RUSTIX_USE_LIBC，防止父原生目标cfg影响子构建后端。Apple宿主链接器、SDKROOT和DEVELOPER_DIR继续由构建环境提供；构建保持离线，临时目录由调用方清理。

citizenchain/Cargo.toml通过工作空间声明pallet-revive，与FRAME组件使用同一锁定SDK来源。Runtime关闭其默认feature，由std及try-runtime传播对应feature。REVM由Revive引入，版本由公民链Cargo.lock固定；SDK仓库锁文件不会自动被消费者继承。

SDK与公民链根工作空间分别声明radium0.7.0的固定Git补丁，来源为https://github.com/paritytech/radium-0.7-fork.git，提交为a5da15a15c90fd169d661d206cf0db592487f52b。Cargo不继承Git依赖仓根patch。

Ethereum ChainId为十进制`2027`（RPC十六进制表达`0x7eb`），后续Ethereum
签名和Revive配置使用同一编号。现有`SS58_FORMAT = 2027`是地址格式前缀，与ChainId
含义和配置分别管理，不能用SS58前缀字段替代ChainId。

公民链已在既有`runtime/primitives/src/core_const.rs`声明独立
`ETHEREUM_CHAIN_ID: u64 = 2027`；Revive配置直接绑定该唯一常量。
原32字节`AccountId`、CID/机构派生、余额账本、原生GMB业务签名及多签授权保持原合同。
Ethereum发送者只采用真实签名恢复的H160，复用官方AccountId32Mapper的
20字节地址加12字节0xEE映射；无新增余额或绑定交易，不能借地址映射接管现有账户。

严格原生模式的已签入口支持Legacy、EIP-2930和EIP-1559，全部要求准确ChainId；
未知类型、无链/错链、非法r/s/y_parity、非规范RLP和尾随数据拒绝。
低s检查仅用于交易入口，不改变ecrecover预编译的ECDSA恢复语义。
nonce沿用System和CheckNonce；上游最大值会回绕到零，严格入口提前拒绝不可递增值。
Ethereum保留标准签名格式，不能把原生GMB业务签名域拼入Ethereum交易。
来源标记SetOrigin的布尔字段不进入编码，仅在验签后由Runtime代码设置；
编码不能伪造EthTransaction来源。
费用仍走Runtime费用接口，未授权的原生业务包装入口继续拒绝。

公民币按原生两位小数记账，单位、发行规则、最低余额与收费规则由现有Runtime实现。金额适配使用能够表达10^16倍率的类型，生产费用通过Runtime已有报价及扣款器处理，不采用默认空费用实现。

FRAME benchmark用于开发阶段测量执行和存储资源，生产使用事先生成的WeightInfo控制
区块资源。当前WeightToFee及LengthToFee均为零，实际金额由OnchainChargeAdapter和
primitives::fee_policy计算，benchmark不会替代公民币制度收费。生产构建不启用
runtime-benchmarks；新增EVM时资源计量与收费适配分别设计。

### 原生与Ethereum金额适配

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

费用适配直接复用现有 `onchain::charge_details` 只读入口，计算函数体不变；
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

### 标准Precompiles及RPC费用边界

Runtime的`Precompiles = ()`仅表示没有自定义业务预编译；官方集合仍组合Builtin，
支持0x01至0x09及P256验证0x100。KZG点求值0x0a仍为明确不支持，不能宣称完整
Cancun预编译支持。

原生业务不增加Precompile或新ABI；任何合约内部Runtime派发仍受既有过滤、权限
和收费路由约束。不能把SDK的System/Storage内置工具当作已授权的公民业务入口。
模拟必须整体回滚账户nonce、余额、存储、事件及费用；实际签名交易继续只在原有
交易扩展扣一次制度费用，内部调用和预编译均不增加手续费、tip、deposit或退款。

Ethereum RPC回执的gasUsed/cumulativeGasUsed只描述资源，不参与GMB扣费计算。
真实费用继续以OnchainTransaction::FeePaid及既有费用查询为真源，倍率10^16、
111分ED、付款人和80/10/10分账保持原合同；非整分金额继续拒绝。

余额查询沿用SDK扣除最低余额和冻结限制后的可用余额，不向原生账本额外补发或增加金额。

Runtime是完整CitizenChain产品的wasm平台组件，受控缓存固定为 `citizenchain/target/wasm/<build|ci|release|publish>/`，不增加旧仓库层、`runs/` 或 `start/`。开发升级下载、冷签会话工作文件和回执属于 `publish/`；Runtime Build 的 Cargo 目标、日志和临时文件属于 `build/`。

## 本机WASM编译入口

本文是 CitizenChain Runtime 唯一技术事实文档，统一收录运行时模块、链上数据、治理、发行、交易、权重与协议边界。

### runtime admins 技术文档

`citizenchain/runtime/admins/` 由四个 crate 组成，管理员链上状态按管理员类型分别保存在各自 pallet。

#### 模块边界

| 模块 | 职责 |
|------|------|
| `adminbase` | 管理员共用类型、生命周期 trait、统一查询 trait 和机构码分类策略；不放业务 storage。 |
| `publicadmin` | 公权机构管理员钱包集合，包括 NRC/PRC/PRB/FRG/NJD 固定治理机构；不保存岗位或 FRG 虚拟省组。 |
| `privateadmin` | 私法人及私权侧/独立非法人机构管理员：公司、协会、私立学校、个体经营、无限合伙等；公法人下属非法人不得被描述为私法人附属类型。 |
| `personaladmin` | 个人多签管理员和个人多签管理员集合变更。个人多签账户生命周期归 `runtime/entity/personalentity`。 |

#### 唯一真源

- 机构管理员集合由 `adminbase::InstitutionAdmins` 表达；storage key 是 `cid_number`，value 只含 `institution_code + admins`。
- 公权、私权机构和个人多签每项统一为 `Admin { account_id, cid_number, family_name, given_name }`。非空 CID 必须是 CTZN 且与 `identity` 的 CID↔账户双向索引完全一致。admins pallet 不复制公民身份真源，也不用占位姓名伪造真实资料。
- 姓、名只展示，账户用于人员识别与签名，但账户本身没有机构业务权限；统一结构不保存岗位、权限、任期或任职来源。字段完整性由机构类型、岗位和个人多签规则分别判定。
- 机构岗位定义和任职关系归 `entity`，与管理员人员集合独立：管理员可以没有岗位，岗位可以空缺；岗位变化不得反向生成、删除或覆盖 admins。
- 个人多签继续由 `personaladmin` 独立管理业务和 storage，但管理员项与机构使用同一个 `Admin` 四字段结构；不使用机构岗位或机构任职关系。
- 各类管理员的链上管理员集合分别保存在各自 pallet 的 `AdminAccounts`。
- runtime 当前通过 `RuntimeAdminAccountQuery` 聚合读取各管理员模块；ADR-039 落地后它只能证明人员属于 admins，业务 pallet 还必须通过 entity 解析完整 `RoleSubject` 的权限和有效任职。
- 机构治理阈值由 public/private entity 的 `InstitutionGovernanceThresholds[cid_number]` 独立保存，与 admins 人数、岗位数解耦；admins pallet 不接收阈值。个人多签继续使用 `internalvote::ActivePersonalThresholds[personal_account]`。任何路径都不建立岗位阈值。
- 创世机构本体、协议账户、机构阈值、默认法定代表人岗位、固定岗位、固定权限和创世任职由 `runtime/genesis/src/institution.rs` 唯一写入：既有公权机构写 `publicentity` / `publicadmin`，公民链技术发展基金会写 `privateentity` / `privateadmin`；不得跨命名空间或从岗位伪造钱包。
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

- 公权机构生命周期由 `publicentity` 发起，只写 `publicadmin`；固定治理机构由创世写入，同样只在 `publicadmin` 承担运行期管理员治理。
- 私权机构生命周期由 `privateentity` 发起，只写 `privateadmin`。
- 个人多签账户生命周期由 `personalentity` 发起，只写 `personaladmin`；管理员更换 call 为 `PersonalAdmins(29).propose_admin_set_change(0)`。
- ADR-039 目标中，公权/私权机构创建必须在同一原子操作中建立 admins、强制 LR、至少一个初始治理岗位及权限、初始任职和初始投票规则；禁止只创建人员名单和空 LR 后赋予管理员临时启动权限。
- 已完成业务把通用机构治理结果交给 entity；entity 只更新岗位、任职和法定代表人，且任职目标必须是该机构既有管理员。
- admins 不接收岗位结果、不解释岗位变化，也不保存任职来源；管理员的新增、删除、换人和姓名更新由独立管理员维护流程处理。
- 机构管理员名册更换不得附带阈值变更；机构阈值只由对应 entity 配置，任职结果同样不能隐式改阈值。
- 国家储委会、省储委会、省储行、国家司法院固定人数；国家司法院岗位为 7 护宪、1 首席、2 次席、5 大法官。
- FRG 在 `publicadmin` 只有一个含 215 个钱包的机构管理员集合；43 个省专员岗位、每岗5人的分组真源在 entity 任职 storage，不存在虚拟省组账户。省域业务只能授权对应省专员 `RoleSubject`，不能授权 FRG 全体 admins。
- 岗位任职不得驱动管理员更换；public/private admins 当前不暴露对外管理员集合变更 extrinsic。
- ADR-039 目标 NodeGuard 保护强制 LR 和创世固定岗位的码、名、固定权限及制度席位；创世机构可以依法增加普通动态岗位，不能再因存在额外岗位而拒绝。固定岗位成员可依法原子轮换。
- 私权创世的公民链基金会固定为一名程伟管理员、三个固定岗位各一席和机构阈值 2；同一程伟钱包同时任职三岗。固定岗位不能改名、停用、增删或扩席，但不能为了满足阈值而伪造多个钱包或降低阈值。
- `publicadmin`、`privateadmin` 没有 `WeightInfo` 和 `weights.rs`；其写入仅由 entity 生命周期内部接口调用。
- 正式创世只接受统一四字段 `Admin` SCALE 布局；旧纯账户、旧三字段记录和 runtime storage migration 均已删除，不保留兼容路径。

#### MODULE_TAG

| 模块 | MODULE_TAG |
|------|------------|
| `personaladmin` | `b"per-mgmt"` |

#### Call Index

| 模块 | 管理员更换 call |
|------|----------------|
| `personaladmin` | `29.0 propose_admin_set_change` |

### entitybase 技术说明

模块：`entitybase`

职责：实体生命周期共用类型与 trait。该 crate 不含 storage、不含 extrinsic、不保存 CID 登记状态。

#### 边界

- 定义 `EntityKind`，区分公权机构、私权机构、个人多签。
- 当前定义 `InstitutionMultisigQuery`，供交易、清算、扫码验签等模块统一查询公权/私权机构账户状态和 admins 人员数据；机构内部/联合投票已改由 `VotePlan` 的 `RoleSubject` 有效任职解析，不得把该查询返回的 CID 全体 admins 用作投票主体。
- 定义 `InstitutionCidQuery`，供 `publicentity` 和 `privateentity` 互查 CID 是否已登记，防止同一 CID 在多个生命周期模块重复写入。
- 定义 `CidInstitutionVerifier`，统一 CID 机构登记、注销凭证验签接口。
- 定义 `InstitutionGovernanceAction` 与 `InstitutionGovernanceProposal`，统一表达本机构内部治理中的 `admins` 完整替换、`InstitutionRoleMutation::{Create,Rename,Delete}`、任职变更和法定代表人结构整体设置或清空；创建岗位不接收 `role_code`，必须原子携带不可变权限和初始任职。
- 定义 `InstitutionGovernanceResult`，承载投票/选举及本机构治理的真实结果；登记call 9复用同一岗位/权限/任职变更结构和实体应用核心，使用Registry来源，不伪造投票结果。创世继续使用原专用初始化入口。
- 已定义 `RoleSubject { cid_number, role_code }`，作为机构业务授权和机构岗位投票资格的唯一主体。
- 已定义 `BusinessActionId { module_tag, action_code: u32 }`、`RoleBusinessPermission { role_subject, business_action_id, operation }` 和 `AuthorizationSubject` 强类型；个人多签使用 discriminant `1` 的独立 `PersonalMultisig(AccountId)` 变体。
- `RolePermissionOperation` 的 SCALE discriminant 固定为 `Propose = 0`、`Vote = 1`；`AuthorizationSubject` 固定为 `Institution = 0`、`PersonalMultisig = 1`。
- Node使用entitybase共享类型解码SCALE；OnChina、CitizenApp和CitizenWallet对同一协议严格解码并拒绝尾随字段。
- 定义 `InstitutionCapabilityPolicy` 与 `InstitutionRoleAuthorizationQuery`，供业务模块校验“CID 顶层能力 + 岗位权限 + 有效任职”；本 crate 只定义 trait，不保存权限 storage、不选择投票引擎。
- `business_action.rs` 是稳定 `module_tag/action_code` 与受保护创世岗位固定权限唯一目录；协议升级与决议发行采用同一联合权限矩阵：NRC/PRC 委员岗位拥有 `Propose + Vote`，PRB 正式 `DIRECTOR / 董事` 只有 `Vote`。该目录不选择投票引擎，也不表示尚未迁移的业务已经按岗位执法。
- CREG 顶层登记业务能力由同文件 `registry_institution_capability_allows` 单源声明，不属于固定创世岗位目录，也不创建岗位、权限或任职。仅允许 `Propose`：`cit-id` 的 0/1/2/3/4/6/7/8、`addr-reg` 的 1/2/3/4、`ins-reg` 的 0；全国地址目录版本 `addr-reg/0`、未列出的动作与 `Vote` 均不在新增范围。
- Runtime 先确认完整 CID 唯一存在于公权 entity，才接入上述目录；私权命名空间、重复 CID、未登记机构均不得借 CREG 类型取得能力。实际业务继续同时校验在册管理员、准确岗位主体、具体权限、有效任职和原有辖区；实名身份与地址限本市，匿名 CID 保持原有全国注册局规则，CREG 不得登记其它 CREG。
- 机构内 `role_code` 与 `role_name` 分别唯一；同名多人属于一个岗位的多个任职席位。一个管理员可以担任多个不同岗位，但同一岗位内不得重复占席。
- 每个机构唯一的 `LR / 法定代表人` 岗位永久存在，任职只能为 0 或 1 人；法定代表人结构与 LR 任职必须原子一致。岗位不保存阈值，阈值属于机构或业务绑定的投票计划。
- 复用 `primitives::multisig` 的账户校验、保留地址、保护地址 trait。

#### 禁止事项

- 不允许在本 crate 增加 storage。
- 不允许把公权、私权、个人多签生命周期状态写到本 crate。
- 不允许恢复单独的 entity-registry pallet。
- 不允许把 `admins`、裸账户、裸 CID 或裸岗位码定义为机构业务授权主体。
- 不允许在本 crate 定义具体业务模块使用哪个投票引擎；该决定归各业务模块。

### personalentity 技术说明

模块：`personalentity`

职责：个人多签账户生命周期。只负责个人多签创建、关闭和投票终态回调中的业务
pending 处理。

#### 边界

- 个人多签管理员真源归 `personaladmin`。
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
  engingcore 终态回调自动完成，不存在人工清理交易。

### privateentity 技术说明

模块：`privateentity`。职责：私权法人、非法人机构、CID 下账户集合、岗位和任职真源。

#### 唯一模型

- 机构唯一主键是 `cid_number`；`Institutions[cid_number]` 保存机构信息。
- `InstitutionAccounts[(cid_number, account_name)]` 是账户正向真源，`AccountRegisteredCid[address]` 只是反向索引；不存在 CID 到账户的重复正向表、生命周期状态或默认账户标志。
- 普通机构必须具有主账户、费用账户；特殊制度账户统一由 `primitives::institution_constraints` 决定。协议账户恰好一个且永久不可关闭，只有 `InstitutionNamed` 可关闭。
- 新机构创建业务必须把全部协议账户以零余额原子建立；后续入金另走账户交易。

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

- call 5 已永久关闭并从 metadata/QR/钱包解码移除。普通机构创建由机构创建业务模块原子提交 admins、完整零余额协议账户、强制 LR、至少一个初始治理岗位及固定权限、初始任职和初始投票规则；不得恢复旧直接创建载荷。
- `update_institution_info`（call 6）：注册局管理员更新目标机构名称。
- `add_institution_account`（call 7）：注册局管理员给目标 CID 新增自定义账户。
- `propose_institution_governance`（call 8）：本机构指定岗位任职人发起内部治理提案；SCALE 在 `actor_cid_number` 后固定编码独立 `proposer_role_code`。入口校验完整 `RoleSubject + pri-mgmt/3 + Propose`，再按同一 CID 拥有 `Vote` 权限的岗位构造内部 `VotePlan`。通过后可原子替换 `admins`、变更动态岗位/任职、整体设置或清空法定代表人结构；岗位任职来源必须是 `InstitutionGovernance`，不得伪装成普选、互选或任命结果。
- `register_institution_admins`（call 9）：注册局管理员按注册局授权直接完整替换目标机构 `admins`，用于注册局管理路径，不改岗位任职。
- `propose_close_private_institution`（call 1）：严格使用 `actor_cid_number + proposer_role_code + institution_account + origin`；只有拥有 `pri-mgmt/2 + Propose` 的有效岗位任职人可发起，投票主体来自拥有对应 `Vote` 权限的岗位，并校验账户属于该 CID 且为自定义账户。
- `apply_institution_governance_result` 是内部回调。call 0、call 4 与 call 5 永久留洞，不复用、
  不兼容；关闭提案否决、超时或执行失败后的 `InstitutionPendingClose` 只由
  engingcore 终态回调清除，不存在人工清理交易。

#### 费用与 ED

- 资料更新、新增账户、本机构治理、注册局直接登记管理员和关闭提案的外层链上操作费只从 `actor_cid_number` 的费用账户收取，管理员钱包只签名。
- 自定义账户关闭通过后，执行手续费按被关闭账户余额计算并从 actor CID 费用账户收取；被关闭账户以 `AllowDeath` 转出余额。收费、转账和索引删除原子执行。
- 普通支出与费用扣款必须保留 ED；只有明确关闭的 `InstitutionNamed` 账户允许死亡。

#### 边界与 ABI

- 只接受私权法人和非法人机构码；跨 namespace CID 重复校验通过 `entitybase::InstitutionCidQuery`。
- ADR-039 目标外层 origin 必须属于 admins，并对目标 `RoleSubject` 有有效任职和业务权限；注册局凭证只作背书，不构成第二授权。
- 本模块不实现投票或转账；投票归 engingcore，机构账户转账归 `multisig`。
- 本模块作为 entity 不决定具体业务使用哪个投票引擎；该决定必须静态写在对应业务模块。
- pallet index：`31`；`MODULE_TAG = b"pri-mgmt"`。
- 不兼容旧 storage、旧 call payload、旧解码或旧创世数据。

### publicentity 技术说明

模块：`publicentity`。职责：公权机构、CID 下账户集合、岗位目录与管理员任职真源。

#### 唯一身份与存储

- 机构唯一主键是 `cid_number`；机构码只从 CID 解析，主账户不得作为身份或管理员 key。
- `Institutions[cid_number]` 保存机构信息；`InstitutionAccounts[(cid_number, account_name)]` 是账户正向真源；`AccountRegisteredCid[address]` 仅作反向索引。
- 不保存机构/账户生命周期状态、默认账户标志、CID 到账户的重复正向表或创世保护旁路。
- 普通机构强制主账户和费用账户；特殊机构由 `primitives::institution_constraints::required_protocol_account_kinds` 返回完整协议账户集合。每种协议账户恰好一个且永远不可关闭，只有 `InstitutionNamed` 自定义账户可关闭。
- 逻辑账户允许零余额；新机构创建业务必须把全部协议账户以零余额原子建立，后续入金另走账户交易。

#### 管理员、岗位与授权

- `PublicAdmins::AdminAccounts[cid_number].admins` 是机构可任职人员名册，每项为统一的 `Admin { account_id, cid_number, family_name, given_name }`。非空 CID 必须引用 `identity` 的 CTZN CID↔账户绑定。该名册不是机构业务授权真源，主账户、费用账户和管理员账户均不能单独授权。
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

- call 5 已永久关闭并从 metadata/QR/钱包解码移除。普通机构创建由机构创建业务模块原子提交 admins、完整零余额协议账户、强制 LR、至少一个初始治理岗位及固定权限、初始任职和初始投票规则；不得恢复旧直接创建载荷。
- `update_institution_info`（call 6）：注册局管理员更新目标机构名称。
- `add_institution_account`（call 7）：注册局管理员给目标 CID 批量新增自定义账户。
- `propose_institution_governance`（call 8）：本机构指定岗位任职人发起内部治理提案；SCALE 在 `actor_cid_number` 后固定编码独立 `proposer_role_code`。入口校验完整 `RoleSubject + pub-mgmt/3 + Propose`，再按同一 CID 拥有 `Vote` 权限的岗位构造内部 `VotePlan`。通过后可原子替换 `admins`、变更动态岗位/任职、整体设置或清空法定代表人结构；岗位任职来源必须是 `InstitutionGovernance`，不得伪装成普选、互选或任命结果。
- `register_institution_admins`（call 9）：注册局管理员按注册局授权直接完整替换目标机构 `admins`，用于注册局管理路径，不改岗位任职。
- `propose_close_public_institution`（call 1）：账户型交易，严格使用 `actor_cid_number + proposer_role_code + institution_account + origin`；只有拥有 `pub-mgmt/2 + Propose` 的有效岗位任职人可发起，投票主体来自拥有对应 `Vote` 权限的岗位，只允许关闭该 CID 下自定义账户。
- `apply_institution_governance_result` 是内部回调，不是 extrinsic。
- call 0、call 4 与 call 5 永久留洞，不复用、不兼容；关闭提案否决、超时或执行失败后的
  `InstitutionPendingClose` 只由 engingcore 终态回调清除，不存在人工清理交易。

#### 费用与 ED

- 资料更新、新增账户、本机构治理、注册局直接登记管理员和关闭提案的外层链上操作费只从 `actor_cid_number` 的费用账户收取，管理员钱包只签名。
- 自定义账户关闭通过后，执行手续费按被关闭账户余额计算并从 actor CID 费用账户收取；随后仅被关闭账户以 `AllowDeath` 把余额转给受益人。收费、转账和账户索引删除处于同一事务。
- 普通支出和费用账户扣款都必须保留 ED；只有显式关闭的 `InstitutionNamed` 账户允许死亡。

#### 模块边界

- 只接受公权机构码；跨公私权 CID 重复校验通过 `entitybase::InstitutionCidQuery`。
- 创世机构本体、协议账户、岗位和初始任职由 `runtime/genesis/src/institution.rs` 写入相同真源并校验完整账户集合。
- 本模块不实现投票或转账；投票统一归 engingcore，机构账户转账归 `multisig`。
- 本模块作为 entity 只提供岗位、权限、任职和机构生命周期真源；具体业务模块决定动作权限、静态选择投票引擎并执行通过后的业务。
- 关闭执行器必须处于 engingcore callback scope，并重新校验提案 owner、CID、账户归属、已绑定 `RoleSubject` 授权、协议账户不可关闭与受益人。

#### ABI

- pallet index：`30`
- `MODULE_TAG = b"pub-mgmt"`
- 不保留旧 storage、旧 call payload 或旧解码兼容；开发期重新创世。

#### 1. 模块职责

`genesis` 只负责：

- 保存 `Genesis` / `Operation` 链阶段；
- 保存开发者能否直接升级 runtime 的一次性开关；
- 在 block#0 写入创世宣言、国家宣言和创世人口；
- 调用 runtime 注入的机构 seeder 写入创世机构、固定岗位、固定岗位权限、创世任职和管理员钱包集合。

本模块不提供 extrinsic，不保存 PoW 出块时间，也不向节点提供出块时间 Runtime API。
PoW 六分钟是 `primitives::pow_const::POW_TARGET_BLOCK_TIME_MS` 固定的难度调整平均目标，
与链阶段无关；有效工作量证明找到后立即出块，没有最短等待或最晚期限。

##### 创世机构源码组织

```text
runtime/genesis/src/
├── lib.rs          # pallet定义、阶段与接口
├── config.rs       # 接收实际Runtime类型的唯一泛型创世构造器
├── institution.rs  # 机构、账户、岗位、任职及管理员集合写入
├── roles.rs        # 原固定岗位、席位与钱包索引映射，不写storage
└── weights.rs      # 原生产权重定义
```

- 岗位协议常量来自 `primitives::governance_skeleton`，钱包来自既有 `CHINA_*` 常量；
- 构建前断言固定钱包数量等于席位总数，且固定岗位钱包不得重复；
- 全部机构必须写入唯一 `LR / 法定代表人` 岗位；岗位可以没有任职，公开 `legal_representative` 结构可为 `None`，不得从管理员首位、机构主账户或其它钱包推导；
- 后续依法任命法定代表人属于 entity 运行期流程，不属于 genesis 职责。

私权创世机构“公民链技术发展基金会”是 SFGY 非营利法人，也是唯一在创世明确携带法定代表人的例外：

- 基金会简称“公民链基金会”，英文全称 `CitizenChain Technology Development Foundation`，英文简称 `CitizenChain Technology Foundation`；CID `GZ018-SFGYR-201206100-2026`，主账户 `0xaa23304c7b663ba25a9d3a2fb1efafdd650ecf2504a2caedc228fe81b46b4333`，费用账户 `0xe4837d50cd5ef677c9b10ea49baf8c7d60cf11b422d748377e9e5f750640e927`；
- 法定代表人为程伟，公民 CID 引用 `CN220-CTZN2-198805200-2026`，法定代表人账户 `0x0cb1d05c0c9c7f05679b60d6f24c7e5719a3985264e41c5e899d4822dca4b06b`（model B //0）；创世不伪造第二份公民记录，后续由对应注册局依法从链上公民真源核验；
- 只创建一条程伟管理员人员记录 `Admin { account_id, cid_number, family_name, given_name }`；同一账户分别任职 `LR / 法定代表人`、`GENESIS_PRODUCT_MANAGER / 创世产品经理`、`GENESIS_PROGRAMMER / 创世程序员`，每岗一席。三岗的岗位码、岗位名和岗位权限永久固定，但基金会仍可增加普通动态岗位；
- `PrivateAdmins::AdminAccounts`、三项 `PrivateManage` 岗位/任职和 `PrivateManage::InstitutionGovernanceThresholds[cid_number] = 2` 在同一创世构建中写入。阈值 2 是机构阈值，不与唯一管理员钱包数量绑定；
- 基金会身份、主/费用协议账户以及三个固定岗位治理骨架受 NodeGuard 保护，成员依法轮换必须通过同一原子治理结果同步更新 admins 和岗位任职，不能裸改其中一侧；NodeGuard 不得禁止新增普通动态岗位。

#### 1.1 创世固定岗位权限

- 创世 seeder 必须为全部创世固定岗位写入固定 `RoleBusinessPermission`，不能只写岗位码、名称和席位。
- 创世 admins 只作为可任职人员集合，不直接取得业务权限；业务权限必须来自固定岗位的有效创世任职。
- 全部创世固定岗位使用既定固定码；动态岗位码生成使用所属 pallet 的 `MODULE_TAG` 作哈希域，但 genesis 不用动态算法替代固定码。
- 普通机构不由 genesis seeder 创建；其运行期创建必须原子建立 LR、至少一个初始治理岗位及权限、任职和投票规则。

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

#### 4. 公共接口

模块只保留：

```rust
pub trait DeveloperUpgradeCheck {
    fn is_enabled() -> bool;
}
```

`upgrade` 使用该接口选择当前允许的升级授权路径。旧 `GenesisPalletApi`、
`TargetBlockTime` trait、`TargetBlockTimeChanged` 事件以及未被调用的阶段事件已经删除。

#### 5. 创世固定真源

固定值来自 `runtime/primitives/src/genesis.rs`：

- `CITIZENS`：创世宣言；
- `COUNTRY`：国家宣言；
- `GENESIS_CITIZEN_MAX = 1_443_497_378`；
- `GENESIS_ISSUANCE = 14_434_973_780_000` 分。

`runtime/genesis/src/config.rs` 把前三项写入 `GenesisConfig`。NodeGuard 使用相同的节点编译期
真源重新构造 RAW key 和 SCALE 值，不信任 runtime metadata、getter 或 Runtime API。

#### 6. NodeGuard 执法

`node/guard/src/phase.rs` 在四条路径执行：

1. 节点启动：读取 block#0 的整个 `GenesisPallet` 前缀，确认创世事实和缺省阶段状态；
2. 普通区块：三个创世事实和 StorageVersion 任何触碰都拒绝；
3. runtime 升级：只接受两字段唯一原子单向转换，并在 `:code` 后复核完整状态；
4. 完整状态导入：整个 pallet 前缀进入共享单遍分区，未知 key、缺失值、错误 SCALE、
   尾随字节和非规范状态全部拒绝。

#### 8. 文件索引

- `citizenchain/runtime/genesis/src/lib.rs`：类型、存储、创世构建和开发者升级查询；
- `citizenchain/runtime/genesis/src/roles.rs`：89 个公权受保护创世机构的固定岗位、席位和钱包索引映射；
- PRS、NLG、NSN、NRP、NSP、NED 六个国家级单例在 block#0 写精确机构身份、制度账户和唯一空缺 `LR / 法定代表人` 岗位，不写成员岗位、任职、admins 或动态阈值。首次组成前先独立登记 admins；其中 NSN、NRP、NED 还受法定成员岗位与人数区间约束。
- `citizenchain/runtime/genesis/src/institution.rs`：公私权创世机构、岗位、任职和管理员钱包唯一写入方；公民链基金会写入 `privateentity` / `privateadmin`，不污染公权目录；
- `citizenchain/runtime/primitives/src/genesis.rs`：三个创世事实的固定真源；
- `citizenchain/runtime/genesis/src/config.rs`：真实 runtime genesis patch；
- `citizenchain/node/guard/src/phase.rs`：节点独立永久规则。

#### 1. 模块职责

`grandpakey` 只负责国家储委会（NRC）和 43 个省储委会（PRC）各自节点的
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

- `citizenchain/runtime/governance/grandpakey/src/`

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

节点后端 `node/grandpakey/src/request.rs` 提供：

- `build_grandpa_key_change_request`
- `submit_grandpa_key_change`
- `get_grandpa_key_change_status`

节点页面位于 `node/frontend/grandpakey/`。管理员选择目标机构委员任职，
输入本机解锁密码，完成管理员交易签名后提交。页面明确区分：

- 正常更换：无投票、旧新私钥双签、延迟生效；
- 紧急恢复：旧私钥不可用、目标机构委员内部投票。

非秘密的待处理状态保存为 `<app_data>/grandpa-key-change.json`；GRANDPA 私钥只保存
在节点 `gran` keystore，不写入该状态文件、日志或前端。

#### 0. 功能需求
`destroy` 的功能需求是：为国家储委会、各省储委会、各省储行提供“机构自有资金销毁”治理流程，由拥有该业务权限的岗位有效任职人发起、岗位快照选民投票，在提案通过后自动或手动执行链上销毁。

模块必须满足以下要求：
- 仅允许 NRC、PRC、PRB 发起销毁提案；`actor_cid_number` 是机构唯一身份，`institution_account` 是具体执行账户，链端必须用账户正反索引验证二者归属一致。
- 发起权限必须按完整 `RoleSubject(actor_cid_number, proposer_role_code)` 校验；投票资格必须来自该业务 `Vote` 权限岗位的有效任职快照，不得由全体 `admins` 派生。
- 销毁金额必须大于 0，且执行时必须保证机构账户保留最小余额 `ED`。
- 提案投票通过后，系统应自动尝试执行销毁；若自动执行失败，提案保持 `STATUS_PASSED`，允许后续手动重试执行。
- 自动执行和手动重试走同一业务绑定：callback scope、owner、内部投票 kind/stage、机构码、机构账户、CID 和销毁 action 必须全部一致；执行时再次确认业务机构仍为 NRC、PRC 或 PRB。
- 自动执行失败不能回滚已通过的投票结果。
- 销毁执行通过 `Currency::slash` 减少机构账户余额与总发行量，实现链上销毁。

#### 1. 模块定位
`destroy` 是"机构资金销毁治理执行"模块，负责：
- 发起机构销毁提案（内部投票提案）。
- 在提案通过后执行销毁（自动尝试 + 手动重试）。

该模块不实现投票计票逻辑，投票由 `engingcore` 承担。
提案数据、元数据、活跃提案限额均由 `engingcore` 统一管控。

代码位置：
- `<本仓根>/runtime/governance/destroy/src/lib.rs`

命名说明：
- 当前Cargo包名、Rust依赖名和Runtime pallet名分别为 `destroy` / `resolution_destroy` / `ResolutionDestroy`。
- 模块位于 `citizenchain/runtime/governance/destroy/`。
- `pallet_index = 13`、call index 与 `MODULE_TAG = b"res-dst"` 保持不变。

---

#### 2. 上下游关系与 Runtime 接线
上游常量（机构、机构阈值、投票时长）：
- `<本仓根>/runtime/primitives/src/count_const.rs`
  - `NRC_ADMIN_COUNT = 19`
  - `PRC_ADMIN_COUNT = 9`
  - `PRB_ADMIN_COUNT = 9`

投票引擎依赖：
- `<本仓根>/runtime/votingengine/engingcore/src/lib.rs`
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
- `<本仓根>/runtime/src/configs.rs`
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
无。提案数据、元数据、活跃提案列表均已移至 `engingcore` 统一管控（lib.rs:103 注释说明）。

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

签名账户必须属于提案创建时 VotePlan 任一完整岗位主体的 `VoterSnapshot`，权限和重试次数由 `engingcore` 统一校验；仅当提案已 `STATUS_PASSED` 且存在 retry state 时可重试执行。用于“提案已通过但自动执行失败（如余额不足）”后的后续重试。

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

---

#### 10. 运维建议
1. 监控 `DestroyExecutionFailed` 事件，出现后优先补齐机构余额，再由提案任一冻结岗位 `VoterSnapshot` 成员调用 `VotingEngine::retry_passed_proposal`（pallet 9.4）。
2. 若 3 次手动执行仍失败，或超过 `ExecutionRetryGraceBlocks` 无人处理，提案会由投票引擎统一转 `STATUS_EXECUTION_FAILED`。

##### 0.1 模块职责
`upgrade` 负责把"Runtime wasm 升级"包装成一个受治理约束的链上流程，核心要求是：
- 仅允许 NRC 和 43 个 PRC 的 `COMMITTEE_MEMBER / 委员` 岗位有效任职账户发起升级提案，仅属于 admins 不构成授权。
- 升级提案必须先经过 `engingcore` 的联合投票。
- 联合阶段由 `VotePlan` 固定绑定 NRC + 43 PRC 委员岗位和 43 PRB `DIRECTOR / 董事` 岗位；有效任职账户用个人钱包直接上链投票，链上按机构阈值形成机构结果。PRB 董事只有投票权，没有提案权。
- 联合投票通过后才允许执行 `set_code`。
- 开发期直升通道只允许国家储委会管理员使用，并且必须受 `DeveloperUpgradeEnabled` 开关约束。
- 投票结果、执行结果必须在链上可追踪。

##### 0.2 提案创建需求
- 提案必须携带非空升级理由 `reason`。
- 提案必须携带非空 wasm `code`。
- 创建提案时同步在 `engingcore` 创建联合投票，使用投票引擎统一分配的 `proposal_id`（本模块不维护独立 ID）。
- 本模块不接收、不生成、不校验人口快照、联合签名、投票资格和计票数据；这些都属于 `engingcore`。

##### 0.3 联合投票回调需求
- 联合投票拒绝时，投票引擎状态保持 `STATUS_REJECTED`。
- 联合投票通过时，模块必须尝试执行 runtime code。
- 联合投票结束后，投票引擎侧状态保持真实业务结果：执行成功写为 `STATUS_EXECUTED`，否决保持 `STATUS_REJECTED`，执行失败写为 `STATUS_EXECUTION_FAILED`。
- 回调直接使用投票引擎的 `proposal_id`，无需映射反查。
- 回调还必须校验 callback scope、`ProposalOwner`、联合 kind、`STAGE_JOINT/STAGE_REFERENDUM`，并复算 `ProposalObject` 中 runtime code 的哈希与提案摘要一致；任何一项不符都不得执行 `set_code`。

##### 0.4 执行失败处理
- 若联合投票已通过但 `set_code` 执行失败，投票引擎状态进入 `STATUS_EXECUTION_FAILED`。
- runtime wasm 不再内嵌在摘要结构里，而是统一存入 `engingcore::ProposalObject`。
- 执行成功、拒绝、执行失败后，wasm 对象继续保留到投票引擎 90 天延迟清理统一删除，不由业务模块手工删除。

##### 0.5 可审计与运维需求
- 需要区分以下事件：提案创建、联合投票终结、升级执行成功、升级执行失败。
- 投票引擎侧状态机：
  - `VOTING → PASSED → EXECUTED`（投票通过且执行成功）
  - `VOTING → REJECTED`（投票拒绝）
  - `VOTING → PASSED → EXECUTION_FAILED`（投票通过但执行失败）
- 所有终态均为不可逆（无重试、无取消）。

#### 1. 模块定位
`upgrade` 是"协议升级治理编排模块"，负责：
- 接收 NRC/PRC 委员岗位有效任职账户提交的 wasm 升级提案；
- 调用 `engingcore` 创建联合投票；
- 在联合投票回调后执行 `set_code`；
- 摘要数据存储在 `engingcore` 的 `ProposalData`；
- 原始 wasm 对象存储在 `engingcore` 的 `ProposalObject`；
- 本模块零本地存储。

代码位置：
- `runtime/governance/upgrade/src/lib.rs`
- `node/upgrade/src/`
- `node/frontend/upgrade/`

命名说明：
- 当前Cargo包名、Rust依赖名和Runtime pallet名分别为 `upgrade` / `runtime_upgrade` / `RuntimeUpgrade`。
- 模块位于 `citizenchain/runtime/governance/upgrade/`。
- `pallet_index = 12`、call index 与 `MODULE_TAG = b"rt-upg"` 保持不变。

节点侧边界：
- node 后端 `runtime_upgrade` 只负责读取 wasm、构建协议升级 call data、生成签名请求、提交签名交易。
- node 前端 `upgrade` 只负责协议升级页面交互和签名流程。
- `developer_direct_upgrade` 属于开发者动作，node 端不提供任何入口，只由塔塔控制台冷签发起；链端 call 与 QR 登记必须保留。
- node 的 `runtime_upgrade` 不获取人口快照、不接收联合签名上下文、不拥有投票引擎状态、不展示投票终态。
- 协议升级提案详情展示真实状态时必须以 `VotingEngine::Proposals.status` 为准，`upgrade` 摘要里不保存业务状态字段。

citizenapp / citizenwallet 边界：
- citizenapp 的 `governance/upgrade` 不发起协议升级提案，不选择 WASM，不获取人口快照，不提交 `propose_runtime_upgrade`。
- citizenapp 只展示协议升级介绍、协议升级提案详情，并保留现有提案详情页投票入口。
- citizenwallet 公民钱包不恢复 upgrade SCALE decoder；大 WASM 交易继续走哈希直签例外，由用户核对显示字段中的代码哈希。

#### 2. 运行时接线
Runtime 配置位置：
- `runtime/src/configs.rs`

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
- `WeightInfo = upgrade::weights::SubstrateWeight<Runtime>`

说明：
- `finalize_joint_vote` 手工 extrinsic 已删除，call index `1` 保持空缺。
- 正常生产路径只能由投票引擎通过 `JointVoteResultCallback` 自动回调本模块，避免 Root 手工回放形成第二条执行入口。

##### 3.1 Proposal（摘要，序列化存入 engingcore ProposalData）
- `proposer: AccountId`：提案发起人（NRC 或 PRC 委员岗位的有效任职账户）
- `reason: BoundedVec<u8, MaxReasonLen>`：升级理由
- `code_hash: Hash`：升级 code 哈希，便于事件与链下审计对齐

说明：
- `Proposal` 只保存业务展示所需摘要，不保存投票状态。
- 协议升级真实状态只能读取 `engingcore::Proposals.status`。

##### 3.2 对象层数据（统一存入 engingcore ProposalObject）
- `kind = 1`：表示 runtime wasm 对象
- `object_len`：wasm 字节长度
- `object_hash`：对象哈希
- `object bytes`：原始 wasm 字节（对象层上限 10MB，业务自身继续限制 5MB）

##### 3.3 模块标识
- `MODULE_TAG = b"rt-upg"`：存入 ProposalData 的前缀，用于区分不同业务模块，防止跨模块误解码。

#### 4. 存储模型
本模块只保留一项本地审计，其余提案数据、投票数据、元数据均存储在 `engingcore`：
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
- 仅在 `genesis` 的 `DeveloperUpgradeEnabled` 为 `true` 时可用。
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

当前实现与 `engingcore` 的协作关系如下：

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

Runtime 层的 `RuntimeJointVoteResultCallback` 负责路由：先尝试 `resolutionissue`，再尝试 `upgrade`。

##### 7.1 执行结果与状态

业务执行返回ProposalExecutionOutcome，由投票引擎统一记录结果：通过且执行成功写STATUS_EXECUTED，否决保持STATUS_REJECTED，通过但执行失败写STATUS_EXECUTION_FAILED。查询层以engingcore的Proposal.status为准，业务摘要只保存proposer、reason及code_hash。

##### 7.2 WASM对象与容量

ProposalData仅保存摘要，Runtime WASM存入投票引擎ProposalObject，通过create_joint_proposal_with_data_and_object原子创建，不提供对象覆写入口。摘要上限100KB，对象层上限10MB，升级业务MaxRuntimeCodeSize为5MB。

##### 7.3 执行权重

propose_runtime_upgrade权重计入机构岗位、有效选民快照及ProposalVotePlans。终结执行成本由投票引擎联合终态回调路径承担，不提供独立finalize_joint_vote权重入口。
##### 7.4 回调入口

升级终结仅通过投票引擎callback进入，不提供手工Root终结入口。

#### 10. 文件索引
- 模块代码：`src/lib.rs`
- Benchmark：`src/benchmarks.rs`
- 权重：`src/weights.rs`
- 技术文档：`RUNTIMEUPGRADE_TECHNICAL.md`

#### 用途

MODULE_TAG 是各业务模块在 `engingcore` 的 `ProposalData` / `ProposalOwner` 中写入的字节标识。投票引擎本身不解析提案数据内容，但会用 `ProposalOwner` 做 owner 校验，禁止跨模块覆写。各模块在读取时仍需校验前缀或独立存储键，防止误解码。

#### 适用场景

MODULE_TAG 仅用于共享 `ProposalData` 存储的模块。若模块使用独立 `StorageMap` 存储提案动作数据，则不需要 MODULE_TAG。

#### TAG 注册表

| MODULE_TAG | 字节值 | 所属 Pallet | 数据格式 |
|------------|--------|------------|----------|
| `b"multisig"` | `[109,117,108,116,105,115,105,103]` | `multisig` | `MODULE_TAG + TransferAction (SCALE)` |
| `b"pub-mgmt"` | `[112,117,98,45,109,103,109,116]` | `publicentity` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"pri-mgmt"` | `[112,114,105,45,109,103,109,116]` | `privateentity` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"per-mgmt"` | `[112,101,114,45,109,103,109,116]` | `personaladmin` | `MODULE_TAG + ACTION_CODE (1 byte) + payload (SCALE)` |
| `b"res-iss"` | `[114,101,115,45,105,115,115]` | `resolutionissue` | `MODULE_TAG + IssuanceProposalData (SCALE)` |
| `b"res-dst"` | `[114,101,115,45,100,115,116]` | `destroy` | `MODULE_TAG + DestroyAction (SCALE)` |
| `b"rt-upg"` | `[114,116,45,117,112,103]` | `upgrade` | `MODULE_TAG + Proposal (SCALE)`; 大对象另存 ProposalObject |
| `b"gra-key"` | `[103,114,97,45,107,101,121]` | `grandpakey` | `MODULE_TAG + KeyReplaceProposal (SCALE)` |

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
3. `publicentity` / `privateentity` 在 TAG 后增加 1 字节 ACTION_CODE 区分 create/close/其他操作
4. `upgrade` 的 runtime wasm 大对象通过 `ProposalObject` 单独存储，ProposalData 中仅存摘要

---

### Governance 目录说明

本目录用于承载 CitizenChain runtime 下的治理相关 pallet 与文档。
当前治理相关 crate 已统一放在本目录下，后续新增治理 pallet 也必须直接落在这里。

#### 1. 定位

`citizenissue` 是公民**首次上链**认证奖励模块。模块不提供外部交易，只接收
`identity` 的 `OnVotingIdentityRegistered` 回调。

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

1. `identity` 校验注册局权限、公民钱包签名、CID 和居住地作用域，写入投票身份；
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

`citizenchain/node/guard/src/citizen.rs` 使用 RAW storage key 和节点本地 SCALE
镜像，不读取 runtime metadata。它检查：

- 创世只能包含 FRAME 规范空状态：存储版本 0、两个计数的精确零值；
- 父状态不得残留待发队列，finalize 前队列必须连续且不超过本块 extrinsic 数；
- 身份必须首次以 `VotingIdentityByCid` 出现，队列中的完整 CID、`AccountIdByCid` 与 `CidByAccountId` 必须双向一致；
- 永久和临时双重防重、人数边界、档位金额必须逐项一致；
- finalize 后队列必须清空，永久标记和累计数必须精确推进；
- 未登记的 `CitizenIssuance` key 变化、收款账户变化或总发行变化一律 fail-closed。

#### 9. CID防重键

IdentityRewardClaimed与PendingIdentityRewardClaimed使用完整规范cid_number为键；账户永久及临时防重表同时保留，任一键重复即拒绝奖励。

#### 0. 功能需求
`fullnodeissue` 的功能需求是：在固定的 PoW 奖励区块高度区间内，按照制度常量为成功出块的全节点作者发放固定金额奖励，并允许矿工自行管理奖励接收账户。

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
`fullnodeissue` 是一个 FRAME pallet，用于在 PoW 链上按固定制度发放全节点铸块奖励。

核心目标：
- 奖励规则常量化（金额、起止高度写死在 `primitives::pow_const`）。
- 奖励触发客观化（仅基于区块高度 + 共识层 `FindAuthor`）。
- 发放可审计（发放均有链上事件）。
- 矿工自主管理奖励接收账户（支持未出块预绑定 + 重绑，无需治理）。

代码位置：
- `<本仓根>/runtime/issuance/fullnodeissue/src/lib.rs`

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
- `fullnodeissue` 不直接使用 `sp-std`，Cargo 依赖保持最小化，避免为未使用 crate 传播额外 feature。

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
- `bind_reward_account` 为 1 次读取 + 1 次写入，生成权重为 `6_000_000` ref time、
  `3545` proof size，再加 1 次数据库读取与 1 次写入。
- `rebind_reward_account` 为 1 次读取 + 1 次写入，生成权重为 `7_000_000` ref time、
  `3545` proof size，再加 1 次数据库读取与 1 次写入。
- `on_finalize` 的预算通过 `on_initialize` 的 `reads_writes(5,7)` 预申报，包含三个审计字段的读写。

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
代码位置：`node/src/rpc.rs` → `reward_bindAccount` / `reward_rebindAccount`

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

#### 11. 奖励账户约束

bind_reward_account允许未出块账户预绑定，绑定表只在真实成为区块作者时读取，不授予出块资格。矿工身份账户不能同时作为奖励接收账户。rebind不设冷却期，但必须切换到不同的新奖励账户；重复绑定当前账户拒绝。on_finalize的奖励预算由on_initialize预申报。
##### 手续费划转准入与岗位权限

国家储委会、省储委会和省储行均使用 `propose_sweep_to_main`。发起机构 CID 与转出的费用账户必须匹配。收款主账户由链上按同一 CID 查询。省储委会由 `COMMITTEE_MEMBER` 岗位发起和投票，达到 6 张赞成票后执行。

固定岗位目录为新创世写入省储委会划转的发起权和投票权。现有链通过 `runtime/src/configs.rs` 注册的 `SingleBlockMigrations` 补齐 43 个固定省储委会的存量权限。升级钩子先检查全部目标岗位和权限容量，再追加缺失项并记录完成标记。原有权限和其他机构记录保持原值。

国家储委会两和基金通过 `propose_transfer` 转账。链上核对两和基金账户属于发起机构，两和基金账户支付本金，同一 CID 的国储会费用账户支付执行手续费。

##### 3. try_execute_sweep（内部方法）

1. 校验提案状态为 `STATUS_PASSED`
2. `InstitutionAsset::can_spend` 检查（action = `OffchainFeeSweepExecute`）
3. 计算手续费：`calculate_onchain_fee(amount)` — 费率 0.1%，有最低值
4. **余额检查**。划转本金与执行手续费扣除后，费用账户余额至少保留链上 ED，当前为 111 分（1.11 元）。
5. **本金上限**。`amount <= (fee_balance - ED) * 80 / 100`，按整数分向下取整。
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
| `InvalidInstitution` | 机构非 NRC/PRC/PRB，或转出账户与发起 CID 的费用账户不匹配 |
| `UnauthorizedAdmin` | 稳定错误码：调用者不是所提交岗位的有效任职人，或该岗位没有 sweep 提案权限 |
| `SweepProposalNotFound` | proposal_id 无对应记录 |
| `SweepProposalNotPassed` | 提案未通过 |
| `InsufficientFeeReserve` | 余额不足以覆盖划转+手续费+保留 |
| `SweepAmountExceedsCap` | 超过可用余额 80% 上限 |
| `InstitutionSpendNotAllowed` | 资产保护检查未通过 |

### 扫码支付清算接口

---

#### 1. 接口范围

L3提供绑定清算行、充值、提现及切换的链上接口；清算行合法性由CidAccountQuery判断，支付意图使用独立签名结构。批次上链的资金处理由settlement模块执行。

#### 2. 源码组成

```
src/
├── batch_item.rs    # PaymentIntent 结构 + 签名域常量
├── bank_check.rs    # 清算行合法性判定 + CidAccountQuery trait
├── deposit.rs       # bind/deposit/withdraw/switch 四函数实现
└── nonce.rs         # L3PaymentNonce 消费辅助
```

#### 3. 清算行合法性模型

清算行 = `K1=S` 私法人或 `K1=F` 非法人(两者皆私权机构),对应 `citizenchain/onchina/platform/codes/category.rs` 的 `InstitutionCategory::PrivateInstitution`。

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
5. `CidAccountQuery::is_clearing_bank_eligible(bank_main)` 通过。OnChina系统负责 `eligible-search` 候选筛选,链上不再保存 `subject_property/sub_type/parent_cid_number` 元数据,这里只确认账户属于已注册且 Active 的 CID 机构账户
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

// 默认 () 实现返回未登记。
```

**runtime 层实现**（`citizenchain/runtime/src/configs.rs` 的 `MultisigCidAccountQuery`）：按公权、私权机构 CID 聚合账户索引：
- `PublicManage::AccountRegisteredCid` / `PrivateManage::AccountRegisteredCid` → `account_info`
- `PublicManage::InstitutionAccounts[(cid_number, account_name)]` / `PrivateManage::InstitutionAccounts[...]` → `find_account`、`account_exists`、`is_clearing_bank_eligible`
- `RuntimeInstitutionAdminQuery` 以 `AdminAccounts[cid_number].admins` → `is_institution_admin`
- `ClearingBankNodes` → `is_registered_clearing_node`

**好处**:
- offchain-transaction 的 Cargo.toml 不新增实体生命周期 pallet 依赖
- pallet 的 `Config` trait 不强制 `T: publicentity::Config` 或 `T: privateentity::Config`

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
| `UserBank` | `StorageMap<L3, InstitutionCidNumber, OptionQuery>` | L3 → 绑定的清算行机构 CID |
| `DepositBalance` | `StorageDoubleMap<InstitutionCidNumber, L3, u128>` | 权威账本:各 L3 在该清算行的存款(分) |
| `BankTotalDeposits` | `StorageMap<InstitutionCidNumber, u128>` | 清算行存款总额(偿付对账) |
| `L3PaymentNonce` | `StorageMap<L3, u64>` | L3 单调递增 nonce(防重放) |

不变式:`BankTotalDeposits[bank_cid] == Σ DepositBalance[bank_cid][*]`。
偿付自动保护:`清算账户链上余额 ≥ BankTotalDeposits[bank_cid]`。

#### 7. 新增 Call(call_index 30-33)

| call_index | 方法 | 费用归类 | 说明 |
|---|---|---|---|
| 30 | `bind_clearing_bank(bank_cid)` | 签名者链上操作 0.1 元 | 绑定即开户无预存 |
| 31 | `deposit(amount)` | 链上资金 0.1% 最低 0.1 元 | 自持 → 清算行清算账户 |
| 32 | `withdraw(amount)` | 链上资金 0.1% 最低 0.1 元 | 清算行清算账户 → 自持 |
| 33 | `switch_bank(new_bank_cid)` | 签名者链上操作 0.1 元 | 旧行存款为零，目标为不同且合格的清算行 |

费用与付款方由 `citizenchain/runtime/src/configs.rs::RuntimeFeeRouter` 的 `OffchainTransaction` 分支显式返回统一 `FeeRoute`，不走兜底。

#### 8. 新增 Event(4 个)

```
BankBound      { user, bank_cid }
Deposited      { user, bank_cid, amount }
Withdrawn      { user, bank_cid, amount }
BankSwitched   { user, old_bank_cid, new_bank_cid }
```

清算行切换时，旧行存款非零返回MustClearBalanceFirst，绑定与余额保持不变且不发切换事件。提现清零后删除旧账项，在新行建立零余额账项，更新UserBank并发出BankSwitched。

#### 9. 新增 Error(19 个)

清算行身份类:`NotRegisteredClearingBank` / `NotMainAccount` / `NotPrivateInstitution` /
`ClearingBankNotActive` / `FeeAccountNameTooLong` / `FeeAccountNotFound`

绑定/切换类:`AlreadyHasBank` / `NoOpenedBank` / `NewBankSameAsCurrent` /
`MustClearBalanceFirst`

存取类:`DepositAmountTooSmall` / `WithdrawAmountTooSmall` /
`InsufficientDepositBalance` / `InsufficientBankLiquidity` / `DepositForbidden` /
`WithdrawForbidden`

L3 签名类:`L3NonceOverflow` / `InvalidL3Nonce`

#### 10. `institution-asset` 新增 4 个 Action

```
L3DepositIn      # L3 充值入清算行主账户
L3WithdrawOut    # 清算行主账户对 L3 提现
L2ClearingDebit  # 扫码清算时扣 payer_bank
L2FeeCollect     # 扫码清算时向 fee_account 收费
```

`()` 默认 fail-open,runtime 的 `RuntimeInstitutionAsset` 会按"清算行主账户是否合法"严格裁决。

### 批次清算与资金保护

---

#### 1. 实现组成

费率管理、偿付保护和批次结算分别由fee_config.rs、solvency.rs及settlement.rs承载，批次项结构归batch_item.rs。
##### 2.1 `fee_config.rs`

清算行费率自治:
- `L2_FEE_RATE_BP_MIN = 1` / `L2_FEE_RATE_BP_MAX = 10`(bp)
- `RATE_CHANGE_DELAY_BLOCKS = 1_680`（7 × 240 块；按六分钟平均目标换算，不承诺自然日最晚出块）
- `do_propose_l2_fee_rate(who, actor_cid_number, institution_account, new_rate)`:按 CID 校验管理员和主账户归属后写 `L2FeeRateProposed`
- `do_set_max_l2_fee_rate(new_max)`:设全局上限
- `activate_pending_rates(now)`:`on_initialize` 调用,搬到期提案到 `L2FeeRateBp`
- `current_rate_bp(bank)`:查当前生效费率(供 settlement 查收款方费率用)

##### 2.2 `solvency.rs`

偿付能力自动保护:
- `ensure_can_debit(bank_main, debit_fen)`:校验扣款后主账户余额仍 ≥ `BankTotalDeposits`
- `solvency_ratio_bp(bank_main)`:返回偿付率(万分之)供监控用

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

清算行管理员真源不再由业务账户表镜像保存，所有内部投票和清算权限统一读取 `admins` 分类模块。

##### 3.1 Storage

```rust
L2FeeRateBp<Bank, u32>                           // 当前生效费率
L2FeeRateProposed<Bank, (u32, BlockNumber)>      // 待生效提案
MaxL2FeeRateBp: StorageValue<u32>                // 全局上限
LastClearingBatchSeq<Bank, u64>                  // 已成功落账的最新批次序号
```

`LastClearingBatchSeq` 与 batch 级签名一起启用。`submit_offchain_batch`
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

清算行 pallet 已从裸 `T::DbWeight` 和空 `WeightInfo`
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
- `Onchain { transaction_amount, payer }`：基础费用为 `max(round(amount × 0.1%), 0.1 元)`；仅普通链上转账的非空备注在现有收费入口另加一次 `FLAT_FEE`。机构普通操作和多签建案传零金额，所以操作费固定为 0.1 元。多签转账非空备注在既有执行收费入口另收一次 FLAT_FEE，执行失败随原业务事务回滚。
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

- `Onchain`：使用现有基础公式；仅普通 `transfer_with_remark` 非空备注在现有 `charge_details` 中额外收取一次 `FLAT_FEE`，从路由中的 `payer` 扣完整费用。备注长度不影响附加费；多签建案的外层操作费仍按原规则，备注费由实际执行时的既有收费入口收取。
- `Vote`：从路由中的 `payer` 扣固定 `FLAT_FEE`。
- `Offchain` / `Free`：外层不扣款。
- `Reject`：返回 `InvalidTransaction::Call`。

`FLAT_FEE` 是唯一100分定额费定义，投票和普通、多签转账非空备注共用。metadata、SDK各语言费用快照和访问接口均统一使用 `FLAT_FEE`，单位在注释中注明为分。节点前端构建期读取同一定义，不新增运行时查询。节点费用守卫使用父块metadata识别旧、新收费规则；历史旧名称仅用于历史回放，新规则生效后禁止通过Runtime升级退回旧名称。SDK费用快照只改字段名，C ABI大小和偏移不变；Runtime、节点、SDK和App需配套更新。

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
- `citizenchain/node/src/rpc.rs`：`fee_blockFees` 只累计 `FeePaid.fee`，不再拼接 FRAME tip 事件。
- `citizenapp/lib/rpc/signed_extrinsic_builder.dart`：热签 payload 和 extrinsic 固定 `tip=0`。
- `citizenwallet/lib/signer/payload_decoder.dart`：冷签前拒绝非零 tip。
- `citizenchain/onchina/`：机构身份仍只使用 CID、机构账户和 `admins`；不建立费用付款方缓存或第二路由表。

### Transaction 目录说明

本目录用于承载 CitizenChain runtime 下的交易相关 pallet 与文档。
当前交易相关 crate 已统一放在本目录下，后续新增交易 pallet 也必须直接落在这里。

补充：

- `institution-asset` 虽然不是 pallet，但属于交易资金边界的公共 crate，也统一放在本目录。

#### 定位

`engingcore` 是链上中国 runtime 的统一投票引擎。

全部投票流程只归投票引擎。业务模块只提交提案语义并调用既定投票接口，不得实现、复刻、绕过或内嵌投票流程，不得自行处理人口快照、投票资格、联合签名、状态推进、计票、通过判定或清理状态机。

机构Track按VotePlan中的完整岗位主体冻结资格，不以CID全体admins作为发起或投票资格；个人多签使用独立管理员主体。

#### 内部投票与业务权限

- `internalvote` 是机构岗位主体与个人多签共用的投票程序；两类授权主体必须用 `AuthorizationSubject` 强类型分离。
- 共享 `VotePlan` 的 SCALE 字段顺序固定为 `business_action_id`、`proposal_owner`、`proposer_subject`、`voter_subjects`、`voting_engine`、`business_object_hash`；最多绑定 256 个投票主体。
- `VotingEngineKind` discriminant 固定为 `Internal = 0`、`Joint = 1`、`Election = 2`、`Legislation = 3`。构造器强制 owner 与 module tag 相同、投票主体不重复，并禁止机构岗位与个人多签主体混用；只有 Popular 选举允许 voter subjects 为空，因为其选民资格来自人口快照，其他引擎一律要求非空岗位/个人主体。
- 机构内部投票和联合投票创建时都必须携带完整 `VotePlan`；核心引擎一次性写入 `ProposalVotePlans`，要求 `ProposalOwner` 与 plan 相同，拒绝重复绑定。个人多签使用独立 personal 创建接口和 `AdminSnapshot`，不得伪造机构岗位主体。
- 目标机构提案由业务模块先校验 `RoleSubject(cid_number, role_code)` 的 `Propose` 权限，并静态选择唯一投票引擎、绑定 `VotePlan`。引擎不得接受调用方选择引擎，也不得把“属于 admins”当业务准入。
- 机构投票资格按 VotePlan 中一个或多个 voter `RoleSubject` 的有效任职账户建立不可变快照；不得自动快照该 CID 的全体 admins。个人多签仍按 personal_account 的管理员集合快照。
- `VoterSnapshot[(proposal_id, RoleSubject)]` 保存每个岗位主体的有效任职账户；`InstitutionTicketCountSnapshot[(proposal_id, cid_number)]` 冻结该机构岗位席位票据总数。同一账户兼任多岗时按 `RoleSubject + account_id` 分别行使各岗位票权，不按账户合并，也不改变机构阈值。
- 投票引擎只负责快照、资格、票据、阈值、计票、终态、重试和清理；业务合法性、业务对象绑定及通过后的具体执行归对应业务 pallet。
- `multisig` 转账允许已登记机构账户和个人多签账户；机构调用显式携带 `actor_cid_number + proposer_role_code + institution_account`，反向索引只校验账户归属，业务模块再校验完整岗位主体权限。个人多签必须同时携带 `actor_cid_number=None`、`proposer_role_code=None`。
- `destroy` 只允许 NRC、PRC、PRB 对应固定岗位；`grandpakey` 只允许 NRC、PRC 委员岗位。业务限制和业务动作权限不得下沉到 `internalvote`。
- FRG 是一个 CID 机构并拥有多个协议账户和 215 名管理员；省域 5 人岗位组属于注册业务权限，目标投票和发起资格必须解析对应省专员 `RoleSubject`，不能把 FRG 全体 admins 纳入。
- 协议升级与决议发行都固定使用联合投票：NRC/43 个 PRC 委员岗位可发起和投票，43 个 PRB `DIRECTOR / 董事` 岗位只投票；岗位只提供资格主体，不能选择引擎或自带岗位阈值。
- 联合投票引擎还会校验 proposer 是 plan 中 NRC/PRC 委员主体的有效任职账户，并要求 plan 精确覆盖 44 个 CHINA_CB 委员主体和 43 个 CHINA_CH 董事主体；缺失、多出、重复或跨类型主体均 fail-closed。

##### 机构阈值与提案快照

- 岗位没有阈值 storage，也不得把岗位数或 admins 钱包数误作阈值；`VotePlan` 只决定哪些岗位任职席位进入快照。
- public/private entity 的 `InstitutionGovernanceThresholds[cid_number]` 是机构治理阈值真源。`internalvote` 在建案事务中通过 runtime provider 读取并写入 `InternalThresholdSnapshot[proposal_id]`；建案后机构配置变化不得改变既有提案阈值。
- 投票引擎不再保存机构 Active/Pending 阈值表，也不通过管理员更换隐式改阈值。个人多签继续使用 `ActivePersonalThresholds[personal_account]`，不受机构阈值重构影响。
- 固定创世机构的阈值由 genesis 写入 entity，不是岗位阈值。公民链基金会只有一个程伟管理员钱包，但三个固定岗位各一席、机构阈值仍为 2；同一钱包的三项岗位任职已形成三张独立岗位票据。

#### 统一机构管理员登记

- 公权/私权继续使用已有 `register_institution_admins` call 9，不增加CREG专用调用或固定岗位。新SCALE顺序统一为 `cid_number + action + actor_cid_number + actor_role_code + expected_state_hash:[u8;32] + initial_threshold:Option<u32>`；`action`复用已有 `InstitutionGovernanceAction`。只接受带管理员名册的ReplaceAdmins及ReplaceAdminsAndMutateRoles，登记不能夹带法定代表人任免；call 8仍只允许本机构治理本机构。
- 登记前通过原RegistryAuthority校验办理人、actor岗位和目标辖区，再比对授权状态指纹。`registration_state_hash`以所属MODULE_TAG为域，按顺序编码目标CID、机构信息、规范管理员账户集合、按岗位码有序的岗位/权限/任职、现有nonce及阈值。余额不参与；每次成功名册写入同时推进现有InstitutionRoleNonce，包含人员资料变化，使旧快照/重复请求失效；不增加流水表或初始化状态storage。StaleRegistrationState要求刷新并重新审阅签名，不自动覆盖并发变更。
- 第一次登记以InstitutionGovernanceThresholds缺失判断，必须显式提交动态岗位和初始阈值；默认空LR岗位不能当作已初始化。名册、权限、任职、nonce和阈值在同一个storage transaction提交，任何后续校验失败全部回滚。阈值按具有本机构治理Vote权限的有效岗位席位计数，至少两席且严格过半，并须有有效Propose岗位；不从管理员钱包数量推导。
- 新增或改任职的Registry来源只接受空的输入assignment_source_ref，由Runtime绑定真实已签名登记载荷（含origin、状态哈希和初始阈值）的引用；存量任职只有人员、任期、状态、来源和引用逐字段与原记录相同时才可保留原来源，追加人员不重写原有任职事实；拒绝伪造来源或来源字段。登记动态岗位使用该引用、MODULE_TAG和现有nonce生成R_码；治理继续使用真实proposal_id及原生成公式，不使用0冒充登记提案。公权/私权共用既有角色校验与原子写入核心，各自入口保持来源边界。
- 初始化后登记只更新名册和显式任职，不允许重新提交初始阈值或创建/改名/删除岗位；LR及受保护创世岗位的任职只能透传原记录，不能借登记接口改任；岗位定义与权限变化继续通过机构自身治理。未在变更中列出的岗位保持原状，删除人员必须同时处理其现有任职，否则InvalidAssignmentResultAdmins并回滚。
- 权重覆盖当前快照与登记变更的岗位/权限/任职数量，并按输入编码大小计入额外工作；管理员写入新增nonce读写也计入回调权重。当前权重由原FRAME实测名册成本作保守组合并显式计入新增读写。

#### 登记编码与冷签协议

- OnChina现有公私权call 9编码器改为统一typed action及actor、32字节expected_state_hash、Option<u32> initial_threshold，字段顺序与Runtime一致；没有新增调用、CREG固定岗位或storage。共享action枚举及治理call 8布局不变。
- 读侧同区块读取原始机构记录、Option名册账户锚点、有序岗位/权限/任职、nonce和阈值，经entitybase共享函数生成与Runtime相同的指纹；不使用当前钱包展示投影、余额或中文来源标签生成签名状态。存量二进制来源引用原样保留，新/改任职只传Registry空引用。
- prepare比对页面已审阅指纹并按登录管理员同区块有效任职、权限和既有辖区确定办理岗位。唯一候选自动确定，多候选选择中文岗位名；未知/失效岗位拒绝。来源及引用由系统从链状态确定，不由输入指定。签名会话的actorCidNumber修正为实际办理注册局CID，保持现有Passkey和统一submit流程。
- 钱包严格解码登记子集并从签名字节展开岗位权限、初始任职、名册、状态锁及显式首次阈值。对旧编码、伪造来源、配置不匹配、LR任免、截断、尾随及公私权动作错配均拒绝。

#### 注册局详情与登录办理

- 现有新增/删除使用统一call 9；目标直接来自当前详情页，关闭旧本地目录增删。链上完整名册、原账户锚点和当前签名钱包分别表达，本地目录不再充当名单或人数真源。
- 首次配置明确动态岗位、业务权限、逐人任职和机构阈值；票据按有效岗位席位计算，同一钱包兼任多岗可形成多席，不生成固定岗位或按钱包人数推导阈值。后续追加不重写原名册/CID/资料/任职来源；删除必须同步处理相关任职并维持有效治理配置。
- 确认前复查状态指纹，唯一办理岗位自动确定、多岗位中文名称选择。一次Passkey与一次钱包签名后只提交原会话；重复点击在组件层同步拦截。HTTP成功仍须回读finalized目标名册与任职；已消费或清理的会话只有链上目标状态吻合才恢复为成功，网络不确定时不盲目重签。既有登录链上守卫和CREG顶层业务能力/真实任职校验继续沿用。

#### 机构管理员登记资源预算

统一表单使用公权/私权call 8自治提案和call 9注册局登记。prepare由实际call参数生成cid_number上下文；自治提交成功只表示提案入链，名册在投票执行后更新。前端拒绝会清空存量CID的有损自治提交，call 9保留原记录。

登记权重以原模型为下限，追加28494000000ps成本、每人4次读取及10333字节身份证明，覆盖办理方与目标方名册，首次配置写入按4r计入。参数n为累计调用work，p/a为累计权限及任职条目，r*p和r*a须换算后计费。每人证明上界由四表最大编码433字节及Trie上界9900字节组成。该保守预算不保证最大逻辑名册可在单个区块证明预算内提交。

#### 公民身份真源

投票资格和参选资格统一通过 `CitizenIdentityReader` 读取 `identity`：

- `voting_subject(who, scope)`：账户在当前作用域内有投票资格时返回完整 `CitizenSubject`。
- `candidate_subject(who, scope)`：账户在当前作用域内有参选资格时返回完整 `CitizenSubject`。
- `population_data(scope)`：人口日期完整就绪时返回作用域、人口分母、资格 revision 和
  护照判定日期；日期尚未推进完成或身份模块处于人口维护故障时返回 `None`。该数据只能
  由 `identity` 产生，裸 `population_count()` 接口已经删除。
- `voting_subject_at(who, population_data)`：按人口数据中的 revision 和日期查询永久 CID 身份历史，并返回 CID + 当前签名钱包的完整主体。

`identity` 是人口数据和身份历史唯一真源，但不生成、不编号、不保存提案快照。
`engingcore::create_population_snapshot(proposal_id, scope)` 遇到 `None` 返回
`PopulationDataNotReady`，不写入任何快照；取得完整数据后才写入
`ProposalPopulationSnapshots[proposal_id]`。投票、分母读取和 90 天清理都只围绕该
proposal_id 快照进行。

OnChina 本地数据库只能用于注册局录入和界面提示，不能作为链上投票资格真源。

资格读取返回 `CitizenSubject { cid_number, account_id }`。联合公投、立法公投和 Popular 选举均以 `(proposal_id, cid_number)` 唯一去重，票据值保存完整主体；同一永久 CID 更换绑定账户后不能再投一票。候选快照和当选结果保存完整主体，候选计票表以候选 CID 为唯一键；当前绑定账户只用于签名授权与审计。人口快照仍只保存作用域、有效总数、资格 revision、判定日期和创建区块，不枚举公民名单。

#### 人口作用域

`PopulationScope` 支持四级：

- `Country`
- `Province(province_code)`
- `City(province_code, city_code)`
- `Town(province_code, city_code, town_code)`

联合公投和立法特别案在创建提案的同一存储事务内调用
`engingcore::create_population_snapshot(proposal_id, scope)`；引擎从 `identity`
读取人口数据并在自身 storage 形成不可变提案快照。联合公投固定使用
全国作用域；立法特别案只从 `actor_cid_number` 的合法 CID 和法定机构码推导国家、
省或市作用域。任一后续写入失败时快照与提案一起回滚，不存在公开准备入口、待消费
快照、客户端作用域参数、独立 snapshot_id 或第二人口真源。

#### 联合投票

- 内部阶段：`JointVote::cast_admin(proposal_id, institution, approve)`。
- 联合公投阶段：`JointVote::cast_referendum(proposal_id, approve)`。
- 联合公投按 `proposal_id + cid_number` 去重，`CitizenReferendumTicket` 保存完整公民主体和票值。
- 联合公投资格由 `CitizenIdentityReader::voting_subject_at(who, population_data)` 返回；提案创建后新增、迁居或被撤销的当前身份均不能改变已有提案的成员集合。
- 公投分母与成员资格来自同一个投票引擎提案快照，快照数据只由 `identity` 提供；累计票数达到该分母后拒绝继续写票，参与率不得超过 100%。
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
- `legislation` 的新法、修法、废法动作分别固定为 `leg-yuan/0,1,2`；发起调用必须显式携带 `proposer_role_code`，业务模块按 `actor CID + 岗位码 + action + Propose` 校验后固定选择立法投票引擎。
- 当前创世只强制创建各机构唯一 LR；NRP/NSN/NED/PRP/PSN/CLEG/CEDU/CSLF 的成员岗位不在创世预造，必须由机构以后依法创建。建案时业务模块从链上岗位权限中解析准确代表岗位并写入 VotePlan。
- 行政签署和国家/省级三人会签固定使用相关机构 LR，护宪终审固定使用 NJD `CONSTITUTION_GUARD`；全部按建案时岗位任职快照判定，不按机构全体管理员快照判定。护宪岗位不是 LR，且必须在建案时恰好冻结 7 个不重复账户。
- `legislation` 在创建提案和投票通过写入前分别复核一次法定路由：固定院序、发起机构、行政签署机构、会签机构、active 账户和 CID 行政区必须全部一致。客户端携带的路由字段不是授权真源。
- 法律范围索引不设置每组1000部法律的固定数量上限。`LawsByScope` 保留原存储前缀、`tier + LawRegion` 键与原哈希及列表SCALE编码，使用普通列表与官方存储追加接口；列表值编码保持。宪法 `Laws[0]`、版本、不可修改条款manifest及 `LawsByScope[宪法][0] == [0]` 保持，剩余错误编码不因删除超量错误而移动。

##### 固定框架

```text
legislationvote/
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

- `electionvote` 统一承载普选、互选的提案、选民/候选快照、投票、计票、结果快照和清理。
- `term_start`、`term_end` 使用自纪元起 `u32` 天，不使用区块高度表达法定任期。
- `electionvote` 只产生不可变当选结果快照，不解释职位、席位、任期或目标机构业务规则，也不得构造 `InstitutionGovernanceResult` 直写 entity。
- 普选/互选底层创建 extrinsic 已物理删除；当前外部只保留 `cast_popular_vote` 与 `cast_mutual_vote`。
- 真实创建必须由 `runtime/public/` 下对应的具体选举业务模块校验本机构发起岗位、目标 `role_code`、候选人、选民范围、席位和任期后调用引擎；结果也必须先回到原具体业务模块复核，再由业务模块调用 entity 任职入口。
- `runtime/public/election` 为公民选举公职人员的业务模块，复用 pallet index 32；当前仅占位，不提供交易入口、业务存储或事件。后续逐步实现具体选举业务规则并接入 `electionvote`，本次不恢复已删除的开发期通用选举实现。
- 机构只能发起本机构岗位选举。最终元数据只保留 `actor_cid_number + role_code`，发起岗位、互选岗位和被选举岗位的 CID 必须相同。
- 提案实例只使用投票引擎生成的全链唯一 `proposal_id`，业务类型由 `BusinessActionId` 表达；不得保留无权威规则表支撑的通用规则编号。

##### 资格真源与快照边界

- 普选必须使用 `identity` 的 `PopulationScope`、完整公民主体资格、`population_data` 和快照时资格查询；投票引擎按 proposal_id 保存不可变人口数据，不接收、不枚举、不保存全国/省/市/镇完整选民列表。
- 互选属于机构岗位业务；目标选民必须来自业务模块 VotePlan 指定的 voter `RoleSubject` 有效任职快照。调用方不得提交或删减选民集合。
- `electionvote` 创建入口按 `ElectionMode` 强制检查资格来源：Popular 必须有人口作用域，Mutual 必须取得已绑定岗位主体快照。
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

- `votingengine/engingcore/src/lib.rs` 只保留 pallet 配置、存储、事件、错误、hooks 和 lifecycle extrinsic。
- `expiry.rs` 承载到期索引、自动终结、有限退避和 dead-letter。
- `execution.rs` 承载异步业务执行、重试期限与管理员恢复入口。
- `lifecycle.rs` 承载状态迁移、终态副作用、回调作用域和统一事件。
- `maintenance.rs` 承载有界票据清理和提案对象清理步骤。
- `tracks.rs` 定义单 Track 生命周期接口和递归 tuple 派发；核心不匹配具体 mode/stage。
- `traits.rs` 只作为稳定 re-export 门面；实际 trait 按 engines、providers、callbacks、finalizers、cleanup 分组。
- `internalvote` 的 proposal、threshold、vote、cleanup 各自独立；`legislationvote` 的代表表决、公投、签署、护宪、结果和清理实现均位于对应真实模块。
- 生产源码单文件不得超过 800 行，不得用纯注释文件或空实现伪造职责边界。

#### 公开超时与提案隔离

- 立法签署、三人会签和护宪终审的公开超时入口统一要求当前区块严格大于提案截止区块，防止任意账户提前终结提案。
- 自动到期结算失败不再回插原到期桶；使用独立计数、指数退避和 dead-letter，确定性错误不会永久阻塞执行重试和历史清理管线。
- 立法特别案公投统一调用 `primitives::constitution::referendum_passed`，不再保留重复数学实现。
- 注册多签待激活动态阈值按 `proposal_id` 隔离，避免同机构并发注册提案互相覆盖。

#### 异步执行与终态维护

- `on_initialize` 在到期桶尚未排空时不再提前返回，执行重试、终态清理和 90 天清理管线每块都能继续获得有界处理机会。
- 投票判定与业务执行已通过 `PendingProposalExecutions` 解耦；队列按 weight 预算执行，错误指数退避并在三次失败后 dead-letter，既有手动重试和终态清理契约已完成适配。
- `finalize_proposal` 承担投票判定与执行入队；系统 `set_code` 成本显式计入业务执行和手动重试预算。投票、清理及回调预算使用各自实测模型组合。
- `legislationvote` 的 signing、guard、referendum、result、cleanup 文件已承载实际规则或清理辅助，不再是纯注释残桩。

#### Track与维护调度

- Runtime 以 `(InternalVote, (JointVote, (LegislationVote, (ElectionVote, ()))))` 注册递归 Track tuple；手动超时、自动超时、模式清理和内部阈值副作用走同一类型路由。
- 自动终结、业务执行、历史清理保留各自配置上界，但共同使用一个维护预算池，按区块轮换优先次序；生产总池为最大区块权重的 `3/4`，不再把三条独立预算相加。
- 延迟 FIFO 与就绪 FIFO 均以单调 `u64` 序号键控，不存在单区块桶容量、顺延窗口或严格头部反复处理。

#### 资格快照与执行重试

- `identity` 以全局 `eligibility_revision` 和每账户不可变版本历史冻结创建时资格；同一区块多次身份写入也能确定顺序，账户查询按版本数二分定位。
- 联合公投、立法公投和 Popular 选举统一写入 `ProposalPopulationSnapshots[proposal_id]`；`identity::PopulationSnapshots/NextSnapshotId`、核心 `ProposalPopulationSnapshotIds`、`Proposal.citizen_eligible_total`、`ReferendumScopes`、`LegislationMeta.referendum_scope`、Popular 全量选民表及 `MaxElectionVoters` 已删除。
- Popular 不受完整选区人数的 `BoundedVec` 限制；Mutual 只使用核心岗位快照，`MutualVoters` 和 `MaxMutualVoters` 已删除。
- 自动执行结果应用阶段的确定性错误不再每块无限重排；Ignored三次退避后进入dead-letter，孤儿执行队列立即删除。

#### 生产权重与维护成本

- 核心权重由当前真实调用生成；业务执行另组合回调预算、对象实际字节和 `SystemWeightInfo::set_code()`，不把系统换码成本混入可重生的框架模型。
- 内部权重沿用既有实测；联合和立法已按资格历史存储重新生成，公民公投写票均真实计入 snapshot 绑定、快照元数据和账户资格版本读取。
- 选举最后一票按候选人数`c`线性计费：Popular 基础约 `54.4m ps`、Mutual 基础约 `51.1m ps`，两者每候选人
  约增加 `1.94m ps`；Popular 读为 `12+c`、Mutual 读为 `11+c`，写均为 10。
- 人口、岗位及签署基准通过既有 provider 和所属公开接口准备状态，计时内调用真实入口；框架、Track、回调和系统换码成本按各自模型显式组合。
- `ProposalTrackHandler` 同时返回 stage timeout、Track chunk cleanup、Track terminal cleanup 权重；手动终结、自动终结和清理维护均按具体 Track 实际值计账。
- 投票维护池 `3/4`、身份维护 `1/8`、法律 `1/32`、广场 `1/32`，合计 `15/16`，为其它 Hook 保留余量。预算不足保留任务。

#### 岗位主体内部投票

- `InternalVoteEngine` 的机构创建接口强制接收业务模块构造的 `VotePlan`；创建事务按 plan 中每个 `RoleSubject` 读取有效任职、写入 `VoterSnapshot`，并按 CID 累加 `InstitutionTicketCountSnapshot`。机构路径不再写 `AdminSnapshot`。
- `internalvote::cast` 的机构调用必须显式声明 `voter_role_code`，以 `InternalVoteTicket::Institution(RoleSubject + account)` 校验和防双投；个人多签使用 `InternalVoteTicket::Personal(account)` 并读取 `AdminSnapshot`。两类主体没有兼容回落。
- 核心投票引擎不再暴露把机构人员名册写入个人多签快照的入口。机构 `admins` 查询只允许业务模块和 entity 用于确认人员名册归属，不能生成投票资格。
- engingcore Config、runtime 接线 不再存在机构管理员人数 provider。机构阈值只由 entity 提供，岗位席位数只来自提案冻结的票据快照；不得恢复通过管理员人数推导机构计票的第二路径。
- 机构阈值继续来自机构固定阈值或机构动态阈值；岗位只决定选民集合，不新增岗位阈值。创建时若机构阈值无法由本次有效岗位快照达到，整笔提案回滚。
- 手动重试和取消同样按提案主体分流：机构读取有效岗位选民快照，个人多签读取个人管理员快照。
- 已接入业务为 public/private 本机构治理与关闭、决议销毁、GRANDPA 密钥紧急恢复、机构普通转账、NRC 安全基金转账、费用账户划转主账户和公民链基金会平台调价。每个业务自己校验 `RoleSubject + BusinessActionId + Propose`、枚举拥有 `Vote` 权限的岗位并固定使用内部投票引擎。GRANDPA 正常更换由目标机构单个委员完成旧、新私钥双签后直接延迟调度，不进入投票引擎。
- public/private治理、Square调价及原清单其余方法均通过完整调用基准生成，不再使用旧的400ms/700KB手工占位；固定席位记录按共享规格准备最大字段，对应PoV模式在基准中明确标注。

### 公民币订阅边界

- 平台与创作者订阅统一使用链上公民币；CitizenChain 是价格、扣款、状态和真实公历到期时间的唯一真源。
- CitizenApp 只对订阅、取消、换套餐和创作者设置套餐分别签名一次；自动续费由 runtime 内部执行。
- Cloudflare 只保存 finalized 完整交易证明、可重建订阅镜像和创作者展示资料，不计算日期、不触发扣款、不保存未来扣款价格真源。
- OnChina 与 CitizenWallet 只承接公民链基金会平台调价提案的统一签名流程：OnChina 展示请求二维码，CitizenWallet 只签名一次并显示响应二维码，OnChina 回扫响应后提交；投票流程完全复用统一投票引擎。

订阅不包含税务pallet、收入台账或征税规则。

---

### 公民币平台订阅与创作者订阅技术架构

#### 1. 目标

平台订阅和创作者订阅统一使用公民币付款，并复用现有 `SquarePost` pallet。订阅期限和自动扣款都以区块唯一的共识 unix 毫秒时间戳为依据；runtime 使用确定性的 UTC 公历算法计算月、季、年，绝不使用区块高度、固定天数或固定毫秒替代真实日期。

#### 2. 系统边界

| 能力 | 唯一负责方 | 边界 |
|---|---|---|
| 平台价格、创作者付款套餐 | CitizenChain | 每次真实扣款读取当前链上价格 |
| 扣款、收款、订阅状态 | CitizenChain | 首扣和到期自动扣款均在 runtime 原子执行 |
| 真实公历到期时间 | CitizenChain | 从当前区块共识时间戳确定性计算 |
| 自动续费调度 | CitizenChain | 到期后按时间顺序处理；停链期间到期周期在恢复出块后补扣 |
| 平台调价 | 统一投票引擎 | `SquarePost` 只创建提案并接收终态回调 |

CitizenApp 对订阅、取消、换套餐以及创作者覆盖设置自己的套餐分别只签名一次，并展示链上时间戳。Cloudflare 不计算日期、不发起扣款、不扫描全链订阅、不决定价格或到期时间；App、设备和 Cloudflare 是否在线均不影响续费。

#### 3. 链上模块

- 路径：`citizenchain/runtime/misc/square/`
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
    IssuerPaused = 4,
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
    charge_fen: u128,
    paid_until: u64,
    subscription_status: SubscriptionStatus,
    price_fen: u128,
    suspend_reason: Option<SuspendReason>,
}
```

所有时间字段均为 unix 毫秒时间戳。字段顺序即固定 SCALE 顺序，Dart、TypeScript、JSON 金标和 runtime 必须逐字节一致。

- `Active`：授权有效且在续费调度内。
- `Cancelled`：用户签名取消，保留当前已付权益至 `paid_until`。
- `Suspended`：暂停续扣、保留粉丝关系、退出调度，等用户动作恢复；`suspend_reason`
  说明原因（创作者改价待再签名 / 余额不足待充值再签 / CID 当前双向绑定不可用）。
- `IssuerPaused`：创作者掉平台会员，粉丝暂停扣费但**仍留调度**，创作者恢复即自动续。
- `Terminated`：仅显式关闭/清档，不由余额不足或掉会员自动进入。
- `charge_fen`：当前已付周期的计价金额，是剩余权益折算的基准；换档当次实际补扣另由事件 `charged_now` 记录。
- `price_fen`：订阅者已授权用于自动续费的价格，用于创作者改价重签检测，不参与剩余权益折算。

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
4. 每个周期扣款均读取当时最新链上价格；平台治理改价自动按新价续。创作者改价（当前价 ≠ `price_fen`）则**不自动续**，转 `Suspended(NeedReconsent)` 待订阅者再签名。
5. 停链期间无法发生状态变更；恢复出块后按到期顺序补扣所有已到期周期，未完成部分在后续区块继续。
6. 续费失败按原因分流（不再一律 `Terminated`）：余额不足 → `Suspended(InsufficientBalance)`（离调度）；创作者掉平台会员 → `IssuerPaused`（留调度、下周期重试、恢复即续）；档位/周期删除 → `Suspended(NeedReconsent)`；公历换算失效等 → `Terminated`。

续费不需要账户再次签名，不依赖 CitizenApp、设备或 Cloudflare 在线，也不存在任何外部续费提交者。

#### 9. 挂起、取消和换套餐

- 挂起恢复：`Suspended` 由订阅者再签名（改价场景）/充值后再签（缺钱场景）恢复，走 `subscribe` 落到首扣路径；`IssuerPaused` 随创作者恢复平台会员在下周期重试时自动续。
- 取消：写 `Cancelled`，保留已确认的 `paid_until`，到期前已付权益仍有效。
- 换套餐（`change_subscription_plan`）**立即生效并折算**：剩余权益 `y = charge_fen × (paid_until−now) ÷ (paid_until−last_charged_at)`；升档补扣 `新价−y`、新周期从现在起算；降档不扣、余额按新档单价折算成延长时长。
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

#### 13. 存储布局与账户绑定

- 订阅使用CID键布局，不读取、转换或保留AccountId旧键。
- SquarePost不提供storage migration、兼容枚举、旧call或双轨查询。
- 边缘投影必须来源于对应链的真实finalized状态。
- 换绑不改变CID对应的订阅关系，自动续费仅使用当前双向有效绑定账户；CID或双向绑定不可用时停止扣款，不回退旧账户。

#### 14. CitizenApp 与签名体验

- 订阅、取消、换套餐和创作者设置套餐各自使用一笔热钱包标准 extrinsic，并等待 finalized；同一业务操作不得追加第二次账户签名，自动续费没有用户交易或签名。
- CitizenApp提供完整页面流程、finalized 状态读取、真实日期展示和创作者一次签名后边缘镜像重试，不实现续费编排。
- finalized证明包含交易哈希、区块哈希和完整已签名 extrinsic；App 按钱包持久化有限证明历史与待重试队列，镜像失败不重复签名。
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

#### 16. 禁止事项

- 禁止用区块高度、固定天数或固定毫秒表示订阅周期。
- 禁止在 CitizenApp 或 Cloudflare 计算并提交订阅到期时间。
- 禁止外部 renew、周期确认、设备签名或 keeper。
- 禁止跳过停链期间已经到期的周期或恢复旧订阅协议。
- 禁止在业务模块实现投票流程。
- 禁止保留旧字段、旧注释、旧 UI 文案或兼容分支。

---

## 执行注册与空间资源边界

严格原生模式复用SDK RuntimeCosts：日志空间按数据长度加256字节固定结构上界计量，不可变数据计入数据长度和AccountId编码长度。非严格模式不增加该空间量，既有计时权重不变。该预算限制状态增长，不改变GMB费用分类、公式、最低费、付款和分账。区块计算预算60秒，普通交易占75%，proof_size上限8MiB。

Runtime使用官方UncheckedExtrinsic包装器、pallet35及完整原有交易扩展，SetOrigin置于CheckMetadataHash后、WeightReclaim前；原生默认标记编码为空，chain-signing保持原生payload编码。RuntimeNativeFee报价只读复用charge_details，付款预检和实际收费均绑定现有OnChargeTransaction；只新增Ethereum金额严格整分后进入现有链上路由，其余Revive原生业务入口拒绝。区块proof_size设8MiB，时间60秒、普通75%、费用权重和字节价格均保留原值。

Revive benchmark特性传播在原锁中补入pallet-revive到已有rand_pcg0.3.1的一条连接；不新增包或版本，不改checksum及非SDK连接。

交易扩展末两项使用官方TransactionExtension嵌套元组，将SetOrigin和WeightReclaim组合在同一项；顺序仍在CheckMetadataHash之后，避免Rust标准元组Debug/Eq上限。默认标记及隐式数据仍为空编码，原生完整签名载荷编码不变；收费器预检显式指定既有Runtime类型，不改变收费配置或计算。

## MLS公钥登记签名域

`runtime/primitives/src/sign.rs` 中唯一登记常量为 `OP_SIGN_MLS_DEVICE_BIND: u8 = 0x1C`。它授权当前 CID 绑定账户登记本机同一32字节 MLS 公钥，链下验签仍使用 `blake2_256(GMB ‖ op_tag ‖ SCALE)`。载荷顺序为 `cid_number`、`binding_revision u64LE`、`account_id`、`public_key`、`issued_at u64LE`；账户和公钥文本均为小写 `0x` 加64位十六进制。

哈希签名域闭集为16个；删除的编号不重用、不提供别名或兼容常量。链上交易、二进制前缀签名与其他现行授权域保持其既有语义。共享 QR 注册表以动作13承载 MLS 登记，正式导出到钱包和链内消费者；生成文件禁止手改。

## Runtime 自身版本与 WASM 自动化

WASM构建仍不回写原仓Runtime或Node源码。当前macOS正式节点把本次公开DER十六进制交付到既有正式数据目录的rpc-certificate.hex，runtime.mjs按固定位置只读消费，不新增配置项、不读取私钥、不改控制台、Runtime、UI或交互。开发数据命名空间不交付正式证书。脚本核对目录及祖先的所有者、权限、链接和ACL，通过O_NOFOLLOW/O_NONBLOCK打开普通单链接文件，以同一文件描述符检查、读取、前后复核；元数据或路径身份变化立即停止。每轮读链固定一份证书快照，仅装入127.0.0.1本机HTTPS请求的ca，保留有效期及主机名校验、超时与64KiB响应边界，不修改全局信任、不重定向、不降级、不自动换证书重试。证书缺失、损坏、过期、旧会话或权限异常均停止派发；同一finalized块双版本读取与分别加1规则不变。GitHub只接收既有版本基线，不读取公开证书文件。

Runtime正式候选的编译入口为scripts/runtime.mjs build。 工具版本复用scripts/build.mjs的checkTools，进程退出状态不明时仍由Runtime入口保留现场。该文件只处理编译输入、WASM构建、产物检查和当前构建的编译现场；配置只声明数据。wasm.yml编排候选检查、测试、编译、发布和历史清理。共用工作流实现由.github/release.mjs提供，WASM候选校验复用本文件的基线和双版本校验。WASM仍使用链上双版本各加1，显示号取目标spec_version；原Runtime及Node源码不写入，仅临时编译输入的VERSION替换两个数字，冻结创世输入、升级授权、表决与宪法规则保持。

候选输入沿用version、transaction_version、source_sha和runtime_base。readRuntimeTarget只读本机既有公开证书及同一finalized块的冻结链身份、双版本；声明读取不读链、不派发。Workflow的prepare核对准确源码及基线加1关系，编译入口重新检查冻结创世与双版本并核对实际WASM。Runner不连接本机RPC；旧请求构造、launch/read/dispatch回调和配置转调均不保留。

wasm.yml只接收workflow_dispatch，使用prepare、build、release、cleanup四阶段，不再派发repository_dispatch维护Run。原Cargo业务测试由Workflow直接执行，Runtime脚本只接受build；测试范围、固定SDK、原锁和三件runtime.wasm、runtime.compact.wasm、runtime.compact.compressed.wasm保持。正式编译只复制Git跟踪普通文件至target/build/wasm/source，使用官方WasmBuilder编译临时输入，原源码不回写，也不在成品补丁改号。现场清理由编译文件管理，工具退出未确认时保留。

Runtime开发升级仍属于控制台既有功能。该功能仅同步正式WASM读取处的静态配置消费，原冷签、授权、交易构造和实际链提交保持；产品编译和GitHub任务不迁入控制台。正式WASM交付不代表开发升级或服务器节点更新已完成。

### 法律地域固定布局

法律主体、提案摘要、新法调用、LawsByScope第二键与LegislationApi v2统一复用primitives::genesis::LawRegion。字段按原顺序占四字节：province_code:[u8;2]原两位大写字母，city_number:u16小端原省内三位市号的数值；省级city_number=0表示不限定市。不把省码转换为数值，不改变原CID/省市解析。写入入口及投票完成回调将地域与已授权发起机构原CID派生结果核对，路由机构码、归属与表决规则不变。查询只接受0—3层级和匹配地域形状，不将非法层级视为市级。

宪法/国家地域的显式NATIONAL编码为00 00 00 00，与原u32=0逐字节相同，不是解析失败默认值。Laws和LawsByScope存储名称、前缀、哈希算法、宪法原键/值、Law字段顺序及后续偏移不变；创世宪法law_id=0、国家立法院归属、众议会发起、众参两院表决和全部冻结条款保持。节点守卫继续按原u32=0冻结布局读取，无需修改。当前只有创世宪法，不增加普通法律迁移、兼容分支或第二套索引，不重建创世数据。

transaction_version是框架RuntimeVersion字段，交易通过CheckTxVersion核验。scripts/runtime.mjs依据真实部署基线生成候选双版本，不能由仓库静态版本推断运行节点版本。

Runtime产物校验由runtime.mjs读取三份实际WASM的runtime_version段，支持SDK原压缩格式并沿用其50MiB解压边界。WASM段长接受u32宽度内合法LEB128补零，保留超长、溢出、截断和段越界拒绝。三个产物必须属于citizenchain且双版本与当前构建的目标一致，不从文件名或Tag推断。runtime.mjs的recordAssets记录完整集合及原始字节摘要，publish读取同一Run的三份实际附件并复核双版本、字节和来源后创建草稿及正式交付；下载回读保持。输出version与runtime记录绑定source_sha、genesis_hash、base_block_hash、spec_name、基线及目标双版本，显示号等于目标spec_version，目标双版本分别为基线加1；配置中的原公开命名和证明接口仅转调该文件的WASM校验，不复制实现。正常退出清理临时源码；工具退出未确认时保留现场。

WASM历史维护直接归原Workflow的cleanup，不派发repository_dispatch维护Run。当前Run按已结束的prepare、build、release结果占用一个保留名额，三项全部成功才占成功名额，否则占失败名额；已完成历史按GitHub真实conclusion选择，保留最近成功1条、失败1条。活动Run不删除；旧资产按准确Run、SHA与Tag归属处理，删除后回读Run不存在并核对两类各最多1条，错误不得吞掉。当前构建的最终conclusion由GitHub在cleanup结束后确定，清理失败或外部强制中断不得报告严格两条已完成。

## 包、分组和源码结构

Runtime为总装配包，src和tests归其自身；genesis和primitives为直接子包。admins、entity、governance、issuance、misc、public、transaction、votingengine仅作目录分组；分组不承载Cargo.toml、src、tests或嵌套workspace。votingengine包含engingcore及internalvote、jointvote、electionvote、legislationvote五个包，基础包唯一Cargo/crate/依赖键为engingcore，无旧包别名或转发。产品根workspace仍统一管理33个Runtime所属包。

每包src与唯一tests平级。src默认平铺真实模块文件；仅primitives的cid/china、engingcore的traits、legislationvote的legislation/representative、公权和私权实体的institution保留一层模块目录。genesis的播种和岗位直接归src/institution.rs、src/roles.rs。每个保留目录至少两个真实直接子项，不增加占位文件或包装层。

primitives的cid和china源码分别归src/cid与src/china，逻辑CID协议及现有机构常量保持单一真源。gen_area_data.py和rederive_accounts.py沿用原维护工具，只同步路径。冻结node/citizenchain.json及内嵌创世WASM/patch、GENESIS_HASH、GENESIS_STATE_ROOT和行政区二进制保持。

## 四个原单源码包的职责拆分

adminbase平铺lib.rs、types.rs、policy.rs、traits.rs，分别承担库入口、原有记录类型、要素/分类策略、生命周期和查询接口；唯一公开出口归包根，不复制实现。publicadmin和privateadmin平铺lib.rs、validation.rs、lifecycle.rs、query.rs；FRAME存储/事件/错误/Config保持，校验、事务写入和身份授权读取分别组织。公权原始字段强制与私权姓名规范化及LR身份回落保持原不同语义。

election平铺lib.rs及election.rs；前者为Cargo必需入口，后者承载现有空FRAME定义。该包没有新增选举业务、类型、调用、事件或存储。

## 本机WASM自动化输入

产品公开describeAction收到WASM release且未指定inputs时，通过scripts/runtime.mjs的automationInputs调用既有readRuntimeTarget读取本机最终块Runtime基线和目标双版本，并读取正式检出的HEAD，交付version、transaction_version、runtime_base、source_sha。已有完整显式inputs原样交付；其它平台仍由各自Workflow准备版本。通用控制台只转发，不计算版本、不增加候选门禁。读取失败原样返回，不派发空参数或替代基线；Workflow继续拥有原冻结链、来源提交与版本规则。

Runtime构建仅在临时源码副本中替换双版本并生成WASM，原Runtime与Node源码不回写；节点Tauri版本继续由节点自身配置提供，不使用Runtime整数版本。产品Runtime只编译一次，测试WASM不进入节点包或正式Runtime三件产物。

node的runtime-tests特性默认关闭，仅由Runtime构建流程显式启用。scripts/runtime.mjs在三份WASM完成版本及内容校验后，恢复临时副本中仅嵌入WASM的构建钩子，通过WASM_FILE复用同版本runtime.wasm，并设置SKIP_WASM_BUILD=1避免额外生成SDK Runtime。旧缓存的嵌入引用不会被跳过开关覆盖，因此先清理revive-dev-runtime、substrate-test-runtime及frame-storage-access-test-runtime的release输出，保留当前产品WASM，再执行节点Ethereum整链校验。

随后解除SKIP_WASM_BUILD，由sc-consensus-grandpa自身构建所需SDK Runtime并完成专属校验；这些SDK WASM只作为构建校验输入，不交付到节点安装包或正式Runtime产物。上述校验失败时停止产物交付。

### 充值离线签名动作
独立host协议工具crates/protocol登记topup动作18及完整审阅字段，类型offchain_sign/offchain_auth、QR_V1 sign_request；禁止hash-only。动作/中文字段唯一源仍为registry/actions.yaml与fields.yaml，由原export_registry生成既有消费表。该host登记不进入runtime wasm，不改变Runtime版本、链上调用或节点；SDK通用opaque签名接口不增加业务分支。

### AccountId 的以太坊收款映射

账户身份、余额和权限仍以唯一 AccountId32 为准。SS58 仅在输入与展示边界编码；EVM 收款使用既有 `Revive.OriginalAccount` 反向索引，不建立第二套账户或余额。地址算法沿用固定 SDK 的 AccountId32Mapper：原生账户取 Keccak-256(AccountId32) 后20字节，以太坊派生账户去掉末尾12字节0xEE。

`runtime/src/evm_accounts.rs` 统一处理新账户、业务登记与历史回填。System.OnNewAccount 和公权/私权机构、公民身份账户登记回调调用同一处理器；机构主/费账户及零余额自定义账户在业务登记时就写映射，个人多签激活的既有入金路径触发 System 钩子。创世机构登记同样接入。映射在 System 账户清除或身份换绑后保留，不随 CID 迁到另一个 AccountId，已有反向绑定不得覆写。fallback 先收到转账不获得别名绑定权，不再因其已建户而阻止原账户登记；fallback 的余额、nonce 和权限原样保留，旧资金不自动合并，登记后的 E 收款按映射进入原 AccountId。

BackfillEvmAccounts 在现有 SingleBlockMigrations 后启动，使用同模块的分步处理器按游标扫描 System.Account、公权与私权 AccountRegisteredCid、有效个人多签账户及公民身份绑定；重复账户由幂等登记跳过。实际创世账户规模超过单区块预算，因此每步最多使用区块预算的四分之一，进度只保存来源阶段与最后一个存储键。续跑接入 System.PostInherents，在正常非空区块中先按预算处理并登记 Mandatory 权重，再接受原生业务交易；不修改挖矿门控或空块共识规则。迁移期间调用过滤器和费用路由拒绝 EVM 写交易，SDK 与节点原生收款解析对缺映射且迁移未完成的以太坊输入拒绝 fallback，避免旧账户被解析成另一个账户。没有普通交易就沿用原规则等待后续非空区块；升级实施需保证有原生交易驱动区块，不能依靠被暂停的EVM交易推进回填。完成标记只在全部来源扫描完成后写入，重启/重复升级继续原游标，完成后重复执行无副作用。正式升级前必须通过 try-runtime 对实际链状态检查冲突；冲突禁止覆写。若预检后实际反向绑定又出现冲突，迁移停在原游标并保留未完成状态，原生业务与治理继续，EVM 写交易保持暂停，修复冲突后再续跑，不以 panic 停链。映射新增读写采用显式保守权重，不宣称已重跑硬件 benchmark。

纯本地未上链账户，以及当前状态已回收且没有业务登记记录的历史地址，还没有可核验的收款映射，首次/再次 SS58 入金或链上登记后自动建立。不提供任意账户免费登记接口，映射不改变金额、费率或签名授权。源码开发不会递增 Runtime 版本；正式升级按实际部署版本规则单独执行。

#### fallback收款与建户资源预算

收款映射由runtime/src/evm_accounts.rs实现并在runtime/src/configs.rs接线。fallback账户已有原生余额不阻止新账户自动映射或历史回填；OriginalAccount绑定其他AccountId时仍拒绝覆盖。fallback账面状态保留，不自动搬移已有资金。

资源成本仍只定义在 EvmAccountMapping::get()。收费扩展预留三个公共分账收款方，普通转账只追加一个本金收款方，因此两者合计仍为四次映射回调预算。全节点奖励、公民奖励分别追加一次，省储行结算按 CHINA_CH 银行数追加；个人多签与多签异步执行按一个本金收款方和三个内部分账收款方给保守上界，现有多签达标记录读取补偿保留。决议发行按 MaxAllocations 在原回调 proof-size 清零之后追加映射预算，防止新增 proof 被丢弃。广场订阅、换档与每次续费按一个收款方追加预算，其每块处理上限沿既有权重控制。

所有适配保留原 benchmark 表达式，其余方法直接委托原 WeightInfo；不重写生成权重文件，不改收费金额、发行公式、签名/QR 编码或业务权限。异步执行预留覆盖不经过签名收费扩展的后台路径；这些保守预算不作为实际手续费。

提案编号：提案编号统一为 proposal_id:u128（全局唯一、永不重置）、year:u32（UTC 公历年）、seq_in_year:u64（跨年从0重置），计数溢出拒绝且整笔回滚。既有调用、事件和所有引用同步扩宽；法律ID、区块号、票数、nonce和清理FIFO序号保持原类型。旧布局转换直接位于 configs.rs 现有 AddPrcFeeSweepPermissions::on_runtime_upgrade 内，独立于权限标记，以 VotingEngine 已有 StorageVersion 0→1保证只执行一次。原地重算ID键及关联索引，保留载荷、投票、状态、余额和清理计划；创世宪法版本仅扩宽内部占位ID，不改变正文和内容哈希。迁移不增加存储表或额外标记键；历史区块仍用其原metadata解码。

CID年份u32：CID年份的生成入参与解析结果统一为u32，范围0～4294967295。哈希输入、校验和及输出统一使用至少四位补零的规范十进制；0000～9999的旧文本不变，10000开始自然增位。CID本体继续使用原字符串，最长为5+5+9+3+10=32字节；CID_NUMBER_MAX_BYTES、既有存储键/值布局和签名槽保持不变，无需迁移或重算已有CID及派生账户。创世年份仍固定2026，冻结创世配置不变。拒绝溢出、非数字及02026等歧义前导零。

## 原生交易统一V5

原生交易统一为V5 General、认证管线1，认证扩展为AccountAuth；证明仅有sr25519、ML首次公钥、ML已登记公钥三种分支。sr账户仍是原公钥32字节，ML账户派生及PqPublicKey存储保持；不转换账户、不迁移余额。两种算法共用账户绑定的签名约束：blake2_256(TRANSACTION_DOMAIN || AccountId || SCALE(ImplicationParts))；ImplicationParts覆盖管线版本、call及全部后续扩展的explicit/implicit，审阅端保留完整业务字段。nonce、权限、费用、质押保护、资源检查及上限保持；运行阶段仍拒绝sr。

认证先确定Signed(account)，之后执行既有业务检查与派发；认证扩展的validate只读，ML首次公钥仅在prepare写入一次。FRAME元数据只声明交易格式5，默认元数据API输出V16。SDK所需外层类型参数不构成旧交易入口。原生生产端不再构造或提交外层签名交易；升级生效后对其明确拒绝。此次不改变存储布局和已有创世，不生成替代链。
