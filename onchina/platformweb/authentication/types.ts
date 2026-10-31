// 登录与角色相关的前端类型集中放在 auth 模块内。
// 管理员按机构码(institution_code)归属机构;workspace/capabilities 由后端会话下发,前端镜像渲染工作台入口。

import type { CapabilitySet } from '../workspace/modules';
import type { InstitutionWorkspace } from '../workspace/types';

export type TokenAdminAuth = {
  accessToken: string;
  accountId: string;
  /** 当前会话绑定的准确机构 CID；由后端根据激活节点绑定派生。 */
  institutionCidNumber: string;
  /** 所属机构码(3/4 字符文本,如 FRG/CREG)。 */
  institutionCode: string;
  /** 行政层级标签(NATIONAL/PROVINCE/CITY/TOWN);私权法人/非法人为空。 */
  adminLevel?: string | null;
  /** 机构能力位(后端单源下发);前端据此渲染工作台入口。 */
  capabilities?: CapabilitySet;
  /** 当前机构工作台清单,用于按机构类型挂载 UI。 */
  workspace?: InstitutionWorkspace;
  /** 管理员姓、名与链上管理员记录保持同名；页面仅在展示时合并。 */
  familyName: string;
  givenName: string;
  scopeProvinceName?: string | null;
  /** 市级及以下机构所属的市。 */
  scopeCityName?: string | null;
  /** 镇级机构所属的镇。 */
  scopeTownName?: string | null;
  /** 当前管理员所属机构简称,字段名与 subjects.cid_short_name 保持唯一命名。 */
  cidShortName?: string | null;
};

export type AdminAuth = TokenAdminAuth;

export function isTokenAuth(auth: AdminAuth): auth is TokenAdminAuth {
  return 'accessToken' in auth;
}
