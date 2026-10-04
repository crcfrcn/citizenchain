// 主体共享类型。实际业务 API 必须放在
// gov/api.ts、private/<type>/api.ts、accounts/api.ts、docs/api.ts。
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
  cid_number: string;
  /** 详情页展示全称;列表不得与简称同时展示。 */
  cid_full_name?: string | null;
  /** 详情页展示简称;机构简称只来自 cid_short_name。 */
  cid_short_name?: string | null;
  /** 主体业务状态,只允许 ACTIVE / REVOKED。 */
  status: 'ACTIVE' | 'REVOKED';
  category: InstitutionCategory;
  subject_property: string;
  p1: string;
  province_name: string;
  city_name: string;
  town_name?: string;
  province_code: string;
  /** 2 位数字市代码(r5 段后 3 字符),作为自动公权目录稳定地域键 */
  city_code?: string;
  town_code?: string;
  institution_code: string;
  /** 教育机构业务分类,只用于教育 tab 分类,不参与身份 ID 生成。 */
  education_type?: EducationType | null;
  /** 私权机构类型。仅私权目标类型机构有值。 */
  private_type?: PrivateType | null;
  /** 合伙企业形态。仅 private_type=PARTNERSHIP 时有值。 */
  partnership_kind?: PartnershipKind | null;
  /** 是否具有法人资格。仅私权目标类型机构有值。 */
  has_legal_personality?: boolean | null;
  /** 从属关系引用:需挂靠的 F 指向所属法人。 */
  parent_cid_number?: string | null;
  /** 法定代表人公开身份。初始化目录机构可为空；姓名只使用姓、名。 */
  legal_representative?: {
    family_name: string;
    given_name: string;
    cid_number: string;
    account_id: string;
  } | null;
  legal_representative_photo_path?: string | null;
  legal_representative_photo_name?: string | null;
  legal_representative_photo_mime?: string | null;
  legal_representative_photo_size?: number | null;
  /** 机构链投影状态。 */
  chain_status?: string | null;
  /** 机构上链交易哈希。 */
  chain_tx_hash?: string | null;
  /** 机构上链区块号。 */
  chain_block_number?: number | null;
  /** 签发机构身份ID(溯源)。 */
  issuer_cid_number?: string | null;
  /** 机构来源类型(溯源)。 */
  institution_source_type?: string | null;
  /** 关联注册提案ID(溯源)。 */
  register_proposal_id?: string | null;
  creator_account_id?: string | null;
  /** 最近更新人的账户 ID。 */
  updater_account_id?: string | null;
  created_at: string;
}

export interface InstitutionAccount {
  cid_number: string;
  account_name: string;
  account_id: string | null;
  account_kind: 'main' | 'fee' | 'stake' | 'safety_fund' | 'he' | 'clearing' | 'named';
  can_close: boolean;
  can_delete: boolean;
  chain_status: MultisigChainStatus;
  chain_synced_at?: string | null;
  chain_tx_hash: string | null;
  chain_block_number: number | null;
  // 账户读侧已切链上真源:链上无创建人/创建时间字段,一律为空。
  creator_account_id?: string | null;
  created_at: string | null;
}

export interface InstitutionListRow {
  cid_number: string;
  cid_full_name?: string | null;
  cid_short_name?: string | null;
  status: 'ACTIVE' | 'REVOKED';
  category: InstitutionCategory;
  subject_property: string;
  p1: string;
  province_name: string;
  city_name: string;
  town_name?: string;
  institution_code: string;
  education_type?: EducationType | null;
  private_type?: PrivateType | null;
  partnership_kind?: PartnershipKind | null;
  has_legal_personality?: boolean | null;
  parent_cid_number?: string | null;
  account_count: number;
  created_at: string;
  /** 创建该机构的登录管理员姓；未命中时为空。 */
  creator_family_name?: string | null;
  /** 创建该机构的登录管理员名；未命中时为空。 */
  creator_given_name?: string | null;
  /** 创建者角色:FEDERAL_REGISTRY / CITY_REGISTRY;未命中 null */
  creator_institution_code?: string | null;
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
  creator_family_name?: string | null;
  creator_given_name?: string | null;
  /** 创建者角色:FEDERAL_REGISTRY / CITY_REGISTRY */
  creator_institution_code?: string | null;
}

/** 机构资料库文档 */
export interface InstitutionDocument {
  id: number;
  cid_number: string;
  file_name: string;
  doc_type: string;
  file_size: number;
  uploader_account_id: string;
  uploaded_at: string;
}

export interface LegalRepresentativePhoto {
  file_path: string;
  file_name: string;
  mime_type: string;
  file_size: number;
}

export interface CreateInstitutionAdminInput {
  /** 机构初始管理员账户 ID。 */
  account_id: string;
  /** 管理员姓；不填时后端按公民资料解析，仍无法解析则使用“管理”。 */
  family_name?: string | null;
  /** 管理员名；不填时后端按公民资料解析，仍无法解析则使用“员”。 */
  given_name?: string | null;
}

// ─── 请求 DTO ─────────────────────────────────────────────────

export interface CreateInstitutionInput {
  subject_property: string;
  p1?: string;
  province_name?: string;
  city_name: string;
  /** 镇级公权机构创建时必填;非镇级不传。 */
  town_name?: string;
  institution: string;
  /** 教育机构业务分类。仅 G/S 学校创建时提交;F+JY 分校不使用。 */
  education_type?: EducationType;
  /**
   * 机构全称。
   * - 私权机构创建时必填,由对应私权类型 tab 锁定身份编码
   * - 法人教育机构(G/S+JY)和手动公权机构(G):**必传**,同步做查重
   * - 自动公权机构:不走手动创建接口
  */
  cid_full_name?: string;
  /** 机构简称,展示简称和链上简称都只使用 cid_short_name。 */
  cid_short_name?: string;
  /**
   * 所属法人 cid_number。仅需挂靠的非法人创建必传;个体经营/无限合伙是独立非法人,
   * 不接受所属法人。规则单一源:后端 subjects/unincorporated_org。
   */
  parent_cid_number?: string;
  private_type?: PrivateType;
  partnership_kind?: PartnershipKind;
  admins: CreateInstitutionAdminInput[];
}

export interface InstitutionCreatePayload {
  subject_property: string;
  p1: string | null;
  province_name: string | null;
  city_name: string;
  town_name: string | null;
  institution: string;
  education_type: EducationType | null;
  cid_full_name: string | null;
  cid_short_name: string | null;
  parent_cid_number: string | null;
  private_type: PrivateType | null;
  partnership_kind: PartnershipKind | null;
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
    subject_property: input.subject_property.trim(),
    p1: nullableTrim(input.p1),
    province_name: nullableTrim(input.province_name),
    city_name: input.city_name.trim(),
    town_name: nullableTrim(input.town_name),
    institution: input.institution.trim(),
    education_type: nullableTrim(input.education_type) as EducationType | null,
    cid_full_name: nullableTrim(input.cid_full_name),
    cid_short_name: nullableTrim(input.cid_short_name),
    parent_cid_number: nullableTrim(input.parent_cid_number),
    private_type: nullableTrim(input.private_type) as PrivateType | null,
    partnership_kind: nullableTrim(input.partnership_kind) as PartnershipKind | null,
    admins: (input.admins ?? []).map((admin) => ({
      account_id: admin.account_id.trim(),
      family_name: nullableTrim(admin.family_name) ?? '管理',
      given_name: nullableTrim(admin.given_name) ?? '员',
    })),
  };
}

export interface CreateInstitutionOutput {
  /** 链签会话 ID，提交钱包签名响应时原样回传。 */
  request_id: string;
  cid_number: string;
  /** 创建成功后的机构全称。 */
  cid_full_name: string | null;
  category: InstitutionCategory;
  /** 机构上链创建专用 QR_V1/k=1。 */
  institution_create_sign_request: string;
}

/** 机构详情页可编辑字段。私权类型创建后不可改。 */
export interface UpdateInstitutionInput {
  cid_full_name?: string | null;
  cid_short_name?: string | null;
  /** 所属法人 cid_number(仅 F;传空串后端会拒) */
  parent_cid_number?: string;
  family_name?: string;
  given_name?: string;
  legal_representative_cid_number?: string;
  legal_representative_photo_path?: string;
  legal_representative_photo_name?: string;
  legal_representative_photo_mime?: string;
  legal_representative_photo_size?: number;
}

/** 法人机构搜索结果项(非法人新增弹窗 + F 详情页"所属法人"选择器用) */
export interface ParentInstitutionRow {
  cid_number: string;
  cid_full_name: string;
  subject_property: string;
  /** 私权机构类型。仅用于展示父级机构事实。 */
  private_type?: PrivateType | null;
  partnership_kind?: PartnershipKind | null;
  category: InstitutionCategory;
  /** 盈利属性。F 创建时按"盈利属性附属于所属法人"推导:公法人父恒 0,私法人父继承该值 */
  p1: string;
  province_name: string;
  city_name: string;
}

/** 所属法人搜索参数。预过滤规则与后端 subjects/unincorporated_org 同源(三处同源缺一有绕过口)。 */
export interface SearchParentsOptions {
  /** 非法人的机构代码:JY=教育分校,GT/GP=私权独立非法人,其它 F 由后端判定是否需挂靠 */
  fInstitution: string;
  /** 非法人落位省/市,用于地域预过滤(市级同市/省级同省/国家级不限) */
  province_name: string;
  city_name: string;
  /** 限定父级属性:S=私权入口 / G=公权入口;不传=两者(详情页改挂) */
  parentProperty?: 'S' | 'G';
}

export interface CreateAccountOutput {
  cid_number: string;
  account_name: string;
  chain_status: MultisigChainStatus;
  chain_synced_at: string | null;
  chain_tx_hash: string | null;
  chain_block_number: number | null;
  account_id: string | null;
}

export interface ListInstitutionsQuery {
  category?: InstitutionCategory;
  province_name?: string;
  city_name?: string;
  /** 精确搜索关键字:匹配 cid_number / cid_full_name / cid_short_name;空=返回空页 */
  q?: string;
  private_type?: PrivateType;
  cursor?: string | null;
  page_size?: number;
}
