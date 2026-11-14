import { useState } from 'react';
import { Alert, Button, Card, Descriptions, Input, Modal, Select, Space } from 'antd';
import { GovernanceReview, GovernanceStatus, useGovernanceState, type GovernanceDraft, type GovernanceViewProps } from './GovernanceReview';
import { governanceProposers, memberName, representativeChanges } from './governance';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
export function RepresentativeView(props: GovernanceViewProps) {
    // 身份或目标切换时销毁草稿和认证流程，防止旧机构变更沿用到新页面。
    return <RepresentativePanel key={`${props.auth.accessToken}|${props.auth.accountId}|${props.cidNumber}`} {...props}/>;
}
function RepresentativePanel(props: GovernanceViewProps) {
    const { state, error, loading, reload } = useGovernanceState(props);
    const [selecting, setSelecting] = useState(false);
    const [citizenCid, setCitizenCid] = useState('');
    const [account, setAccount] = useState<string>();
    const [draft, setDraft] = useState<GovernanceDraft | null>(null);
    if (!state)
        return <GovernanceStatus loading={loading} error={error} reload={reload}/>;
    const legalRole = state.roles.find(r => r.roleCode === state.legalRoleCode && r.active);
    const writable = props.canWrite !== false && !!legalRole && governanceProposers(props.auth, state).length > 0;
    const current = state.representative;
    const propose = (target: string | null) => {
        try {
            const changes = representativeChanges(state, target, citizenCid);
            const person = state.members.find(m => m.accountId === target);
            setDraft({ title: target ? '确认更换法定代表人' : '确认解除法定代表人', changes, rows: [{ label: '法定代表人', before: current ? `${current.familyName}${current.givenName} · ${current.cidNumber}` : '空缺', after: target ? `${memberName(person)} · ${citizenCid || person?.cidNumber}` : '解除并清空链上记录' }, { label: '专属岗位任职', before: legalRole?.assignments.map(a => memberName(state.members.find(m => m.accountId === a.accountId))).join('；') || '空缺', after: target ? memberName(person) : '空缺' }] });
            setSelecting(false);
        }
        catch (e) {
            notice.error(e, '法定代表人变更校验失败');
        }
    };
    return <Card title="法定代表人" extra={<Space><Button onClick={reload}>刷新</Button><Button type="primary" disabled={!writable} onClick={() => { setAccount(undefined); setCitizenCid(''); setSelecting(true); }}>更换法定代表人</Button></Space>}>
    <Descriptions column={1} bordered><Descriptions.Item label="姓名">{current ? `${current.familyName}${current.givenName}` : '空缺'}</Descriptions.Item><Descriptions.Item label="公民CID">{current?.cidNumber || '-'}</Descriptions.Item><Descriptions.Item label="链上登记账户">{current?.accountId || '-'}</Descriptions.Item></Descriptions>
    {!legalRole && <Alert style={{ marginTop: 16 }} type="info" message="该机构未配置法定代表人岗位，仅展示当前记录"/>}
    <Alert style={{ marginTop: 16 }} type="info" message="本页同步法定代表人记录及其专属岗位任职，不新增或删除管理员。"/>
    <Button style={{ marginTop: 24 }} danger disabled={!writable || !current || !!legalRole?.minAssignments} onClick={() => propose(null)}>发起解除</Button>
    <Modal cancelText="取消" open={selecting} title="选择新的法定代表人" onCancel={() => setSelecting(false)} onOk={() => {
            if (account)
                propose(account);
        }} okButtonProps={{ disabled: !account }} okText="核对变更" zIndex={CID_MODAL_Z_INDEX.business}>
      <Alert type="info" message="选择现有链上管理员并填写其公民CID；后端及链端核对资料与任职是否一致，不在本页新增管理员。"/>
      <Select aria-label="新法定代表人" style={{ width: '100%', marginTop: 16 }} placeholder="选择公民身份" value={account} onChange={value => { setAccount(value); setCitizenCid(state.members.find(m => m.accountId === value)?.cidNumber || ''); }} options={state.members.map(m => ({ value: m.accountId, label: memberName(m) }))}/>
      <Input aria-label="新法定代表人公民CID" placeholder="所选管理员对应的公民CID" value={citizenCid} onChange={e => setCitizenCid(e.target.value)} style={{ marginTop: 12 }}/>
    </Modal>
    <GovernanceReview auth={props.auth} state={state} draft={draft} onClose={() => setDraft(null)} onSubmitted={reload}/>
  </Card>;
}
