// 主体共享类型。实际业务 API 必须放在
// onchina/registry/frontend/government_api.ts、onchina/registry/frontend/<类型>_api.ts、onchina/platform/frontend/accounts_api.ts、onchina/platform/frontend/docs_api.ts。
// 铁律:一个 cid_number 下可挂多个 account_name。

export type InstitutionCategory = 'GOV_INSTITUTION' | 'PRIVATE_INSTITUTION';

export const InstitutionCategoryLabel: Record<InstitutionCategory, string> = {
  GOV_INSTITUTION: '公权机构',
  PRIVATE_INSTITUTION: '私权机构',
};

export type MultisigChainStatus =
  | 'NOT_ON_CHAIN'
  | 'PENDING_ON_CHAIN'
  | 'ACTIVE_ON_CHAIN'
  | 'REVOKED_ON_CHAIN';

export type PrivateType =
  | 'SOLE'
  | 'PARTNERSHIP'
  | 'COMPANY'
  | 'CORPORATION'
  | 'WELFARE'
  | 'ASSOCIATION';

export type PartnershipKind = 'GENERAL' | 'LIMITED';

export type EducationType =
  | 'NATIONAL_CITIZEN_EDU_COMMITTEE'
  | 'CITY_CITIZEN_EDU_COMMITTEE'
  | 'EARLY_SCHOOL'
  | 'PRIMARY_SCHOOL'
  | 'SECONDARY_SCHOOL'
  | 'UNIVERSITY';

export interface Institution {
  cidNumber: string;
  /** 详情页展示全称;列表不得与简称同时展示。 */
  cidFullName?: string | null;
  /** 详情页展示简称;机构简称只来自 cid_short_name。 */
  cidShortName?: string | null;
  /** 主体业务状态,只允许 ACTIVE / REVOKED。 */
  status: 'ACTIVE' | 'REVOKED';
  category: InstitutionCategory;
  subjectProperty: string;
  p1: string;
  provinceName: string;
  cityName: string;
  townName?: string;
  provinceCode: string;
  /** 省内唯一的 3 位数字市编号，保留前导零；与原省代码一起确定地域。 */
  cityCode?: string;
  townCode?: string;
  institutionCode: string;
  /** 教育机构业务分类,只用于教育 tab 分类,不参与身份 ID 生成。 */
  educationType?: EducationType | null;
  /** 私权机构类型。仅私权目标类型机构有值。 */
  privateType?: PrivateType | null;
  /** 合伙企业形态。仅 private_type=PARTNERSHIP 时有值。 */
  partnershipKind?: PartnershipKind | null;
  /** 是否具有法人资格。仅私权目标类型机构有值。 */
  hasLegalPersonality?: boolean | null;
  /** 从属关系引用:需挂靠的 F 指向所属法人。 */
  parentCidNumber?: string | null;
  /** 法定代表人公开身份。初始化目录机构可为空；姓名只使用姓、名。 */
  legalRepresentative?: {
    family_name: string;
    given_name: string;
    cid_number: string;
    account_id: string;
  } | null;
  photoPath?: string | null;
  photoName?: string | null;
  photoMime?: string | null;
  photoSize?: number | null;
  /** 机构链投影状态。 */
  chainStatus?: string | null;
  /** 机构上链交易哈希。 */
  chainTxHash?: string | null;
  /** 机构上链区块号。 */
  chainBlockNumber?: number | null;
  /** 签发机构身份ID(溯源)。 */
  issuerCidNumber?: string | null;
  /** 机构来源类型(溯源)。 */
  institutionSourceType?: string | null;
  /** 关联注册提案ID(溯源)。 */
  registerProposalId?: string | null;
  creatorAccountId?: string | null;
  /** 最近更新人的账户 ID。 */
  updaterAccountId?: string | null;
  createdAt: string;
}

export interface InstitutionAccount {
  cidNumber: string;
  accountName: string;
  accountId: string | null;
  accountKind: 'main' | 'fee' | 'stake' | 'safety_fund' | 'he' | 'clearing' | 'federal_citizen_security_fund' | 'named';
  canClose: boolean;
  canDelete: boolean;
  chainStatus: MultisigChainStatus;
  chainSyncedAt?: string | null;
  chainTxHash: string | null;
  chainBlockNumber: number | null;
  // 账户读侧已切链上真源:链上无创建人/创建时间字段,一律为空。
  creatorAccountId?: string | null;
  createdAt: string | null;
}

export interface InstitutionListRow {
  cidNumber: string;
  cidFullName?: string | null;
  cidShortName?: string | null;
  status: 'ACTIVE' | 'REVOKED';
  category: InstitutionCategory;
  subjectProperty: string;
  p1: string;
  provinceName: string;
  cityName: string;
  townName?: string;
  institutionCode: string;
  educationType?: EducationType | null;
  privateType?: PrivateType | null;
  partnershipKind?: PartnershipKind | null;
  hasLegalPersonality?: boolean | null;
  parentCidNumber?: string | null;
  accountCount: number;
  createdAt: string;
  /** 创建该机构的登录管理员姓；未命中时为空。 */
  creatorFamilyName?: string | null;
  /** 创建该机构的登录管理员名；未命中时为空。 */
  creatorGivenName?: string | null;
  /** 创建者角色:FEDERAL_REGISTRY / CITY_REGISTRY;未命中 null */
  creatorInstitutionCode?: string | null;
}

export interface PageResult<T> {
  items: T[];
  page_size: number;
  next_cursor?: string | null;
  has_more: boolean;
  /** 确定性目录版本。普通分页接口没有该字段。 */
  manifest_version?: string | null;
  /** 确定性目录状态。普通分页接口没有该字段。 */
  catalog_status?: string | null;
}

export interface InstitutionDetail {
  institution: Institution;
  accounts: InstitutionAccount[];
  /** 创建该机构的登录管理员姓、名；页面只在展示时合并。 */
  creatorFamilyName?: string | null;
  creatorGivenName?: string | null;
  /** 创建者角色:FEDERAL_REGISTRY / CITY_REGISTRY */
  creatorInstitutionCode?: string | null;
}

/** 机构资料库文档 */
export interface InstitutionDocument {
  id: number;
  cidNumber: string;
  fileName: string;
  docType: string;
  fileSize: number;
  uploaderAccountId: string;
  uploadedAt: string;
}

export interface LegalRepresentativePhoto {
  filePath: string;
  fileName: string;
  mimeType: string;
  fileSize: number;
}

export interface CreateInstitutionAdminInput {
  /** 机构初始管理员账户 ID。 */
  accountId: string;
  /** 管理员姓；不填时后端按公民资料解析，仍无法解析则使用“管理”。 */
  familyName?: string | null;
  /** 管理员名；不填时后端按公民资料解析，仍无法解析则使用“员”。 */
  givenName?: string | null;
}

// ─── 请求 DTO ─────────────────────────────────────────────────

export interface CreateInstitutionInput {
  subjectProperty: string;
  p1?: string;
  provinceName?: string;
  cityName: string;
  /** 镇级公权机构创建时必填;非镇级不传。 */
  townName?: string;
  institution: string;
  /** 教育机构业务分类。仅 G/S 学校创建时提交;F+JY 分校不使用。 */
  educationType?: EducationType;
  /**
   * 机构全称。
   * - 私权机构创建时必填,由对应私权类型 tab 锁定身份编码
   * - 法人教育机构(G/S+JY)和手动公权机构(G):**必传**,同步做查重
   * - 自动公权机构:不走手动创建接口
  */
  cidFullName?: string;
  /** 机构简称,展示简称和链上简称都只使用 cid_short_name。 */
  cidShortName?: string;
  /**
   * 所属法人 cid_number。仅需挂靠的非法人创建必传;个体经营/无限合伙是独立非法人,
   * 不接受所属法人。规则单一源:后端 onchina/platform/institutions/subjects_unincorporated_org.rs。
   */
  parentCidNumber?: string;
  privateType?: PrivateType;
  partnershipKind?: PartnershipKind;
  admins: CreateInstitutionAdminInput[];
}

export interface InstitutionCreatePayload {
  subjectProperty: string;
  p1: string | null;
  provinceName: string | null;
  cityName: string;
  townName: string | null;
  institution: string;
  educationType: EducationType | null;
  cidFullName: string | null;
  cidShortName: string | null;
  parentCidNumber: string | null;
  privateType: PrivateType | null;
  partnershipKind: PartnershipKind | null;
  admins: CreateInstitutionAdminInput[];
}

function nullableTrim(value?: string | null): string | null {
  const trimmed = (value ?? '').trim();
  return trimmed ? trimmed : null;
}

/**
 * 创建机构第 1 步 payload 的唯一前端构造点。
 *
 * 扫码授权和正式提交必须共用这一份 JSON；可空字段统一显式写入 null，
 * 避免 prepare 阶段与 create 阶段因为 undefined 被 JSON.stringify 省略而哈希不一致。
 */
export function buildInstitutionCreatePayload(input: CreateInstitutionInput): InstitutionCreatePayload {
  return {
    subjectProperty: input.subjectProperty.trim(),
    p1: nullableTrim(input.p1),
    provinceName: nullableTrim(input.provinceName),
    cityName: input.cityName.trim(),
    townName: nullableTrim(input.townName),
    institution: input.institution.trim(),
    educationType: nullableTrim(input.educationType) as EducationType | null,
    cidFullName: nullableTrim(input.cidFullName),
    cidShortName: nullableTrim(input.cidShortName),
    parentCidNumber: nullableTrim(input.parentCidNumber),
    privateType: nullableTrim(input.privateType) as PrivateType | null,
    partnershipKind: nullableTrim(input.partnershipKind) as PartnershipKind | null,
    admins: (input.admins ?? []).map((admin) => ({
      accountId: admin.accountId.trim(),
      familyName: nullableTrim(admin.familyName) ?? '管理',
      givenName: nullableTrim(admin.givenName) ?? '员',
    })),
  };
}

export interface CreateInstitutionOutput {
  /** 链签会话 ID，提交钱包签名响应时原样回传。 */
  requestId: string;
  cidNumber: string;
  /** 创建成功后的机构全称。 */
  cidFullName: string | null;
  category: InstitutionCategory;
  /** 机构上链创建专用 QR_V1/k=1。 */
  institutionCreateSignRequest: string;
}

/** 机构详情页可编辑字段。私权类型创建后不可改。 */
export interface UpdateInstitutionInput {
  cidFullName?: string | null;
  cidShortName?: string | null;
  /** 所属法人 cid_number(仅 F;传空串后端会拒) */
  parentCidNumber?: string;
  familyName?: string;
  givenName?: string;
  representativeCid?: string;
  photoPath?: string;
  photoName?: string;
  photoMime?: string;
  photoSize?: number;
}

/** 法人机构搜索结果项(非法人新增弹窗 + F 详情页"所属法人"选择器用) */
export interface ParentInstitutionRow {
  cidNumber: string;
  cidFullName: string;
  subjectProperty: string;
  /** 私权机构类型。仅用于展示父级机构事实。 */
  privateType?: PrivateType | null;
  partnershipKind?: PartnershipKind | null;
  category: InstitutionCategory;
  /** 盈利属性。F 创建时按"盈利属性附属于所属法人"推导:公法人父恒 0,私法人父继承该值 */
  p1: string;
  provinceName: string;
  cityName: string;
}

/** 所属法人搜索参数。预过滤规则与后端 onchina/platform/institutions/subjects_unincorporated_org.rs 同源(三处同源缺一有绕过口)。 */
export interface SearchParentsOptions {
  /** 非法人的机构代码:JY=教育分校,GT/GP=私权独立非法人,其它 F 由后端判定是否需挂靠 */
  fInstitution: string;
  /** 非法人落位省/市,用于地域预过滤(市级同市/省级同省/国家级不限) */
  provinceName: string;
  cityName: string;
  /** 限定父级属性:S=私权入口 / G=公权入口;不传=两者(详情页改挂) */
  parentProperty?: 'S' | 'G';
}

export interface CreateAccountOutput {
  cidNumber: string;
  accountName: string;
  chainStatus: MultisigChainStatus;
  chainSyncedAt: string | null;
  chainTxHash: string | null;
  chainBlockNumber: number | null;
  accountId: string | null;
}

export interface ListInstitutionsQuery {
  category?: InstitutionCategory;
  provinceName?: string;
  cityName?: string;
  /** 精确搜索关键字:匹配 cid_number / cid_full_name / cid_short_name;空=返回空页 */
  q?: string;
  privateType?: PrivateType;
  cursor?: string | null;
  pageSize?: number;
}
