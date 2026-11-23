// 编辑机构管理员和岗位信息，通过统一流程准备链上登记。

import { InstitutionWalletChangeModal, InstitutionWalletRecoveryPanel } from '@gmb/onchina-platform/institutions/GovernanceReview';
import { IdentityCidText } from '@gmb/onchina-platform/codes/identityCid';
import scanIcon from '../../../icons/scan.svg?url';
import { useEffect, useRef, useState } from 'react';
import { Alert, Button, Card, Checkbox, Descriptions, Form, Input, InputNumber, Modal, Select, Space, Spin, Table, Typography } from 'antd';
import { decodeSs58, tryEncodeSs58 } from '@gmb/onchina-platform/codes/ss58';
import { ScanAccountModal } from '@gmb/onchina-platform/qr/ScanAccountModal';
import { useChainSign } from '@gmb/onchina-platform/qr/useChainSign';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { useAuth } from '@gmb/onchina-platform/authentication/useAuth';
import { ApiError } from '@gmb/onchina-platform/http';
import { notice } from '@gmb/onchina-platform/controls/notice';
import { CID_MODAL_Z_INDEX } from '@gmb/onchina-platform/controls/modalStack';
import { buildInstitutionRegistration, buildInstitutionConfiguration, buildInstitutionAdminGovernance, governanceProposers, institutionWalletRoles, dateFromDay, dayFromDate, getGovernanceState, institutionRegistrationPermissions, permissionLabel,
  type GovernanceState, type InstitutionGovernanceAssignmentTargetInput, type InstitutionGovernanceRoleMutationInput,
  type InstitutionRegistrationInput } from '@gmb/onchina-platform/institutions/governance';
import { runInstitutionRegistration, readRegistrationReceipt, saveRegistrationReceipt,
  type PendingInstitutionRegistration, type InstitutionAdminMode, type RegistryAdminsSharedState } from './cityRegistryAdminsApi';

type Person = { account: string; familyName: string; givenName: string };
type Role = { name: string; term: boolean; permissions: string[]; assignments: InstitutionGovernanceAssignmentTargetInput[] };
type Assignment = { roleCode: string; assignments: InstitutionGovernanceAssignmentTargetInput[] };
const emptyPerson = (): Person => ({ account: '', familyName: '', givenName: '' });

/** 所有机构复用同一表单；市注册局只把当前详情页上下文传入，不维护专用调用或岗位。 */
export function AddCityRegistryAdminModal({ state }: { state: RegistryAdminsSharedState }) {
  const { auth } = useAuth();
  if (!auth) return null;
  return <InstitutionAdminChangeModal auth={auth} cidNumber={state.cityRegistryCid} open={state.addCityRegistryOpen}
    removing={state.deleteCityRegistryAdmin} label="市注册局"
    onClose={() => { state.setAddCityRegistryOpen(false); state.setDeleteCityRegistryAdmin(null); }}
    onSubmitted={async () => { await state.refreshCityRegistryAdmins(); }} />;
}

type RemoveTarget = { id: string; institutionCidNumber: string; rosterAccountId: string; familyName: string; givenName: string };
export function InstitutionAdminChangeModal({ auth, cidNumber: cid, open, removing = null, replacing = null, configuration = false, mode = 'registration', label = '机构', onClose, onSubmitted }: {
  auth: AdminAuth; cidNumber: string | null; open: boolean; removing?: RemoveTarget | null; replacing?: RemoveTarget | null; configuration?: boolean;
  mode?: InstitutionAdminMode; label?: string; onClose: () => void; onSubmitted: () => void | Promise<void>;
}) {
  const [snapshot, setSnapshot] = useState<GovernanceState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [people, setPeople] = useState<Person[]>([emptyPerson()]);
  const [roles, setRoles] = useState<Role[]>([]);
  const [assignments, setAssignments] = useState<Assignment[]>([]);
  const [threshold, setThreshold] = useState<number>();
  const [actorRole, setActorRole] = useState<string>();
  const [review, setReview] = useState<InstitutionRegistrationInput | null>(null);
  const [pending, setPending] = useState<PendingInstitutionRegistration | null>(null);
  const pendingRef = useRef<PendingInstitutionRegistration | null>(null);
  const [busy, setBusy] = useState(false);
  const [closedRecovery, setClosedRecovery] = useState(false);
  const [scanIndex, setScanIndex] = useState<number | null>(null);
  const context = `${auth?.accessToken}|${auth?.accountId}|${auth?.institutionCidNumber}|${cid}|${open}|${mode}|${configuration}|${removing?.rosterAccountId}|${replacing?.rosterAccountId}`;
  const current = useRef(context);
  current.current = context;
  const operation = useRef<AbortController | null>(null);
  const { signChain, chainSignModal } = useChainSign('机构管理员登记签名');
  const reload = () => { setReview(null); setRevision(r => r + 1); };

  useEffect(() => {
    setPeople([emptyPerson()]); setRoles([]); setAssignments([]); setThreshold(undefined); setActorRole(undefined);
    setReview(null); setBusy(false); setClosedRecovery(false); setScanIndex(null);
    const receipt = auth && cid && open ? readRegistrationReceipt(auth, cid, mode) : null;
    pendingRef.current = receipt; setPending(receipt);
    return () => { operation.current?.abort(); };
  }, [context, removing?.id]);

  useEffect(() => {
    setSnapshot(null); setError(null);
    if (!open || !auth || !cid) { setLoading(false); return; }
    let cancelled = false;
    setLoading(true);
    getGovernanceState(auth, cid).then(value => {
      if (cancelled) return;
      if (value.cidNumber !== cid || !value.registrationStateHash || value.governanceThreshold === undefined || !value.registrationRoles) throw new Error('登记链状态未就绪');
      setSnapshot(value);
    }).catch(error => { if (!cancelled) setError(error instanceof Error ? error.message : String(error)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [context, revision]);

  const available = snapshot ? mode === 'governance' ? governanceProposers(auth, snapshot).map(role => ({ roleCode: role.roleCode, roleName: role.roleName })) : snapshot.registrationRoles ?? [] : [];
  const candidates=available.filter(r=>!replacing || !cid?.includes('-FRG') || snapshot?.roles.some(role=>role.roleCode===r.roleCode && role.assignments.some(a=>a.effective && a.accountId===snapshot.members.find(m=>m.accountId===replacing.rosterAccountId)?.authorizationAccountId)));
  const initial = snapshot?.governanceThreshold === null;
  const permissionOptions = snapshot ? institutionRegistrationPermissions(snapshot) : [];
  const personOptions = configuration && snapshot ? snapshot.members.map(m=>({value:m.authorizationAccountId,label:`${m.familyName}${m.givenName}`})) : people.flatMap((person, index) => {
    try { return [{ value: decodeSs58(person.account.trim()), label: `${person.familyName}${person.givenName || '（待填写姓名）'}（第${index + 1}人）` }]; } catch { return []; }
  });
  const remember = (receipt: PendingInstitutionRegistration | null) => {
    pendingRef.current = receipt; setPending(receipt);
    if (auth && cid) saveRegistrationReceipt(auth, cid, receipt, mode);
  };
  const close = () => {
    if (busy) return;
    onClose();
  };
  const buildReview = () => {
    if (!snapshot || !cid || snapshot.cidNumber !== cid) return;
    try {
      if (removing && removing.institutionCidNumber !== cid) throw new Error('删除目标不属于当前机构');
      const mutations: InstitutionGovernanceRoleMutationInput[] = roles.map(role => ({ mutation: 'CREATE', roleName: role.name.trim(), termRequired: role.term,
        permissions: role.permissions.map(key => {
          const option = permissionOptions.find(option => option.value === key);
          if (!option) throw new Error('请选择有效的业务权限');
          return option.permission;
        }), assignments: role.assignments }));
      const draft = replacing ? { replacement: { accountId: replacing.rosterAccountId, newWallet: decodeSs58(people[0].account.trim()) } } : removing ? { removeAccountId: removing.rosterAccountId } : {
        addedAdmins: people.map(person => ({ accountId: decodeSs58(person.account.trim()), familyName: person.familyName, givenName: person.givenName })),
        ...(mode === 'governance' ? initial ? { initialRoles: mutations, initialThreshold: threshold } : { assignments } : {}),
      };
      if (configuration) {setReview(buildInstitutionConfiguration(snapshot,{initialRoles:mutations,initialThreshold:threshold},actorRole));return;}
      setReview(mode === 'governance' ? buildInstitutionAdminGovernance(auth, snapshot, draft, actorRole) : buildInstitutionRegistration(snapshot, draft, actorRole));
    } catch (error) { notice.error(error, '登记校验失败'); }
  };

  const submit = async () => {
    if (!auth || !cid || busy || (operation.current && !operation.current.signal.aborted) || (!review && !pendingRef.current)) return;
    const owner = context;
    const controller = new AbortController(); operation.current = controller;
    const ensureCurrent = () => { controller.signal.throwIfAborted(); if (current.current !== owner) throw new Error('办理身份或页面目标已变化'); };
    setBusy(true);
    try {
      const result = await runInstitutionRegistration({ auth, cidNumber: cid, input: review,
        pending: pendingRef.current, sign: signChain, remember, check: ensureCurrent, signal: controller.signal, mode });
      ensureCurrent();
      setReview(null);
      notice.success(result.recovered ? '链上管理员变更结果已恢复核对' : mode === 'governance' ? '治理提案已确认上链，投票通过并执行后名册生效' : configuration ? '首次治理配置已在链上确认' : '管理员登记已在链上确认');
      onClose();
      await onSubmitted();
    } catch (error) {
      if (controller.signal.aborted || current.current !== owner) return;
      if (!pendingRef.current) reload();
      if (pendingRef.current && error instanceof ApiError && (error.status === 404 || error.status === 409 && error.message.includes('已被消费'))) setClosedRecovery(true);
      notice.error(error, pendingRef.current ? '结果未确认，保留原签名供恢复' : '登记未完成');
    } finally { if (operation.current === controller) { operation.current = null; if (current.current === owner) setBusy(false); } }
  };

  // 原会话已清理/消费后若又有别的事务改变名册，允许显式重读；绝不自动重签旧变更。
  const recheckClosedRecovery = async () => {
    if (!auth || !cid || busy) return;
    const owner = context; setBusy(true);
    try {
      const fresh = await getGovernanceState(auth, cid);
      if (current.current !== owner || fresh.cidNumber !== cid) return;
      remember(null); setClosedRecovery(false); setReview(null); setSnapshot(fresh);
      setPeople([emptyPerson()]); setRoles([]); setAssignments([]); setThreshold(undefined); setActorRole(undefined);
      onClose(); await onSubmitted();
    } catch (error) { if (current.current === owner) notice.error(error, '读取当前链上名册失败'); }
    finally { if (current.current === owner) setBusy(false); }
  };

  const renderAssignments = (rows: InstitutionGovernanceAssignmentTargetInput[], update: (rows: InstitutionGovernanceAssignmentTargetInput[]) => void, term: boolean) => <Space direction="vertical" style={{ width: '100%' }}>
    {rows.map((row, index) => <Space key={index} wrap>
      <Select aria-label={`任职人员${index + 1}`} placeholder="选择新增名册人员" style={{ minWidth: 180 }} value={row.accountId || undefined} options={personOptions}
        onChange={accountId => update(rows.map((r, i) => i === index ? { ...r, accountId } : r))} />
      {term && <><Input aria-label={`任期开始${index + 1}`} type="date" value={dateFromDay(row.termStart)} onChange={e => { try { update(rows.map((r, i) => i === index ? { ...r, termStart: e.target.value ? dayFromDate(e.target.value) : 0 } : r)); } catch (error) { notice.error(error); } }} />
      <Input aria-label={`任期结束${index + 1}`} type="date" value={dateFromDay(row.termEnd)} onChange={e => { try { update(rows.map((r, i) => i === index ? { ...r, termEnd: e.target.value ? dayFromDate(e.target.value) : 0 } : r)); } catch (error) { notice.error(error); } }} /></>}
      <Button danger onClick={() => update(rows.filter((_, i) => i !== index))}>移除此任职</Button>
    </Space>)}
    <Button onClick={() => update([...rows, { accountId: '', termStart: 0, termEnd: 0 }])}>添加任职人员</Button>
  </Space>;

  return <>
    <Modal title={pending ? '恢复机构管理员变更' : `${mode === 'governance' ? '提案：' : ''}${configuration ? '首次治理配置：' : replacing ? '更换钱包：' : removing ? '删除' : '新增'}${label}管理员`} open={open} width={900} destroyOnClose
      onCancel={close} closable={!busy} maskClosable={!busy} zIndex={CID_MODAL_Z_INDEX.business}
      footer={<Space><Button disabled={busy} onClick={close}>关闭</Button>
        {pending ? <>{closedRecovery && <Button disabled={busy} onClick={() => void recheckClosedRecovery()}>重新核对当前名册</Button>}<Button type="primary" loading={busy} onClick={() => void submit()}>恢复原会话结果</Button></> : review ? <>
          <Button disabled={busy} onClick={() => setReview(null)}>返回修改</Button><Button type="primary" loading={busy} onClick={() => void submit()}>{mode === 'governance' ? '确认并签名提交治理提案' : '确认并签名上链'}</Button>
        </> : <Button type="primary" disabled={!snapshot || loading || !!error || !candidates.length} onClick={buildReview}>核对登记</Button>}
      </Space>}>
      <Typography.Paragraph>目标机构身份CID号：<IdentityCidText value={cid ?? '当前详情页目标未就绪'} /></Typography.Paragraph>
      {mode === 'governance' && <Alert type="info" message="本机构治理提案需投票通过并执行后生效" />}
      {pending ? <Alert type="info" showIcon message="已有签名会话待恢复" description={closedRecovery ? '原会话已清理或消费，但当前链状态不再完全吻合。可以重新核对当前名册；这不会撤销已签交易，也不会自动重签旧变更。' : '恢复使用原请求和原签名，不重新准备或重复签名。'} /> : <>
        {loading && <Spin tip="读取链上名册及有效岗位"><div style={{ minHeight: 60 }} /></Spin>}
        {error && <Alert type="error" message="登记链状态读取失败" description={error} action={<Button onClick={reload}>重试</Button>} />}
        {snapshot && !loading && <>
          {candidates.length === 1 ? <Typography.Paragraph>办理岗位：{candidates[0].roleName}</Typography.Paragraph> : <Form.Item label="办理岗位">
            <Select aria-label="办理岗位" placeholder="请选择办理岗位名称" value={actorRole} disabled={!!review} onChange={setActorRole} options={candidates.map(r => ({ value: r.roleCode, label: r.roleName }))} /></Form.Item>}
          {!candidates.length && <Alert type="warning" message="当前管理员没有有效的登记办理岗位" />}
          {review ? <>
            <Descriptions column={1} bordered size="small">
              {review.admins && <Descriptions.Item label="登记名册人数">{review.admins.length}</Descriptions.Item>}
              <Descriptions.Item label="机构阈值">{review.initialThreshold ?? '沿用现有阈值'}</Descriptions.Item>
              <Descriptions.Item label="登记类型">{configuration ? '首次治理配置，钱包名单不变' : mode === 'registration' ? '仅维护管理员钱包，岗位人员和阈值不变' : removing ? '本机构治理删除成员' : '本机构治理新增成员'}</Descriptions.Item>
            </Descriptions>
            <Table rowKey="accountId" pagination={false} dataSource={review.admins} columns={[
              { title: '名册人员', render: (_, m) => { const old = snapshot.members.find(p => p.accountId === m.accountId); return m.familyName ? `${m.familyName}${m.givenName ?? ''}` : old ? `${old.familyName}${old.givenName}` : '资料待完善'; } },
              { title: '名册账户', render: (_, m) => tryEncodeSs58(m.accountId) },
            ]} />
            {(review.roleMutations ?? []).map((role, index) => <Typography.Paragraph key={index}>创建岗位：{role.roleName}；权限：{role.permissions?.map(permissionLabel).join('、')}；任职：{role.assignments?.map(a => `${personOptions.find(p => p.value === a.accountId)?.label ?? tryEncodeSs58(a.accountId)} ${role.termRequired ? `${dateFromDay(a.termStart)}至${dateFromDay(a.termEnd)}` : '无任期'}`).join('、')}</Typography.Paragraph>)}
            {(review.assignmentChanges ?? []).map(change => <Typography.Paragraph key={change.roleCode}>岗位：{snapshot.roles.find(r => r.roleCode === change.roleCode)?.roleName}；目标任职：{change.assignments.map(a => `${personOptions.find(p => p.value === a.accountId)?.label ?? snapshot.members.find(p => p.authorizationAccountId === a.accountId)?.familyName ?? tryEncodeSs58(a.accountId)} ${a.termStart ? `${dateFromDay(a.termStart)}至${dateFromDay(a.termEnd)}` : '无任期'}`).join('、') || '空缺'}</Typography.Paragraph>)}
          </> : replacing ? <><Typography.Paragraph>原钱包：{tryEncodeSs58(replacing.rosterAccountId)}；保留人员、岗位及权限记录。</Typography.Paragraph><Input aria-label="新管理员钱包" placeholder="新钱包账户（SS58）" value={people[0].account} onChange={e => setPeople([{ ...people[0], account: e.target.value }])}/></> : removing ? <Typography.Paragraph>删除钱包：{removing.familyName}{removing.givenName}。岗位人员记录保持原样，钱包退出当前管理员名单。</Typography.Paragraph> : <>
            {!configuration && <>{people.map((person, index) => <Space key={index} wrap align="start" style={{ marginBottom: 12 }}>
              <Typography.Text>新增人员{index + 1}</Typography.Text>
              <Input aria-label={`管理员姓${index + 1}`} placeholder="姓" value={person.familyName} onChange={e => setPeople(people.map((p, i) => i === index ? { ...p, familyName: e.target.value } : p))} />
              <Input aria-label={`管理员名${index + 1}`} placeholder="名" value={person.givenName} onChange={e => setPeople(people.map((p, i) => i === index ? { ...p, givenName: e.target.value } : p))} />
              <Input aria-label={`管理员账户${index + 1}`} placeholder="管理员账户（SS58）" value={person.account} style={{ width: 390 }} onChange={e => setPeople(people.map((p, i) => i === index ? { ...p, account: e.target.value } : p))}
                suffix={<Button type="text" size="small" aria-label={`扫描管理员账户${index + 1}`} onClick={() => setScanIndex(index)}><img src={scanIcon} width={18} alt="" /></Button>} />
              <Button danger disabled={people.length === 1} onClick={() => setPeople(people.filter((_, i) => i !== index))}>移除此人员</Button>
            </Space>)}
            <Button onClick={() => setPeople([...people, emptyPerson()])}>添加名册人员</Button></>}
            {(mode === 'governance' || configuration) && (initial ? <>
              <Alert style={{ marginTop: 16 }} type="info" message="首次登记须明确岗位权限、任职和机构阈值" description="治理票数按有效岗位席位计算，同一人员可兼任不同岗位；岗位名称和权限由办理人明确配置。" />
              {roles.map((role, index) => <div key={index} style={{ marginTop: 16, border: '1px solid #ddd', padding: 12 }}>
                <Form.Item label="岗位名称"><Input aria-label={`新岗位名称${index + 1}`} value={role.name} onChange={e => setRoles(roles.map((r, i) => i === index ? { ...r, name: e.target.value } : r))} /></Form.Item>
                <Form.Item label="业务权限"><Select aria-label={`业务权限${index + 1}`} mode="multiple" style={{ width: '100%' }} value={role.permissions} options={permissionOptions} optionFilterProp="label" onChange={permissions => setRoles(roles.map((r, i) => i === index ? { ...r, permissions } : r))} /></Form.Item>
                <Checkbox checked={role.term} onChange={e => setRoles(roles.map((r, i) => i === index ? { ...r, term: e.target.checked, assignments: [] } : r))}>岗位要求任期</Checkbox>
                {renderAssignments(role.assignments, assignments => setRoles(roles.map((r, i) => i === index ? { ...r, assignments } : r)), role.term)}
                <Button danger onClick={() => setRoles(roles.filter((_, i) => i !== index))}>移除此新岗位</Button>
              </div>)}
              <Button style={{ marginTop: 12 }} onClick={() => setRoles([...roles, { name: '', term: false, permissions: [], assignments: [] }])}>添加动态岗位</Button>
              <Form.Item label="初始治理阈值"><InputNumber aria-label="初始治理阈值" min={2} precision={0} value={threshold} onChange={value => setThreshold(value ?? undefined)} /></Form.Item>
            </> : <>
              <Typography.Paragraph style={{ marginTop: 16 }}>为新增人员选择现有岗位并明确任职；保留原成员任职及来源。</Typography.Paragraph>
              {assignments.map((change, index) => {
                const role = snapshot.roles.find(r => r.roleCode === change.roleCode);
                return <div key={index} style={{ marginBottom: 12 }}><Select aria-label={`任职岗位${index + 1}`} placeholder="选择岗位名称" style={{ minWidth: 220 }} value={change.roleCode || undefined}
                  options={snapshot.roles.filter(r => r.active && !r.assignmentLocked).map(r => ({ value: r.roleCode, label: r.roleName }))}
                  onChange={roleCode => setAssignments(assignments.map((a, i) => i === index ? { roleCode, assignments: [] } : a))} />
                  {role && <Typography.Paragraph>岗位权限：{role.permissions.map(permissionLabel).join('、') || '无业务权限'}</Typography.Paragraph>}
                  {renderAssignments(change.assignments, targets => setAssignments(assignments.map((a, i) => i === index ? { ...a, assignments: targets } : a)), !!role?.termRequired)}
                  <Button danger onClick={() => setAssignments(assignments.filter((_, i) => i !== index))}>移除此岗位任职</Button></div>;
              })}
              <Button onClick={() => setAssignments([...assignments, { roleCode: '', assignments: [] }])}>选择任职岗位</Button>
            </>)}
          </>}
        </>}
      </>}
    </Modal>
    <ScanAccountModal open={scanIndex !== null} onClose={() => setScanIndex(null)} onResolved={account => { setPeople(people.map((p, i) => i === scanIndex ? { ...p, account } : p)); setScanIndex(null); }} />
    {chainSignModal}
  </>;
}


/** 公权/私权详情共用管理员入口；登记权限与本机构提案权限分别来自真实候选。 */
export function InstitutionAdministratorsPanel({ auth, cidNumber, canWrite, onSubmitted }: {
  auth: AdminAuth; cidNumber: string; canWrite: boolean; onSubmitted: () => void;
}) {
  const [snapshot, setSnapshot] = useState<GovernanceState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [walletTarget, setWalletTarget] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [draft, setDraft] = useState<{ mode: InstitutionAdminMode; removing?: RemoveTarget; replacing?: RemoveTarget; configuration?: boolean } | null>(null);
  useEffect(() => {
    let cancelled = false; setSnapshot(null); setError(null); setDraft(null); setWalletTarget(null);
    getGovernanceState(auth, cidNumber).then(state => { if (!cancelled) setSnapshot(state); })
      .catch(error => { if (!cancelled) setError(error instanceof Error ? error.message : String(error)); });
    return () => { cancelled = true; };
  }, [auth.accessToken, auth.accountId, auth.institutionCidNumber, cidNumber, revision]);
  const canPropose = canWrite && !!snapshot && governanceProposers(auth, snapshot).length > 0;
  const canRegister = canWrite && !!snapshot?.registrationRoles?.length;
  const fixed=/^[A-Z0-9]{5}-(FRG|NRC|PRC|PRB|NJD)/.test(cidNumber);
  return <Card title="机构治理">
    {auth.institutionCidNumber === cidNumber && <InstitutionWalletRecoveryPanel auth={auth} mode="GOVERNANCE" onSubmitted={() => { setRevision(r => r + 1); onSubmitted(); }}/>}
    <Typography.Paragraph>管理员集合</Typography.Paragraph>
    {error && <Alert type="error" message="管理员链状态读取失败" description={error} action={<Button onClick={() => setRevision(r => r + 1)}>重试</Button>} />}
    {!snapshot && !error && <Spin tip="读取链上名册"><div style={{ minHeight: 40 }} /></Spin>}
    <Space wrap><Typography.Text type="secondary">岗位与任职请使用对应治理页面</Typography.Text>
      <Button disabled={!canRegister || fixed} onClick={() => setDraft({ mode: 'registration' })}>注册局直接登记管理员</Button>
      <Button disabled={!canRegister || snapshot?.governanceThreshold!==null || !snapshot.members.length} onClick={()=>setDraft({mode:'registration',configuration:true})}>首次治理配置</Button></Space>
    <Table rowKey="accountId" pagination={false} dataSource={snapshot?.members ?? []} columns={[
      { title: '姓名', render: (_, member) => `${member.familyName}${member.givenName}` || '资料待完善' },
      { title: '绑定钱包账户', render: (_, member) => tryEncodeSs58(member.signerId) },
      { title: '操作', render: (_, member) => <Space>
        {member.signerId.toLowerCase() !== auth.accountId.toLowerCase() && <Button disabled={!canWrite || !snapshot || !institutionWalletRoles(auth, snapshot, member.accountId).length} onClick={() => setWalletTarget(member.accountId)}>提案更换钱包</Button>}
        <Button disabled={!canRegister || cidNumber.includes('-FRG') && !snapshot?.registrationRoles?.some(r=>snapshot.roles.some(role=>role.roleCode===r.roleCode && role.assignments.some(a=>a.effective && a.accountId===member.authorizationAccountId)))} onClick={() => setDraft({mode:'registration',replacing:{id:member.accountId,institutionCidNumber:cidNumber,rosterAccountId:member.accountId,familyName:member.familyName,givenName:member.givenName}})}>登记更换钱包</Button>
        <Button danger disabled={!canRegister || fixed} onClick={() => setDraft({ mode: 'registration', removing: { id: member.accountId, institutionCidNumber: cidNumber, rosterAccountId: member.accountId, familyName: member.familyName, givenName: member.givenName } })}>登记删除</Button>
      </Space> },
    ]} />
    {walletTarget && <InstitutionWalletChangeModal key={`${auth.accessToken}|${cidNumber}|${walletTarget}`} auth={auth} cidNumber={cidNumber} mode="GOVERNANCE" targetAccount={walletTarget}
      onClose={() => setWalletTarget(null)} onSubmitted={() => { setRevision(r => r + 1); onSubmitted(); }}/>}
    <InstitutionAdminChangeModal key={`${auth.accessToken}|${auth.accountId}|${cidNumber}|${draft?.mode ?? ''}`} auth={auth} cidNumber={cidNumber} open={!!draft}
      mode={draft?.mode} removing={draft?.removing} replacing={draft?.replacing} configuration={draft?.configuration} onClose={() => setDraft(null)} onSubmitted={() => { setRevision(r => r + 1); onSubmitted(); }} />
  </Card>;
}
