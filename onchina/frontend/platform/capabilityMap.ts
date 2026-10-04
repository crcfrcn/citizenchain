// 控制台能力位类型 + 空能力兜底。
//
// 能力单源在【后端】(src/platform/capability.rs),由登录会话下发(auth.capabilities)。
// 前端只镜像渲染工作台入口,不在此硬编码权限——render-gating 非安全边界,后端始终对越权独立拒绝。
// 新增机构类型只需在后端 capability.rs 补能力位,前端无需改动。

export type CapabilitySet = {
  canViewCitizens: boolean;
  canViewInstitutions: boolean;
  canViewPrivate: boolean;
  canViewEducation: boolean;
  canViewFederalRegistryAdmins: boolean;
  canViewCityRegistryAdmins: boolean;
  canCrudCityRegistryAdmins: boolean;
  /** 只读「本机构管理员」位:非注册局法人可查看本机构链上管理员列表(只读)。 */
  canViewOwnAdmins: boolean;
  canManageInstitutions: boolean;
  canRegisterInstitutions: boolean;
  canBusinessWrite: boolean;
  canViewCityRegistry: boolean;
  canViewFederalRegistry: boolean;
  /** 立法:查看立法/提案/大屏(立法机构通用只读位)。 */
  canViewLegislation: boolean;
  /** 立法:发起法律案(发起院/教委会/自治会;参议会无此位)。 */
  canProposeLegislation: boolean;
  /** 立法：当前代表机构表决；仅提案机构无此能力。 */
  canCastRepresentativeVote: boolean;
  /** 立法:行政签署/三人会签/护宪终审(另线程接入,本轮恒 false)。 */
  canSignLegislation: boolean;
  /** 立法:发起任免案(政府;Phase 4 接入,本轮恒 false)。 */
  canProposePersonnel: boolean;
  /** 立法:发起预算案(政府;Phase 4 接入,本轮恒 false)。 */
  canProposeBudget: boolean;
};

/** 空能力:未登录、能力未下发或未知机构码时的兜底(不显示任何受限 tab)。 */
export const EMPTY_CAPABILITIES: CapabilitySet = {
  canViewCitizens: false,
  canViewInstitutions: false,
  canViewPrivate: false,
  canViewEducation: false,
  canViewFederalRegistryAdmins: false,
  canViewCityRegistryAdmins: false,
  canCrudCityRegistryAdmins: false,
  canViewOwnAdmins: false,
  canManageInstitutions: false,
  canRegisterInstitutions: false,
  canBusinessWrite: false,
  canViewCityRegistry: false,
  canViewFederalRegistry: false,
  canViewLegislation: false,
  canProposeLegislation: false,
  canCastRepresentativeVote: false,
  canSignLegislation: false,
  canProposePersonnel: false,
  canProposeBudget: false,
};
