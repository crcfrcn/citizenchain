# CitizenChain OnChina 技术文档

当前构建、工作目录与自动化的统一规则见 CitizenChainNode.md 当前合同；OnChina 的实际模块实现与调用入口见本文末节。

**聊天客户端的逻辑功能只能在 TataChatSDK 中实现；聊天服务端的逻辑功能只能在 CitizenServe.tatachat 中实现。公民、途遇及其他产品只依赖使用。**

CitizenChainOnChina 涉及聊天时只作为依赖使用方；本条不代表尚未接入聊天的产品已经具备聊天能力。

- 消息、会话、群组、加密、协议、传输、同步、重试、聊天存储、附件、通话及聊天界面行为，按客户端与服务端职责分别归 TataChatSDK 和 CitizenServe.tatachat；新增功能、缺陷修复和平台差异也必须在所属产品内完成。
- 消费产品只提供产品入口、身份与业务权益结果、服务地址及授权、主题和公开接口要求的平台配置；只通过公开接口接入，禁止复制、重写、包装成另一套聊天内核或维护产品专属聊天实现。CitizenServe、TuyuServe 的产品身份与权益授权不包含聊天数据面的实现职责。
- 本机开发直接依赖仓库路径；公民、途遇等产品的正式版本依赖塔塔聊天正式 Release；第三方市场分发使用公开市场版本。依赖使用不以公开市场发布为前置条件，也不改变实现归属。

本文是 CitizenChain OnChina 唯一技术事实文档，统一收录机构工作台、链上交互、前后端边界与部署约束。

OnChina的reqwest跟随产品原始锁统一固定的版本，启用json与rustls，和节点共用由Cargo生成的原始工作区锁。依赖收敛不修改链上Runtime、机构授权、数据库或HTTP业务逻辑，也不能替代实际TLS初始化、请求处理及应用运行验收。

#### 1. 功能定位

OnChina 地址库模块负责读取本地 `china.sqlite.addresses`，并构造 `AddressRegistry` 链上地址变更 call data。

模块路径：

```text
citizenchain/onchina/
├── platform/       # 公共平台与共享功能
├── registry/       # 注册局办理
├── legislation/    # 立法业务
├── judicial/       # 现有司法工作台
└── private/        # 私权机构业务（membership 为基金会现有业务）
```

#### 2. 数据边界

- 地址主数据仍在 `citizenchain/onchina/platform/codes/china.sqlite`。
- 后端只读打开 SQLite，不在运行态复制或改写地址主数据。
- 链上 call data 只用于地址变更冷签，不在 OnChina 后端直接提交 extrinsic。
- 前端只展示查询结果和生成的 call data，不绕过 QR_V1/冷签流程。

#### 3. API

| 方法 | 路径 | 用途 |
|---|---|---|
| `GET` | `/api/admin/address/names` | 查询某省市镇下的地址名称列表 |
| `GET` | `/api/admin/address/items` | 查询某地址名称编号下的完整地址列表 |
| `POST` | `/api/admin/address/chain-call` | 构造 AddressRegistry 裸 SCALE call data |

#### 4. 权限

- 后端按登录态 `VisibleScope` 转为省 / 市 / 镇码过滤。
- FRG 只能访问本节点绑定的省级组地址。
- CREG 只能访问本节点绑定的市级地址。
- 未获得 `can_view_institutions` 的普通机构不显示地址库入口，也不得调用地址库管理接口。
- runtime 会再次校验签名管理员与 `registrar_account` 是否具备本省/本市地址更新权。

#### 5. 链上调用

OnChina 使用 `onchina/registry/address/chain_call.rs` 构造以下 call data：

```text
AddressRegistry(33).set_catalog_version(0)
AddressRegistry(33).set_address_name(1)
AddressRegistry(33).remove_address_name(2)
AddressRegistry(33).set_address(3)
AddressRegistry(33).remove_address(4)
```

链交易动作码统一为：

```text
action = (33 << 8) | call_index
```

#### 6. 验收

```text
cargo check --manifest-path citizenchain/Cargo.toml -p onchina
npm --prefix citizenchain/onchina/platform/frontend run build
sqlite3 citizenchain/onchina/platform/codes/china.sqlite "PRAGMA integrity_check"
```

---

#### 1. 功能需求

OnChina 后端负责多机构工作台、管理员身份、行政区、机构、公民、管理员、扫码签名、公开查询和链侧凭证。它运行在 `citizenchain/onchina/`，属于公民链产品内部能力。

#### 2. 当前结构

```text
citizenchain/onchina/
├── platform/       # 公共平台与共享功能
├── registry/       # 注册局办理
├── legislation/    # 立法业务
├── judicial/       # 现有司法工作台
└── private/        # 私权机构业务（membership 为基金会现有业务）
```

#### 3. 目录铁律

- 后端按功能归入 `<本仓根>/onchina/{platform,registry,legislation,judicial,private}/`，功能目录内直接放实现文件；前端归各自 frontend。不建立 src 或 backend 中间层。
- 注册局办理只归 `onchina/registry`，私权机构自身业务只归 `onchina/private`；不建立职责重复的源码副本。
- 通用链连接和 finalized 读取归 `onchina/platform/chain`；通用会话、交易提交与恢复归 `onchina/platform/transactions`。业务调用编码和投影校验仍归所属功能，公共层调用原实现，不复制或改写业务规则。
- 二维码适配归 `onchina/platform/qr`，管理员认证归 `onchina/platform/authentication`，通用辖区规则归 `onchina/platform/authorization`，不得以其它目录重建同一实现。
- 非法人机构公共能力归 `onchina/platform/institutions/subjects_unincorporated_org.rs`。
- 平台工作台装配归 `onchina/platform/workspace`；各机构工作台声明归对应模块的 workspace。声明不保存第二份授权真源。
- 本轮只移动既有实现及必要引用，不新增进程、装卸、升级、业务、UI 或交互能力。未来架构能力不得用空清单或假入口表示已完成。

#### 4. Store 和表边界

后端只承认结构化 PostgreSQL 表为主数据。`store/` 可以封装访问和短期缓存，但不得保存第二份业务主数据。

- 机构主写入只进入 `onchina/platform/institutions`、`onchina/registry/institutions`、`onchina/registry/institutions`、`onchina/platform/institutions` 和 `onchina/platform/documents`。
- 公民主写入只进入 `onchina/registry/citizens`、`subjects`、`citizens`、`citizen_documents`、`passport_numbers` 和 `sequence_counters`。
- 管理员写入只进入 `admins`（本地展示元数据）和短生命周期安全运行态表；管理员字段固定为 `account_id + cid_number + family_name + given_name`，成员资格与岗位范围只来自链上，禁止建立本地管理员授权范围表。
- 旧机构直接创建 API 当前固定返回 501，不写 `chain_sign_sessions`、机构业务草稿、占用表或本地机构投影。第 6 步的新业务模块必须原子提交 CID 基础资料、admins、LR、初始治理岗位/权限/任职/投票规则和协议账户后，才能重新开放创建。
- 创建机构和创建公民只有两种业务结果：链上确认成功后写正式投影；未链上确认就是失败，并删除对应短期签名会话，不保留名称、CID、管理员或公民档案占用。
- 公权机构唯一真源是链上 `PublicManage`;`subjects/gov/accounts` 中的公权行只是本地查询投影,投影版本只记录在 `chain_projection_state`。
- `subjects.chain_status` 只作机构查询投影，不得成为授权真源。账户 `chain_status` 由真实链账户 DTO 提供，本地 `accounts` 不承担账户状态真源。
- 交易索引只读取 finalized 区块；余额事件必须匹配当前 Runtime 的真实事件/字段名，已登记事件解码、金额或索引越界、事件数量不完整时整块失败，不允许跳过后推进。`tx_records(block_number,event_index)` 唯一；记录、`tx_indexer_state` 及 `chain_projection_state(wallet-transactions)` 的 genesis/hash/number 锚点同事务提交。旧索引没有可信锚点或已有重复记录时明确停止，钱包查询也拒绝返回无可信锚点的历史；历史补录/修复必须另行授权，不能自动删除或重置持久数据。
- 公民本地编辑在既有一次 Passkey 后，按绑定、CID、计数器锁序重新读取档案并复核范围与不可变字段；护照分配、主行及相关索引在同一事务提交，响应返回实际数据库行。过期绑定修订与零行更新均失败。管理员 ID 复用 `sequence_counters` 原子分配，接续已有 ID 并显式拒绝容量溢出。
- 冷签会话创建时保存原绑定 ID、准确机构 CID 和机构码；prepare 升级不得覆盖这三个原始锚点。HTTP 与后台恢复共用带续租的数据库互斥，所有恢复上下文更新检查租约；后台按最近检查时间轮转，单条损坏会话不阻塞其余记录。后台只扫描已留提交/最终确认锚点的会话，只核实 canonical finalized + ExtrinsicSuccess 并补写，不签名、不重提、不要求原管理员继续任职。业务投影与消费标记同事务，原机构不匹配或历史锚点不完整则保留等待处理；重复占号恢复保留已补正本。
- 当前 schema 声明包含名册准确 CID、机构链状态、会话租约及交易唯一索引。此任务只修改声明及读写源码；禁止据此自动改造正在使用的持久数据库。存量无归属 CID/无可信索引锚点/无会话原绑定的记录不得猜测补齐，须在后续获准的数据库方案中逐项处理。测试、编译、安装及持久数据修复均须独立执行授权，源代码变更不代表已部署。
- 立法读取保留已验证会话的原省、市范围，并按法律 houses 中现有机构 CID 的原省码与省内市号匹配。省代码是永久固定的两位大写字母，省内市号保持原编号；CID及现有省市解析规则保持。
- 立法发起机构直接采用登录绑定的完整 CID，并用于同一签名会话归属；本机构同时为表决院时复用该 CID，其他路由机构仍经原唯一查询解析，校验 CID 格式与机构码。
- 法律地域唯一使用 primitives::genesis::LawRegion：原两位大写字母省码与原省内市号。新法请求不接收数字地域占位，后端从登录绑定的原机构 CID 派生并核对会话；修法、废法继续核对目标法律 ID、层级、地域及原机构 CID。预算、任免预留 DTO 同样不接收地域占位，但不增加业务实现。
- 逐席投票按提案、院次、机构 CID、岗位码和账户关联，同钱包不同岗位独立计票；前端只调整内部 key 与 DTO，不改页面布局、控件或扫码步骤；账户分类完整包含联邦公民安全基金账户，市编号说明固定为省内唯一的三位数字。

- CORS 的指定 origin 与显式通配策略共享 PATCH 和现行认证请求头；生产 origin 限制不变，不因预检修复扩大可信来源。

- `node_institution_bindings` 只保存本节点当前绑定的链上身份键：`candidate_id / institution_code / institution_cid_number`。FRG 绑定始终是一个 FRG 机构身份，不得拆成虚拟省组身份；省级办理范围来自管理员账户在 entity 中有效的 `PROVINCE_COMMISSIONER_<省码>` 任职，机构 CID/全称/简称/主账户来自 FRG 主体投影且只作身份与展示。绑定表不得保存名称或省市镇权限派生值。
- FRG 候选身份统一按机构 CID，不按省拆分；当前账户办理省份只从该账户在同一 finalized 区块的有效省专员岗位派生，守卫重核且不持久化权限缓存。缺少或同时存在多个省任职时明确拒绝范围不明，选择/多省操作契约另行确认，不擅自新增 UI 选择步骤。
- 任意已绑定机构（包括私权机构）都按准确 CID 在固定 finalized 区块同步机构和实际账户，事务内重核绑定并保留机构附件、照片与创建资料。本机构详情在首次读取时补齐，独立后台循环继续刷新；机构状态来自实际投影，账户详情与账户列表共用真实链账户 DTO，禁止以账户字段非空推断已上链。
- 公权全量同步的机构、账户、genesis/hash/number 必须来自同一 finalized 视图。公民全量枚举以当前投票身份居住地优先、未升级者以登记居住地筛选；已有投影迁出时仍刷新地域与修订，旧范围查询不能继续读取，链下档案/附件不自动复制或删除。

- `admin_sessions.candidate_id` 必须与 active binding 严格一致；旧会话、解绑后会话、重绑前会话和候选不一致会话一律失效，不存在兼容回落。
- 审计写入统一走结构化审计入口，详情字段只保存事实，不保存 UI 文案。

##### 4.1 公权机构链投影

- 显式 `sync-gov` 必须从链上 `PublicManage::Institutions` 与 `PublicManage::InstitutionAccounts` 全量读取,再写入本地 `subjects/gov/accounts` 投影。
- `serve` 启动时先读取链 `genesis_hash` 与 finalized head,再比对 `chain_projection_state(public-gov)` 的 `chain_genesis_hash / chain_block_hash / chain_block_number / item_count / account_count`;一致则直接启动并跳过全量同步,不一致或无投影才全量同步。链不可达、锚点无法确认或同步失败时 fail-closed。
- OnChina 不得在启动时从 `china.sqlite` 重新生成公权机构；`china.sqlite` 只提供行政区名称和镇级索引校验/展示。
- 投影状态写入 `chain_projection_state(projection_key='public-gov')`;旧 `gov_manifest`、`ensure-gov`、`reconcile-gov`、`check-gov` 均不得恢复。
- 普通列表、联邦注册局详情和本机构显示页只能读取 `gov.source='CHAIN'` 的公权投影；本地手工/pending 行不能冒充链上公权机构真源。
- `audit-chain-catalog` 只做创世链目录验收,不得用本地派生结果灌库。
- CitizenApp 公权机构接口只读取链上投影并下发 `chain_genesis_hash / chain_block_hash / chain_block_number / synced_at` 作为同步锚点;`manifest_version` 由 genesis hash + finalized block hash/number + 投影数量组成,不得使用本地 `synced_at` 单独推进版本,也不得把 OnChina PostgreSQL 当成公权机构真源。
- `PublicManage::InstitutionInfo` 按当前 runtime 精确字段序解码；机构存在即表示 active，
  不得在 OnChina 追加已删除的 lifecycle/status 尾字段，也不得用兼容分支吞掉尾随字节。
- 2026-07-16 创世准备验收使用 preview 块 0 的真实 node 和全新临时 PostgreSQL：启动投影
  49,593 个机构、99,231 个账户，33 项创世目录抽样对账通过，`/api/health` 返回
  `UP`，公权目录版本锚定同一 genesis/block#0，前端首页真实返回“链上中国平台”。该
  preview 不替代正式冻结锚点，验收结束后节点、OnChina、PostgreSQL 与临时目录均已清理。

#### 5. 公民录入和护照号

- 公民由注册局管理员在 OnChina 当前办理城市下一次交易录入,不再由前端手填 `cid_number`。
- 联邦注册局管理员必须先选择分管省内城市后才能录入公民;市注册局管理员直接锁定本市。
- 公民姓名统一为 `family_name` 和 `given_name`；展示姓名时由前端按中文顺序组合，数据库不保留拼接姓名或带 `citizen_` 前缀的别名列。
- 公民身份 CID 由 `onchina/platform/codes/generator.rs` 生成,机构代码固定为 `CTZN`,个人码 R5 市段固定为 `000`。
- 护照号由 `onchina/registry/passports/number.rs` 生成,OnChina 自持完整算法。
- 创建公民不得要求 `account_id`;未成年人或暂未开户公民可以先建立本地电子护照档案。
- 推送链上公民身份时才录入严格格式的 `account_id`;签名请求使用当次 `signer_public_key` 验证该账户授权，`ss58_address` 只从账户字节派生用于边界展示，不进入授权存储。请求必须显式提供 `identity_level=voting/candidate`：投票身份要求对应账户签 `VotingIdentityPayload`，参选身份要求对应账户签 `CandidateIdentityPayload`。
- 未满 16 周岁不得推送链上公民身份。年龄不入链载荷:OnChina 在生成签名二维码前按 `citizen_birth_date` 校验 ≥16(BFF 防误推门,投票/竞选都拦);runtime `citizen-identity` 只在竞选身份按 `birth_date` 实时算龄复核 `>= 16`,投票身份不在链上算/存年龄(能否投票由 `citizen_status=Normal` + 护照有效期窗口 + 注册局 `voting_eligible` 判定)。
- 出生省市镇必填,字段为 `birth_province_code / birth_city_code / birth_town_code`;创建后不得被普通编辑流程修改。
- 居住/办理行政区直接使用链上中国统一行政区字段 `province_code / city_code / town_code`;前端只允许在当前办理城市下选择 `town_code`,不得恢复旧的第二套居住字段。
- 护照有效期自动计算:创建时年满 16 周岁为 10 年,未满 16 周岁为 5 年,字段为 `passport_valid_from / passport_valid_until`。
- `citizens` 表当前字段只表达公民档案、身份 CID、护照号、可为空的 `account_id`、出生地、居住地、护照有效期和投票资格。
- 公民资料库独立使用 `citizen_documents` 表和 `/api/admin/citizens/{cid_number}/documents` 接口,不得复用机构 `docs` 表或 `onchina/platform/documents` 逻辑。资料类型固定为“护照相片 / 出生证明 / 监护人护照 / 其他材料”,文件本体写入磁盘,表内只保存元数据和内容哈希。
- `passport_numbers` 是护照号全局索引表;`passport_number_recycle_pool` 只保存可回收护照号,不得保存旧公民个人资料。

#### 6. 链交互边界

链交互按业务归属放置：

- 机构注册信息凭证、账户列表 DTO 和 handler：`onchina/platform/institutions/subjects_chain_*.rs`
- 投票资格提示查询：`onchina/registry/citizens/chain_vote.rs`
- 公民链上身份推送：`onchina/registry/citizens/chain_identity.rs`
  - `POST /api/admin/citizens/{cid_number}/onchain/prepare` 只消费一次 Passkey，建立 180 秒 `citizen_onchain_operations` 操作并生成 `a=2 citizen_identity` 签名请求；请求体必须包含 `account_id` 和 `identity_level`。
  - `identity_level=voting` 编码 `VotingIdentityPayload`，完成后生成 `0x0a00 register_voting_identity` 注册局管理员链上签名二维码。
  - `identity_level=candidate` 编码 `CandidateIdentityPayload`，完成后生成 `0x0a01 upgrade_to_candidate_identity` 注册局管理员链上签名二维码；该交易同时写入投票身份和参选身份。
  - `POST /api/admin/citizens/{cid_number}/onchain/complete` 不再二次认证；它按签名响应 `id` 校验管理员、机构、CID、账户、身份级别和完整 payload，原子消费操作后生成管理员最终链签二维码。账户绑定和上链投影只在最终链交易确认后一次性落库。
- 联合投票本地人数查询：`onchina/registry/citizens/chain_joint_vote.rs`
- 地址变更调用：`onchina/registry/address/chain_call.rs`
- 立法法律只读链读：`onchina/legislation/laws/chain_read.rs` 负责读取 `Law`、`LawVersion`、`LawVersionLabels` 和宪法不可修改条款 manifest；`LawView.version_title/version_title_en` 只能来自链上 `LawVersionLabels[(law_id, version)]`。
- 立法提案写入：`onchina/legislation/laws/chain_propose.rs` 的新法、修法、废法载荷必须在 `actor_cid_number` 后紧接必填 `proposer_role_code`。后端只做 1..64 字节和 SCALE 顺序校验，真正发起权限由 runtime 按完整岗位主体校验；不得从管理员登录态推导岗位码或恢复无岗位码旧载荷。
- `onchina/legislation/proposals/chain_read_proposal.rs` 必须按当前核心 `Proposal` 布局解码；人口分母已独立存于投票引擎 `ProposalPopulationSnapshots`，不得在 Proposal 镜像恢复 `citizen_eligible_total` 尾字段。
- 通用 SCALE、genesis hash、RPC URL 和交易提交辅助：`onchina/platform/chain` 和 `onchina/platform/transactions`

业务模块不得新增全局链目录，不得在 handler 内手写 pallet/call 字节或二维码动作码。动作码、payload、签名/验签规则以 `CitizenChainOnChina.md` 为唯一登记入口。

PublicManage/PrivateManage call 5 及 `0x1e05/0x1f05` 已永久留洞，后端不得继续编码旧 `cid_number + cid_full_name + cid_short_name + town_code + admins + actor_cid_number` 载荷。新创建协议由第 6 步独立业务模块另行登记；必须携带注册局完整 `RoleSubject`，并原子绑定目标机构的岗位、权限、任职和投票规则，不能只凭注册局 admins 或在创建后补权限。

#### 7. HTTPS 和机构 CA

正式入口固定为 `https://onchina.local:8964`。OnChina 启动时在 `ONCHINA_TLS_DIR` 生成并持久化本机构节点私有 CA：

- `onchina-org-root-ca.crt`：员工浏览器可下载和安装的 CA 公钥证书。
- `onchina-org-root-ca.key`：仅保存在节点服务器本地的 CA 私钥，禁止通过 HTTP、日志或前端接口暴露。
- `onchina-server.crt` / `onchina-server.key`：由本机构 CA 签发的 `onchina.local` 服务证书。
- `onchina-cert-profile.txt`：服务证书策略标记；标记变化最多触发服务证书检查或重签，绝对不能触发根 CA 轮换。

根 CA 只允许在根证书与根私钥同时不存在的首次初始化生成，有效期固定到 2036-01-01。服务重启、程序升级、配置标识变化均只加载磁盘中的真实根证书和对应私钥，不得重新构造、覆盖或轮换根 CA。根材料不完整、损坏、公私钥不匹配、CA 用途错误、自签名无效或超出有效期时必须失败关闭，由管理员人工处理。

`onchina.local` 服务证书有效期不超过 397 天；证书、公私钥、SAN、ServerAuth 用途、根签名关系均正确且距离到期超过 30 天时直接复用。只有服务证书缺失、损坏、策略或域名不符、签名关系错误或进入到期前 30 天时，才使用现有根 CA 成对重签服务证书和私钥。服务证书重签不得写入根证书或根私钥。

未登录公共接口 `/api/platform/ca-certificate` 只返回 CA 公钥证书 PEM，用于员工首次访问时下载并导入浏览器/系统受信任根证书；`/api/platform/ca-certificate/info` 只返回文件名、证书主题、SHA-256 指纹和有效期展示信息。

#### 8. 错误码和提示边界

后端统一通过 `ApiError.error_code` 暴露稳定业务错误码。HTTP `401` 只表示管理员登录态无效；公民档案不存在、账户不匹配、签名失败等业务错误不得返回 `401`。

注册局机构主账户缺失必须返回稳定错误码 `ONCHINA_REGISTRY_MAIN_ACCOUNT_MISSING`;正常目标态下该错误只能在链投影异常或绑定自愈失败时出现,不得再被前端降级成通用“请求内容不正确”。

数据库错误必须展开 PostgreSQL SQLSTATE、message、detail 和 hint，禁止只向前端或日志传 `db error`。

#### 9. 管理员写操作

市注册局本地目录维护、公民档案与资料修改、Passkey 管理、节点解绑和链写动作必须使用各自安全档。业务 handler 只负责构造业务动作，二维码协议包装和签名结果识别归 `onchina/platform/qr/`。

公民身份上链(`CITIZEN_ONCHAIN_PUSH`)固定为一次业务操作：管理员 Passkey 一次、目标公民账户签名一次、管理员最终链交易签名一次。最终链签已经承担管理员账户授权，不得再叠加安全 grant 冷签；`complete` 依靠一次性 `citizen_onchain_operations` 防串单、防过期和防重放。

联邦注册局机构 `admins` 和岗位任职不得本地直接改库；换届只能构造链上治理或注册局登记动作后由 entity 写入。市注册局本地登记目录每省每市最多 30 人，统计必须同时带省和市，但该目录不是链上管理员资格真源。NJD、普通公权机构、私权机构和非法人组织的本机构管理员/岗位维护也必须走链上 `propose_institution_governance`，不得在 OnChina 内建立第二套管理员集合。

`INSTITUTION_CREATE` 旧扫码授权链路当前不允许进入 prepare/submit；创建 handler 在鉴权后固定返回 501，不能生成安全 grant、链签会话或旧 call data。第 6 步启用新业务时必须重新登记完整原子载荷和权限主体，不能沿用本段已删除字段集。

`PASSKEY_COLD_SIGN` 正式提交的安全门统一在 `onchina/platform/authentication/actions.rs::require_admin_security_grant`：先消费 `X-Passkey-Assertion`，再消费 `x-cid-security-grant`，任一缺失、过期、归属不匹配或 payload hash 不匹配都 fail-closed，不允许降级为 SESSION 或只验冷签 grant。机构资料上传、资料删除、机构详情更新等链下写操作虽然不直接提交链交易，也必须按各自后端 `grant_payload` 逐字段绑定授权：上传资料为 `target/file_name/doc_type/file_size`，删除资料为 `target/doc_id/file_name`，机构详情更新为 `target/cid_number/cid_full_name/parent_cid_number/family_name/given_name/legal_representative_cid_number/legal_representative_photo_path`。

链上写的 passkey 那一半由**类型闸门**强制，不靠 handler 自觉（2026-07-30 创世前审计整改）：
`auth/passkey::require_passkey_assertion` 成功时返回 `PasskeyProof`，该类型字段私有、
无公开构造函数，全仓唯一产出点就是这个函数；冷签会话唯一创建入口
`Db::insert_chain_sign_session(&session, &PasskeyProof)` 把它列为必填参数。
于是「没做 passkey 就发起链上写」是**编译期错误**而非运行期漏检。
整改前 8 个冷签会话创建点中有 4 个只有会话态；类型闸门落地时又抓出人工复查漏掉的
第 5 个（`complete_citizen_onchain_signature`）。

passkey 的消费点统一钉在**创建冷签会话那一步**，不在 prepare 载荷步、也不在 submit 步：
- `POST /api/admin/chain/submit` 只消费钱包冷签那一半，不再要求第二次 passkey——
  「链上写 = passkey 一次 + 钱包一次」是三档契约明文，两次 passkey 属于违约；
  提交侧安全由会话归属校验（`session.account_id == ctx.account_id`）、签名者一致校验和
  链上授权复核共同保证。
- 公民上链是 prepare → 公民回签 → complete → submit 四步，那唯一一次 passkey 在
  `complete`（真正创建冷签会话、授权链上写的一刻），prepare 只构造待签载荷属 Session 档。
- `POST /api/admin/address/chain-call` 直接返回待冷签 call、不经会话表，拿不到类型约束，
  故在 handler 内显式断言。

机构管理员列表 API 联合读取链上 `admins(account_id + cid_number + family_name + given_name)` 人员集合与 entity 岗位、`InstitutionRolePermissions` 和有效任职。一次读取必须固定到同一个 finalized 区块：名册 `account_id` 继续作为岗位任职关联锚点，带 CID 管理员对外返回和鉴权使用该 CID 的当前绑定 `account_id`；换绑后岗位不重写，但新账户立即取得签名权、旧账户立即失权。`onchina/platform/institutions/admins_chain_roles.rs` 负责公权/私权岗位路由、任职合并和 FRG 省专员范围解析；管理员即使没有岗位也必须保留人员行，姓名只展示。本地联系方式、照片和 Passkey 不得成为管理员资格或岗位真源；业务授权必须由完整 `RoleSubject + BusinessActionId + operation` 查询，不按账户或前端标签推断。

链上机构唯一查询先读取 `PublicManage::Institutions[cid_number]`，未命中再读取 `PrivateManage::Institutions[cid_number]`，不建立本地分流真源；公私权 CID 不重复由 runtime 与 NodeGuard 的链上不变式保证。2026-07-24 当前 fresh 链全量投影精确为 49,593 个机构和 99,232 个账户：49,593 个机构主账户、49,593 个机构费用账户、43 个省储行质押账户，以及两和基金、安全基金、联邦公民安全基金三个独立基金账户。非营利法人“公民链技术发展基金会” `GZ018-SFGYR-201206100-2026` 属私权创世机构，只参加独立私权存在性审计，不冒充公权目录行。启动抽样当前覆盖 32 个派生公权机构、1 个公权常量机构和该基金会，共 34 项。

链上机构管理员无论来自 `PublicAdmins` 还是 `PrivateAdmins`，都统一解码为 `account_id + cid_number + family_name + given_name`。OnChina 严格镜像 runtime 分层解析：非空管理员 CID 通过 `CitizenIdentity::AccountIdByCid / CidByAccountId` 正反向闭环取得当前账户；私权法定代表人名册 CID 为空时，使用同一机构 `PrivateManage::Institutions.legal_representative.cid_number`；冻结公权无 CID 管理员和私权非 LR 无 CID 管理员直接使用名册 `account_id`。CID 已撤销、绑定缺失或正反向不闭合时失败关闭，不回退名册旧账户。个人多签只按账户治理且不进入 OnChina。本地公民库只能用于输入补全和展示，不能变成链上授权真源。

机构治理链写入口：

- `POST /api/admin/institution/governance/prepare`：请求必须独立携带必填 `proposer_role_code`；后端只接受当前节点绑定机构 CID，按 `cid_number + action + register_nonce + signature + actor_cid_number + proposer_role_code + credential_signer_public_key + scope` 的 runtime 顺序构造签名载荷并写入 `chain_sign_sessions`。管理员登录态不产生业务权限，最终由 runtime 校验完整岗位主体。管理员集合、岗位、任职和法定代表人任命/更换/解除都只进入链上 call data，不写本地正式投影；解除时提交 `clear_legal_representative=true`，不得同时提交 `legal_representative_cid_number`。
- `POST /api/admin/institution/admins/register/prepare`：注册局管理员发起 `register_institution_admins`，目标机构 CID 从请求读取，actor CID 只来自当前节点绑定注册局 CID。
- 提交阶段复用统一链签会话 submit。机构治理 purpose 进块后只记录审计；OnChina 读侧继续读取链上 `admins / InstitutionRoles / InstitutionRoleAssignments`，禁止在提交成功后本地直接改管理员或岗位真源。
- 创建动态岗位时前端不得提交岗位码；runtime 使用所属 pallet 的 `MODULE_TAG`、CID、单调 nonce 和真实 proposal_id 生成 `R_<32 位大写十六进制>`，删除后永久不复用。
- 法定代表人治理使用 runtime `InstitutionLegalRepresentativeChange::Set/Clear`，任命/更换时三字段同时写入，解除时三字段同时清空。

#### 10. 机构工作台能力映射

工作台类型和工作台入口由 `onchina/platform/workspace/` 生成，底层能力位由 `onchina/platform/modules/capability.rs` 单源下发给前端。runtime 已经实现 FRG 省级组登记权高于 CREG 本市登记权，OnChina 能力表必须只镜像这个目标状态，不能另行降权：

- `FRG` 是 Tier1 创世注册局，进入 `registry` 工作台，能力必须是 `CREG` 的超集：可进入公民、私权、教育、公权机构、市注册局和联邦注册局，并可在本省范围内登记机构、写业务、维护市注册局管理员、维护本省联邦注册局管理员。
- `CREG` 是 Tier2 下级注册局，进入 `registry` 工作台，保留本市公民/机构/业务写入能力；同时必须能进入“联邦注册局”入口，只读查看本省联邦注册局管理员列表，不得发起联邦注册局管理员编辑或更换。
- `NJD` 进入 `judicial` 工作台，不复用注册局 UI。当前工作台按 `operations / display / records` 分类：显示页可只读查看本机构信息和链上 active admin 列表；管理员变更和岗位治理入口必须构造 `propose_institution_governance` 链动作，护宪终审按专属业务能力接入。
- 立法机构进入 `legislation` 工作台或通用工作台的立法入口，立法能力由 `onchina/legislation/proposals/category.rs` 和 `can_*_legislation` 位决定。
- 私权机构进入 `private` 工作台，只下发本机构信息、链上 active admin 与准确 CID 授权模块；普通公权、立法和非法人机构分别使用 `public`、`legislation`、`unincorporated` 工作台种类，可共用通用显示壳但不得恢复 `generic` 权限语义。
- 登录、扫码登录轮询、鉴权检查和工作台返回统一携带 active binding 的准确 `institution_cid_number`；后端未能解析准确 CID 时 fail-closed，前端不得根据 `institution_code` 猜测。
- 平台会员模块只在准确 CID 等于同一 finalized 区块的 `SquarePost::PlatformCidNumber` 时下发。`onchina/private/membership/` 只读取 finalized 价格、构造 `propose_set_platform_price` 和校验链上 `admins`，不保存价格、不实现投票。
- 所有链交易签名响应统一提交到 `POST /api/admin/chain/submit`；业务域只 prepare，不得新建第二套 submit handler。平台调价 prepare 必须携带 `proposer_role_code`，按 `actor_cid_number + proposer_role_code + membership_level + new_price_fen` 编码；prepare/submit 复核节点绑定和准确平台 CID，业务授权最终只认链上岗位任职与 `sqr-sub/5` 权限，不把 active admins 当授权真源。
- `NRC`、`PRC`、`PRB` 走节点桌面端，不获得 OnChina 网页能力。
- `PMUL` 和其它个人主体不获得 OnChina 网页能力。
- 前端工作台展示只使用后端下发的 `workspace` 和 `capabilities`；后端 handler、scope 和链上 active admin 校验仍是安全边界。

##### 10.1 本机构只读接口

- `GET /api/admin/own-institution` 返回当前 active binding 对应机构的 `InstitutionDetailOutput`，用于非注册局工作台“显示”页。
- 本接口不接受前端传入 `cid_number`；后端只从当前节点 active binding 的 `institution_cid_number` 定位本机构，避免变成任意机构详情读取入口。
- 返回数据仍来自结构化 `subjects/accounts` 投影；管理员资格由登录守卫、节点绑定和链上 active admins 校验决定。
- `GET /api/admin/own-institution-admins` 目标返回链上 active `admins` 账户与 entity 有效机构岗位任职的联合结果。

##### 10.2 CitizenApp 公权机构只读接口

- `GET /api/app/public-institutions` 提供匿名只读公权机构链上投影分页;请求字段为 `province_name / city_name / since_version / after_cid / page_size`。
- `GET /api/app/public-institutions/version` 返回当前 scope 的 `manifest_version / chain_genesis_hash / chain_block_hash / chain_block_number / synced_at / count`。
- `manifest_version` 由 `chain_projection_state(public-gov)` 的 `chain_genesis_hash / chain_block_hash / chain_block_number / item_count / account_count` 生成,只作为 CitizenApp 本地缓存游标;链上 `PublicManage` 仍是唯一真源。
- 接口只下发行政区 code,不下发行政区名称副本;CitizenApp 通过内置行政区字典按 `province_code / city_code / town_code` join 名称。
- 接口不得读取 `china.sqlite` 运行态派生公权机构,也不得把本地 `subjects/gov/accounts` 投影作为授权真源。

#### 11. 验收

2026-07-25 正式创世前依赖告警复验：OnChina 生产二进制和全部测试目标自身均通过
`cargo clippy -p onchina --all-targets --no-deps -- -D warnings`，144 项单元测试全部通过。
测试夹具允许在各自 `#[cfg(test)]` 模块内使用断言式 `expect/unwrap`，生产模块不得据此
扩大豁免范围。用户完成 runtime 路径二次确认后，
`cargo clippy --workspace --all-targets -- -D warnings` 与
`cargo test --workspace --all-targets` 均已通过，OnChina 传递依赖不再遗留 Clippy 告警。
本轮只处理 lint、测试夹具与注释边界，没有修改订阅业务逻辑。

2026-07-17 机构治理运行态补验：当前源码 `citizenchain-fresh --tmp` 使用 `WASM_BUILD_FROM_SOURCE=1` 构建后启动成功，OnChina 使用临时内嵌 PostgreSQL 和 `ONCHAIN_WS_URL=ws://127.0.0.1:19944` 连接 fresh 链启动成功；启动期完成公权链投影 `49,593` 个机构与 `99,231` 个账户，首页 HTTP 返回 200，`subjects` 表旧 `legal_rep_*` 列为 0，新 `legal_representative_*` 三字段列齐备。交互式 CitizenWallet 扫码签名需要真实管理员登录会话和扫码设备，本次仅完成链、数据库、服务和页面基础运行态，不伪造扫码签名结果。

正式创世前曾使用管理员三字段布局；该布局现已全部废弃。OnChina 当前对公权、私权机构统一按四字段 `Admin` 解码与治理/登记编码，不兼容任何旧三字段机构布局；个人多签同样使用统一四字段 SCALE 结构。

2026-07-19 私权创世公民链基金会第 6 步验收：`institution_lookup` 已实现在相同 CID 主键下依次读取 `PublicManage` 和 `PrivateManage`，公权全量目录迭代继续只读取公权 storage；启动抽样固定增加公民链基金会，全量公权审计先独立核验基金会存在，再执行 49,593 个公权机构的双向比对。OnChina 137 项测试通过；没有把基金会复制进本地公权投影、没有读取本地公民数据库生成法定代表人，也没有新增第二套基金会身份常量。

2026-08-01 正式创世前管理员换绑接管验收：登录反查、管理员列表、岗位合并和会话复查统一固定到 finalized 区块，带 CID 管理员只接受当前绑定账户，岗位继续以名册账户关联。当次历史冻结 plain chainspec 的创世哈希为 `0x278e68bced2dabf9690701188272da22d216fdaa2c617e7dcbe100df3e8bcbfa`；该值已被同日新创世替代，不是当前部署锚点。临时 OnChina 从链投影得到 49,593 个机构、99,232 个账户并完成 34 项抽样对账。真实 `POST /api/admin/auth/qr/sign-request` 验证 FRG 冻结无 CID 管理员与基金会当前绑定账户均返回 200，无在册账户返回 403 / `ONCHINA_LOGIN_ADMIN_NOT_ONCHAIN`。OnChina 191 项测试和 `cargo clippy -p onchina --all-targets -- -D warnings` 通过；未修改 runtime、创世或远端状态。

```text
rg "mod chain;|crate::chain|chain::" citizenchain/onchina -g '*.rs'
cargo check --manifest-path citizenchain/Cargo.toml -p onchina
ONCHINA_EMBEDDED_PG=0 DATABASE_URL=<local_pg> ONCHAIN_WS_URL=<chain_ws> cargo run --manifest-path citizenchain/Cargo.toml -p onchina -- sync-gov
curl -kfsS https://onchina.local:8964/api/health
curl -kfsS https://onchina.local:8964/api/platform/ca-certificate/info
curl -kfsS -o /tmp/onchina-org-root-ca.crt https://onchina.local:8964/api/platform/ca-certificate
curl -ksS -i https://onchina.local:8964/api/admin/auth/check -H "authorization: Bearer <token>"
```

涉及数据库、登录、管理员列表、机构详情和扫码签名的变更必须跑真实 HTTP 接口。只通过 `cargo check` 不能证明连接池、SQL 字段顺序和扫码验签流程正确。

---

#### 1. 功能需求

本文件集中登记 OnChina 的行政区、CID 号、权限、扫码签名、错误码和高并发数据边界。它承接旧 CID 文档中仍然有效的数据安全规则，并删除独立产品部署和旧路径口径。

#### 2. 行政区数据

行政区名称字段仅为 `country_name`、`province_name`、`city_name`、`town_name`、`division_name`，不得以泛化名称字段替代。国家全称、简称的链定义见 `CitizenChainRuntime.md` 的“Runtime 修改与链协议边界”。本段是命名合同，不表示所有字段均已在当前接口使用或验收。

- 开发真源：`citizenchain/onchina/platform/codes/china.sqlite`
- 生产读取：`ONCHINA_CHINA_DB` 指向随包只读 SQLite
- 省级常量：`citizenchain/runtime/primitives/cid/code.rs`
- 镇下完整地址：`addresses` 单表保存当前有效地址；开发库随安装包发布，链上 `AddressRegistry` 记录单条地址变更事实和当前哈希

加载时必须校验：

- SQLite 省表与 runtime primitives 省码一致。
- 省名和市名全国唯一。
- `(province_code, city_code, town_code)` 不重复。
- 镇下地址使用 `address_name_code(3位) + address_local_no(4位)` 模型。
- 旧地址结构、墓碑表和变更日志表必须清除。
- 地址库只保存当前有效数据,不保留旧地址历史。
- 地址链上变更只同步对应的地址名称或完整地址，不全量上链地址库。

#### 3. CID 号

机构全称、简称字段仅为 `cid_full_name`、`cid_short_name`；管理员集合仅为 `admins`。字段结构由所属真实接口定义，不另建字典或字段登记库。

CID 号格式为 `R5-K3P1C1-N9-D4`。

- `R5`：省码 2 位 + 市码 3 位。
- `K3`：主体属性 `K1` + 机构类型 `T2`。
- `P1`：盈利属性。
- `C1`：校验位。
- `N9`：9 位稳定散列序列。
- `D4`：年份。

CID 号生成和校验唯一源码目录为 `citizenchain/onchina/platform/codes/`。任何端不得维护第二份号码格式、机构码表或省码表。

公权机构 CID 与机构信息不在 OnChina 运行态生成。所有公权机构唯一真源是链上 `PublicManage::Institutions` / `PublicManage::InstitutionAccounts`;OnChina 只保存 `sync-gov` 同步出的本地查询投影,投影状态以 `chain_projection_state(public-gov)` 为准。CitizenApp 内置的 `assets/public_institutions/` 是从链上创世状态导出的公权机构快照缓存,只能用于本地快速展示和增量同步,不得作为第二真源。

##### 3.1 公民 CID 和护照号

- 公民 CID 的机构代码固定为 `CTZN`;个人码不携带办理市码,R5 市段固定为 `000`。
- 公民护照号由 `citizenchain/onchina/registry/passports/number.rs` 生成,格式为省码 2 位 + Crockford Base32 主体 8 位 + 校验位 1 位。
- 护照号终身唯一;`passport_numbers` 负责全局查重。
- 护照号资源回收只允许通过 `passport_number_recycle_pool` 回收号码本身,不得保存旧公民姓名、出生地、账户、公民 CID 或其它个人资料。
- 公民档案本地创建阶段允许没有 `account_id`;儿童或暂未开户公民不得被强制生成账户。
- 链上公民身份推送准备阶段才接收并严格校验 `account_id`;目标公民必须以对应 `signer_public_key` 对 `VotingIdentityPayload` 签名。签名公钥只用于当次验签，不建立第二账户真源；前端、审计展示和普通 DTO 如需人类可读地址，只展示从 `account_id` 派生的 `ss58_address`。
- 公民选举/被选举范围由出生地、居住地和投票规则共同决定,不在公民档案中保存独立范围字段。

#### 4. 权限范围

OnChina 管理端只承认当前节点 active binding 绑定机构的链上 active admin 登录。工作台身份仅由 `institution_code + workspace` 表达，注册局复用同一路径，不得新增专用机构身份字段、独立身份表或独立授权分支。登录态的 `admin_level`、`scope_province_name`、`scope_city_name`、`scope_town_name` 和 `capabilities` 只承载链上身份派生的范围与可见能力，不得成为授权真源。

管理员登录反查、节点绑定确认、会话签发、周期撤权复查和管理员列表必须使用同一个链上分层解析器，并把每次解析固定到单一 finalized 区块。带 CID 管理员只认 CID 当前绑定 `account_id`；私权 LR 的有效 CID 可来自本机构法定代表人记录；冻结公权无 CID 管理员与私权非 LR 无 CID 管理员按名册 `account_id`；个人多签不进入 OnChina。岗位任职仍以名册 `account_id` 为关联锚点，不能把岗位锚点误当成换绑后的签名账户，也不能把本地 PostgreSQL 变成第二授权真源。

所有业务列表和 CRUD 必须将登录态转换为后端 scope 条件：

- FRG：按登录管理员在链上 `InstitutionRoleAssignments` 中的省岗位码限制；机构 CID 登记地址不得参与管理员权限派生。
- CREG：按本节点绑定的市级范围限制。
- 注册局辖区按链上身份派生的省 / 市范围限制；其它机构的专属业务分别执行自身岗位规则。
- 非注册局机构：按当前绑定的准确机构 CID 限制，地理范围不授予注册局办理资格。
- NJD 进入司法院工作台；普通公权、私权和非法人组织进入通用机构工作台。本机构信息与管理员展示沿用已接入入口；账户治理等专属业务由自身链上岗位授权，注册局公民、目录及登记接口不向其它机构开放。
- NRC / PRC / PRB 使用节点桌面端，不进入 OnChina 网页塔塔控制台。
- PMUL 和其它个人主体不进入 OnChina 网页塔塔控制台。

禁止先读取全量数据再在 Rust 或前端过滤。

节点绑定只保存链上身份键，不保存名称或行政权限派生值。会话必须绑定 `candidate_id`，每次请求都与当前 active binding 严格比对；任何缺失或不一致都失败关闭并清除会话。

#### 5. 扫码签名和验签

扫码协议只有 `QR_V1`。OnChina 的登录、Passkey 更新、管理员集合变更、机构登记链写动作和其它需要冷钱包确认的动作，都必须使用统一 QR 组件。

OnChina 的 `Session / Passkey / PasskeyColdSign` 是网站业务鉴权等级，不是钱包
账户 `SignMode`。`PasskeyColdSign` 中的冷签和未登录场景的单独冷签都由
CitizenWallet 执行，统一归为 `Cold`；Passkey 本身绝不进入 `SignMode`。强制冷签业务
只能接收 CitizenWallet 回扫，不得因浏览器或联网端存在私钥而改走 `Hot`。

业务模块只提供：

- 动作码。
- 签名原文或链上 call data。
- 签名摘要。
- 中文展示字段。

统一组件负责：

- 生成二维码。
- 解析二维码。
- 识别签名响应。
- 展示中文确认字段。
- 执行本地验签或提交前校验。

#### 6. 错误码

后端统一输出稳定 `error_code`。HTTP 状态只表达传输和登录态语义，业务错误必须通过 `error_code` 区分。

- 登录态无效：`401`
- 权限不足：`403`
- 输入无效、签名失败、challenge 过期、账户不匹配：业务错误码，不得伪装为登录态失效。
- 数据库错误：日志必须展开 SQLSTATE、message、detail 和 hint。

前端只允许在统一 notice 入口翻译错误。业务组件不得直接显示后端英文错误或浏览器原始异常。

##### 6.1 登录错误码

OnChina 管理员登录必须使用登录专用错误码，禁止继续把登录验签错误映射到绑定类 `ONCHINA_BIND_*` 口径。

| 错误码 | 中文提示 |
|---|---|
| `ONCHINA_TLS_CA_UNAVAILABLE` | 机构 CA 证书暂不可用，请确认链上中国平台已正常启动 |
| `ONCHINA_LOGIN_CAMERA_UNSUPPORTED` | 当前浏览器不支持摄像头扫码，请更换新版浏览器 |
| `ONCHINA_LOGIN_CAMERA_INSECURE_CONTEXT` | 当前页面不是 HTTPS 安全环境，无法使用摄像头 |
| `ONCHINA_LOGIN_CAMERA_PERMISSION_DENIED` | 摄像头权限被拒绝，请在浏览器中允许摄像头权限 |
| `ONCHINA_LOGIN_CAMERA_OPEN_FAILED` | 无法打开摄像头，请检查摄像头权限或设备占用 |
| `ONCHINA_LOGIN_QR_PARSE_FAILED` | 签名二维码解析失败，请重新扫码 |
| `ONCHINA_LOGIN_QR_NOT_RESPONSE` | 扫到的不是登录签名响应二维码 |
| `ONCHINA_LOGIN_QR_MISSING_FIELD` | 签名二维码缺少必要字段，请重新扫码 |
| `ONCHINA_LOGIN_QR_BAD_PROTO` | 二维码协议不正确，请使用新版公民钱包扫码 |
| `ONCHINA_LOGIN_QR_BAD_KIND` | 二维码类型不正确，请扫描公民钱包生成的签名响应 |
| `ONCHINA_LOGIN_QR_BAD_PUBKEY` | 签名账户格式无效 |
| `ONCHINA_LOGIN_QR_BAD_SIGNATURE` | 签名格式无效 |
| `ONCHINA_LOGIN_WALLET_CODE_INVALID` | 钱包码无效，请出示完整的 `QR_V1/k=5` 钱包码 |
| `ONCHINA_LOGIN_ORIGIN_REQUIRED` | 登录来源缺失，请刷新页面后重试 |
| `ONCHINA_LOGIN_SESSION_REQUIRED` | 登录会话缺失，请刷新页面后重试 |
| `ONCHINA_LOGIN_DOMAIN_REQUIRED` | 登录域名缺失，请使用 `https://onchina.local:8964` 访问 |
| `ONCHINA_LOGIN_CHALLENGE_CREATE_FAILED` | 登录请求保存失败，请稍后重试 |
| `ONCHINA_LOGIN_REQUEST_INVALID` | 登录请求内容不完整，请重新扫码 |
| `ONCHINA_LOGIN_RESULT_PARAM_REQUIRED` | 登录轮询参数缺失，请刷新页面后重试 |
| `ONCHINA_LOGIN_CHALLENGE_NOT_FOUND` | 登录二维码不存在或已失效，请重新生成 |
| `ONCHINA_LOGIN_CHALLENGE_CONSUMED` | 登录二维码已使用，请重新生成 |
| `ONCHINA_LOGIN_SESSION_MISMATCH` | 登录会话不匹配，请关闭多余页面后重新生成二维码 |
| `ONCHINA_LOGIN_CHALLENGE_EXPIRED` | 登录二维码已过期，请重新生成 |
| `ONCHINA_LOGIN_SIGNER_MISMATCH` | 签名账户和登录账户不一致 |
| `ONCHINA_LOGIN_SIGNATURE_VERIFY_FAILED` | 签名验签失败，请重新扫码签名 |
| `ONCHINA_LOGIN_COMPLETE_FAILED` | 登录签名响应处理失败，请查看服务日志 |
| `ONCHINA_LOGIN_RESULT_SAVE_FAILED` | 登录结果保存失败，请稍后重试 |
| `ONCHINA_LOGIN_RESULT_QUERY_FAILED` | 查询登录结果失败，请稍后重试 |
| `ONCHINA_LOGIN_ADMIN_NOT_ONCHAIN` | 当前钱包不是任何受 OnChina 支持机构的链上有效管理员 |
| `ONCHINA_LOGIN_DESKTOP_GOVERNANCE_UNSUPPORTED` | 国家储委会、省储委会、省储行使用节点桌面端管理，不支持登录链上中国平台 |
| `ONCHINA_LOGIN_PERSONAL_MULTISIG_UNSUPPORTED` | 个人多签账户不支持登录链上中国平台 |
| `ONCHINA_LOGIN_CHAIN_UNREACHABLE` | 无法连接区块链节点，请确认节点已启动并同步 |
| `ONCHINA_LOGIN_NODE_BINDING_REQUIRED` | 请先确认本节点绑定机构 |
| `ONCHINA_LOGIN_NODE_BINDING_MISSING` | 本节点尚未绑定机构，请重新扫码登录并确认绑定 |
| `ONCHINA_LOGIN_NODE_BINDING_INVALID` | 节点机构绑定状态异常，无法登录 |
| `ONCHINA_LOGIN_NODE_BINDING_QUERY_FAILED` | 节点机构绑定状态查询失败，请稍后重试 |
| `ONCHINA_LOGIN_NODE_BINDING_ALREADY_INACTIVE` | 节点机构绑定已解除，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_NOT_FOUND` | 节点机构绑定请求不存在，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_CONSUMED` | 节点机构绑定请求已使用，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_CHALLENGE_EXPIRED` | 节点机构绑定请求已过期，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_REQUEST_INVALID` | 节点机构绑定请求不完整，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_CANDIDATE_NOT_FOUND` | 所选机构不在本次登录候选中，请重新扫码登录 |
| `ONCHINA_LOGIN_NODE_BINDING_ADMIN_MISMATCH` | 当前管理员已不属于所选机构，无法绑定本节点 |
| `ONCHINA_LOGIN_PERSIST_FAILED` | 登录会话保存失败，请稍后重试 |

#### 7. 投票职责边界

OnChina 只签发投票引擎已经定义的资格凭证、人口快照或身份凭证。OnChina 不实现投票流程，不处理计票、状态推进、通过/否决判定，也不得内嵌投票引擎逻辑。

#### 8. 验收

```text
sqlite3 citizenchain/onchina/platform/codes/china.sqlite "PRAGMA integrity_check"
rg "旧独立身份系统名|backend/src|frontend/api|frontend/chain" memory AGENTS.md citizenchain/onchina --glob '!tasks/**' --glob '!docs/**' --glob '!**/node_modules/**' --glob '!**/dist/**'
```

---

#### 1. 功能需求

OnChina 前端是公民链内置多机构工作台，负责管理员登录、工作台分发、管理员目录、公民电子护照、机构登记、机构账户、资料库、审计和扫码签名确认。

#### 2. 当前结构

```text
citizenchain/onchina/
├── platform/       # 公共平台与共享功能
├── registry/       # 注册局办理
├── legislation/    # 立法业务
├── judicial/       # 现有司法工作台
└── private/        # 私权机构业务（membership 为基金会现有业务）
```

#### 3. 前端目录规则

- 功能模块的 API 调用放在所属模块 frontend，沿用当前 `<功能>_api.ts` 文件命名；平台公共请求工具只有一份。
- 通用 HTTP 封装只允许放在 `onchina/platform/frontend/helpers_http.ts`，不得承载业务接口。
- 二维码解析、生成、签名响应识别和确认页字段展示必须走现有 `core` 统一实现；链交易固定复用 `onchina/platform/frontend/core_useChainSign.tsx` 的请求二维码、CitizenWallet 一次签名响应二维码和 OnChina 回扫流程。
- 摄像头设备层唯一使用仓库根 `citizenchain/crates/scanner/` 的 `ScannerView`，固定走
  `jsQR + canvas`。`onchina/platform/frontend/core_CitizenSignaturePanel.tsx` 与 `onchina/platform/frontend/core_ScanAccountModal.tsx` 只负责
  各自入口状态、允许码型、提示和业务动作；禁止恢复产品内 `cameraScanner.ts`、
  `BarcodeDetector` 或直接导入 `jsqr`。产品 `.npmrc` 固定 `install-links=true`，使共享
  包及其依赖独立安装在 OnChina 依赖树，不与 Node 的并发安装互相清理。
- 四端节点 CI 的统一工具链指纹覆盖仓库全部 `package-lock.json`，并继续实际执行共享扫码适配器、节点前端与 OnChina 前端门禁。
- 业务组件不得自己解析 `QR_V1`，不得自己翻译扫码端字段名。
- `onchina/platform/frontend/core_citizenQr.ts`严格映射生成器的1..5码型，各业务入口按自身允许码型拒绝其他输入，不放宽共享注册表。
- 前端不得恢复独立 `frontend/api/` 或 `frontend/chain/` 业务目录。
- 机构差异不得继续塞进 `App.tsx`。新增机构 UI 必须进入 `onchina/<所属模块>/frontend` 的对应页面文件，`App.tsx` 只保留登录态、布局和工作台路由。
- 前端行政权限只消费登录完成或 `/api/admin/auth/check` 返回的 `scope_*`；不得从机构详情、管理员目录或本地缓存反推权限。服务端判定会话失效时必须清除整个前端登录态并重新登录。

#### 4. 页面和文案规则

- 公权机构、公安局和私权机构列表必须展示连续序号。
- 机构详情页身份字段统一显示为 `身份ID`。
- 公民列表和详情页身份字段统一显示为 `身份CID`;护照号字段显示为 `护照号`。
- 公民列表姓名由 `family_name + given_name` 组合展示；新增弹窗必须拆成“姓”和“名”两个必填输入框。所有人员模型统一使用这两个字段，不保存拼接姓名或带主体前缀的别名。
- 机构详情页不得展示 `SubjectProperty 类型` 或机构链上状态。
- 账户链上状态只允许在机构账户列表展示。
- 扫码确认页左侧分类名必须是中文，右侧内容必须是用户能核对的值。
- 账户字段必须展示 SS58 地址，不得把原始公钥 hex 当作普通用户字段展示。
- 联邦注册局管理员进入公民入口时先显示本省城市卡片,进入某市后默认分页显示该市全部公民;市注册局管理员直接进入本市公民列表。
- 公民列表页标题上方显示 `xx省 · xx市`,列表工具栏左侧显示“公民列表”,右侧放搜索框和“新增公民”按钮;搜索框为空时表示当前市全部公民。
- 公民列表使用 cursor 分页,不得恢复 offset 分页或“空搜索清空列表”的旧行为。
- 点击公民列表行进入公民详情页,不得再弹出公民详情 Modal。
- 新增公民弹窗不得出现手填身份 CID、手填护照有效期、居住省市选择或投票账户公钥输入框。
- 新增公民弹窗必须展示当前办理城市对应的居住省市,只允许选择居住镇;出生省市镇必须选择。
- 新增公民请求只提交 `province_name / city_name / town_code` 和 `birth_*` 字段;不得向后端发送旧的第二套居住字段。
- 旧机构直接创建流程已关闭：共享创建弹窗当前只保留资料录入布局，按钮固定禁用并说明必须原子提交 LR、初始治理岗位、不可变权限、初始任职和投票规则；前端不得调用旧 API、生成旧签名二维码或提交 `0x1e05/0x1f05`。
- 第 6 步接入新机构创建业务时，表单必须在同一业务载荷中收集机构资料、至少两个 admins、至少一个初始治理岗位及其权限和任职、初始投票规则；动态岗位码仍由 runtime 生成，前端不得手填或预生成。
- 所有 `PASSKEY_COLD_SIGN` 正式业务提交必须同时携带冷签 grant 和 Passkey assertion。创建机构、创建/删除账户、公民身份上链等可直接使用 `onchina/platform/frontend/admins_securityApi.ts::createColdSignSubmitHeaders`；已由组件先取得 grant 的资料上传/删除、机构详情更新等必须使用 `securityGrantSubmitHeaders`。业务模块禁止手写 `x-cid-security-grant` 或只提交 grant 不提交 Passkey assertion。
- 机构资料上传、资料删除、机构详情更新的扫码授权 payload 必须与后端 `grant_payload` 逐字段同形；资料上传使用 `target/file_name/doc_type/file_size`，资料删除使用 `target/doc_id/file_name`，机构详情更新使用 `target/cid_number/cid_full_name/parent_cid_number/family_name/given_name/legal_representative_cid_number/legal_representative_photo_path`。
- 每个机构必须存在唯一 `LR / 法定代表人` 岗位且允许空缺；前端不得要求用户手填 LR 岗位码，也不得把“管理员”当成统一岗位名。
- 股份公司等私权机构只有所有最小必填字段及至少两个不重复管理员 `account_id` 均合法时才启用生成按钮。协会 `SFAS` 必须显式选择盈利或非盈利，前端不得固定为非盈利。
- 公民详情页负责链上身份上链:未满 16 周岁、无选举资格或档案非正常时禁用推送;推送时必须先选择“投票身份”或“参选身份”,再录入公民 `account_id`、生成目标公民签名二维码,验签后展示注册局管理员链上交易二维码。
- “投票身份”提交 `identity_level=voting`,链交易为 `CitizenIdentity.register_voting_identity(10.0)`;“参选身份”提交 `identity_level=candidate`,链交易为 `CitizenIdentity.upgrade_to_candidate_identity(10.1)`。
- 公民详情页底部必须显示公民独立资料库,资料类型固定为“护照相片 / 出生证明 / 监护人护照 / 其他材料”。该区域只调用 `onchina/registry/frontend/citizens_api.ts` 的公民资料接口,不得复用机构资料库 `onchina/platform/frontend/docs_DocsLibrary.tsx`。
- 投票账户只有一个输入框,用于填写 SS58 地址或点击扫码图标回填账户;提交后列表和详情只显示 SS58 地址。
- 机构管理员列表使用 `onchina/platform/frontend/admins_InstitutionAssignmentCard.tsx` 展示由 `family_name + given_name` 合并的姓名、管理员账户、岗位、任期、任职来源和余额；没有岗位的管理员仍显示，岗位栏为空。同一账户在同一机构有多个岗位时按任职分别展示。岗位权限不作为卡片字段，由对应业务模块按硬规则决定。
- 注册局管理员列表保持既有表格布局。非注册局机构的本机构管理员列表必须使用卡片墙，桌面端一行两张管理员卡片，小屏一行一张；不得再显示“管理员信息 / 操作”两列表头。
- 非注册局本机构管理员卡片中，当前登录管理员自己的 passkey 按钮文案固定为“密钥”，按钮放在“余额”行右侧靠右；未设置 passkey 时继续用红点提示。
- 管理员列表不得提供本地合并姓名编辑入口；登录态、注册局目录、机构创建和治理输入统一传递 `family_name / given_name`，只在 UI 渲染时合并。联邦注册局管理员岗位目录完全只读，换届由治理业务写入 entity 后自动反映。市注册局本地登记目录仍可新增/删除，但不得成为链上管理员资格或岗位真源。
- 非注册局工作台“显示”页必须调用 `/api/admin/own-institution` 展示本机构完整信息，至少包括机构全称、简称、身份ID、机构码、机构类别、主体状态、行政层级、辖区、盈利属性、主账户、主账户地址、主账户状态、账户数量和创建时间；法定代表人姓名、CID、账户读取 entity 链上公开字段，有值时显示；证件照片仍为 OnChina 链下字段。教育分类、私权类型、合伙类型、所属法人等字段有值时再显示。
- 立法法律列表和详情页的版本显示必须优先使用后端 `LawView.versionTitle/versionTitleEn`；只有链上版本标签为空时才显示 `vN`，不得在前端硬编码 `v1=创世版`。
- 立法新法、修法、废法编辑器必须显式收集“提案发起岗位码”并以 `proposerRoleCode` 提交；当前管理员登录成功不代表拥有立法发起权限，前端不得自动选择岗位或提供投票引擎选择器。

#### 5. 提示入口

所有用户提示统一由 `citizenchain/onchina/platform/frontend/helpers_notice.ts` 管理。业务组件只允许调用统一 notice 方法，禁止直接调用 Ant Design `message.*`、`Modal.confirm`、`Modal.warning` 或浏览器 `alert`。

统一入口负责：

- 同一时刻只显示一个提示。
- 将扫码签名、网络和后端错误翻译为中文。
- 优先按后端 `ApiError.error_code` 映射业务提示；例如注册局主账户缺失使用 `ONCHINA_REGISTRY_MAIN_ACCOUNT_MISSING`,不得让稳定业务错误退化为通用 400 文案。
- 将用户取消类错误显示为取消提示或静默。
- 将无法识别的英文错误降级为中文兜底提示。

业务组件捕获异常时必须把原始错误对象传给 notice 入口，不得先取 `error.message` 再传入。

`NotAllowedError` 是浏览器通用取消/拒绝错误，不得在 notice 全局层直接翻译成摄像头权限错误。摄像头扫码、passkey/WebAuthn 等浏览器能力必须在各自客户端封装中给出具体中文原因。

#### 6. 首次 HTTPS 信任

登录页和已登录后台必须在当前页面不是可信 HTTPS 安全上下文时提供机构 CA 证书下载入口，指向 `/api/platform/ca-certificate`。当前页面已经是 `https:` 且 `window.isSecureContext=true` 时，说明浏览器已信任当前 OnChina HTTPS 页面，不得继续显示 CA 下载安装提示。未信任本节点证书时，页面应明确提示先安装机构 CA 证书并重开浏览器；安装完成前不得把 passkey 失败误提示为摄像头权限问题。

macOS 导入机构 CA 时必须提示导入“系统”钥匙串并将证书设为“始终信任”；如出现 `-25294`，先在钥匙串中删除同名旧证书，再下载当前节点的新 CA 证书重新导入。

passkey 客户端在调用 `navigator.credentials.create/get` 前必须检查 `window.isSecureContext`、`PublicKeyCredential` 和 `navigator.credentials`，并分别提示“证书未信任 / 浏览器不支持 / 用户取消”。

摄像头扫码客户端在调用 `getUserMedia` 前必须检查 HTTPS 安全上下文和 `navigator.mediaDevices.getUserMedia`，避免把证书未信任误判成摄像头权限被拒绝。

#### 7. 扫码签名

管理员扫码登录、Passkey 更新、机构内部治理、注册局直接登记机构管理员和其它链写动作统一使用 `QR_V1`。

- OnChina 页面生成 `sign_request`。
- CitizenWallet 扫描、只签名一次并显示 `sign_response` 响应二维码。
- OnChina 页面回扫签名响应，统一完成验签、dry-run、广播和进块确认。
- 公权/注册局详情页显示“机构治理”tab：可构造本机构治理；注册局管理员额外显示“注册局直接登记管理员”动作。
- 私权机构详情页显示同一“机构治理”tab，只构造本机构内部治理，不开放注册局直接登记按钮。
- 动态岗位创建不得显示或提交岗位码；runtime 按所属 pallet 的 `MODULE_TAG` 和链上 nonce 生成。发起既有业务提案时，页面必须独立显示并提交当前任职的 `proposer_role_code`，不得把“治理中创建/改名的目标岗位”和“本次提案发起岗位”混成一个字段。

2026-07-19 第 5B：公权/私权机构治理页面新增必填“提案发起岗位码”，平台调价页面同样独立提交 `proposer_role_code`（公民链基金会可预填 `GENESIS_PRODUCT_MANAGER`）。业务模块固定选择内部投票引擎，前端不得提供引擎选择器；管理员登录成功本身不代表拥有任何业务发起权限。
- 法定代表人任命/更换只填公民 CID；后端从公民档案读取 `family_name/given_name/account`。解除法定代表人使用“解除法定代表人并清空链上公开结构”复选框，前端禁止和公民 CID 同时提交。

CitizenApp 不承担管理员登录 QR 职责。前端文案不得引导用户使用 CitizenApp 处理管理员登录签名请求。

#### 8. 工作台权限

前端只按后端会话下发的 `workspace` 和 `capabilities` 渲染工作台，不在组件内重新推导业务权限。当前目标状态如下：

- `FRG` 登录且已设置 passkey 后进入 `registry` 工作台，显示完整注册局业务入口，包含公民、私权机构、教育机构、公权机构、市注册局和联邦注册局。
- `CREG` 登录且已设置 passkey 后进入 `registry` 工作台，也显示联邦注册局入口，但该入口只能展示本省联邦注册局管理员列表，不能显示编辑、更换等操作入口。
- `NJD` 登录后进入 `judicial` 工作台，不复用注册局 tab。页面按“操作 / 显示 / 记录”分类；显示页只读展示本机构完整信息和本机构链上 active admin 卡片列表，当前登录管理员自己那张卡片显示“密钥”按钮。
- 私权机构登录后进入 `private` 工作台，只展示本机构信息、链上 active admin 卡片和后端按准确 CID 下发的授权模块，不得复用注册局目录与登记 UI。
- 普通公权机构进入 `public`、立法机构进入 `legislation`、非法人组织进入 `unincorporated` 工作台；前端可复用 `GenericWorkspace` 显示壳，但后端工作台种类不得退化成 `generic` 权限语义。
- 只有登录态 `institution_cid_number` 与 finalized `PlatformCidNumber` 精确一致时才显示平台会员价格模块。调价固定使用 `onchina/private/frontend/membership_api.ts` prepare、`onchina/platform/frontend/core_useChainSign.tsx` 一次签名响应回扫和统一 chain submit，不得自建签名弹窗或提交器。
- `NRC`、`PRC`、`PRB` 不显示前端工作台入口，登录阶段返回节点桌面端专用错误。
- `PMUL` 和其它个人主体不显示前端工作台入口，登录阶段返回个人多签不支持错误。
- 注册局管理员未设置 passkey 时，只显示自己机构的管理员列表入口，用于先完成本机 passkey 设置；设置完成后再显示完整注册局业务入口。
- 联邦注册局管理员列表的操作列只允许 `FRG` 看到；`CREG` 进入同一入口时必须是只读表格。

#### 9. 验收

```text
npm --prefix citizenchain/onchina/platform/frontend run build
rg "旧独立身份系统名" citizenchain/onchina/platform/frontend --glob '!node_modules/**' --glob '!dist/**'
rg "NotAllowedError.*摄像头" citizenchain/onchina/platform/frontend --glob '!node_modules/**' --glob '!dist/**'
```

涉及登录、权限、扫码或页面展示的变更，必须启动真实本地服务并检查真实页面；只通过 `npm run build` 不算完成。

管理员模型现统一为 `account_id + cid_number + family_name + given_name`，登录态、Header、机构治理批量输入、市注册局管理员新增以及联邦/本机构管理员列表必须使用同一字段布局。ADR-039 第 3 步已关闭旧机构首次登记提交路径；当前创建按钮固定禁用，不能把历史验收解读为旧创建流程仍有效。TypeScript检查与Vite构建必须针对当前源码独立验收，历史通过记录不能代表当前重构已通过。

---

#### 1. 定位

OnChina 是公民链内置的链上中国多机构工作台，负责各机构通过“节点端 + 浏览器”进入本机构后台。注册局、司法院、立法院、行政机关、学校、公益组织、公司等机构共用同一套登录、绑定、权限和运行态框架；具体 UI 由当前登录 `account_id` 所属的链上机构管理员身份决定。

注册局不再是 OnChina 根 UI 的同义词，而是 `workspace` 机构工作台中的一种类型。OnChina 仍负责 CID 号、行政区、注册局既有业务、公民电子护照档案、机构登记、机构公开查询、链上公民身份提交和链侧调用生成；非注册局机构按自己的工作台能力进入“操作 / 显示 / 记录”三类页面，不复用注册局业务 UI。

OnChina 不是第五个产品。仓库产品只保留：

- 公民 `citizenapp`
- 公民链 `citizenchain`
- 公民钱包 `citizenwallet`
- 官方网站 `citizenweb`

##### 1.1 账户标识目标契约

- 公民身份唯一主键是永久 `cid_number`；当前 `account_id` 只是该 CID 当前版本的签名、
  鉴权和付款凭证。换绑 finalized 后，新账户直接接管同一 CID，旧账户、旧私钥和旧设备
  不参与后续控制权判定。
- PostgreSQL、Rust、TypeScript、JSON、缓存和链上调用中的单一账户字段统一为 `account_id`；多账户结构使用准确的 `<role>_account_id`。
- 账户与 32 字节公钥的文本形式统一为小写 `0x` 加 64 位十六进制；`ss58_address` 仅作派生展示值，不作为登录、权限或数据库关系真源。
- 登录验签必须从 `signer_public_key` 得到 `signer_account_id`，再读取链上 admins、有效岗位任职和岗位权限；节点绑定或本地投影不得产生第二套授权。
- OnChina PostgreSQL 业务库按最终 schema 重建，不写迁移、双读或兼容列；密钥和 Secret 不在删除范围。
- 2026-07-22 已删除并重建本机 `127.0.0.1:5433/onchina` 业务库；重建后旧账户列和旧账户索引均为零，账户与公钥格式约束按最终 schema 生效。
- 完整目标与进度见 ADR-040 和任务卡 `账户官方统一.md`。

##### 1.2 与 CitizenApp 边缘架构的边界

OnChina 可以向 CitizenApp 或 Cloudflare 边缘层提供公开目录、链上投影、机构资料查询和受控服务端聚合能力，但不得成为 CitizenApp 的链上状态真源。

固定边界：

- CitizenApp 链上余额、身份、提案、投票和交易成功判断以端上轻节点读取的 finalized runtime storage 为准。
- OnChina / Citizen API 可以广播 CitizenApp 已经本地签名完成的 extrinsic，但不接触私钥、不修改交易载荷、不把广播成功解释为链上成功。
- OnChina 的主入口仍是机构内网工作台 `https://onchina.local:8964`；如后续提供公网投影服务，必须以独立受控服务节点、反向代理、白名单、限流和审计为边界，不直接暴露国储会核心节点 RPC。
- OnChina 不恢复旧独立后端目录，不新建 `backend/src/`、`backend/chain/` 或 `frontend/chain/` 业务壳。

#### 2. 技术栈

- 后端：Rust + Axum + PostgreSQL
- 前端：React + TypeScript + Vite + Ant Design
- 链交互：Substrate RPC、SCALE、统一 QR_V1 扫码签名协议
- 行政区开发真源：`citizenchain/onchina/platform/codes/china.sqlite`

#### 3. 启动流程

0. 节点桌面端默认不启动 OnChina；用户在节点设置页“链上中国平台”行点击“启动”并二次确认后，节点才拉起 OnChina 子进程。
1. 读取 `DATABASE_URL`、`ONCHINA_CHINA_DB`、链 RPC 和安全配置。
2. 直接初始化 PostgreSQL 最终 schema、约束和索引，不执行迁移或字段回填。
3. 为 `subjects/citizens/citizen_documents/gov/private/accounts/docs/audit/institution_admins` 创建 `province_code` 分区。
4. 读取随包只读行政区 SQLite，并断言 SQLite 省表与 runtime primitives `PROVINCE_CODE_INFOS` 一致。
5. 初始化登录、节点绑定和链路运行态结构化表。
6. 初始化链上公权机构查询投影，并按 finalized 区块启动公民绑定投影和交易索引 worker。

schema 初始化和链上业务投影必须分离。schema 入口只允许幂等创建最终结构与分区，不得携带 `ALTER/DROP`、旧列清理或兼容读取；公权机构目录只能从链上唯一真源投影。

局域网访问入口固定为 `https://onchina.local:8964`。OnChina 监听 `0.0.0.0:8964`，通过 mDNS 广告 `onchina.local`，TLS 自签证书目标主机为 `onchina.local`。节点设置页只负责启动服务，不自动打开浏览器；管理员在自己的电脑浏览器中访问该固定入口。

登录身份不由节点安装前预配置。管理员使用 CitizenWallet 扫码后，后端校验 `signer_public_key` 与签名，得到并严格比较 `account_id`，再查询链上 active admin 所属机构；该账户属于一个机构时直接进入本机构工作台，属于多个机构时先选择机构再进入对应工作台。未绑定节点返回候选机构并要求浏览器二次确认绑定，已绑定节点只允许该机构 active admin 登录。节点绑定只作为本机机构归属结果缓存，权限真源仍是链上 active admin 集合。解绑或换机构必须走 `NODE_BINDING_UNBIND` 扫码签名安全动作：当前本机会话管理员发起，CitizenWallet 签名一次并显示响应二维码，OnChina 回扫后停用 active binding 并清退本节点管理员会话，再重新登录绑定新机构。

##### 3.1 机构工作台框架

- 后端 `citizenchain/onchina/platform/` 是当前登录机构工作台清单的唯一生成层，只输出 `workspace_kind / workspace_title / workspace_sections / workspace_modules`，不保存第二份管理员授权真源。实例级模块必须按登录态准确 `institution_cid_number` 和 finalized 链状态判定。
- 平台工作台外壳与装配为 `onchina/platform/frontend/workspace_WorkspaceShell.tsx` 和 `onchina/platform/frontend/workspace_WorkspaceRouter.tsx`；注册局、私权和司法页面分别归所属模块 frontend，保持原有 UI、入口和权限条件。
- 工作台顶层分类固定为 `operations`（操作）、`display`（显示）、`records`（记录）。操作放本机构可发起的提案、投票、管理员变更等动作；显示放本机构身份、权限和管理员；记录放登录、链写、投票、管理员变更等事实记录。
- 现有公权机构统一在创世阶段上链。OnChina 不在运行期生成既有公权机构目录，只读取链上机构和链上 active admins，并用本地 `subjects/accounts/admins` 投影补齐展示字段。
- 注册局工作台必须保持既有功能和 UI；私权机构不得看到注册局公民、机构目录和登记入口。新增司法院、立法院、学校、公司、公益组织等机构 UI 时，只能新增或扩展对应工作台目录，不得把机构差异重新塞回 `onchina/platform/frontend/App.tsx`。
- 平台会员价格是准确 CID 实例模块：只有登录机构 CID 等于 finalized `SquarePost::PlatformCidNumber` 时，清单才包含 `platform_membership_price`。OnChina 只读取 finalized 价格，不在 PostgreSQL 保存第二份价格。

#### 4. 行政区和 CID 号真源

- 国家码、省级行政区码和机构码常量唯一真源：`citizenchain/runtime/primitives/cid/code.rs`。
- 市、镇和地址段开发真源：`citizenchain/onchina/platform/codes/china.sqlite`。
- CID 号生成和校验唯一源码目录：`citizenchain/onchina/platform/codes/`。

生产环境中 `ONCHINA_CHINA_DB` 固定指向随包只读 SQLite。市镇地址段变更只能修改开发库并重新发布安装包，禁止运行期在线编辑行政区。

#### 5. 结构化表

- `ids(cid_number, kind, province_code, city_code)`：全局身份 ID 索引。
- `subjects`：主体公共展示字段，按省分区；机构行缓存 `cid_full_name/cid_short_name`、行政区、业务状态、私权分类，以及链上公开法定代表人的 `family_name/given_name/cid_number/legal_representative_account_id`。法定代表人照片只属于 OnChina 链下资料，不进入 `InstitutionInfo`；数据库不保存拼接姓名列。
- `citizens`：以 `cid_number` 归属公民档案、姓、名、护照号、`province_code/city_code/town_code`
  居住/办理地、`birth_*` 出生地和电子护照有效期字段，按省分区。可空 `account_id`
  只是当前 finalized 绑定的本地投影，并与单调 `binding_revision`、
  `binding_finalized_block_number`、`binding_finalized_block_hash` 同步保存；不得用本地旧账户
  回退覆盖链上缺失或较新版本。数据库不保存 SS58，需要展示时由当前 `account_id` 派生
  `ss58_address`。
- `citizen_documents`：公民独立资料库元数据,资料类型固定为“护照相片 / 出生证明 / 监护人护照 / 其他材料”,文件本体在磁盘；不得与机构 `docs` 共表。
- `gov`：公权机构扩展字段，按省分区；链上投影写 `source='CHAIN'`，人工公权机构写 `source='MANUAL'`。
- `private`：私权机构扩展字段，按省分区；分类字段使用 `private_type/partnership_kind/has_legal_personality`。
- `accounts`：机构账户，主键按 `(province_code, cid_number, account_name)` 收敛。
- `docs`：机构资料库元数据，文件本体在磁盘。
- `audit`：结构化审计记录，按省分区。
- `admins`：机构管理员本地元数据缓存；账户列统一为 `account_id`，创建来源账户为 `creator_account_id`。所有公权、私权机构的链上管理员项统一为 `account_id + cid_number + family_name + given_name`；本地缓存不是成员资格、公民身份或岗位权限真源。
- `chain_sign_sessions`：公民/机构链交易的短期签名会话，保存 `account_id`、call_data、nonce、signing_hash 及恢复和正式投影所需上下文；它不是业务草稿，不参与 CID/名称占用，提交前拒绝可以删除；已经尝试网络提交时保留恢复锚点，直到确认 finalized 成功且本地投影完成后删除。
- `node_institution_bindings`、`node_binding_challenges`：本节点首次登录绑定机构和绑定确认挑战；绑定表使用 `bound_account_id`，挑战使用 `account_id`，并只保存链上身份键，禁止保存机构名称和省市镇权限派生值。解绑 / 换机构由 `NODE_BINDING_UNBIND` 的 Passkey 本地动作停用 active binding。
- `admin_sessions`：会话以 `account_id` 保存账户身份，并保存签发时的 `candidate_id`；每次鉴权与当前 active binding 严格比对，解绑、重绑或候选不一致时立即删除会话，不允许回落。
- `admin_login_sign_requests`、`admin_qr_login_results`、`admin_action_challenges`、`admin_security_grants`：登录和扫码签名运行态。管理员登录先扫描完整 `QR_V1/k=5 wallet_contact` 钱包码；后端按协议读取规范 account_id，并在同一 finalized 状态反查当前 Active 管理员身份后，生成 `QR_V1/k=1,a=1` 定向请求。`b.u` 必须是目标账户公钥且数据库 `account_id` 不得为空。签名响应只能证明持有该目标账户私钥，不得改写目标账户。
- `chain_requests`、`chain_nonces`、`tx_records`、`tx_indexer_state`：链路幂等、防重放和索引运行态；交易发送方、接收方固定使用 `sender_account_id/recipient_account_id`。

`cid_number` 是唯一且不可变的身份标识。不得新增 `identity_key`、`generation_key` 等第二身份键。

##### 5.1 公民 finalized 绑定单源

- `citizenchain/onchina/platform/chain/citizen_identity.rs` 是公民公开身份快照的唯一链读取
  入口。同一次读取必须固定在一个 finalized 区块，并同时读取 `CidRegistry`、
  `AccountIdByCid`、`BindingRevisionByCid`、`CidByAccountId`、
  `VotingIdentityByCid` 和 `CandidateIdentityByCid`。
- 正反绑定不闭环、revision 缺失或为零、竞选身份没有投票身份、非创世块缺少
  `Timestamp.Now` 时一律失败关闭。创世块没有 timestamp inherent，区块号 0 可按链上时间
  0 处理。
- 本地投影只接受更高 revision，或同 revision 且账户完全一致的幂等写入；revision 回退和
  同 revision 改账户必须拒绝。Revoked 投影清空当前 `account_id`，但保留 revision 和
  finalized 锚点。
- 创世法定代表人的本地档案也必须通过同一 finalized 快照取得绑定账户和 revision，
  不得从基金会管理员常量硬编码当前账户；链下 `creator_account_id` 只作创建来源审计，
  不得成为当前控制权或授权真源。
- 注册局全局查询接口固定为
  `GET /api/admin/citizens/{cid_number}/binding`。接口只向已登录的 FRG/CREG 管理员返回
  链上公开绑定，不读取或泄露链下护照、资料、通讯录等档案；所有注册局都从链上同一
  finalized 真源查询，不按本地办理地限制。
- 占号、注册局换绑、身份推送和吊销只有在目标 extrinsic finalized 且
  `ExtrinsicSuccess` 后，才读取该交易所在 finalized 区块的完整快照回写本地。前端动作完成后
  必须重新调用全局查询接口，不得把扫码账户或 prepare 会话字段直接显示为已生效绑定。

旧机构直接创建入口已关闭：OnChina 当前创建 API 固定返回 501，前端创建按钮固定禁用，不生成 `0x1e05/0x1f05` 或旧签名会话。后续如恢复机构登记，公权与私权机构管理员必须统一使用 `Admin { account_id, cid_number, family_name, given_name }`，机构治理阈值必须作为 entity 机构配置独立提交，不能由管理员人数或岗位数推导；具体登记规则需另立方案确认。

#### 6. 高并发策略

OnChina 高并发目标建立在结构化表、组合索引、省分区和省市范围查询之上。

必备原则：

- 后台列表必须在 SQL 层携带 `province_code` / `city_code` 条件。
- 联邦注册局机构 `admins` 查询本省业务数据时必须带 `province_code`。
- 市注册局机构 `admins` 查询本市业务数据时必须带 `province_code + city_code`。
- 页面列表只读持久化结果，禁止同步触发全量对账。
- 高频公开查询可增加短 TTL 缓存，但缓存不得成为主数据。

必备索引：

- `subjects(province_code, city_code, kind, status, cid_number)`
- `subjects(province_code, city_code, cid_full_name)`
- `citizens(province_code, city_code, created_at DESC, id DESC)`
- `citizens(province_code, city_code, cid_number, passport_no, account_id)`
- `citizens(province_code, city_code, town_code, created_at DESC, id DESC)`
- `citizen_documents(province_code, cid_number, uploaded_at DESC, id DESC)`
- `gov(province_code, city_code, town_code, institution_code)`
- `private(province_code, city_code, private_type, cid_number)`
- `accounts(province_code, cid_number)`
- `docs(province_code, cid_number, uploaded_at DESC)`
- `audit(province_code, city_code, created_at DESC)`
- `admins(institution_code, city_name)`
- `admins(account_id)`（严格规范文本，不做 `lower(...)` 兼容）

#### 7. 管理员和安全

管理员唯一字段统一为 `admins`。OnChina 不恢复独立管理员身份表、授权真源或授权分支。

- 登录态与本地缓存保留 `account_id`、公民 CID、姓、名展示字段；链上机构管理员解码无论来自公权还是私权 pallet，都统一为 `account_id + cid_number + family_name + given_name`。数据库不建立第二套公民 CID 或授权真源。
- 机构管理员列表联合读取链上 `admins` 人员集合与 entity 机构岗位任职；没有岗位的管理员仍必须返回，但管理员账户本身不具备机构业务权限。本地联系方式、照片和 Passkey 仅是私密资料，不得成为管理员资格或岗位真源。
- 联邦注册局管理员目录从 `PublicAdmins::AdminAccounts` 读取账户集合，并从 `PublicManage::InstitutionRoleAssignments` 的 `PROVINCE_COMMISSIONER_<省码>` 岗位取得 43 省归属；本地不保存第二份省组权限真源。
- FRG 管理员的 `scope_province_name` 只由其链上省岗位码派生；FRG 机构 CID 的登记地址只作机构展示元数据，禁止覆盖管理员授权省份。
- 联邦注册局和市注册局不提供“编辑本地管理员姓名”的入口。联邦注册局岗位任职目录完全只读，换届由治理业务写入 entity；下级市注册局本地登记目录新增/删除仍走安全动作，但不能替代链上管理员资格校验。
- 登录态：用于普通读取和低风险操作。
- `PASSKEY_COLD_SIGN`：用于管理员安全写操作、Passkey 更新、管理员集合变更、节点解绑和链写入二次确认。
- 扫码请求：统一使用 `QR_V1 / k=1 sign_request`。
- 签名响应：统一使用 `QR_V1 / k=2 sign_response`。

业务模块只传入动作码、签名原文、摘要和展示字段，不得自己包装二维码协议。

链交易提交必须先走 `system_dryRun` 预检。预检返回 `InvalidTransaction`、RPC
`RuntimeApi` 错误或 wasm trap 时，OnChina 直接返回“交易未提交”的失败结果，禁止继续调用
`author_submitExtrinsic`。这样浏览器只看到一次明确失败，不会把 runtime 校验崩溃再次透传成提交阶段错误。

#### 8. 前端规则

前端所有用户提示统一走 `citizenchain/onchina/platform/frontend/helpers_notice.ts`。业务组件不得直接调用 Ant Design `message.*`、`Modal.confirm`、`Modal.warning` 或浏览器 `alert`。

机构详情页身份字段统一显示为 `身份ID`，不得使用代码框包裹，不得展示 `SubjectProperty 类型` 或机构链上状态。机构链上状态只属于机构账户，允许在账户列表展示。

扫码确认页的左侧分类名必须是中文，右侧内容是用户可核对的值；机器字段不得直接渲染给用户。

##### 8.1 法律文库展示

- 法律文库只读详情页展示公民宪法时，`law_id=0` 的标题、章、节、条、款均以链上结构化正文为唯一展示真源。
- 章、节、条标题直接显示链上 `title/titleEn`，只有空标题才使用兜底标题；款正文直接显示链上 `Clause.text/textEn`，不得在 UI 层额外拼接“第 x 款 / Paragraph x”。
- 后端 `LawView.immutableArticleNumbers` 只从 `LegislationYuan.ConstitutionImmutableManifest` 投影宪法不可修改条款号，普通法律保持空数组。
- 后端 `LawView.versionTitle/versionTitleEn` 只从 `LegislationYuan.LawVersionLabels[(law_id, version)]` 投影版本标签；公民宪法创世版本显示“创世版 / Genesis Edition”，无标签版本继续显示 `vN`。
- 前端不可修改条款徽章中文固定显示“不可修改条款”，英文固定显示“Immutable Clause”。
- 法律详情页切换中英文时，徽章必须紧跟当前语言的条标题：中文模式只在中文条标题后显示“不可修改条款”，英文模式只在英文条标题后显示“Immutable Clause”，禁止在同一个徽章内混排中英文。
- 徽章必须与当前语言条标题行内垂直居中；中文徽章使用更小字号，避免抢占条标题视觉层级。
- 法律编辑弹窗里的结构定位只允许显示“章序 / 节序 / 条序 / 款序”，不得把结构序号伪装成正文标题。

#### 9. 发布和 CI 边界

OnChina 属于 `citizenchain`。不再保留独立 旧独立身份系统 CI、独立 旧独立身份系统安装包 或独立产品发布入口。

- 修改 `citizenchain/onchina/**`：执行 OnChina 后端编译、测试和真实 HTTP 验收。
- 修改 `citizenchain/onchina/*/frontend/**`：执行前端 build，并通过真实页面检查关键流程。
- 修改节点桌面端 OnChina 启动入口：同步检查 `citizenchain/node/src/onchina_proc.rs`、`citizenchain/node/src/settings/onchina_platform.rs` 和节点设置页，确认 OnChina 不随节点默认启动。
- 涉及 QR、签名、链交易载荷、CID 号格式时：必须同步更新 `CitizenChainOnChina.md` 和相关端实现。

#### 10. 验收口径

涉及 API、数据库、登录、权限、扫码或页面展示的任务，必须使用真实本地服务、真实 PostgreSQL、真实 HTTP 接口或真实页面验收。只通过编译、类型检查或前端 build 不算完成。

CID 身份总任务第 5 步于 2026-07-30 使用当前源码 fresh runtime、独立链端口、两套独立
PostgreSQL 和两个不同市注册局 OnChina 实例完成真实验收。两套实例均投影 49,593 个机构和
99,232 个机构账户，并对 `CN220-CTZN2-198805200-2026` 返回完全一致的 finalized 绑定：
Active、`binding_revision=1`、当前账户
`0x0cb1d05c0c9c7f05679b60d6f24c7e5719a3985264e41c5e899d4822dca4b06b`，区块和创世哈希
均为 `0x49622cb851a0815af75573e281b992565cb31df509c2e3d3b847858c351ef46e`。该哈希只是本步
隔离 fresh 验收锚点，不是正式创世冻结结果。

本步同时确认：当前仍在运行的旧冻结链 WASM 没有 `BindingRevisionByCid` 元数据，新
OnChina 会按设计返回失败而不是回退旧账户。正式环境必须等后续唯一正式创世重生并冻结后
整体切换，禁止把新 OnChina 单独部署到旧 runtime。

账户标识统一第 10 步于 2026-07-24 使用 Pixel 8a、仓库外 fresh 链和独立
PostgreSQL 完成总验收。当前 fresh genesis hash 为
`0xafac9d55a77a10780b5c5cb29da6118ecf4a7b9652960e52502ebacf5d403535`，
OnChina 投影为 49,593 个机构和 99,232 个账户；新增的一个计数来自联邦公民安全
基金账户，不是机构重复行。CitizenApp 与 CitizenWallet 使用同一公开测试助记词恢复
出相同规范 `account_id`，旧字段、非法大小写和 SS58 主键入口均失败关闭。
Cloudflare staging 的 D1/KV/R2 可清理真写通过，production 只读。仓库没有 fresh
创世管理员公开账户对应的私钥或助记词，因此本次没有伪造管理员成功登录或线上业务
签名；链内正例由全 workspace runtime externalities 测试覆盖。

最低检查：

```text
rg "旧独立身份系统名" memory AGENTS.md citizenchain/onchina --glob '!tasks/**' --glob '!docs/**'
cargo check --manifest-path citizenchain/Cargo.toml -p onchina
cargo test --manifest-path citizenchain/Cargo.toml -p onchina
npm --prefix citizenchain/onchina/platform/frontend exec tsc -- --noEmit
```

如果工作区存在其它线程的未完成改动，验收必须说明受影响的命令和原因，不得把其它线程的失败混入本任务结论。

---

### OnChina 架构总览

OnChina 是公民链 `citizenchain` 内置的链上中国平台能力，不再作为独立产品存在。仓库当前只保留四个产品：公民、公民链、公民钱包和官方网站；OnChina 属于公民链产品内部的多机构工作台、注册局业务、机构登记、行政区和管理后台能力。

#### 产品归属

- 产品归属：公民链 `citizenchain`
- 源码目录：`citizenchain/onchina/`
- 产品级文档：`CitizenChainOnChina.md`
- 模块职责、接口和目录说明统一维护在本文，不建立第二技术文档目录。
- 管理后台前端：`citizenchain/onchina/platform/frontend/`

#### 源码边界

- `onchina/platform/authentication`：登录、扫码认证、会话、Passkey 和统一安全动作调度。
- `onchina/platform/authorization`：通用岗位与辖区上下文。
- `onchina/platform/chain`、`onchina/platform/codes`：链读取、CID 与行政区字典。
- `onchina/platform/transactions/session.rs`：唯一冷签会话模型、有效期和会话持久化。
- `onchina/platform/transactions/handler.rs`：统一提交、恢复和业务回写调度；原业务校验及投影仍从所属模块调用。
- `onchina/platform/institutions`：通用机构、账户、管理员展示 DTO 与本机构查询。
- `onchina/platform/documents`：机构资料库；`onchina/platform/audit`：统一审计。
- `onchina/platform/database`、`onchina/platform/security`、`onchina/platform/communication`：现有数据访问、HTTPS、防护、响应与路由。
- `onchina/platform/indexer`：公共钱包索引及当前绑定机构投影；公民增量投影由注册局独立运行。
- `onchina/registry/citizens`、`onchina/registry/passports`、`onchina/registry/documents`：公民办理、护照和公民资料库。
- `onchina/registry/admins`：联邦与市注册局名册、机构管理员登记、办理校验及其数据查询。
- `onchina/registry/institutions`、`onchina/registry/address`、`onchina/registry/projection`：机构登记、地址和注册局投影。
- `onchina/legislation`：法律、提案、表决调用、看板及现有预留模型。
- `onchina/judicial`：现有司法工作台；`onchina/private`：私权机构工作台与基金会 membership 业务。

前端源码归 `onchina/<所属部分>/frontend`；宿主与各模块分别使用现有入口构建，依赖声明及原始锁仍归 `onchina/platform/frontend`。通用机构信息面板归 `onchina/platform/frontend/institutions_OwnInstitutionInfoPanel.tsx`，司法页只组合现有公共面板。各模块消费公共前端接口，组件 JSX、样式、事件和请求语义保持不变。

#### 数据真源

OnChina 以 PostgreSQL 结构化表作为唯一持久化真源。进程内缓存只允许承载短生命周期运行态和性能缓存，不得成为第二份业务主数据。

核心表：

- `ids`：全局唯一 CID 号索引。
- `subjects`：公民、公权机构、私权机构公共主体表，按 `province_code` 分区。
- `citizens`：以永久 `cid_number` 归属公民档案；当前 `account_id` 仅是 finalized 钱包
  绑定投影，并与 `binding_revision`、finalized 区块号和区块哈希一起保存。换绑不得迁移
  CID 档案，也不得回退到本地旧账户。
- `citizen_documents`：公民独立资料库元数据，资料类型固定为“护照相片 / 出生证明 / 监护人护照 / 其他材料”，按 `province_code` 分区；不得与机构资料库共表。
- `gov`：公权机构扩展表，按 `province_code` 分区。
- `private`：私权机构扩展表，按 `province_code` 分区。
- `accounts`：机构账户表，按 `province_code` 分区。
- `docs`：机构资料库元数据表，按 `province_code` 分区。
- `audit`：审计表，按 `province_code` 分区。
- `admins`：机构管理员本地元数据缓存；成员资格真源是链上 active admin 集合。
- `node_institution_bindings`：本节点首次登录确认后的机构绑定结果；限制本节点后续登录机构，不作为权限真源；解绑 / 换机构通过 `NODE_BINDING_UNBIND` 冷签安全动作停用 active binding 后重新绑定。
- `admin_*`：登录、会话、扫码签名和安全动作运行态。
- `chain_requests`、`chain_nonces`、`tx_records`、`tx_indexer_state`：链路幂等、防重放和索引运行态。

废弃快照表、旧机构行表、旧独立产品部署表不得保留为兼容数据源。

#### 权限边界

管理员登录资格、当前签名账户及业务授权统一按本文“权限范围”的链上分层解析器核验；机构 `admins` 是人员名册，业务权限来自有效岗位任职与岗位权限，不能由本地数据或工作台标签产生。个人多签不进入 OnChina；不得恢复 `registry_org_code` 专用分支或第二授权真源。

- 联邦注册局机构 `admins`：联合读取全量管理员钱包和省专员岗位任职，本省 5 席置顶；目录完全只读，换届由治理业务写入 entity，业务数据仍按所属省限制。
- 市注册局机构 `admins`：只能读取和写入所属市数据。
- SQL 查询必须在数据库层携带 `province_code` / `city_code` 范围条件，禁止取全量后在 Rust 或前端过滤。

#### 公开接口

公开接口只读取结构化表，不要求管理员 token，由全局限流保护：

- 机构搜索、机构详情和机构账户读取 `subjects/accounts`。
- 电子护照状态按钱包公钥精确查询 `citizens`。
- 投票人数快照读取 `citizens` 聚合计数。
- 投票凭证只签发投票引擎已经定义的凭证，不实现投票流程。

注册局管理员另有全局链上绑定查询
`GET /api/admin/citizens/{cid_number}/binding`。它要求当前 `institution_code` 为
FRG/CREG，只返回同一 finalized 区块中的 CID 状态、当前账户、绑定版本、投票/竞选标志和
区块锚点，不读取本地公民档案。两个注册局节点查询同一 CID 必须得到相同结果。

#### 禁止项

- 禁止恢复独立 旧独立身份系统产品、目录、CI、部署包或文档入口。
- 注册局源码唯一归属为 `onchina/registry`，不得以其它路径维护第二份登记实现。
- 禁止恢复 `backend/src`、独立 `backend/chain`、独立 `frontend/api` 或独立 `frontend/chain`。
- 禁止在业务模块内复刻 QR 协议、扫码签名、验签或交易载荷解析。
- 禁止保留旧命名、旧文案、旧接口、旧部署脚本或旧文档作为兼容口径。

#### 实施与验收约束

- 开工前确认任务属于 `citizenchain/onchina/`，并按范围读取本技术文档；行政区、CID 号、
  权限、扫码签名、错误码和跨端接口必须以所属实际接口与对应技术章节为准。
- 扫码、签名、验签和交易载荷只使用 `QR_V1` 与统一协议实现，不得在前后端复制协议。
- 页面字段分类名使用中文，账户地址只在展示边界使用 `ss58_address`；授权和持久化继续使用
  `account_id`。
- 修改后必须同步本技术文档、完善必要中文注释并清理旧命名、旧路径和兼容残留。
- 涉及真实接口、数据库、登录、权限、扫码或页面展示时，必须完成真实服务、真实数据库、
  真实 HTTP 接口或真实页面验收后才能完成任务。

## 目录整合与平台输入

前端二维码生成文件归入 `onchina/platform/frontend/core_qrBodies.g.ts`，导出器及调用方同时调整。中国行政区静态数据直接位于 `onchina/platform/codes/area_code_2024.csv.gz`；数据字节与查询语义保持不变。Runtime 与上游不参与本次目录调整。
## 目录与业务边界

目录结构、Rust 模块定位和前端导入按以下归属维护。模块进程与前端产物可分别构建和替换，重启生效；现有 HTTP 路径、QR/SCALE 字节、业务序列化字段、数据库结构、签名次数、业务条件、UI 和交互保持。健康接口仅追加实际可用模块的软件元数据。

唯一 Cargo 包仍在 `onchina/platform/Cargo.toml`，其中五个真实 binary 目标分别指向 platform、registry、legislation、private、judicial 的现有 main.rs。宿主不静态编入其他业务模块；各入口 include 同一 platform/interface.rs，独立链接原公共实现与自己的业务。没有新增 Cargo 包、依赖、锁文件或源码包装层。

源码根为 onchina，下一层是 platform、registry、legislation、judicial、private，再下一层是实际功能目录或 frontend；功能目录内只有文件。公民资料归 registry/documents，护照算法归 registry/passports；机构资料归 platform/documents。私权机构登记归 registry/institutions，基金会会员价格治理归 private/membership。通用安全、数据库、交易、机构、链读取、索引和审计仍只有原有实现。

各模块 frontend 使用原组件入口生成自己的 module.js 与 module.css；宿主入口只持有公共组件、React、认证 Context、请求和扫码状态的唯一实例，并通过公开导出供模块使用。原组件 JSX、文字、样式、事件及请求语义保持。Vite 禁止模块静态引用另一模块的运行实现，跨模块页面通过已装载模块公开导出调用；类型引用继续由现有 TypeScript 工程发现。OnChina 不再使用旧的整体前端缓存实现。

原内联回归和二维码金标随实现迁移，输入与预期字节不变；新增结构回归核对实际模块路径、前端导入、旧目录残留以及迁移前的 HTTP 方法和路径清单。检查通过与编译、运行验收分别记录在唯一任务卡。

### 框架、鉴权与回归约束

冷签会话、提交与恢复归 `platform/transactions`；注册局业务编码、finalized 身份校验和正式档案投影仍归 `registry`。HTTP 方法、URL、路由分区和中间件顺序由 `platform/modules/routes.fixture.json` 管理；`layout.fixture.json` 只登记目录闭集、真实入口、定义归属、原能力位及退役路径负例，不保存任务进度或“修复完成”状态。结构扫描包含生产和测试源码，以及仓库消费者与三份根技术文档。

公民与管理员 ID 均使用 `sequence_counters` 原子递增并显式处理容量边界；已有 ID 保留。公民正本编辑锁后重读，出生日期等不可变字段不能并发覆盖，护照和相关索引一起提交；Passkey 有效期在获得绑定事务锁后按当前时间检查。链日期以真实日历校验并统一为 YYYY-MM-DD。Passkey 凭证消费绑定当时的账户与准确节点绑定，后续写入重核。机构治理、注册局登记管理员、法律案发起、代表表决及平台调价的链签名准备均复用一次现有 Passkey 认证；验证成功才发送业务请求，取消或失败不发送，后续钱包扫码提交不重复触发认证。后端校验不得降级。

上传文件上限 10MiB，请求体另允许 64KiB multipart 开销；破损流失败并清理本次部分文件，元数据失败清理未提交文件。节点解绑和直接名册治理按现行动作类型执行一次 Passkey；其它 grant 不重复消费。挑战与 grant 的有效期须在等待事务锁后重新判定。

测试声明复用原锁的 Tower，通过真实 Router 验证 CORS、方法、鉴权及上传边界；前端使用独立 Vitest 配置，不依赖开发 TLS/HMR 输入。数据库集成回归只接受控制台显式交付的本机 ONCHINA_TEST_DATABASE_URL，库名以 onchina_test 开头，用例只创建和清理自己 UUID 命名的 schema；默认 ignored，普通测试不隐式连接数据库。

回归覆盖当前 Runtime 元数据事件解码、溢出和坏事件停止、交易记录/锚点/进度原子性、并发 ID/公民编辑、绑定切换、名册准确归属、公民迁出正本保留、立法跨省同市号、同钱包多岗位票据及恢复互斥。源码断言与静态检查不能代替测试、类型检查、编译或运行验收；具体授权和执行证据只登记在唯一任务卡。



## 模块独立构建与装载

唯一专属构建入口是 scripts/onchina.mjs。构建目标直接读取现有 platform/Cargo.toml 的真实 binary 声明；业务模块的 ID、独立软件版本、前端入口、公开导出、HTTP 路由、提交 purpose、安全动作和工作台能力由对应 main.rs 实际实现提供。构建时执行已编译程序的 --describe 取得声明，该动作在日志、数据库、TLS 和链连接初始化之前返回。夹具不作为生产配置，既不新增 module.json，也不从目录名猜测业务。

scripts/build.mjs只保留GitHub正式整包编译限制；它直接调用scripts/onchina.mjs的buildPreparedOnChina公开实现，传入准确检出、已准备依赖、原生平台和本轮工作根。专属脚本复用现有checkWork、runBuildProcess、platformCargoEnvironment、npmCLI及Windows运行库处理，不恢复旧资源回执、工具库存、租约、GitHub管理或发布框架。调用方拥有的现场不由内部接口重新申请或清理；前端worker只消费调用方明确传入的工程、输出及实际程序声明。

npm run build调用同一个专属脚本的build协调入口，默认构建开发用宿主与全部模块，交付到target/onchina/<原生平台>/all；node scripts/onchina.mjs build --module <ID>只构建和交付所选目标，默认到同级<ID>目录。--platform必须匹配现有产品声明的本机系统和架构；--output可指定一个不存在的新交付目录，拒绝覆盖已有目录，也不能指向源码、活动现场或其父目录。它不生成正式节点安装包、Tag或Release，不安装或改链。

开发构建使用target/onchina/<平台>/work作为专属互斥现场，与正式整包的target/build/<平台>分开。只读Git清单复制当前完整源码，依赖按已有npm/Cargo锁及官方包管理器准备，工具版本直接核对scripts/config.mjs现有声明。不存在未授权的资源获取器；工具缺失、版本不符或原生平台不符明确失败。只有成功申请现场的调用者负责清理；普通失败及成功交付后清理，信号/超时等无法确认后代退出时保留现场，不清理别人的活动任务。默认开发交付在全部验收成功后替换；显式交付使用独占新目录，并支持跨磁盘复制。

开发构建先运行结构回归、公共前端装载回归和所选Rust目标回归。all另运行完整前端typecheck和业务Vitest；单模块仅检查所选入口和原工具配置，不构建其他模块，也不把其他模块的业务测试作为产物入口。四个正式节点Workflow已有完整前端typecheck、Vitest、结构及Rust前置测试登记，失败不会进入编译，本轮不重复登记另一套Workflow测试。

buildPreparedOnChina只编译所选Cargo目标与原前端入口。frontendOnly是公开函数的可选内部参数：不执行Cargo build，但要求对应真实业务程序已存在，复制其原字节与新前端配对后仍须验收。未消费的prepared/frontend命令入口已移除，frontend-worker只作内部子进程使用；导入脚本不执行工具、资源准备或构建。

npm run dev沿用构建后预览流程；npm run preview经专属脚本定位同平台all/frontend，使用现有前端依赖中与原声明一致的Vite，仍要求原开发TLS证书输入。前端开发依赖按onchina/platform/frontend原锁准备，预览不自动下载替代Vite。/api与/modules由同一受校验HTTPS代理访问正在运行的宿主，预览不维护另一套模块资源副本。

宿主交付文件为 onchina（Windows 为 onchina.exe）和原 frontend；每个模块交付为 modules/<ID>/onchina-<ID>（Windows 带 .exe）以及同目录 frontend/module.js、frontend/module.css 和必需运行库。JS、CSS 首行均包含与后端一致的软件 ID、版本和接口版本；IIFE 导出也包含同一元数据。每次模块软件升级必须更新该模块 main.rs 的 VERSION，前后端作为同一套产物交付。共享接口发生不兼容变更时才调整 MODULE_INTERFACE_VERSION；它与链上 Runtime spec_version 无关。模块装载框架不改变 CID、CN、省代码、省内市编号及现有解析；法律地域协议单独采用本文件所述 LawRegion。

运行时只从一个实际模块目录装载：可用 ONCHINA_MODULE_DIR 指定绝对路径；macOS 默认是用户 Library/Application Support/gmb.dev/onchina-modules，在 .app 外；其它平台默认是宿主程序同级 modules。整套节点包包含初始模块交付文件；macOS 首次部署须在获得安装授权后把它们放入上述实际目录，不能只把 .app 内的构建产物当作已启用模块。本构建入口不安装、不发布、不修改链或持久数据；不隐式复制初始模块，也不在 App 启动时覆盖已独立升级的模块。

装载在宿主启动时执行一次：验证真实目录和程序、声明、接口、前后端版本、能力与工作台同一归属、HTTP 方法/路径匹配形状与归属冲突，再以原机构 CA 校验的回环 HTTPS 拉起模块，确认健康后登记。模块说明读取及就绪等待采用最多4个并行任务；归属与路由冲突按目录排序校验，只登记成功模块。模块缺失、冲突、版本不符或启动失败时隔离该模块；公共服务及其他模块继续运行。没有热加载、轮询替换、安装按钮或双轨装配。增加、替换或移出模块时，先停止宿主，再调整其实际软件目录，最后重新启动；节点关闭宿主父管道后等待宿主退出；宿主从初始化阶段监听父管道，先停止接收请求并最多排空10秒，再关闭模块父管道，给予全部模块合计12秒退出时间，最后回收仍未退出的模块。模块退出未经确认时宿主不报告停止完成；节点等待40秒超时保留句柄并返回现有错误通道。移出的是软件，不删除数据库、文件资料、待完成会话或审计。

宿主保留原公共 HTTP 路由与中间件顺序，模块业务方法/URL 不变；上传请求限额由真实路由声明原样带到转发处。未匹配的 API 返回原 JSON 错误格式，不落入前端 HTML。原请求头、二维码、Passkey 和钱包冷签提交字段保持；宿主在消费凭证或会话前按实际登记归属转发，模块独立执行原业务校验。公民业务的 finalized 身份核验和正式投影归注册局；通用事务仍共同核对原绑定、租约、投影及会话完成标记。后台恢复各自只领取所属 purpose；模块缺失不会消费或清理其待完成会话。

公共机构投影、私权机构类别字符串及类别映射归 platform；注册局录入、查询、资料和公民投影归 registry。类别映射只是原有机构类别归属迁移，所有字符串和值保持。宿主初始化现有公共数据库结构，模块只连接已有库；本次未改变 schema。今后模块自己的数据升级应由模块自身负责，不能借独立升级重建公共表或清除其他模块数据。

前端在原启动阶段仅装载一次健康接口登记的模块；健康声明与每组同源JS/CSS装载各自最多等待15秒。地址全部通过校验后才插入资源节点；失败或超时清除该模块节点、导出、事件和计时器，保留失败隔离结果，迟到资源不能恢复为可用模块。实际导出和前后端元数据均须匹配。公共 React、认证 Context、请求/401 状态从宿主公开接口取得；工作台按后端准确实例业务清单优先，再按实际模块的机构类声明选择，新增机构模块不需要增加宿主名称分支。原注册局管理员组件移至 registry/frontend/admins_ProvinceDetailView.tsx，专属状态和 API 移至同目录 admins_cityRegistryAdminsApi.ts；通用本机构管理员组件留在平台。基金会会员治理仍只是 private 中准确 CID 授权的一项业务。

现有结构、HTTP 方法/路径金标、能力位金标、真实路由回归、恢复和授权回归随所有者同步；每个业务二进制单独对照原能力金标核验自身真实声明。模块装载回归覆盖正常资源、版本不符、样式失败、跨源拒绝、工作台冲突及准确实例优先。源码维护及静态核对与实际类型检查、测试、编译、部署验收分别记录在唯一任务卡。

## 当前交付归属

OnChina 的完整正式交付归本仓四个平台 Workflow，使用唯一 scripts/onchina.mjs 的 buildPreparedOnChina 公开实现。它直接读取真实 Cargo 目标，执行已编译模块的 --describe，编译同一声明的前端并交付宿主、模块及所需运行库；控制台不实现模块编译。依赖由本仓各自原始 npm/Cargo 锁及官方包管理器准备，构建与测试输出只进入本轮 target 现场，工程直接使用本仓准确检出，不创建整仓源码副本或资源回执。测试由Workflow执行，build.mjs只做编译和完整产物。整个原 Run 结束后的历史清理、恢复不重派与版本规则统一见 CitizenChainNode.md 当前合同；OnChina 模块自身 VERSION 和 MODULE_INTERFACE_VERSION 不由交付命名改写。


### 模块装载与维护边界

登记机构管理员的现有 call 9 必须依次编码目标 CID、管理员集合、操作注册局 CID、actor_role_code；岗位码复用原治理表单输入，后端不代选岗位，原链端权限校验保持。界面、钱包扫码顺序与全部 HTTP 方法/路径保持；链签名准备前完成后端要求的一次Passkey认证。完整载荷回归使用现有 RuntimeCall 解码交叉核验；测试代码维护不表示已执行。

五处链签名准备均通过现有 passkeySubmitHeaders 取得一次断言，不增加页面控件或改变钱包扫码流程。法律地域直接保留原省码字母和省内市号，旧数字占位及错误默认值已删除。

内部实例工作台查询并行度最多4，单次请求含响应体读取最多5秒；该期限不应用于正式交易转发、扫码签名或最终确认。模块就绪最长等待120秒，说明进程最多10秒，独立等待不会串行阻塞其他同批模块。开发与预览的/api和/modules使用同一现有HTTPS代理和机构CA校验，不复制另一套前端资源。路径归属统一按原生relative边界判断，支持Windows分隔符并拒绝跨盘、父目录和同名前缀目录。

能力夹具只为真实所属模块声明业务能力；无owner的NRC、PRC、PRB和未知码回归必须经过真实基础能力分支。工作台夹具完整保留分区、动作、文字、顺序及enabled值。普通回归使用独立测试进程和临时证书核验真实HTTPS转发、证书/主机名拒绝、退出和同路径程序替换，不连接业务数据库或链。真实业务IIFE验收由唯一verifyPreparedOnChina在编译后、打包或对外交付前登记：向既有结构测试及前端测试明确传入ONCHINA_TEST_MODULE_ROOT和ONCHINA_TEST_MODULE_IDS，只核验所选模块。实际--describe、JS/CSS首行、IIFE导出和宿主真实公开对象/React/认证Provider须一致；任一失败中止交付。普通单元测试未提供产物时明确跳过，不能把伪DOM回归作为真实产物验收。


### 法律地域协议

共享 Rust 类型及原机构 CID 派生入口唯一归 runtime/primitives/src/genesis.rs 的 LawRegion；不修改 CID 解析或地区字典。SCALE 顺序为 province_code:[u8;2]、city_number:u16 LE，固定四字节，保持原新法参数位置和后续字段偏移。省级为原字母省码及零市号；市级为原字母省码及原三位市号的数值，显示保留前导零。宪法/国家级使用显式四零字节，不表示国家代码改变或错误默认值；CN 保持。非法层级、省码或市号拒绝，不使用 MAX、数字省码映射或全国回退。

法律只读 DTO 的 region 为 provinceCode/cityNumber 展示投影；全国 provinceCode 为空、cityNumber 为0，仅按层级解释。前端只发送层级和原业务字段；后端核对原会话范围，链端入口和执行回调均复核授权机构 CID 的地域。Runtime 查询 API v2 接收相同四字节地域，非法参数返回空列表；不同省相同市号分别索引。UI布局、操作顺序、Passkey及钱包扫码交互保持。

当前只有创世宪法，不设计普通法律存量迁移，不改宪法字节、索引、国家立法院归属、众议会发起和两院表决规则，不改节点守卫。Runtime升级统一遵循CitizenChainRuntime.md的版本政策：每次正式升级两个版本各自加1，钱包同步交易版本；OnChina不生成或写回版本号，不以spec_version代替transaction_version。地域源码和配套维护已完成，本轮没有执行升级，保留版本数值。本节为源码合同，相关回归已编写但未运行；不表示类型检查、编译、节点守卫或链上部署已经通过。


公共机构治理通过platform/institutions/model.rs的GovernanceCitizen与platform/database/records.rs的只读查询取得CID、姓、名及可空账户。查询沿用原修剪、空输入、created_at倒序及未找到语义，真实数据库失败原样返回；不静态编入注册局完整档案、写入或投影业务。Passkey一次性断言消费使用PostgreSQL官方GenericClient，连接与事务共用原SQL；事务回滚恢复可消费状态，提交后仍只消费一次。既有真实数据库回归覆盖查询字段、顺序、缺失与故障和断言事务语义，仍需控制台提供隔离测试数据库后显式执行，不以普通编译代替集成验收。
