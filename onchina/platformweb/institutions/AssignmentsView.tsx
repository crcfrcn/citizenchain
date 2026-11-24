// 展示和编辑机构任职，生成对应治理请求。

import { IdentityCidInput } from '../controls/IdentityCidInput';
import { useState } from 'react';
import { Alert, Button, Card, Input, Modal, Select, Space, Table, Typography } from 'antd';
import { GovernanceReview, GovernanceStatus, useGovernanceState, type GovernanceDraft, type GovernanceViewProps } from './GovernanceReview';
import { assignmentSummary, dateFromDay, dayFromDate, governanceProposers, validateAssignments, type GovernanceRole, type GovernanceState, type InstitutionGovernanceAssignmentTargetInput } from './governance';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
export type AssignmentRow = {
    personCid: string;
    start: string;
    end: string;
};
export function assignmentRows(state: GovernanceState, role: Pick<GovernanceRole, 'termRequired' | 'minAssignments' | 'maxAssignments'>, rows: AssignmentRow[]): InstitutionGovernanceAssignmentTargetInput[] {
    const result = rows.map(row => ({ personCid: row.personCid, termStart: role.termRequired ? dayFromDate(row.start) : 0, termEnd: role.termRequired ? dayFromDate(row.end) : 0 }));
    validateAssignments(state, role, result);
    return result;
}
/** 新建岗位的可选初始任职也使用同一人员/日期编辑器，不维护第二套文本解析。 */
export function AssignmentEditor({ state, termRequired, rows, onChange }: {
    state: GovernanceState;
    termRequired: boolean;
    rows: AssignmentRow[];
    onChange: (rows: AssignmentRow[]) => void;
}) {
    const update = (index: number, patch: Partial<AssignmentRow>) => onChange(rows.map((row, i) => i === index ? { ...row, ...patch } : row));
    return <Space direction="vertical" style={{ width: '100%' }}>
    {rows.map((row, index) => <Space key={index} wrap>
      <IdentityCidInput aria-label={`任职人员${index + 1}`} style={{ minWidth: 320 }} value={row.personCid} placeholder="完整人员CID" onChange={e => update(index, { personCid: e.target.value })}/>

      {termRequired && <><Input aria-label={`任期开始${index + 1}`} type="date" value={row.start} onChange={e => update(index, { start: e.target.value })}/><Input aria-label={`任期结束${index + 1}`} type="date" value={row.end} onChange={e => update(index, { end: e.target.value })}/></>}
      <Button danger onClick={() => onChange(rows.filter((_, i) => i !== index))}>移除此任职</Button>
    </Space>)}
    <Button onClick={() => onChange([...rows, { personCid: '', start: '', end: '' }])}>添加任职人员</Button>
  </Space>;
}
export function AssignmentsView(props: GovernanceViewProps) {
    // 身份或目标切换时销毁草稿和认证流程，防止旧机构变更沿用到新页面。
    return <AssignmentsPanel key={`${props.auth.accessToken}|${props.auth.personCid}|${props.auth.bindingRevision}|${props.auth.accountId}|${props.auth.institutionCidNumber}|${props.auth.roleCode}|${props.cidNumber}`} {...props}/>;
}
function AssignmentsPanel(props: GovernanceViewProps) {
    const { state, error, loading, reload } = useGovernanceState(props);
    const [roleCode, setRoleCode] = useState<string>();
    const [edit, setEdit] = useState(false);
    const [rows, setRows] = useState<AssignmentRow[]>([]);
    const [draft, setDraft] = useState<GovernanceDraft | null>(null);
    if (!state)
        return <GovernanceStatus loading={loading} error={error} reload={reload}/>;
    const role = state.roles.find(r => r.roleCode === roleCode) ?? state.roles[0];
    const writable = props.canWrite !== false && governanceProposers(props.auth, state).length > 0;
    const confirm = () => {
        if (!role)
            return;
        try {
            const assignments = assignmentRows(state, role, rows);
            setDraft({ title: '确认岗位任职变更', changes: { assignmentChanges: [{ roleCode: role.roleCode, assignments }] }, rows: [{ label: role.roleName, before: assignmentSummary(state, role.termRequired, role.assignments), after: assignmentSummary(state, role.termRequired, assignments) }] });
            setEdit(false);
        }
        catch (e) {
            notice.error(e, '任职校验失败');
        }
    };
    return <Card title="任职管理" extra={<Space><Button onClick={reload}>刷新</Button><Button type="primary" disabled={!writable || !role?.active || role.assignmentLocked} onClick={() => { setRows(role.assignments.map(a => ({ personCid: a.personCid, start: dateFromDay(a.termStart), end: dateFromDay(a.termEnd) }))); setEdit(true); }}>发起任职变更</Button></Space>}>
    <Space><Typography.Text>岗位</Typography.Text><Select aria-label="所选岗位" style={{ minWidth: 240 }} value={role?.roleCode} onChange={setRoleCode} options={state.roles.map(r => ({ value: r.roleCode, label: r.roleName }))}/></Space>
    {role?.assignmentLocked && <Alert style={{ marginTop: 16 }} type="info" message={role.roleCode === state.legalRoleCode ? '法定代表人专属任职请在法定代表人页面办理' : '该岗位涉及专属治理或专属人员任免，本页只读'}/>}
    <Table rowKey="personCid" pagination={false} dataSource={role?.assignments ?? []} columns={[
            { title: '任职人员', render: (_, a) => a.personCid },
            { title: '任期开始', render: (_, a) => role?.termRequired ? dateFromDay(a.termStart) || '-' : '无任期' },
            { title: '任期结束', render: (_, a) => role?.termRequired ? dateFromDay(a.termEnd) || '-' : '无任期' },
            { title: '状态', render: (_, a) => a.effective ? '有效' : a.active ? '未生效或已到期' : '已结束' },
            { title: '任职来源', dataIndex: 'sourceLabel' },
        ]}/>
    <Modal cancelText="取消" title={`调整${role?.roleName ?? ''}任职`} open={edit} onCancel={() => setEdit(false)} onOk={confirm} okText="核对变更" width={800} zIndex={CID_MODAL_Z_INDEX.business}>
      <Alert type="info" message="提交完整目标任职集合；任职人员必须具有有效identity绑定。日期按UTC日序提交。"/>
      {role && <AssignmentEditor state={state} termRequired={role.termRequired} rows={rows} onChange={setRows}/>}
    </Modal>
    <GovernanceReview auth={props.auth} state={state} draft={draft} onClose={() => setDraft(null)} onSubmitted={reload}/>
  </Card>;
}
