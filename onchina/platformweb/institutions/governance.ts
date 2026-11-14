// 通用本机构管理员与机构治理 API；注册局专属名册 API 归注册局。
import type { AdminAuth } from '../authentication/types';
import type { InstitutionDetail } from './api';
import { adminHeaders, adminRequest, request } from "../workspace/http";
import { passkeySubmitHeaders } from '../authentication/security';
export type OwnInstitutionAdminRow = {
    accountId: string;
    familyName: string;
    givenName: string;
    roleCode: string;
    roleName: string;
    termRequired: boolean;
    termStart: number;
    termEnd: number;
    assignmentSource: number;
    assignmentSourceLabel: string;
    assignmentSourceRef: string;
    balanceFen?: string | null;
    isSelf: boolean;
};
export type OwnInstitutionAdminListOutput = {
    institutionCode: string;
    cidShortName?: string | null;
    rows: OwnInstitutionAdminRow[];
};
export type InstitutionGovernanceAdminInput = {
    accountId: string;
    familyName: string;
    givenName: string;
    cidNumber?: string;
};
export type InstitutionRolePermissionInput = {
    moduleTag: string;
    actionCode: number;
    operation: 'PROPOSE' | 'VOTE';
};
export type InstitutionGovernanceRoleMutationInput = {
    mutation: 'CREATE' | 'RENAME' | 'DELETE';
    roleCode?: string;
    roleName?: string;
    termRequired?: boolean;
    permissions?: InstitutionRolePermissionInput[];
    assignments?: InstitutionGovernanceAssignmentTargetInput[];
};
export type InstitutionGovernanceAssignmentTargetInput = {
    accountId: string;
    termStart: number;
    termEnd: number;
};
export type InstitutionGovernanceAssignmentChangeInput = {
    roleCode: string;
    assignments: InstitutionGovernanceAssignmentTargetInput[];
};
export type PrepareInstitutionGovernanceInput = {
    cidNumber: string;
    proposerRoleCode: string;
    admins?: InstitutionGovernanceAdminInput[];
    roleMutations?: InstitutionGovernanceRoleMutationInput[];
    assignmentChanges?: InstitutionGovernanceAssignmentChangeInput[];
    representativeCid?: string | null;
    clearLegalRepresentative?: boolean;
};
export type PrepareInstitutionChainOutput = {
    requestId: string;
    cidNumber: string;
    chainAction: number;
    callDataHex: string;
    signRequest: string;
    expiresAt: number;
};
export async function listOwnInstitutionAdmins(auth: AdminAuth): Promise<OwnInstitutionAdminListOutput> {
    return request<OwnInstitutionAdminListOutput>('/api/admin/own-institution-admins', {
        method: 'GET',
        headers: adminHeaders(auth),
    });
}
export async function getOwnInstitution(auth: AdminAuth): Promise<InstitutionDetail> {
    return request<InstitutionDetail>('/api/admin/own-institution', {
        method: 'GET',
        headers: adminHeaders(auth),
    });
}
/** 链签名准备前完成一次现有Passkey认证，取消时不发送业务请求。 */
export async function prepareInstitutionGovernance(auth: AdminAuth, input: PrepareInstitutionGovernanceInput, signal?: AbortSignal): Promise<PrepareInstitutionChainOutput> {
    const headers = await passkeySubmitHeaders(auth, { 'content-type': 'application/json' });
    signal?.throwIfAborted();
    return adminRequest<PrepareInstitutionChainOutput>('/api/admin/institution/governance/prepare', auth, {
        method: 'POST',
        headers, signal,
        body: JSON.stringify(input),
    });
}
export type GovernanceMember = {
    accountId: string;
    signerId: string;
    cidNumber: string;
    familyName: string;
    givenName: string;
};
export type GovernancePermission = InstitutionRolePermissionInput;
export type GovernanceAssignment = {
    accountId: string;
    termStart: number;
    termEnd: number;
    active: boolean;
    effective: boolean;
    sourceLabel: string;
    sourceRef: string;
};
export type GovernanceRole = {
    roleCode: string;
    roleName: string;
    termRequired: boolean;
    active: boolean;
    fixed: boolean;
    assignmentLocked: boolean;
    minAssignments: number;
    maxAssignments: number | null;
    permissions: GovernancePermission[];
    assignments: GovernanceAssignment[];
};
export type GovernanceRepresentative = {
    familyName: string;
    givenName: string;
    cidNumber: string;
    accountId: string;
};
export type GovernanceState = {
    cidNumber: string;
    blockHash: string;
    blockNumber: number;
    currentDay: number;
    moduleTag: string;
    legalRoleCode: string;
    members: GovernanceMember[];
    roles: GovernanceRole[];
    representative: GovernanceRepresentative | null;
    // 可选仅兼容纯治理读者；登记构造器必须核验这三项真实链上字段。
    registrationStateHash?: string;
    governanceThreshold?: number | null;
    registrationRoles?: { roleCode: string; roleName: string }[];
};
export type GovernanceChanges = Pick<PrepareInstitutionGovernanceInput, 'roleMutations' | 'assignmentChanges' | 'representativeCid' | 'clearLegalRepresentative'>;
// 仅合并同时发生的同会话同机构请求；不把查询缓存作为权限或长期状态真源。
const governanceRequests = new Map<string, Promise<GovernanceState>>();
export function getGovernanceState(auth: AdminAuth, cidNumber: string): Promise<GovernanceState> {
    const key = auth.accessToken + '|' + cidNumber;
    let pending = governanceRequests.get(key);
    if (!pending) {
        pending = adminRequest<GovernanceState>(`/api/institutions/${encodeURIComponent(cidNumber)}/governance`, auth)
            .finally(() => governanceRequests.delete(key));
        governanceRequests.set(key, pending);
    }
    return pending;
}
/** 岗位身份和权限都来自同一读取结果，选择发起岗位不额外请求资格RPC。 */
export function governanceProposers(auth: AdminAuth, state: GovernanceState): GovernanceRole[] {
    if (auth.institutionCidNumber !== state.cidNumber)
        return [];
    return state.roles.filter(role => role.active
        && role.permissions.some(p => p.moduleTag === state.moduleTag && p.actionCode === 3 && p.operation === 'PROPOSE')
        && role.assignments.some(a => a.effective && state.members.some(m => m.accountId === a.accountId && m.signerId.toLowerCase() === auth.accountId.toLowerCase())));
}
export function dayFromDate(value: string): number {
    if (!/^\d{4}-\d{2}-\d{2}$/.test(value))
        throw new Error('请选择有效日期');
    const [year, month, day] = value.split('-').map(Number);
    const millis = Date.UTC(year, month - 1, day);
    if (new Date(millis).toISOString().slice(0, 10) !== value)
        throw new Error('日期不存在');
    const result = millis / 86400000;
    if (result <= 0 || result > 4294967295)
        throw new Error('任期日期超出范围');
    return result;
}
export function dateFromDay(day: number): string { return day ? new Date(day * 86400000).toISOString().slice(0, 10) : ''; }
export function memberName(member?: GovernanceMember): string { return member ? `${member.familyName}${member.givenName}` || member.accountId : '不在当前名册'; }
export function assignmentSummary(state: GovernanceState, termRequired: boolean, assignments: InstitutionGovernanceAssignmentTargetInput[]): string {
    return assignments.map(a => `${memberName(state.members.find(m => m.accountId === a.accountId))} · ${a.accountId}：${termRequired ? `${dateFromDay(a.termStart)} 至 ${dateFromDay(a.termEnd)}` : '无任期'}`).join('；') || '空缺';
}
const registrationBusinessActions: [string, string, number][] = [
    ['公民投票资格上链', 'cit-id', 0], ['公民参选资格上链', 'cit-id', 1], ['更新投票资格', 'cit-id', 2], ['更新参选资格', 'cit-id', 3],
    ['撤销资格', 'cit-id', 4], ['公民CID占号', 'cit-id', 6], ['换绑钱包账户', 'cit-id', 7], ['吊销公民CID', 'cit-id', 8],
    ['设置地址名称', 'addr-reg', 1], ['移除地址名称', 'addr-reg', 2], ['设置地址', 'addr-reg', 3], ['移除地址', 'addr-reg', 4],
    ['机构登记', 'ins-reg', 0],
];
export function permissionLabel(p: GovernancePermission): string {
    const action = (p.moduleTag === 'pub-mgmt' || p.moduleTag === 'pri-mgmt') && p.actionCode === 3 ? '机构治理'
        : (p.moduleTag === 'pub-mgmt' || p.moduleTag === 'pri-mgmt') && p.actionCode === 2 ? '机构关闭'
            : registrationBusinessActions.find(([, tag, code]) => tag === p.moduleTag && code === p.actionCode)?.[0] ?? `${p.moduleTag} / ${p.actionCode}`;
    return `${action} · ${p.operation === 'PROPOSE' ? '发起' : '表决'}`;
}
export function validateAssignments(state: GovernanceState, role: Pick<GovernanceRole, 'termRequired' | 'minAssignments' | 'maxAssignments'>, assignments: InstitutionGovernanceAssignmentTargetInput[]): void {
    if (assignments.length < role.minAssignments || role.maxAssignments !== null && assignments.length > role.maxAssignments)
        throw new Error('任职人数不符合岗位规则');
    const seen = new Set<string>();
    for (const a of assignments) {
        if (!state.members.some(m => m.accountId === a.accountId))
            throw new Error('任职人员必须属于现有链上管理员名册');
        if (seen.has(a.accountId))
            throw new Error('任职人员不能重复');
        seen.add(a.accountId);
        if (role.termRequired ? !Number.isInteger(a.termStart) || !Number.isInteger(a.termEnd) || a.termStart <= 0 || a.termEnd < a.termStart : a.termStart !== 0 || a.termEnd !== 0)
            throw new Error('任期与岗位规则不一致');
    }
}
/** 三个独立页显式构造非名册载荷，拒绝夹带管理员集合。 */
export function nonAdminGovernance(state: GovernanceState, changes: GovernanceChanges, proposerRoleCode: string): PrepareInstitutionGovernanceInput {
    if ('admins' in changes)
        throw new Error('此页面不处理管理员名册');
    if (!proposerRoleCode)
        throw new Error('请选择有权发起的岗位');
    const roleMutations = changes.roleMutations;
    for (const mutation of roleMutations ?? []) {
        if (mutation.mutation === 'CREATE') {
            if (mutation.roleCode || !mutation.roleName?.trim() || !mutation.permissions?.length)
                throw new Error('新岗位必须填写名称和权限，岗位码由链端生成');
            if (new TextEncoder().encode(mutation.roleName!).length > 128 || mutation.permissions!.length > 256)
                throw new Error('岗位名或权限数量超出上限');
            const permissionKeys = new Set<string>();
            for (const p of mutation.permissions!) {
                const key = `${p.moduleTag}:${p.actionCode}:${p.operation}`;
                if (!p.moduleTag.trim() || new TextEncoder().encode(p.moduleTag).length > 32 || !Number.isInteger(p.actionCode) || p.actionCode < 0 || p.actionCode > 4294967295 || !['PROPOSE', 'VOTE'].includes(p.operation) || permissionKeys.has(key))
                    throw new Error('岗位权限为空、重复或格式无效');
                permissionKeys.add(key);
            }
            validateAssignments(state, { termRequired: !!mutation.termRequired, minAssignments: 0, maxAssignments: null }, mutation.assignments ?? []);
        }
        else {
            if (mutation.mutation === 'RENAME' && (!mutation.roleName?.trim() || new TextEncoder().encode(mutation.roleName).length > 128))
                throw new Error('岗位名称为空或超出上限');
            const role = state.roles.find(r => r.roleCode === mutation.roleCode);
            if (!role || role.fixed || !role.active)
                throw new Error('此岗位不可改名或删除');
            if (mutation.permissions?.length || mutation.assignments?.length || mutation.termRequired)
                throw new Error('既有岗位定义只允许改名或删除');
        }
    }
    for (const change of changes.assignmentChanges ?? []) {
        const role = state.roles.find(r => r.roleCode === change.roleCode);
        if (!role?.active)
            throw new Error('岗位不存在或已停用');
        const legalChange = changes.representativeCid || changes.clearLegalRepresentative;
        if (role.assignmentLocked && !(legalChange && role.roleCode === state.legalRoleCode))
            throw new Error('该岗位须通过专属治理流程调整');
        validateAssignments(state, role, change.assignments);
    }
    if (changes.representativeCid && changes.clearLegalRepresentative)
        throw new Error('更换与解除不能同时提交');
    if (!roleMutations?.length && !changes.assignmentChanges?.length && !changes.representativeCid && !changes.clearLegalRepresentative)
        throw new Error('没有治理变更');
    return { cidNumber: state.cidNumber, proposerRoleCode, roleMutations, assignmentChanges: changes.assignmentChanges,
        representativeCid: changes.representativeCid, clearLegalRepresentative: changes.clearLegalRepresentative };
}
/** 法代信息和专属岗位任职按同一载荷同步；不隐式新增管理员。 */
export function representativeChanges(state: GovernanceState, accountId: string | null, citizenCid?: string): GovernanceChanges {
    const role = state.roles.find(r => r.roleCode === state.legalRoleCode && r.active);
    if (!role)
        throw new Error('该机构未配置法定代表人岗位');
    if (accountId === null) {
        validateAssignments(state, role, []);
        return { clearLegalRepresentative: true, assignmentChanges: [{ roleCode: role.roleCode, assignments: [] }] };
    }
    const member = state.members.find(m => m.accountId === accountId);
    if (!member)
        throw new Error('须选择现有链上管理员，管理员新增不在本页办理');
    const representativeCid = citizenCid?.trim() || member.cidNumber;
    if (!representativeCid)
        throw new Error('请填写该成员对应的公民CID');
    if (role.termRequired)
        throw new Error('该法代岗位要求专门任期流程，本页不代填');
    const assignments = [{ accountId: member.accountId, termStart: 0, termEnd: 0 }];
    validateAssignments(state, role, assignments);
    return { representativeCid, assignmentChanges: [{ roleCode: role.roleCode, assignments }] };
}
// 以下解析只供原管理员联合变更表单复用，名册及登记逻辑不在本轮改写。
export function parseRolePermissions(text?: string) {
    return (text ?? '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean).map((line) => {
        const [moduleTag, actionCodeRaw, operation] = line.split(/[,，]/).map((part) => part.trim());
        const actionCode = Number(actionCodeRaw);
        if (!moduleTag || !Number.isInteger(actionCode) || actionCode < 0 || !['PROPOSE', 'VOTE'].includes(operation)) {
            throw new Error('岗位权限每行格式必须是：模块标签,动作码,PROPOSE或VOTE');
        }
        return { moduleTag: moduleTag, actionCode: actionCode, operation: operation as 'PROPOSE' | 'VOTE' };
    });
}
export function parseInitialAssignments(text?: string) {
    return (text ?? '').split(/\r?\n/).map((line) => line.trim()).filter(Boolean).map((line) => {
        const [account, termStartRaw = '0', termEndRaw = '0'] = line.split(/[,，]/).map((part) => part.trim());
        const termStart = Number(termStartRaw || 0);
        const termEnd = Number(termEndRaw || 0);
        if (!account || !Number.isInteger(termStart) || !Number.isInteger(termEnd) || termStart < 0 || termEnd < 0) {
            throw new Error('初始任职每行格式必须是：管理员账户,任期开始,任期结束');
        }
        return { accountId: account, termStart: termStart, termEnd: termEnd };
    });
}
export function parseAssignmentsText(text?: string): InstitutionGovernanceAssignmentChangeInput[] {
    const byRole = new Map<string, InstitutionGovernanceAssignmentChangeInput>();
    for (const raw of (text ?? '').split(/\r?\n/)) {
        const line = raw.trim();
        if (!line)
            continue;
        const [roleCode, account, termStartRaw = '0', termEndRaw = '0'] = line
            .split(/[,，]/)
            .map((part) => part.trim());
        if (!roleCode || !account) {
            throw new Error('任职每行格式必须是：岗位码,管理员账户,任期开始,任期结束');
        }
        const termStart = Number(termStartRaw || 0);
        const termEnd = Number(termEndRaw || 0);
        if (!Number.isInteger(termStart) || !Number.isInteger(termEnd) || termStart < 0 || termEnd < 0) {
            throw new Error('任期必须是非负整数日序');
        }
        const row = byRole.get(roleCode) ?? { roleCode: roleCode, assignments: [] };
        row.assignments.push({ accountId: account, termStart: termStart, termEnd: termEnd });
        byRole.set(roleCode, row);
    }
    return Array.from(byRole.values());
}


/** 与现有公私权Runtime MaxAdmins一致，不再以本地目录计数决定上链人数。 */
export const MAX_INSTITUTION_ADMINS = 1989;
export type InstitutionRegistrationInput = {
    cidNumber: string;
    // 存量成员只传锚点，让后端从同区块原记录保留CID和资料，不重建名册事实。
    admins: ({ accountId: string } & Partial<InstitutionGovernanceAdminInput>)[];
    expectedStateHash: string;
    actorRoleCode?: string;
    initialThreshold?: number;
    roleMutations?: InstitutionGovernanceRoleMutationInput[];
    assignmentChanges?: InstitutionGovernanceAssignmentChangeInput[];
};
export type InstitutionRegistrationDraft = {
    addedAdmins?: InstitutionGovernanceAdminInput[];
    removeAccountId?: string;
    initialRoles?: InstitutionGovernanceRoleMutationInput[];
    initialThreshold?: number;
    assignments?: InstitutionGovernanceAssignmentChangeInput[];
};

/** 业务选择显示中文；稳定动作标识只在系统内构造，不生成岗位或预选权限。 */
export function institutionRegistrationPermissions(state: GovernanceState): { label: string; value: string; permission: GovernancePermission }[] {
    const specs: [string, string, number, 'PROPOSE' | 'VOTE'][] = [
        ['机构治理：发起', state.moduleTag, 3, 'PROPOSE'], ['机构治理：表决', state.moduleTag, 3, 'VOTE'],
    ];
    if (/^[A-Z0-9]{5}-(CREG|FRG)/.test(state.cidNumber)) {
        for (const [label, moduleTag, code] of registrationBusinessActions) specs.push([label, moduleTag, code, 'PROPOSE']);
    }
    return specs.map(([label, moduleTag, actionCode, operation]) => ({ label, value: `${moduleTag}:${actionCode}:${operation}`, permission: { moduleTag, actionCode, operation } }));
}

export function registrationActorRole(state: GovernanceState, selected?: string): string {
    const roles = state.registrationRoles ?? [];
    const role = selected ? roles.find(role => role.roleCode === selected) : roles.length === 1 ? roles[0] : undefined;
    if (!role) throw new Error(roles.length ? '请选择有效的办理岗位名称' : '当前管理员没有可办理此业务的有效岗位');
    return role.roleCode;
}

/** 完整名册和显式任职原子登记；不按钱包人数推导阈值，不重写未变更任职。 */
export function buildInstitutionRegistration(state: GovernanceState, draft: InstitutionRegistrationDraft, actorRoleCode?: string): InstitutionRegistrationInput {
    if (!/^0x[0-9a-f]{64}$/.test(state.registrationStateHash ?? '') || state.governanceThreshold === undefined) throw new Error('登记链状态未就绪，请刷新');
    const actor = registrationActorRole(state, actorRoleCode);
    const added = draft.addedAdmins ?? [];
    const removed = draft.removeAccountId;
    if (removed && (!state.members.some(m => m.accountId === removed) || added.length)) throw new Error('删除目标不在当前链上名册');
    if (!removed && !added.length) throw new Error('没有管理员变更');
    const seen = new Set(state.members.flatMap(m => [m.accountId.toLowerCase(), m.signerId.toLowerCase()]));
    for (const member of added) {
        if (!/^0x[0-9a-f]{64}$/.test(member.accountId) || seen.has(member.accountId)) throw new Error('管理员账户无效或已在名册中');
        if (!member.familyName.trim() || !member.givenName.trim() || new TextEncoder().encode(member.familyName.trim()).length > 128 || new TextEncoder().encode(member.givenName.trim()).length > 128) throw new Error('请填写有效的管理员姓和名');
        seen.add(member.accountId);
    }
    const admins = [...state.members.filter(m => m.accountId !== removed).map(m => ({ accountId: m.accountId })), ...added.map(m => ({ ...m, familyName: m.familyName.trim(), givenName: m.givenName.trim() }))];
    if (!admins.length || admins.length > MAX_INSTITUTION_ADMINS) throw new Error('管理员人数超出链上范围');
    const initial = state.governanceThreshold === null;
    const mutations = draft.initialRoles ?? [];
    if (initial ? !mutations.length || draft.initialThreshold === undefined : mutations.length > 0 || draft.initialThreshold !== undefined) throw new Error('首次登记须初始化岗位和阈值，后续登记不得重置');
    const names = new Set(state.roles.map(r => r.roleName));
    const rosterState = { ...state, members: [...state.members.filter(m => m.accountId !== removed), ...added.map(m => ({ ...m, cidNumber: m.cidNumber ?? '', signerId: m.accountId }))] };
    for (const mutation of mutations) {
        const name = mutation.roleName?.trim();
        if (mutation.mutation !== 'CREATE' || mutation.roleCode || !name || names.has(name) || new TextEncoder().encode(name).length > 128 || !mutation.permissions?.length) throw new Error('首次动态岗位必须填写不重复的中文名称和权限，不得输入岗位码');
        names.add(name);
        const permissions = new Set<string>();
        for (const p of mutation.permissions) {
            const key = `${p.moduleTag}:${p.actionCode}:${p.operation}`;
            if (!institutionRegistrationPermissions(state).some(option => option.value === key) || permissions.has(key)) throw new Error('岗位权限无效或重复');
            permissions.add(key);
        }
        validateAssignments(rosterState, { termRequired: !!mutation.termRequired, minAssignments: 0, maxAssignments: null }, mutation.assignments ?? []);
    }
    const changes = new Map<string, InstitutionGovernanceAssignmentChangeInput>();
    for (const change of draft.assignments ?? []) {
        const role = state.roles.find(r => r.roleCode === change.roleCode && r.active && !r.assignmentLocked);
        if (!role || initial || changes.has(change.roleCode) || change.assignments.some(a => !added.some(m => m.accountId === a.accountId))) throw new Error('请选择可任职的现有岗位和新增人员');
        validateAssignments(rosterState, role, change.assignments);
        changes.set(change.roleCode, { roleCode: change.roleCode, assignments: change.assignments });
    }
    for (const role of state.roles) {
        const deleting = !!removed && role.assignments.some(a => a.accountId === removed);
        if (!deleting && !changes.has(role.roleCode)) continue;
        if (role.assignmentLocked) throw new Error('受保护岗位须先通过原治理或任免流程处理');
        const kept = role.assignments.filter(a => a.accountId !== removed);
        if (kept.some(a => !a.active)) throw new Error('该岗位存在已结束任职，请先通过任职治理处理');
        const merged = [...kept.map(a => ({ accountId: a.accountId, termStart: a.termStart, termEnd: a.termEnd })), ...(changes.get(role.roleCode)?.assignments ?? [])];
        validateAssignments(rosterState, role, merged);
        changes.set(role.roleCode, { roleCode: role.roleCode, assignments: merged });
    }
    if (removed && state.representative?.accountId === removed) throw new Error('须先通过原流程解除法定代表人');
    const effective = (term: boolean, a: InstitutionGovernanceAssignmentTargetInput) => term ? a.termStart > 0 && a.termStart <= state.currentDay && state.currentDay <= a.termEnd : a.termStart === 0 && a.termEnd === 0;
    const seats = [
        ...state.roles.filter(r => r.active).map(r => ({ permissions: r.permissions, termRequired: r.termRequired, assignments: changes.get(r.roleCode)?.assignments ?? r.assignments.filter(a => a.active) })),
        ...mutations.map(r => ({ permissions: r.permissions!, termRequired: !!r.termRequired, assignments: r.assignments ?? [] })),
    ];
    let voters = 0, proposer = false;
    for (const role of seats) {
        const active = role.assignments.filter(a => effective(role.termRequired, a));
        const permits = (op: string) => role.permissions.some(p => p.moduleTag === state.moduleTag && p.actionCode === 3 && p.operation === op);
        if (permits('PROPOSE') && active.length) proposer = true;
        if (permits('VOTE')) voters += active.length;
    }
    const threshold = initial ? draft.initialThreshold! : state.governanceThreshold!;
    if (!Number.isInteger(threshold) || !proposer || voters < 2 || threshold < 1 || threshold > voters || threshold * 2 <= voters) throw new Error('机构阈值须按有效治理岗位席位严格过半，至少两席，并保留有效发起岗位');
    if (added.some(m => !seats.some(r => r.assignments.some(a => a.accountId === m.accountId)))) throw new Error('请为每位新增管理员明确选择任职岗位');
    return { cidNumber: state.cidNumber, admins, expectedStateHash: state.registrationStateHash!, actorRoleCode: actor,
        ...(initial ? { initialThreshold: threshold, roleMutations: mutations.map(m => ({ ...m, roleName: m.roleName!.trim() })) } : {}),
        assignmentChanges: [...changes.values()] };
}


/** 响应丢失/会话已消费时只核对链上目标结果，不据此构造或重签另一笔交易。 */
export function institutionRegistrationMatches(state: GovernanceState, input: InstitutionRegistrationInput): boolean {
    const sorted = (values: string[]) => [...values].sort().join('|');
    if (state.cidNumber !== input.cidNumber || sorted(state.members.map(m => m.accountId)) !== sorted(input.admins.map(m => m.accountId))) return false;
    if (input.initialThreshold !== undefined && state.governanceThreshold !== input.initialThreshold) return false;
    if (input.admins.some(m => {
        const actual = state.members.find(a => a.accountId === m.accountId);
        return !actual || m.cidNumber !== undefined && actual.cidNumber !== m.cidNumber || m.familyName !== undefined && actual.familyName !== m.familyName || m.givenName !== undefined && actual.givenName !== m.givenName;
    })) return false;
    const assignmentsMatch = (actual: GovernanceAssignment[], expected: InstitutionGovernanceAssignmentTargetInput[]) =>
        actual.length === expected.length && actual.every(a => a.active && expected.some(e => e.accountId === a.accountId && e.termStart === a.termStart && e.termEnd === a.termEnd));
    for (const change of input.assignmentChanges ?? []) {
        const actual = state.roles.find(r => r.roleCode === change.roleCode);
        if (!actual || !assignmentsMatch(actual.assignments, change.assignments)) return false;
    }
    for (const created of input.roleMutations ?? []) {
        const actual = state.roles.find(r => r.roleName === created.roleName && r.active);
        if (!actual || actual.termRequired !== !!created.termRequired || !assignmentsMatch(actual.assignments, created.assignments ?? []) ||
            sorted(actual.permissions.map(p => `${p.moduleTag}:${p.actionCode}:${p.operation}`)) !== sorted((created.permissions ?? []).map(p => `${p.moduleTag}:${p.actionCode}:${p.operation}`))) return false;
    }
    return true;
}


/** 登记与本机构治理共用名册/任职构造；治理仍由原call 8投票执行，不获得登记权限。 */
export function buildInstitutionAdminGovernance(auth: AdminAuth, state: GovernanceState, draft: InstitutionRegistrationDraft, selected?: string): InstitutionRegistrationInput {
    if (auth.institutionCidNumber !== state.cidNumber) throw new Error('只能发起本机构治理');
    if (state.governanceThreshold == null) throw new Error('本机构须先由注册局完成初始化');
    // 现有私权治理解析器会清空管理员CID；该后端文件不在本步14文件内，拒绝有损提交。
    // 注册局call 9已保留原始名册CID，可通过同一详情页的登记入口办理。
    if (state.moduleTag === 'pri-mgmt' && state.members.some(member => member.cidNumber)) throw new Error('现有私权治理接口不能无损保留名册CID，请由注册局使用本机构详情的登记入口办理');
    const registrationRoles = governanceProposers(auth, state).map(role => ({ roleCode: role.roleCode, roleName: role.roleName }));
    const input = buildInstitutionRegistration({ ...state, registrationRoles }, draft, selected);
    input.admins = input.admins.map(member => {
        const stored = state.members.find(old => old.accountId === member.accountId);
        return stored ? { accountId: stored.accountId, cidNumber: stored.cidNumber, familyName: stored.familyName, givenName: stored.givenName } : member;
    });
    return input;
}
