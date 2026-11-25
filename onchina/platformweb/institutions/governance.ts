// 机构岗位与人员CID任职合同；钱包只属于当前identity绑定，不属于机构名单。
import type { AdminAuth } from '../authentication/types';
import type { InstitutionDetail } from './api';
import { adminHeaders, adminRequest, request } from '../workspace/http';
import { passkeySubmitHeaders } from '../authentication/security';
export type InstitutionRolePermissionInput = { moduleTag: string; actionCode: number; operation: 'PROPOSE' | 'VOTE' };
export type InstitutionGovernanceAssignmentTargetInput = { personCid: string; termStart: number; termEnd: number };
export type InstitutionGovernanceAssignmentChangeInput = { roleCode: string; assignments: InstitutionGovernanceAssignmentTargetInput[] };
export type InstitutionGovernanceRoleMutationInput = { mutation: 'CREATE' | 'RENAME' | 'DELETE' | 'CONFIGURE'; roleCode?: string; roleName?: string; active?: boolean; termRequired?: boolean; seats?: number; permissions?: InstitutionRolePermissionInput[]; assignments?: InstitutionGovernanceAssignmentTargetInput[] };
export type InstitutionPolicyInput = { permissionChanges: { roleCode: string; permissions: GovernancePermission[] }[]; threshold?: number; expectedStateHash: string };
export type PrepareInstitutionGovernanceInput = { cidNumber: string; proposerRoleCode: string; roleMutations?: InstitutionGovernanceRoleMutationInput[]; assignmentChanges?: InstitutionGovernanceAssignmentChangeInput[]; representativeCid?: string | null; clearLegalRepresentative?: boolean; representativeFamilyName?: string; representativeGivenName?: string; policy?: InstitutionPolicyInput };
export type PrepareInstitutionChainOutput = { requestId: string; cidNumber: string; chainAction: number; callDataHex: string; signRequest: string; expiresAt: number };
export type GovernancePermission = InstitutionRolePermissionInput;
export type GovernanceAssignment = InstitutionGovernanceAssignmentTargetInput & { active: boolean; effective: boolean; sourceLabel: string; sourceRef: string };
export type GovernanceRole = { roleCode: string; roleName: string; termRequired: boolean; seats: number; active: boolean; fixed: boolean; assignmentLocked: boolean; minAssignments: number; maxAssignments: number; permissions: GovernancePermission[]; assignments: GovernanceAssignment[] };
export type RoleChoice = { roleCode: string; roleName: string };
export type GovernanceState = { policyAvailable?: boolean; cidNumber: string; blockHash: string; blockNumber: number; currentDay: number; moduleTag: string; legalRoleCode: string; maxSeats: number; roles: GovernanceRole[]; representative: { familyName: string; givenName: string; personCid: string } | null; registrationStateHash: string; governanceThreshold: number | null; registrationRoles: RoleChoice[]; proposerRoles: RoleChoice[] };
export type GovernanceChanges = Omit<PrepareInstitutionGovernanceInput, 'cidNumber' | 'proposerRoleCode'>;
export type InstitutionRegistrationInput = { cidNumber: string; expectedStateHash: string; actorRoleCode: string; initialThreshold?: number; roleMutations?: InstitutionGovernanceRoleMutationInput[]; assignmentChanges?: InstitutionGovernanceAssignmentChangeInput[] };
export type InstitutionRegistrationDraft = { initialRoles?: InstitutionGovernanceRoleMutationInput[]; initialThreshold?: number; assignments?: InstitutionGovernanceAssignmentChangeInput[] };
export async function getOwnInstitution(auth: AdminAuth): Promise<InstitutionDetail> { return request('/api/admin/own-institution', { method: 'GET', headers: adminHeaders(auth) }); }
const requests = new Map<string, Promise<GovernanceState>>();
export function getGovernanceState(auth: AdminAuth, cidNumber: string): Promise<GovernanceState> {
  const key = JSON.stringify([auth.accessToken, auth.personCid, auth.bindingRevision, auth.institutionCidNumber, auth.roleCode, cidNumber]);
  let pending = requests.get(key);
  if (!pending) { pending = adminRequest<GovernanceState>(`/api/institutions/${encodeURIComponent(cidNumber)}/governance`, auth).finally(() => requests.delete(key)); requests.set(key, pending); }
  return pending;
}
export async function prepareInstitutionGovernance(auth: AdminAuth, input: PrepareInstitutionGovernanceInput, signal?: AbortSignal): Promise<PrepareInstitutionChainOutput> {
  const fields = ['cidNumber', 'proposerRoleCode', 'roleMutations', 'assignmentChanges', 'representativeCid', 'clearLegalRepresentative', 'representativeFamilyName', 'representativeGivenName', 'policy'];
  if (Object.keys(input).some(key => !fields.includes(key))) throw new Error('管理员钱包和名单动作已停用');
  const rows=[...(input.assignmentChanges??[]).flatMap(c=>c.assignments),...(input.roleMutations??[]).flatMap(m=>m.assignments??[])];
  if(rows.some(a=>!a.personCid||Object.keys(a).some(key=>!['personCid','termStart','termEnd'].includes(key)))) throw new Error('任职只接受人员CID和任期');
  if (input.cidNumber !== auth.institutionCidNumber || input.proposerRoleCode !== auth.roleCode) throw new Error('必须使用本次所选机构和岗位');
  const headers = await passkeySubmitHeaders(auth, { 'content-type': 'application/json' }); signal?.throwIfAborted();
  return adminRequest('/api/admin/institution/governance/prepare', auth, { method: 'POST', headers, signal, body: JSON.stringify(input) });
}
export function governanceProposers(auth: AdminAuth, state: GovernanceState): RoleChoice[] { return auth.institutionCidNumber === state.cidNumber ? state.proposerRoles.filter(r => r.roleCode === auth.roleCode) : []; }
export function dayFromDate(value: string): number {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) throw new Error('日期格式无效');
  const time = Date.parse(value + 'T00:00:00Z'); if (!Number.isFinite(time) || new Date(time).toISOString().slice(0, 10) !== value) throw new Error('日期不存在'); return Math.floor(time / 86400000);
}
export function dateFromDay(day: number): string { return day ? new Date(day * 86400000).toISOString().slice(0, 10) : ''; }
export function assignmentSummary(_state: GovernanceState, term: boolean, rows: InstitutionGovernanceAssignmentTargetInput[]): string { return rows.map(a => `${a.personCid}：${term ? `${dateFromDay(a.termStart)} 至 ${dateFromDay(a.termEnd)}` : '无任期'}`).join('；') || '空缺'; }
export function validateAssignments(state: GovernanceState, role: Pick<GovernanceRole, 'termRequired' | 'minAssignments' | 'maxAssignments'>, rows: InstitutionGovernanceAssignmentTargetInput[]): void {
  if (rows.length < role.minAssignments || rows.length > role.maxAssignments) throw new Error('任职人数不符合岗位席位规则');
  const seen = new Set<string>();
  for (const a of rows) {
    // 前端只提示规范格式；登记状态和当前绑定必须由后端同块读取identity验证。
    const parts = /^([A-Z0-9]{5})-(CTZN|NATP)[A-Z0-9]-[0-9]{9}-([0-9]{4,10})$/.exec(a.personCid);
    const year = parts ? Number(parts[3]) : NaN;
    if (!parts || year > 0xffff_ffff || String(year).padStart(4, '0') !== parts[3] || state.moduleTag === 'pub-mgmt' && parts[2] !== 'CTZN') throw new Error('人员CID类别或格式无效');
    if (seen.has(a.personCid)) throw new Error('同岗位人员CID不能重复'); seen.add(a.personCid);
    if (role.termRequired ? !Number.isInteger(a.termStart) || !Number.isInteger(a.termEnd) || a.termStart <= 0 || a.termEnd < a.termStart : a.termStart !== 0 || a.termEnd !== 0) throw new Error('任期与岗位规则不一致');
  }
}
export function buildGovernance(state: GovernanceState, changes: GovernanceChanges, proposerRoleCode: string): PrepareInstitutionGovernanceInput {
  const fields = ['roleMutations', 'assignmentChanges', 'representativeCid', 'clearLegalRepresentative', 'representativeFamilyName', 'representativeGivenName', 'policy'];
  if (Object.keys(changes).some(key => !fields.includes(key))) throw new Error('此页面不接收管理员钱包或名单');
  if (!proposerRoleCode) throw new Error('请选择发起岗位');
  const checkPermissions = (values: GovernancePermission[]) => {
    if (!values.length) throw new Error('链上岗位权限不能为空');
    const keys = new Set<string>();
    for (const p of values) {
      const key = `${p.moduleTag}:${p.actionCode}:${p.operation}`;
      if (!p.moduleTag || new TextEncoder().encode(p.moduleTag).length > 32 || !Number.isInteger(p.actionCode) || p.actionCode < 0 || p.actionCode > 0xffffffff || !['PROPOSE', 'VOTE'].includes(p.operation) || keys.has(key)) throw new Error('权限无效或重复');
      keys.add(key);
    }
  };
  if (changes.policy) {
    const p = changes.policy;
    if (!state.policyAvailable || changes.roleMutations?.length || changes.assignmentChanges?.length || changes.representativeCid || changes.clearLegalRepresentative || changes.representativeFamilyName || changes.representativeGivenName) throw new Error('当前政策不可用或夹带人事变更');
    if (p.expectedStateHash !== state.registrationStateHash || !/^0x[0-9a-f]{64}$/i.test(p.expectedStateHash)) throw new Error('政策状态指纹已变化');
    if (p.threshold !== undefined && (!Number.isInteger(p.threshold) || p.threshold < 2 || p.threshold > 0xffffffff)) throw new Error('共同阈值无效');
    if (!p.permissionChanges.length && p.threshold === undefined) throw new Error('没有政策变更');
    const codes = new Set<string>();
    for (const c of p.permissionChanges) {
      if (!state.roles.some(r => r.roleCode === c.roleCode && !r.fixed && r.roleCode.startsWith('R_')) || codes.has(c.roleCode)) throw new Error('政策岗位无效或重复');
      codes.add(c.roleCode); checkPermissions(c.permissions);
    }
    return { cidNumber: state.cidNumber, proposerRoleCode, policy: p };
  }
  const used = new Set<string>();
  for (const m of changes.roleMutations ?? []) {
    if (m.mutation === 'CREATE') {
      if (m.active !== undefined || m.roleCode || !m.roleName?.trim() || !m.permissions?.length || !Number.isInteger(m.seats) || m.seats! < 1 || m.seats! > state.maxSeats) throw new Error('新岗位须填写名称、正整数席位及权限，岗位码由链生成');
      validateAssignments(state, { termRequired: !!m.termRequired, minAssignments: 0, maxAssignments: m.seats! }, m.assignments ?? []);
      checkPermissions(m.permissions);
    } else {
      const role = state.roles.find(r => r.roleCode === m.roleCode && (r.active || m.mutation === 'CONFIGURE'));
      if (!role || role.fixed || used.has(role.roleCode)) throw new Error('固定、重复或无效岗位不可改写');
      used.add(role.roleCode);
      if (m.mutation === 'CONFIGURE') {
        if (!state.policyAvailable || !role.roleCode.startsWith('R_') || typeof m.active !== 'boolean' || typeof m.termRequired !== 'boolean' || !Number.isInteger(m.seats) || m.seats! < 1 || m.seats! > state.maxSeats || m.roleName !== undefined || m.permissions !== undefined || m.assignments !== undefined) throw new Error('岗位属性配置无效');
        const rows = changes.assignmentChanges?.find(c => c.roleCode === role.roleCode)?.assignments ?? role.assignments;
        if (!m.active && rows.length) throw new Error('停用岗位必须同案清空任职');
        validateAssignments(state, { termRequired: m.termRequired, minAssignments: 0, maxAssignments: m.seats! }, rows);
      } else {
        if (m.active !== undefined || m.permissions || m.assignments || m.seats !== undefined || m.termRequired !== undefined) throw new Error('既有岗位只允许改名或删除');
        if (m.mutation === 'RENAME' && !m.roleName?.trim()) throw new Error('岗位名称不能为空');
      }
    }
  }
  const changed = new Set<string>();
  for (const c of changes.assignmentChanges ?? []) {
    const role = state.roles.find(r => r.roleCode === c.roleCode);
    const config = changes.roleMutations?.find(m => m.mutation === 'CONFIGURE' && m.roleCode === c.roleCode);
    if (!role || (!role.active && !config) || changed.has(c.roleCode) || changes.roleMutations?.some(m => m.mutation === 'DELETE' && m.roleCode === c.roleCode)) throw new Error('任职岗位无效或重复');
    changed.add(c.roleCode);
    if (role.assignmentLocked && !(c.roleCode === state.legalRoleCode && (changes.representativeCid || changes.clearLegalRepresentative))) throw new Error('专属任职须使用所属业务入口');
    validateAssignments(state, config ? { termRequired: !!config.termRequired, minAssignments: 0, maxAssignments: config.seats! } : role, c.assignments);
  }
  if (!changes.roleMutations?.length && !changes.assignmentChanges?.length && !changes.representativeCid && !changes.clearLegalRepresentative) throw new Error('没有治理变更');
  return { cidNumber: state.cidNumber, proposerRoleCode, ...changes };
}
export function representativeChanges(state: GovernanceState, personCid: string | null, familyName = '', givenName = ''): GovernanceChanges {
  const role = state.roles.find(r => r.roleCode === state.legalRoleCode && r.active); if (!role || role.seats !== 1) throw new Error('未配置合法的唯一法代岗位');
  const rows = personCid ? [{ personCid, termStart: 0, termEnd: 0 }] : []; validateAssignments(state, role, rows);
  if (!personCid) return { clearLegalRepresentative: true, assignmentChanges: [{ roleCode: role.roleCode, assignments: [] }] };
  if (!familyName.trim() || !givenName.trim()) throw new Error('法代姓和名不能为空');
  return { representativeCid: personCid, representativeFamilyName: familyName.trim(), representativeGivenName: givenName.trim(), assignmentChanges: [{ roleCode: role.roleCode, assignments: rows }] };
}
const registrationActions: [string, string, number][] = [['机构登记', 'ins-reg', 0], ['公民投票资格', 'cit-id', 0], ['公民参选资格', 'cit-id', 1], ['更新投票资格', 'cit-id', 2], ['更新参选资格', 'cit-id', 3], ['撤销资格', 'cit-id', 4], ['公民CID占号', 'cit-id', 6], ['换绑钱包', 'cit-id', 7], ['吊销CID', 'cit-id', 8], ['设置地址名称', 'addr-reg', 1], ['移除地址名称', 'addr-reg', 2], ['设置地址', 'addr-reg', 3], ['移除地址', 'addr-reg', 4]];
export function permissionLabel(p: GovernancePermission): string { const name = p.actionCode === 3 && ['pub-mgmt', 'pri-mgmt'].includes(p.moduleTag) ? '机构治理' : registrationActions.find(([, tag, action]) => tag === p.moduleTag && action === p.actionCode)?.[0] ?? `${p.moduleTag} / ${p.actionCode}`; return `${name} · ${p.operation === 'PROPOSE' ? '发起' : '表决'}`; }
export function institutionRegistrationPermissions(state: GovernanceState) { const choices: [string, string, number, 'PROPOSE' | 'VOTE'][] = [['机构治理：发起', state.moduleTag, 3, 'PROPOSE'], ['机构治理：表决', state.moduleTag, 3, 'VOTE']]; for (const [label, module, action] of registrationActions) if (state.cidNumber.includes('-CREG') || state.cidNumber.includes('-FRG')) choices.push([label, module, action, 'PROPOSE']); return choices.map(([label, moduleTag, actionCode, operation]) => ({ label, value: `${moduleTag}:${actionCode}:${operation}`, permission: { moduleTag, actionCode, operation } })); }
export function registrationActorRole(state: GovernanceState, selected?: string): string { if (!selected || !state.registrationRoles.some(r => r.roleCode === selected)) throw new Error('请选择当前有效的办理岗位'); return selected; }
export function buildInstitutionConfiguration(state: GovernanceState, draft: Pick<InstitutionRegistrationDraft, 'initialRoles' | 'initialThreshold'>, selected?: string): InstitutionRegistrationInput {
  if (state.governanceThreshold !== null || !draft.initialRoles?.length) throw new Error('首次治理配置不能重置');
  const threshold = draft.initialThreshold; if (!Number.isInteger(threshold) || threshold! < 1) throw new Error('请填写有效治理阈值');
  buildGovernance(state, { roleMutations: draft.initialRoles }, 'validation');
  let votes = 0, proposer = false;
  for (const role of draft.initialRoles) { const active = (role.assignments ?? []).filter(a => role.termRequired ? a.termStart <= state.currentDay && state.currentDay <= a.termEnd : true); if (role.permissions?.some(p => p.moduleTag === state.moduleTag && p.actionCode === 3 && p.operation === 'VOTE')) votes += active.length; if (active.length && role.permissions?.some(p => p.moduleTag === state.moduleTag && p.actionCode === 3 && p.operation === 'PROPOSE')) proposer = true; }
  if (!proposer || votes < 2 || threshold! > votes || threshold! * 2 <= votes) throw new Error('治理阈值须按有效岗位席位严格过半，并保留发起岗位');
  return { cidNumber: state.cidNumber, actorRoleCode: registrationActorRole(state, selected), expectedStateHash: state.registrationStateHash, initialThreshold: threshold, roleMutations: draft.initialRoles };
}
export function institutionRegistrationMatches(state: GovernanceState, input: InstitutionRegistrationInput): boolean {
  if (state.cidNumber !== input.cidNumber || 'admins' in input || 'walletChanges' in input || input.initialThreshold !== undefined && state.governanceThreshold !== input.initialThreshold) return false;
  const same = (actual: GovernanceAssignment[], expected: InstitutionGovernanceAssignmentTargetInput[]) => actual.length === expected.length && actual.every(a => a.active && expected.some(e => e.personCid === a.personCid && e.termStart === a.termStart && e.termEnd === a.termEnd));
  for (const change of input.assignmentChanges ?? []) { const role = state.roles.find(r => r.roleCode === change.roleCode); if (!role || !same(role.assignments, change.assignments)) return false; }
  for (const m of input.roleMutations ?? []) { const role = state.roles.find(r => r.roleName === m.roleName && r.active); if (!role || role.seats !== m.seats || role.termRequired !== !!m.termRequired || !same(role.assignments, m.assignments ?? [])) return false; if (JSON.stringify([...role.permissions].sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b)))) !== JSON.stringify([...(m.permissions ?? [])].sort((a,b)=>JSON.stringify(a).localeCompare(JSON.stringify(b))))) return false; }
  return true;
}
export function buildInstitutionRoleGovernance(auth: AdminAuth, state: GovernanceState, draft: InstitutionRegistrationDraft, selected?: string): InstitutionRegistrationInput {
  if (auth.institutionCidNumber !== state.cidNumber) throw new Error('只能发起本机构治理');
  if (Object.keys(draft).some(key=>key!=='assignments')) throw new Error('岗位任职不接收其他变更');
  buildGovernance(state, { assignmentChanges: draft.assignments }, selected ?? auth.roleCode);
  return { cidNumber: state.cidNumber, actorRoleCode: selected ?? auth.roleCode, expectedStateHash: state.registrationStateHash, assignmentChanges: draft.assignments };
}
