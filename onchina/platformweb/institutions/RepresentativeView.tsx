// 法代始终对应唯一LR岗位，以人员CID任免，不编辑独立机构钱包。
import { useState } from 'react';
import { Alert, Button, Card, Descriptions, Input, Modal, Space } from 'antd';
import { IdentityCidInput } from '../controls/IdentityCidInput';
import { IdentityCidText } from '../codes/identityCid';
import { GovernanceReview, GovernanceStatus, useGovernanceState, type GovernanceDraft, type GovernanceViewProps } from './GovernanceReview';
import { governanceProposers, representativeChanges } from './governance';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
export function RepresentativeView(props: GovernanceViewProps) { return <RepresentativePanel key={`${props.auth.accessToken}|${props.auth.personCid}|${props.auth.bindingRevision}|${props.auth.accountId}|${props.auth.institutionCidNumber}|${props.auth.roleCode}|${props.cidNumber}`} {...props}/>; }
function RepresentativePanel(props: GovernanceViewProps) {
  const { state, error, loading, reload } = useGovernanceState(props);
  const [open, setOpen] = useState(false), [personCid, setPersonCid] = useState(''), [familyName, setFamilyName] = useState(''), [givenName, setGivenName] = useState('');
  const [draft, setDraft] = useState<GovernanceDraft | null>(null);
  if (!state) return <GovernanceStatus loading={loading} error={error} reload={reload}/>;
  const role = state.roles.find(r => r.roleCode === state.legalRoleCode && r.active);
  const writable = props.canWrite !== false && role?.seats === 1 && governanceProposers(props.auth, state).length > 0;
  const current = state.representative;
  const propose = (person: string | null) => {
    try {
      const changes = representativeChanges(state, person, familyName, givenName);
      setDraft({ title: person ? '确认更换法定代表人' : '确认解除法定代表人', changes, rows: [
        { label: '法定代表人', before: current ? `${current.familyName}${current.givenName} · ${current.personCid}` : '空缺', after: person ? `${familyName}${givenName} · ${person}` : '解除并清空链上记录' },
        { label: '唯一LR任职', before: role?.assignments.map(a => a.personCid).join('；') || '空缺', after: person || '空缺' },
      ] }); setOpen(false);
    } catch (failure) { notice.error(failure, '法定代表人变更校验失败'); }
  };
  return <Card title="法定代表人" extra={<Space><Button onClick={reload}>刷新</Button><Button type="primary" disabled={!writable} onClick={() => { setPersonCid(''); setFamilyName(''); setGivenName(''); setOpen(true); }}>更换法定代表人</Button></Space>}>
    <Descriptions column={1} bordered><Descriptions.Item label="姓名">{current ? `${current.familyName}${current.givenName}` : '空缺'}</Descriptions.Item><Descriptions.Item label="人员CID"><IdentityCidText value={current?.personCid || '-'} /></Descriptions.Item><Descriptions.Item label="席位容量">{role?.seats ?? '-'}</Descriptions.Item></Descriptions>
    <Alert type="info" message="每个机构固定一个法定代表人席位，资料与LR任职原子同步。签名钱包由identity当前绑定确定。"/>
    <Button danger style={{ marginTop: 24 }} disabled={!writable || !current || !!role?.minAssignments} onClick={() => propose(null)}>发起解除</Button>
    <Modal title="选择新的法定代表人" cancelText="取消" open={open} onCancel={() => setOpen(false)} onOk={() => propose(personCid)} okText="核对变更" zIndex={CID_MODAL_Z_INDEX.business}>
      <IdentityCidInput aria-label="新法定代表人身份CID号" placeholder="完整人员CID" value={personCid} onChange={e => setPersonCid(e.target.value)}/>
      <Input aria-label="法定代表人姓" placeholder="姓" value={familyName} onChange={e => setFamilyName(e.target.value)} style={{ marginTop: 12 }}/>
      <Input aria-label="法定代表人名" placeholder="名" value={givenName} onChange={e => setGivenName(e.target.value)} style={{ marginTop: 12 }}/>
    </Modal>
    <GovernanceReview auth={props.auth} state={state} draft={draft} onClose={() => setDraft(null)} onSubmitted={reload}/>
  </Card>;
}
