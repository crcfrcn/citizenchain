import { useState } from 'react';
import { Alert, Button, Checkbox, Form, Input, InputNumber, Modal, Select, Space, Steps, Table, Typography } from 'antd';
import { GovernanceReview, GovernanceStatus, useGovernanceState, type GovernanceDraft, type GovernanceViewProps } from './GovernanceReview';
import { AssignmentEditor, assignmentRows, type AssignmentRow } from './AssignmentsView';
import { assignmentSummary, governanceProposers, permissionLabel, type GovernancePermission, type GovernanceRole } from './governance';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
export function RolesView(props: GovernanceViewProps) {
    // 身份或目标切换时销毁草稿和认证流程，防止旧机构变更沿用到新页面。
    return <RolesPanel key={`${props.auth.accessToken}|${props.auth.accountId}|${props.cidNumber}`} {...props}/>;
}
function RolesPanel(props: GovernanceViewProps) {
    const { state, error, loading, reload } = useGovernanceState(props);
    const [query, setQuery] = useState('');
    const [creating, setCreating] = useState(false);
    const [step, setStep] = useState(0);
    const [name, setName] = useState('');
    const [term, setTerm] = useState(false);
    const [permissions, setPermissions] = useState<GovernancePermission[]>([]);
    const [assignments, setAssignments] = useState<AssignmentRow[]>([]);
    const [selected, setSelected] = useState<GovernanceRole | null>(null);
    const [mode, setMode] = useState<'view' | 'rename'>('view');
    const [draft, setDraft] = useState<GovernanceDraft | null>(null);
    if (!state)
        return <GovernanceStatus loading={loading} error={error} reload={reload}/>;
    const writable = props.canWrite !== false && governanceProposers(props.auth, state).length > 0;
    const createDraft = () => {
        try {
            if (!name.trim() || !permissions.length)
                throw new Error('请填写岗位名称并添加权限');
            const targets = assignmentRows(state, { termRequired: term, minAssignments: 0, maxAssignments: null }, assignments);
            setDraft({ title: '确认岗位创建提案', changes: { roleMutations: [{ mutation: 'CREATE', roleName: name.trim(), termRequired: term, permissions, assignments: targets }] }, rows: [{ label: '岗位名称', before: '未创建', after: name.trim() }, { label: '岗位权限', before: '无', after: permissions.map(permissionLabel).join('；') }, { label: '任期规则', before: '无', after: term ? '要求任期' : '无任期' }, { label: '初始任职', before: '无', after: assignmentSummary(state, term, targets) }] });
            setCreating(false);
        }
        catch (e) {
            notice.error(e, '岗位校验失败');
        }
    };
    const remove = (role: GovernanceRole) => setDraft({ title: '确认岗位删除提案', changes: { roleMutations: [{ mutation: 'DELETE', roleCode: role.roleCode }] }, rows: [{ label: '岗位名称', before: role.roleName, after: '删除（以链端制度约束为准）' }, { label: '岗位权限', before: role.permissions.map(permissionLabel).join('；'), after: '删除' }, { label: '岗位任职', before: assignmentSummary(state, role.termRequired, role.assignments), after: '清空' }] });
    return <section>
    <Space style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 16 }}><Typography.Title level={4}>岗位管理</Typography.Title><Space><Button onClick={reload}>刷新</Button><Button type="primary" disabled={!writable} onClick={() => { setName(''); setTerm(false); setPermissions([]); setAssignments([]); setStep(0); setCreating(true); }}>新增岗位</Button></Space></Space>
    <Input.Search aria-label="搜索岗位名称" placeholder="搜索岗位名称" value={query} onChange={e => setQuery(e.target.value)} style={{ maxWidth: 360, marginBottom: 16 }}/>
    <Table rowKey="roleCode" pagination={false} dataSource={state.roles.filter(r => r.roleName.includes(query))} columns={[
            { title: '岗位名称', dataIndex: 'roleName' }, { title: '岗位类型', render: (_, r) => r.fixed ? '固定岗位' : '自定义' },
            { title: '权限摘要', render: (_, r) => r.permissions.map(permissionLabel).join('；') || '无业务权限' },
            { title: '有效任职', render: (_, r) => `${r.assignments.filter(a => a.effective).length} 人` },
            { title: '任期规则', render: (_, r) => r.termRequired ? '有任期' : '无任期' },
            { title: '操作', render: (_, r) => <Space><Button type="link" onClick={() => { setSelected(r); setMode('view'); }}>查看</Button>{writable && r.active && !r.fixed && <><Button type="link" onClick={() => { setSelected(r); setMode('rename'); setName(r.roleName); }}>改名</Button><Button type="link" danger onClick={() => remove(r)}>删除</Button></>}</Space> },
        ]}/>
    <Alert type="info" message="固定岗位受制度规则保护；既有岗位权限和任期规则不提供改写入口。"/>
    <Modal cancelText="取消" title="创建岗位" open={creating} onCancel={() => setCreating(false)} width={820} zIndex={CID_MODAL_Z_INDEX.business} footer={<Space><Button onClick={() => setCreating(false)}>取消</Button><Button disabled={step === 0} onClick={() => setStep(step - 1)}>上一步</Button>{step < 2 ? <Button onClick={() => {
                    if (step === 0 && !name.trim()) {
                        notice.error(new Error('请填写岗位名称'));
                        return;
                    }
                    setStep(step + 1);
                }}>下一步</Button> : <Button type="primary" onClick={createDraft}>核对变更</Button>}</Space>}>
      <Steps size="small" current={step} items={[{ title: '基本信息' }, { title: '权限与任期' }, { title: '可选初始任职' }]}/>
      {step === 0 && <Form layout="vertical"><Form.Item label="岗位名称"><Input aria-label="新岗位名称" maxLength={128} value={name} onChange={e => setName(e.target.value)}/></Form.Item><Typography.Text type="secondary">岗位码由链端生成。</Typography.Text></Form>}
      {step === 1 && <Space direction="vertical" style={{ width: '100%', marginTop: 16 }}><Checkbox checked={term} onChange={e => setTerm(e.target.checked)}>要求任期</Checkbox>
        {permissions.map((p, index) => <Space key={index} wrap><Input aria-label={`权限模块${index + 1}`} placeholder="业务模块标签" value={p.moduleTag} onChange={e => setPermissions(permissions.map((v, i) => i === index ? { ...v, moduleTag: e.target.value } : v))}/><InputNumber aria-label={`业务动作${index + 1}`} min={0} precision={0} value={p.actionCode} onChange={v => setPermissions(permissions.map((p, i) => i === index ? { ...p, actionCode: v ?? 0 } : p))}/><Select aria-label={`权限操作${index + 1}`} value={p.operation} onChange={v => setPermissions(permissions.map((p, i) => i === index ? { ...p, operation: v } : p))} options={[{ value: 'PROPOSE', label: '发起' }, { value: 'VOTE', label: '表决' }]}/><Button danger onClick={() => setPermissions(permissions.filter((_, i) => i !== index))}>移除</Button></Space>)}
        <Button onClick={() => setPermissions([...permissions, { moduleTag: state.moduleTag, actionCode: 3, operation: 'PROPOSE' }])}>添加岗位权限</Button><Typography.Text type="secondary">业务权限使用原有模块标识；是否允许授予由链端裁决。</Typography.Text>
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
    <GovernanceReview auth={props.auth} state={state} draft={draft} onClose={() => setDraft(null)} onSubmitted={reload}/>
  </section>;
}
