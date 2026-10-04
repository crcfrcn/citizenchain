// 登录与角色相关的前端类型集中放在 auth 模块内。
// 管理员按机构码(institution_code)归属机构;workspace/capabilities 由后端会话下发,前端镜像渲染工作台入口。

import type { CapabilitySet } from '../platform/capabilityMap';
import type { InstitutionWorkspace } from '../workspace/types';

export type TokenAdminAuth = {
  access_token: string;
  account_id: string;
  /** 当前会话绑定的准确机构 CID；由后端根据激活节点绑定派生。 */
  institution_cid_number: string;
  /** 所属机构码(3/4 字符文本,如 FRG/CREG)。 */
  institution_code: string;
  /** 行政层级标签(NATIONAL/PROVINCE/CITY/TOWN);私权法人/非法人为空。 */
  admin_level?: string | null;
  /** 机构能力位(后端单源下发);前端据此渲染工作台入口。 */
  capabilities?: CapabilitySet;
  /** 当前机构工作台清单,用于按机构类型挂载 UI。 */
  workspace?: InstitutionWorkspace;
  /** 管理员姓、名与链上管理员记录保持同名；页面仅在展示时合并。 */
  family_name: string;
  given_name: string;
  scope_province_name?: string | null;
  /** 市级及以下机构所属的市。 */
  scope_city_name?: string | null;
  /** 镇级机构所属的镇。 */
  scope_town_name?: string | null;
  /** 当前管理员所属机构简称,字段名与 subjects.cid_short_name 保持唯一命名。 */
  cid_short_name?: string | null;
};

export type AdminAuth = TokenAdminAuth;

export function isTokenAuth(auth: AdminAuth): auth is TokenAdminAuth {
  return 'access_token' in auth;
}
