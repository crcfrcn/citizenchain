// 展示机构岗位并提供创建、改名和删除治理入口。

import { useState } from 'react';
import { Alert, Button, Checkbox, Form, Input, InputNumber, Modal, Select, Space, Steps, Table, Typography } from 'antd';
import { GovernanceReview, GovernanceStatus, useGovernanceState, type GovernanceDraft, type GovernanceViewProps } from './GovernanceReview';
import { AssignmentEditor, assignmentRows, type AssignmentRow } from './AssignmentsView';
import { assignmentSummary, governanceProposers, permissionLabel, dateFromDay, type GovernancePermission, type GovernanceRole } from './governance';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
export function RolesView(props: GovernanceViewProps) {
    // 身份或目标切换时销毁草稿和认证流程，防止旧机构变更沿用到新页面。
    return <RolesPanel key={`${props.auth.accessToken}|${props.auth.personCid}|${props.auth.bindingRevision}|${props.auth.accountId}|${props.auth.institutionCidNumber}|${props.auth.roleCode}|${props.cidNumber}`} {...props}/>;
}
function RolesPanel(props: GovernanceViewProps) {
    const { state, error, loading, reload } = useGovernanceState(props);
    const [query, setQuery] = useState('');
    const [creating, setCreating] = useState(false);
    const [step, setStep] = useState(0);
    const [name, setName] = useState('');
    const [term, setTerm] = useState(false);
    const [seats, setSeats] = useState<number>();
    const [permissions, setPermissions] = useState<GovernancePermission[]>([]);
    const [assignments, setAssignments] = useState<AssignmentRow[]>([]);
    const [selected, setSelected] = useState<GovernanceRole | null>(null);
    const [mode, setMode] = useState<'view' | 'rename'>('view');
    const [editing, setEditing] = useState<{ role: GovernanceRole; kind: 'attributes' | 'permissions' } | null>(null);
    const [enabled, setEnabled] = useState(true);
    const [thresholdEditing, setThresholdEditing] = useState(false);
    const [threshold, setThreshold] = useState<number>();
    const [draft, setDraft] = useState<GovernanceDraft | null>(null);
    if (!state)
        return <GovernanceStatus loading={loading} error={error} reload={reload}/>;
    const writable = props.canWrite !== false && governanceProposers(props.auth, state).length > 0;
    const createDraft = () => {
        try {
            if (!name.trim() || !permissions.length || !Number.isInteger(seats) || seats! < 1 || seats! > state.maxSeats)
                throw new Error('请填写岗位名称并添加权限');
            const targets = assignmentRows(state, { termRequired: term, minAssignments: 0, maxAssignments: seats ?? 0 }, assignments);
            setDraft({ title: '确认岗位创建提案', changes: { roleMutations: [{ mutation: 'CREATE', roleName: name.trim(), termRequired: term, seats, permissions, assignments: targets }] }, rows: [{ label: '席位容量', before: '未创建', after: String(seats) }, { label: '岗位名称', before: '未创建', after: name.trim() }, { label: '岗位权限', before: '无', after: permissions.map(permissionLabel).join('；') }, { label: '任期规则', before: '无', after: term ? '要求任期' : '无任期' }, { label: '初始任职', before: '无', after: assignmentSummary(state, term, targets) }] });
            setCreating(false);
        }
        catch (e) {
            notice.error(e, '岗位校验失败');
        }
    };
    const remove = (role: GovernanceRole) => setDraft({ title: '确认岗位删除提案', changes: { roleMutations: [{ mutation: 'DELETE', roleCode: role.roleCode }] }, rows: [{ label: '岗位名称', before: role.roleName, after: '删除（以链端制度约束为准）' }, { label: '岗位权限', before: role.permissions.map(permissionLabel).join('；'), after: '删除' }, { label: '岗位任职', before: assignmentSummary(state, role.termRequired, role.assignments), after: '清空' }] });
    const openEdit = (role: GovernanceRole, kind: 'attributes' | 'permissions') => {
        setTerm(role.termRequired); setSeats(role.seats); setEnabled(role.active);
        setPermissions(role.permissions.map(p => ({ ...p })));
        setAssignments(role.assignments.map(a => ({ personCid: a.personCid, start: dateFromDay(a.termStart), end: dateFromDay(a.termEnd) })));
        setEditing({ role, kind });
    };
    const finishEdit = () => {
        if (!editing) return;
        try {
            const role = editing.role;
            if (editing.kind === 'permissions') {
                if (!permissions.length) throw new Error('链上岗位权限不能为空');
                setDraft({ title: '确认岗位原生权限政策', changes: { policy: { permissionChanges: [{ roleCode: role.roleCode, permissions }], expectedStateHash: state.registrationStateHash } }, rows: [{ label: role.roleName, before: role.permissions.map(permissionLabel).join('；'), after: permissions.map(permissionLabel).join('；') }] });
            } else {
                if (!Number.isInteger(seats) || seats! < 1 || seats! > state.maxSeats) throw new Error('岗位席位无效');
                const targets = assignmentRows(state, { termRequired: term, minAssignments: 0, maxAssignments: seats! }, assignments);
                if (!enabled && targets.length) throw new Error('停用岗位必须同案清空任职');
                setDraft({ title: '确认岗位属性及任职变更', changes: { roleMutations: [{ mutation: 'CONFIGURE', roleCode: role.roleCode, seats, termRequired: term, active: enabled }], assignmentChanges: [{ roleCode: role.roleCode, assignments: targets }] }, rows: [
                    { label: '席位', before: String(role.seats), after: String(seats) },
                    { label: '任期', before: role.termRequired ? '要求任期' : '无任期', after: term ? '要求任期' : '无任期' },
                    { label: '状态', before: role.active ? '启用' : '停用', after: enabled ? '启用' : '停用' },
                    { label: '任职', before: assignmentSummary(state, role.termRequired, role.assignments), after: assignmentSummary(state, term, targets) },
                ] });
            }
            setEditing(null);
        } catch (error) { notice.error(error, '岗位校验失败'); }
    };
    return <section>
    <Space style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}><Typography.Title level={4}>岗位管理</Typography.Title><Space><Button onClick={reload}>刷新</Button>{state.policyAvailable && <Button disabled={!writable} onClick={() => { setThreshold(state.governanceThreshold ?? undefined); setThresholdEditing(true); }}>治理阈值</Button>}<Button type="primary" disabled={!writable} onClick={() => { setName(''); setTerm(false); setSeats(undefined); setPermissions([]); setAssignments([]); setStep(0); setCreating(true); }}>新增岗位</Button></Space></Space>
    <Input.Search aria-label="搜索岗位名称" placeholder="搜索岗位名称" value={query} onChange={e => setQuery(e.target.value)} style={{ maxWidth: 360, marginBottom: 16 }}/>
    <Table rowKey="roleCode" pagination={false} dataSource={state.roles.filter(r => r.roleName.includes(query))} columns={[
            { title: '岗位名称', dataIndex: 'roleName' }, { title: '席位容量', dataIndex: 'seats' }, { title: '岗位类型', render: (_, r) => r.fixed ? '固定岗位' : '自定义' },
            { title: '权限摘要', render: (_, r) => r.permissions.map(permissionLabel).join('；') || '无业务权限' },
            { title: '有效任职', render: (_, r) => `${r.assignments.filter(a => a.effective).length} 人` },
            { title: '任期规则', render: (_, r) => r.termRequired ? '有任期' : '无任期' },
            { title: '操作', render: (_, r) => <Space><Button type="link" onClick={() => { setSelected(r); setMode('view'); }}>查看</Button>{writable && state.policyAvailable && !r.fixed && r.roleCode.startsWith('R_') && <><Button type="link" onClick={() => openEdit(r, 'attributes')}>配置</Button><Button type="link" onClick={() => openEdit(r, 'permissions')}>权限</Button></>}{writable && r.active && !r.fixed && <><Button type="link" onClick={() => { setSelected(r); setMode('rename'); setName(r.roleName); }}>改名</Button><Button type="link" danger onClick={() => remove(r)}>删除</Button></>}</Space> },
        ]}/>
    <Alert type="info" message="固定岗位受制度规则保护；普通岗位属性、原生权限和共同阈值按当前链能力开放。"/>
    <Modal cancelText="取消" title="创建岗位" open={creating} onCancel={() => setCreating(false)} width={820} zIndex={CID_MODAL_Z_INDEX.business} footer={<Space><Button onClick={() => setCreating(false)}>取消</Button><Button disabled={step === 0} onClick={() => setStep(step - 1)}>上一步</Button>{step < 2 ? <Button onClick={() => {
                    if (step === 0 && !name.trim()) {
                        notice.error(new Error('请填写岗位名称'));
                        return;
                    }
                    setStep(step + 1);
                }}>下一步</Button> : <Button type="primary" onClick={createDraft}>核对变更</Button>}</Space>}>
      <Steps size="small" current={step} items={[{ title: '基本信息' }, { title: '权限与任期' }, { title: '可选初始任职' }]}/>
      {step === 0 && <Form layout="vertical"><Form.Item label="岗位名称"><Input aria-label="新岗位名称" maxLength={128} value={name} onChange={e => setName(e.target.value)}/></Form.Item><Form.Item label="席位数量"><InputNumber aria-label="新岗位席位数量" min={1} max={state.maxSeats} precision={0} value={seats} onChange={v => setSeats(v ?? undefined)}/></Form.Item><Typography.Text type="secondary">岗位码由链端生成。</Typography.Text></Form>}
      {step === 1 && <Space direction="vertical" style={{ width: '100%', marginTop: 16 }}><Checkbox checked={term} onChange={e => setTerm(e.target.checked)}>要求任期</Checkbox>
        <PermissionEditor permissions={permissions} onChange={setPermissions} moduleTag={state.moduleTag}/>
      </Space>}
      {step === 2 && <AssignmentEditor state={state} termRequired={term} rows={assignments} onChange={setAssignments}/>}
    </Modal>
    <Modal cancelText="取消" open={!!selected} title={mode === 'view' ? '岗位详情' : '岗位改名'} onCancel={() => setSelected(null)} zIndex={CID_MODAL_Z_INDEX.business} onOk={() => {
            if (!selected)
                return;
            if (mode === 'view') {
                setSelected(null);
                return;
            }
            if (!name.trim()) {
                notice.error(new Error('请填写岗位名称'));
                return;
            }
            setDraft({ title: '确认岗位改名提案', changes: { roleMutations: [{ mutation: 'RENAME', roleCode: selected.roleCode, roleName: name.trim() }] }, rows: [{ label: '岗位名称', before: selected.roleName, after: name.trim() }] });
            setSelected(null);
        }} okText={mode === 'view' ? '关闭' : '核对变更'}>
      {mode === 'view' ? <><p>岗位码：{selected?.roleCode}</p><p>岗位名称：{selected?.roleName}</p><p>权限：{selected?.permissions.map(permissionLabel).join('；') || '无'}</p></> : <Input aria-label="新岗位名称" value={name} onChange={e => setName(e.target.value)} maxLength={128}/>}
    </Modal>
    <Modal cancelText="取消" open={!!editing} title={editing?.kind === 'permissions' ? '原生权限政策' : '岗位属性及任职'} onCancel={() => setEditing(null)} onOk={finishEdit} okText="核对变更" width={820} zIndex={CID_MODAL_Z_INDEX.business}>
      {editing?.kind === 'permissions' ? <PermissionEditor permissions={permissions} onChange={setPermissions} moduleTag={state.moduleTag}/> : <Space direction="vertical" style={{ width: '100%' }}>
        <Form.Item label="目标席位"><InputNumber aria-label="目标席位" min={1} max={state.maxSeats} precision={0} value={seats} onChange={value => setSeats(value ?? undefined)}/></Form.Item>
        <Checkbox checked={term} onChange={event => setTerm(event.target.checked)}>要求任期</Checkbox>
        <Checkbox checked={enabled} onChange={event => setEnabled(event.target.checked)}>启用岗位</Checkbox>
        <AssignmentEditor state={state} termRequired={term} rows={assignments} onChange={setAssignments}/>
        <Button danger onClick={() => setAssignments([])}>清空本次目标任职</Button>
      </Space>}
    </Modal>
    <Modal cancelText="取消" title="共同治理阈值" open={thresholdEditing} onCancel={() => setThresholdEditing(false)} zIndex={CID_MODAL_Z_INDEX.business} onOk={() => {
        if (!Number.isInteger(threshold) || threshold! < 2) { notice.error(new Error('共同阈值必须至少为2')); return; }
        setDraft({ title: '确认共同阈值政策', changes: { policy: { permissionChanges: [], threshold, expectedStateHash: state.registrationStateHash } }, rows: [{ label: '共同阈值', before: String(state.governanceThreshold), after: String(threshold) }] });
        setThresholdEditing(false);
    }} okText="核对变更"><InputNumber aria-label="共同阈值" min={2} precision={0} value={threshold} onChange={value => setThreshold(value ?? undefined)}/><p>按现有阈值表决，通过并执行后影响后续提案；新阈值须满足各已配置原生业务的选民约束。</p></Modal>
    <GovernanceReview auth={props.auth} state={state} draft={draft} onClose={() => setDraft(null)} onSubmitted={reload}/>
  </section>;
}

/** 创建与政策替换共用权限编辑器，链上权限由原生规则最终校验。 */
function PermissionEditor({ permissions, onChange, moduleTag }: { permissions: GovernancePermission[]; onChange: (values: GovernancePermission[]) => void; moduleTag: string }) {
    return <Space direction="vertical" style={{ width: '100%' }}>{permissions.map((p, index) => <Space key={index} wrap>
        <Input aria-label={`权限模块${index + 1}`} placeholder="业务模块标签" value={p.moduleTag} onChange={e => onChange(permissions.map((v, i) => i === index ? { ...v, moduleTag: e.target.value } : v))}/>
        <InputNumber aria-label={`业务动作${index + 1}`} min={0} precision={0} value={p.actionCode} onChange={value => onChange(permissions.map((v, i) => i === index ? { ...v, actionCode: value ?? 0 } : v))}/>
        <Select aria-label={`权限操作${index + 1}`} value={p.operation} onChange={value => onChange(permissions.map((v, i) => i === index ? { ...v, operation: value } : v))} options={[{ value: 'PROPOSE', label: '发起' }, { value: 'VOTE', label: '表决' }]}/>
        <Button danger onClick={() => onChange(permissions.filter((_, i) => i !== index))}>移除</Button>
    </Space>)}<Button onClick={() => onChange([...permissions, { moduleTag, actionCode: 3, operation: 'PROPOSE' }])}>添加岗位权限</Button></Space>;
}
