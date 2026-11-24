// 首次岗位治理配置；不提供管理员钱包维护或名单编辑。

import { IdentityCidInput } from '@gmb/onchina-platform/controls/IdentityCidInput';
import { IdentityCidText } from '@gmb/onchina-platform/codes/identityCid';
import { useEffect, useRef, useState } from 'react';
import { Alert, Button, Card, Checkbox, Descriptions, Form, Input, InputNumber, Modal, Select, Space, Spin, Typography } from 'antd';
import { useChainSign } from '@gmb/onchina-platform/qr/useChainSign';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { ApiError } from '@gmb/onchina-platform/http';
import { notice } from '@gmb/onchina-platform/controls/notice';
import { CID_MODAL_Z_INDEX } from '@gmb/onchina-platform/controls/modalStack';
import { buildInstitutionConfiguration, dateFromDay, dayFromDate, getGovernanceState, institutionRegistrationPermissions, permissionLabel,
  type GovernanceState, type InstitutionGovernanceAssignmentTargetInput, type InstitutionGovernanceRoleMutationInput,
  type InstitutionRegistrationInput } from '@gmb/onchina-platform/institutions/governance';
import { runInstitutionRegistration, readRegistrationReceipt, saveRegistrationReceipt, type PendingInstitutionRegistration } from './registration';

type Role = { name: string; term: boolean; seats?:number; permissions: string[]; assignments: InstitutionGovernanceAssignmentTargetInput[] };

/** 沿用现有正常岗位配置合同；人员CID和席位字段在Runtime落地后统一接入。 */
export function InstitutionConfigurationModal({ auth, cidNumber, open, onClose, onSubmitted }: {
  auth: AdminAuth; cidNumber: string; open: boolean; onClose: () => void; onSubmitted: () => void | Promise<void>;
}) {
  const [snapshot, setSnapshot] = useState<GovernanceState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [roles, setRoles] = useState<Role[]>([]);
  const [threshold, setThreshold] = useState<number>();
  const [actorRole, setActorRole] = useState<string>();
  const [review, setReview] = useState<InstitutionRegistrationInput | null>(null);
  const [pending, setPending] = useState<PendingInstitutionRegistration | null>(null);
  const pendingRef = useRef<PendingInstitutionRegistration | null>(null);
  const [busy, setBusy] = useState(false);
  const [closedRecovery, setClosedRecovery] = useState(false);
  const context = `${auth.accessToken}|${auth.personCid}|${auth.bindingRevision}|${auth.accountId}|${auth.institutionCidNumber}|${auth.roleCode}|${cidNumber}|${open}`;
  const current = useRef(context); current.current = context;
  const operation = useRef<AbortController | null>(null);
  const { signChain, chainSignModal } = useChainSign('首次岗位治理配置签名');
  const reload = () => { setReview(null); setRevision(r => r + 1); };

  useEffect(() => {
    setRoles([]); setThreshold(undefined); setActorRole(undefined); setReview(null); setBusy(false); setClosedRecovery(false);
    const receipt = open ? readRegistrationReceipt(auth, cidNumber) : null;
    pendingRef.current = receipt; setPending(receipt);
    return () => { operation.current?.abort(); };
  }, [context]);
  useEffect(() => {
    setSnapshot(null); setError(null);
    if (!open) { setLoading(false); return; }
    let cancelled = false; setLoading(true);
    getGovernanceState(auth, cidNumber).then(value => {
      if (cancelled) return;
      if (value.cidNumber !== cidNumber || !value.registrationStateHash || value.governanceThreshold === undefined || !value.registrationRoles) throw new Error('岗位配置链状态未就绪');
      setSnapshot(value);
    }).catch(failure => { if (!cancelled) setError(failure instanceof Error ? failure.message : String(failure)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [context, revision]);

  const candidates = snapshot?.registrationRoles ?? [];
  const permissionOptions = snapshot ? institutionRegistrationPermissions(snapshot) : [];
  const remember = (receipt: PendingInstitutionRegistration | null) => {
    pendingRef.current = receipt; setPending(receipt); saveRegistrationReceipt(auth, cidNumber, receipt);
  };
  const buildReview = () => {
    if (!snapshot || snapshot.cidNumber !== cidNumber) return;
    try {
      const mutations: InstitutionGovernanceRoleMutationInput[] = roles.map(role => ({ mutation: 'CREATE', roleName: role.name.trim(), termRequired: role.term, seats:role.seats,
        permissions: role.permissions.map(key => {
          const option = permissionOptions.find(option => option.value === key);
          if (!option) throw new Error('请选择有效的业务权限');
          return option.permission;
        }), assignments: role.assignments }));
      setReview(buildInstitutionConfiguration(snapshot, { initialRoles: mutations, initialThreshold: threshold }, actorRole ?? auth.roleCode));
    } catch (failure) { notice.error(failure, '岗位配置校验失败'); }
  };
  const submit = async () => {
    if (busy || (operation.current && !operation.current.signal.aborted) || (!review && !pendingRef.current)) return;
    const owner = context; const controller = new AbortController(); operation.current = controller;
    const check = () => { controller.signal.throwIfAborted(); if (current.current !== owner) throw new Error('办理身份或页面目标已变化'); };
    setBusy(true);
    try {
      const result = await runInstitutionRegistration({ auth, cidNumber, input: review, pending: pendingRef.current, sign: signChain, remember, check, signal: controller.signal });
      check(); setReview(null); notice.success(result.recovered ? '岗位配置结果已恢复核对' : '首次岗位治理配置已在链上确认');
      onClose(); await onSubmitted();
    } catch (failure) {
      if (controller.signal.aborted || current.current !== owner) return;
      if (!pendingRef.current) reload();
      if (pendingRef.current && failure instanceof ApiError && (failure.status === 404 || failure.status === 409 && failure.message.includes('已被消费'))) setClosedRecovery(true);
      notice.error(failure, pendingRef.current ? '结果未确认，保留原签名供恢复' : '岗位配置未完成');
    } finally { if (operation.current === controller) { operation.current = null; if (current.current === owner) setBusy(false); } }
  };
  // 已签的正常岗位配置只恢复原会话；页面切换或冲突不会自动重签。
  const recheck = async () => {
    if (busy) return;
    const owner = context; setBusy(true);
    try {
      const fresh = await getGovernanceState(auth, cidNumber);
      if (current.current !== owner || fresh.cidNumber !== cidNumber) return;
      remember(null); setClosedRecovery(false); setReview(null); setSnapshot(fresh);
      setRoles([]); setThreshold(undefined); setActorRole(undefined); onClose(); await onSubmitted();
    } catch (failure) { if (current.current === owner) notice.error(failure, '读取当前岗位配置失败'); }
    finally { if (current.current === owner) setBusy(false); }
  };
  const renderAssignments = (role: Role, roleIndex: number) => <Space direction="vertical" style={{ width: '100%' }}>
    {role.assignments.map((row, index) => {
      const update = (patch: Partial<InstitutionGovernanceAssignmentTargetInput>) => setRoles(roles.map((r, i) => i === roleIndex ? { ...r, assignments: r.assignments.map((a, n) => n === index ? { ...a, ...patch } : a) } : r));
      return <Space key={index} wrap>
        <IdentityCidInput aria-label={`任职人员${index + 1}`} placeholder="完整人员CID" style={{ minWidth: 320 }} value={row.personCid} onChange={e => update({personCid:e.target.value})}/>

        {role.term && <><Input aria-label={`任期开始${index + 1}`} type="date" value={dateFromDay(row.termStart)} onChange={e => { try { update({ termStart: e.target.value ? dayFromDate(e.target.value) : 0 }); } catch (failure) { notice.error(failure); } }}/>
          <Input aria-label={`任期结束${index + 1}`} type="date" value={dateFromDay(row.termEnd)} onChange={e => { try { update({ termEnd: e.target.value ? dayFromDate(e.target.value) : 0 }); } catch (failure) { notice.error(failure); } }}/></>}
        <Button danger onClick={() => setRoles(roles.map((r, i) => i === roleIndex ? { ...r, assignments: r.assignments.filter((_, n) => n !== index) } : r))}>移除此任职</Button>
      </Space>;
    })}
    <Button onClick={() => setRoles(roles.map((r, i) => i === roleIndex ? { ...r, assignments: [...r.assignments, { personCid: '', termStart: 0, termEnd: 0 }] } : r))}>添加任职人员</Button>
  </Space>;
  return <>
    <Modal title={pending ? '恢复首次岗位治理配置' : '首次岗位治理配置'} open={open} width={900} destroyOnClose
      onCancel={() => { if (!busy) onClose(); }} closable={!busy} maskClosable={!busy} zIndex={CID_MODAL_Z_INDEX.business}
      footer={<Space><Button disabled={busy} onClick={onClose}>关闭</Button>
        {pending ? <>{closedRecovery && <Button disabled={busy} onClick={() => void recheck()}>重新核对当前岗位</Button>}<Button type="primary" loading={busy} onClick={() => void submit()}>恢复原会话结果</Button></> : review ? <>
          <Button disabled={busy} onClick={() => setReview(null)}>返回修改</Button><Button type="primary" loading={busy} onClick={() => void submit()}>确认并签名上链</Button>
        </> : <Button type="primary" disabled={!snapshot || loading || !!error || !candidates.length} onClick={buildReview}>核对岗位配置</Button>}
      </Space>}>
      <Typography.Paragraph>目标机构身份CID号：<IdentityCidText value={cidNumber}/></Typography.Paragraph>
      {pending ? <Alert type="info" showIcon message="已有岗位配置签名会话待恢复" description="恢复使用原请求和原签名，不重新准备或重复签名。"/> : <>
        {loading && <Spin tip="读取岗位配置"><div style={{ minHeight: 60 }}/></Spin>}
        {error && <Alert type="error" message="岗位配置读取失败" description={error} action={<Button onClick={reload}>重试</Button>}/>}
        {snapshot && !loading && <>
          {candidates.length === 1 ? <Typography.Paragraph>办理岗位：{candidates[0].roleName}</Typography.Paragraph> : <Form.Item label="办理岗位">
            <Select aria-label="办理岗位" placeholder="请选择办理岗位名称" value={actorRole} disabled={!!review} onChange={setActorRole} options={candidates.map(r => ({ value: r.roleCode, label: r.roleName }))}/></Form.Item>}
          {!candidates.length && <Alert type="warning" message="当前身份没有有效的登记办理岗位"/>}
          {review ? <><Descriptions column={1} bordered size="small"><Descriptions.Item label="机构阈值">{review.initialThreshold}</Descriptions.Item></Descriptions>
            {(review.roleMutations ?? []).map((role, index) => <Typography.Paragraph key={index}>创建岗位：{role.roleName}；席位：{role.seats}；权限：{role.permissions?.map(permissionLabel).join('、')}；任职：{role.assignments?.map(a => `${a.personCid} ${role.termRequired ? `${dateFromDay(a.termStart)}至${dateFromDay(a.termEnd)}` : '无任期'}`).join('、')}</Typography.Paragraph>)}</> : <>
            <Alert type="info" message="首次配置须明确岗位权限、任职和机构阈值"/>
            {roles.map((role, index) => <div key={index} style={{ marginTop: 16 }}>
              <Form.Item label="岗位名称"><Input aria-label={`岗位名称${index + 1}`} value={role.name} onChange={e => setRoles(roles.map((r, i) => i === index ? { ...r, name: e.target.value } : r))}/></Form.Item>
              <Form.Item label="席位数量"><InputNumber aria-label={`岗位席位${index + 1}`} min={1} max={snapshot.maxSeats} precision={0} value={role.seats} onChange={value => setRoles(roles.map((r,i) => i === index ? {...r,seats:value ?? undefined} : r))}/></Form.Item>
              <Form.Item label="业务权限"><Select aria-label={`业务权限${index + 1}`} mode="multiple" style={{ width: '100%' }} value={role.permissions} options={permissionOptions} optionFilterProp="label" onChange={permissions => setRoles(roles.map((r, i) => i === index ? { ...r, permissions } : r))}/></Form.Item>
              <Checkbox checked={role.term} onChange={e => setRoles(roles.map((r, i) => i === index ? { ...r, term: e.target.checked, assignments: [] } : r))}>岗位要求任期</Checkbox>
              {renderAssignments(role, index)}<Button danger onClick={() => setRoles(roles.filter((_, i) => i !== index))}>移除此新岗位</Button>
            </div>)}
            <Button style={{ marginTop: 12 }} onClick={() => setRoles([...roles, { name: '', term: false, permissions: [], assignments: [] }])}>添加动态岗位</Button>
            <Form.Item label="初始治理阈值"><InputNumber aria-label="初始治理阈值" min={2} precision={0} value={threshold} onChange={value => setThreshold(value ?? undefined)}/></Form.Item>
          </>}
        </>}
      </>}
    </Modal>{chainSignModal}
  </>;
}

export function InstitutionConfigurationPanel({ auth, cidNumber, canWrite, onSubmitted }: {
  auth: AdminAuth; cidNumber: string; canWrite: boolean; onSubmitted: () => void;
}) {
  const [snapshot, setSnapshot] = useState<GovernanceState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    let cancelled = false; setSnapshot(null); setError(null); setOpen(false);
    getGovernanceState(auth, cidNumber).then(value => { if (!cancelled) setSnapshot(value); })
      .catch(failure => { if (!cancelled) setError(failure instanceof Error ? failure.message : String(failure)); });
    return () => { cancelled = true; };
  }, [auth.accessToken, auth.accountId, auth.institutionCidNumber, cidNumber, revision]);
  return <Card title="首次岗位治理配置">
    <Typography.Paragraph>岗位与任职请使用对应治理页面</Typography.Paragraph>
    {error && <Alert type="error" message="岗位链状态读取失败" description={error} action={<Button onClick={() => setRevision(r => r + 1)}>重试</Button>}/>}
    {!snapshot && !error && <Spin tip="读取岗位配置"><div style={{ minHeight: 40 }}/></Spin>}
    <Button disabled={!canWrite || !snapshot?.registrationRoles?.length || snapshot.governanceThreshold !== null} onClick={() => setOpen(true)}>首次治理配置</Button>
    {open && <InstitutionConfigurationModal key={`${auth.accessToken}|${auth.personCid}|${auth.bindingRevision}|${auth.accountId}|${auth.institutionCidNumber}|${auth.roleCode}|${cidNumber}`} auth={auth} cidNumber={cidNumber} open
      onClose={() => setOpen(false)} onSubmitted={() => { setRevision(r => r + 1); onSubmitted(); }}/>}
  </Card>;
}
