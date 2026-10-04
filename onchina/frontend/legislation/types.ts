// 立法与表决前端类型,camelCase 逐字镜像后端 DTO
// (onchina/src/domains/legislation/law/model.rs 与 chain_read_proposal.rs);枚举数值与链端对齐。

/** 提案类型(可扩展维度;本轮仅 law 实现,personnel/budget 预留)。 */
export type ProposalCategory = 'law' | 'personnel' | 'budget';

/** 立法动作(对应 propose_enact/amend/repeal_law)。 */
export type LawActionInput = 'enact' | 'amend' | 'repeal';

/** 法律层级(对齐链 Tier::as_u8)。 */
export const LAW_TIER = {
  CONSTITUTION: 0,
  NATIONAL: 1,
  PROVINCIAL: 2,
  MUNICIPAL: 3,
} as const;

/** 表决类型(对齐链 VoteType::as_u8)。 */
export const VOTE_TYPE = {
  REGULAR: 0,
  REGULAR_EDUCATION: 1,
  MAJOR: 2,
  MAJOR_EDUCATION: 3,
  SPECIAL: 4,
} as const;

/** 法律状态(对齐链 LawStatus)。 */
export const LAW_STATUS = {
  PENDING: 0,
  EFFECTIVE: 1,
  REPEALED: 2,
} as const;

/** 提案阶段(对齐链 STAGE_LEG_*)。 */
export const LEG_STAGE = {
  HOUSE: 10,
  REFERENDUM: 11,
  SIGN: 12,
  OVERRIDE: 13,
  GUARD: 14,
} as const;

/** 款(最末层正文)。 */
export interface LawClause {
  number: number;
  text: string;
  textEn?: string | null;
}

/** 条(目录 + 正文 + 款列表)。 */
export interface LawArticle {
  number: number;
  title: string;
  titleEn?: string | null;
  body: string;
  bodyEn?: string | null;
  clauses: LawClause[];
}

/** 节(目录 + 条列表)。 */
export interface LawSection {
  number: number;
  title: string;
  titleEn?: string | null;
  articles: LawArticle[];
}

/** 章(目录 + 节列表)。 */
export interface LawChapter {
  number: number;
  title: string;
  titleEn?: string | null;
  sections: LawSection[];
}

/** 立法代表机构引用；机构身份只保存 CID。 */
export interface HouseRef {
  cidNumber: string;
}

/** 本机构可发起的提案候选(发起菜单单源自后端 /api/legislation/proposable)。 */
export interface ProposableCandidate {
  category: ProposalCategory;
  tier: number;
  voteTypes: number[];
}

/** 法律只读视图(Law 主体 + 办理端展示版本全文)。 */
export interface LawView {
  lawId: number;
  version: number;
  versionTitle?: string | null;
  versionTitleEn?: string | null;
  effectiveVersion?: number | null;
  latestVersion: number;
  pendingVersion?: number | null;
  tier: number;
  scopeCode: number;
  status: number;
  voteType: number;
  title: string;
  titleEn?: string | null;
  contentHash: string;
  proposalId: number;
  publishedAt: number;
  effectiveAt: number;
  houses: HouseRef[];
  /** 宪法不可修改条款号;普通法律为空。 */
  immutableArticleNumbers: number[];
  chapters: LawChapter[];
}

/** 发起法律案请求体(houses/executive/legislature 由后端按宪法路由解析,前端不传)。 */
export interface ProposeLawInput {
  lawAction: LawActionInput;
  /** 当前管理员用于发起该业务的机构岗位码。 */
  proposerRoleCode: string;
  tier: number;
  scopeCode: number;
  voteType: number;
  title: string;
  titleEn?: string | null;
  /** 正文:章>节>条>款(立法/修法携带;废法为空)。 */
  chapters: LawChapter[];
  effectiveAt: number;
  /** 修法/废法目标法律 ID;立法为 null。 */
  lawId?: number | null;
}

/** 计票(院内 u32 / 公投 u64,前端统一 number)。 */
export interface VoteTally {
  yes: number;
  no: number;
}

/** 提案进度只读投影(不含计票判定,只搬运链上事实)。 */
export interface LegProposalState {
  proposalId: number;
  kind: number;
  stage: number;
  status: number;
  representativeRule: number;
  currentBody: number;
  voteProcedure: number;
  needsGuard: boolean;
  representativeBodies: HouseRef[];
  startBlock: number;
  endBlock: number;
  representativeTally: VoteTally;
  referendumTally: VoteTally;
}

// ── 任免案 / 预算案预留类型(Phase 4)──
// 镜像后端 personnel/budget 子域 schema。对应业务模块尚未实现，当前仅锁 schema；
// ProposeMenu 仅渲染 category==='law'，发起/表决 UI 待业务模块接入代表机构表决后增加。

/** 任免动作(对齐后端 PersonnelAction)。 */
export type PersonnelActionInput = 'appoint' | 'dismiss' | 'replace';

/** 任免职书正文(预留)。 */
export interface PersonnelDecision {
  action: PersonnelActionInput;
  officeInstitutionCode: string;
  officeTitle: string;
  officeSeat: number;
  nomineeCidNumber: string;
  nomineeName: string;
  termIndex: number;
  termYears: number;
  reason: string;
}

/** 发起任免案请求体(预留;houses 后端解析,不传前端)。 */
export interface ProposePersonnelInput {
  tier: number;
  scopeCode: number;
  voteType: number;
  decision: PersonnelDecision;
}

/** 预算科目「目」(最末层,唯一携金额;金额单位分,u128 以 string 承载防 JS Number 精度丢失)。 */
export interface BudgetSubitem {
  code: string;
  name: string;
  revenue: string;
  expenditure: string;
}

/** 预算科目「项」(目录 + 目列表)。 */
export interface BudgetItem {
  code: string;
  name: string;
  subitems: BudgetSubitem[];
}

/** 预算科目「款」(目录 + 项列表)。 */
export interface BudgetSection {
  code: string;
  name: string;
  items: BudgetItem[];
}

/** 预算科目「类」(目录 + 款列表)。 */
export interface BudgetClass {
  code: string;
  name: string;
  sections: BudgetSection[];
}

/** 预算总案(预留)。金额单位分,string 承载 u128。 */
export interface BudgetPlan {
  budgetEntityCode: string;
  fiscalYear: number;
  categories: BudgetClass[];
  totalRevenue: string;
  totalExpenditure: string;
}

/** 发起预算案请求体(预留;houses 后端解析)。 */
export interface ProposeBudgetInput {
  tier: number;
  scopeCode: number;
  voteType: number;
  plan: BudgetPlan;
}
