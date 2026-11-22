// 汇总机构治理草稿、当前链状态和提交审阅。

import { IdentityCidText } from '../codes/identityCid';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Button, Card, Descriptions, Input, Modal, Select, Space, Spin, Table, Typography } from 'antd';
import { ScanAccountModal } from '../qr/ScanAccountModal';
import { tryEncodeSs58 } from '../codes/ss58';
import type { AdminAuth } from '../authentication/types';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
import { submitChainSign, useChainSign } from '../qr/useChainSign';
import { getGovernanceState, governanceProposers, nonAdminGovernance, prepareInstitutionGovernance, type GovernanceChanges, type GovernanceState, buildInstitutionWalletDraft, institutionWalletRoles, normalizeInstitutionWalletAccount, runInstitutionWalletChange, readInstitutionWalletReceipts, rememberInstitutionWallet, reviewInstitutionWalletConflict, memberName, type InstitutionWalletMode, type InstitutionWalletRunResult, type PendingInstitutionWallet } from './governance';
export type GovernanceViewProps = {
    auth: AdminAuth;
    cidNumber: string;
    canWrite?: boolean;
};
export type GovernanceDraft = {
    title: string;
    changes: GovernanceChanges;
    rows: {
        label: string;
        before: string;
        after: string;
    }[];
};
export function useGovernanceState({ auth, cidNumber }: GovernanceViewProps) {
    const [state, setState] = useState<GovernanceState | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [revision, setRevision] = useState(0);
    const reload = useCallback(() => setRevision(value => value + 1), []);
    useEffect(() => {
        let cancelled = false;
        setLoading(true);
        setError(null);
        setState(null);
        getGovernanceState(auth, cidNumber).then(value => {
            if (!cancelled)
                setState(value);
        }).catch(error => {
            if (!cancelled)
                setError(error instanceof Error ? error.message : String(error));
        }).finally(() => {
            if (!cancelled)
                setLoading(false);
        });
        return () => { cancelled = true; };
    }, [auth.accessToken, auth.accountId, cidNumber, revision]);
    return { state, error, loading, reload };
}
export function GovernanceStatus({ loading, error, reload }: {
    loading: boolean;
    error: string | null;
    reload: () => void;
}) {
    if (loading)
        return <Spin tip="读取治理数据"><div style={{ minHeight: 80 }}/></Spin>;
    if (error)
        return <Alert type="error" showIcon message="治理数据读取失败" description={error} action={<Button onClick={reload}>重试</Button>}/>;
    return null;
}
export function GovernanceReview({ auth, state, draft, onClose, onSubmitted }: {
    auth: AdminAuth;
    state: GovernanceState;
    draft: GovernanceDraft | null;
    onClose: () => void;
    onSubmitted: () => void;
}) {
    const [roleCode, setRoleCode] = useState<string>();
    const [busy, setBusy] = useState(false);
    const operation = useRef<AbortController | null>(null);
    useEffect(() => () => { operation.current?.abort(); }, [auth.accessToken, auth.accountId, state.cidNumber]);
    const { signChain, chainSignModal } = useChainSign('机构治理提案签名');
    const roles = governanceProposers(auth, state);
    useEffect(() => { setRoleCode(undefined); }, [draft]);
    const submit = async () => {
        if (!draft || !roleCode)
            return;
        setBusy(true);
        const controller = new AbortController();
        operation.current = controller;
        try {
            const fresh = await getGovernanceState(auth, state.cidNumber);
            controller.signal.throwIfAborted();
            const fingerprint = (s: GovernanceState) => JSON.stringify([s.roles, s.members, s.representative]);
            if (fingerprint(fresh) !== fingerprint(state))
                throw new Error('链上治理数据已变化，请刷新后重新核对');
            if (!governanceProposers(auth, fresh).some(role => role.roleCode === roleCode))
                throw new Error('所选发起岗位已不可用');
            const input = nonAdminGovernance(fresh, draft.changes, roleCode);
            const prepared = await prepareInstitutionGovernance(auth, input, controller.signal);
            controller.signal.throwIfAborted();
            const signed = await signChain(prepared.requestId, prepared.signRequest);
            controller.signal.throwIfAborted();
            await submitChainSign(auth, prepared.requestId, signed.accountId, signed.signature);
            notice.success('治理提案已提交，等待投票通过并执行');
            onClose();
            onSubmitted();
        }
        catch (error) {
            if (!controller.signal.aborted)
                notice.error(error, '治理提案提交失败');
        }
        finally {
            if (operation.current === controller && !controller.signal.aborted)
                setBusy(false);
        }
    };
    return <>
    <Modal open={!!draft} title={draft?.title} onCancel={() => {
            if (!busy)
                onClose();
        }} closable={!busy} maskClosable={!busy} zIndex={CID_MODAL_Z_INDEX.business} footer={<Space><Button disabled={busy} onClick={onClose}>取消</Button><Button type="primary" loading={busy} disabled={!roleCode || !roles.length} onClick={() => void submit()}>验证并签名提交提案</Button></Space>}>
      <Typography.Paragraph>目标机构身份CID号：<IdentityCidText value={state.cidNumber} /></Typography.Paragraph>
      <Descriptions column={1} bordered size="small">{draft?.rows.map((row, index) => <Descriptions.Item key={index} label={row.label}><div>变更前：<IdentityCidText value={row.before || '无'} /></div><div>变更后：<IdentityCidText value={row.after || '无'} /></div></Descriptions.Item>)}</Descriptions>
      <Typography.Paragraph style={{ marginTop: 16 }}>发起岗位</Typography.Paragraph>
      <Select aria-label="发起岗位" style={{ width: '100%' }} placeholder="请选择当前有效且具有发起权限的岗位" value={roleCode} onChange={setRoleCode} options={roles.map(role => ({ value: role.roleCode, label: role.roleName }))}/>
      {!roles.length && <Alert style={{ marginTop: 12 }} type="info" message="当前身份没有可用的本机构治理发起岗位"/>}
      <Alert style={{ marginTop: 16 }} type="info" message="提交的是治理提案，投票通过并执行后才生效。"/>
    </Modal>
    {chainSignModal}
  </>;
}


export type InstitutionWalletChangeModalProps = GovernanceViewProps & {
    mode: InstitutionWalletMode; targetAccount?: string; initialReceipt?: PendingInstitutionWallet;
    onClose: () => void; onSubmitted: (result: InstitutionWalletRunResult) => void;
};
/** 一个共享组件处理本人双签和内部投票，界面永不写个人CID绑定。 */
export function InstitutionWalletChangeModal({ auth, cidNumber, mode, targetAccount, initialReceipt, onClose, onSubmitted }: InstitutionWalletChangeModalProps) {
    const { state, loading, error: readError, reload } = useGovernanceState({ auth, cidNumber });
    const [newWallet, setNewWallet] = useState('');
    const [roleCode, setRoleCode] = useState<string>();
    const [scanning, setScanning] = useState(false);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [pending, setPending] = useState<PendingInstitutionWallet | null>(() => initialReceipt ?? readInstitutionWalletReceipts(auth).find(r => r.draft.mode === mode && (!targetAccount || r.draft.adminAccount === targetAccount)) ?? null);
    const operation = useRef<AbortController | null>(null);
    const running = useRef(false);
    const { signChain, chainSignModal } = useChainSign();
    useEffect(() => () => { operation.current?.abort(); }, [auth.accessToken, auth.accountId, auth.institutionCidNumber, cidNumber, mode, targetAccount]);
    const member = state?.members.find(m => targetAccount ? m.accountId === targetAccount : m.signerId.toLowerCase() === auth.accountId.toLowerCase());
    const roles = state && member ? institutionWalletRoles(auth, state, member.accountId) : [];
    const target = pending?.draft.adminAccount ?? member?.accountId;
    const oldWallet = pending?.draft.expectedWallet ?? member?.signerId;
    const submit = async () => {
        if (running.current) return;
        running.current = true; setBusy(true); setError(null);
        const controller = new AbortController(); operation.current = controller;
        const check = () => controller.signal.throwIfAborted();
        try {
            const draft = !pending && state && target ? buildInstitutionWalletDraft(auth, state, mode, target, newWallet, roleCode) : undefined;
            const result = await runInstitutionWalletChange({ auth, state: state ?? undefined, draft, pending,
                signal: controller.signal, check, sign: signChain,
                remember: (receipt, previous) => { check(); setPending(receipt); rememberInstitutionWallet(receipt, previous); } });
            check();
            notice.success(result.observed ? '链上已显示新钱包，原交易回执未确认；没有重复提交交易'
                : result.mode === 'GOVERNANCE' ? '钱包更换提案已提交，等待原岗位投票通过并执行' : '本人的机构钱包更换交易已完成');
            onClose(); onSubmitted(result);
        } catch (failure) {
            if (!controller.signal.aborted) setError(failure instanceof Error ? failure.message : String(failure));
        } finally {
            running.current = false;
            if (!controller.signal.aborted) setBusy(false);
        }
    };
    const reviewConflict = async () => {
        if (!pending || running.current) return;
        const controller = new AbortController(); operation.current = controller;
        running.current = true; setBusy(true); setError(null);
        try {
            await reviewInstitutionWalletConflict(auth, pending); controller.signal.throwIfAborted();
            rememberInstitutionWallet(null, pending); setPending(null); setNewWallet(''); setRoleCode(undefined); reload();
        } catch (failure) { if (!controller.signal.aborted) setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { running.current = false; if (!controller.signal.aborted) setBusy(false); }
    };
    let validDraft = false;
    let validationError: string | null = null;
    if (state && target) {
        try { buildInstitutionWalletDraft(auth, state, mode, target, newWallet, roleCode); validDraft = true; } catch (failure) { if (newWallet.trim()) validationError = failure instanceof Error ? failure.message : String(failure); }
    }
    return <>
      <Modal open title={pending ? '恢复原钱包更换交易' : mode === 'SELF' ? '本人更换机构管理员钱包' : '发起管理员钱包更换提案'}
        onCancel={() => { operation.current?.abort(); onClose(); }} maskClosable={false} zIndex={CID_MODAL_Z_INDEX.business}
        footer={<Space><Button onClick={() => { operation.current?.abort(); onClose(); }}>关闭</Button><Button type="primary" loading={busy}
          disabled={!pending && (loading || !!readError || !validDraft)} onClick={() => void submit()}>{pending ? '恢复原交易' : mode === 'SELF' ? '确认并开始双签' : '确认并签名提案'}</Button></Space>}>
        {!pending && <GovernanceStatus loading={loading} error={readError} reload={reload}/>}
        <Descriptions column={1} bordered size="small">
          <Descriptions.Item label="机构身份CID号"><IdentityCidText value={cidNumber}/></Descriptions.Item>
          <Descriptions.Item label="管理员">{memberName(member)}</Descriptions.Item>
          <Descriptions.Item label="原钱包账户"><Typography.Text style={{ wordBreak: 'break-all' }}>{tryEncodeSs58(oldWallet)}</Typography.Text></Descriptions.Item>
          <Descriptions.Item label="机构钱包版本">{pending?.draft.expectedRevision ?? member?.walletRevision ?? '读取中'}</Descriptions.Item>
        </Descriptions>
        {pending ? <>
          <Typography.Paragraph style={{ marginTop: 12 }}>新钱包账户：<Typography.Text style={{ wordBreak: 'break-all' }}>{tryEncodeSs58(pending.draft.newWallet)}</Typography.Text></Typography.Paragraph>
          <Alert type="info" showIcon message="已保存原交易签名" description="恢复只查询或提交原会话，不重新准备交易，也不重新认证或签名。"/>
          <Button style={{ marginTop: 12 }} disabled={busy} onClick={() => void reviewConflict()}>核对当前版本并重新审阅</Button>
        </> : <>
          <Typography.Paragraph style={{ marginTop: 16, marginBottom: 8 }}>新钱包账户</Typography.Paragraph>
          <Space.Compact style={{ width: '100%' }}><Input aria-label="新钱包账户" value={newWallet} disabled={busy} placeholder="输入新钱包SS58地址或扫描新账户码" onChange={e => { setNewWallet(e.target.value); setError(null); }}/>
            <Button disabled={busy} onClick={() => setScanning(true)}>扫描账户码</Button></Space.Compact>
          {validationError && <Typography.Paragraph type="danger" style={{ marginTop: 8 }}>{validationError}</Typography.Paragraph>}
          {mode === 'GOVERNANCE' && <><Typography.Paragraph style={{ marginTop: 16, marginBottom: 8 }}>发起岗位</Typography.Paragraph><Select aria-label="钱包治理发起岗位" style={{ width: '100%' }} value={roleCode} disabled={busy}
            placeholder="请选择有效发起岗位" onChange={setRoleCode} options={roles.map(role => ({ value: role.roleCode, label: role.roleName }))}/>
            {!roles.length && <Alert type="info" message="当前身份没有目标管理员所在岗位的治理发起权限"/>}</>}
          <Alert style={{ marginTop: 16 }} type="info" showIcon message={mode === 'SELF' ? '新钱包确认接收后，使用原钱包签署本人更换交易。' : '新钱包确认接收后，由所选岗位发起提案；投票通过并执行后生效。'}
            description="只更换本机构管理员钱包，身份CID号、岗位、任期、辖区和余额不随之迁移。"/>
        </>}
        {error && <Alert style={{ marginTop: 12 }} type="error" showIcon message={error}/>}
        <ScanAccountModal open={scanning} onClose={() => setScanning(false)} onResolved={account => { try { setNewWallet(tryEncodeSs58(normalizeInstitutionWalletAccount(account))); setScanning(false); } catch (e) { setError(String(e)); } }}/>
      </Modal>
      {chainSignModal}
    </>;
}

/** 新钱包登录后也可看见本人旧签名会话；不把原账户权限复制到新会话。 */
export function InstitutionWalletRecoveryPanel({ auth, onSubmitted, mode }: { auth: AdminAuth; mode?: InstitutionWalletMode; onSubmitted: (result: InstitutionWalletRunResult) => void }) {
    const [receipts, setReceipts] = useState<PendingInstitutionWallet[]>(() => readInstitutionWalletReceipts(auth).filter(r => !mode || r.draft.mode === mode));
    const [selected, setSelected] = useState<PendingInstitutionWallet | null>(null);
    useEffect(() => {
        const update = () => setReceipts(readInstitutionWalletReceipts(auth).filter(r => !mode || r.draft.mode === mode));
        setSelected(null); update();
        window.addEventListener('institution-wallet-receipts-changed', update);
        window.addEventListener('storage', update);
        return () => { window.removeEventListener('institution-wallet-receipts-changed', update); window.removeEventListener('storage', update); };
    }, [auth.accessToken, auth.accountId, auth.institutionCidNumber, mode]);
    return <>
      {receipts.map(receipt => <Alert key={receipt.requestId} style={{ marginBottom: 12 }} type="info" showIcon message={receipt.draft.mode === 'SELF' ? '有已签名的本人钱包更换交易待核对' : '有已签名的钱包治理提案待核对'}
        action={<Button onClick={() => setSelected(receipt)}>恢复原交易</Button>}/>)}
      {selected && <InstitutionWalletChangeModal key={`${auth.accessToken}|${selected.requestId}`} auth={auth} cidNumber={selected.draft.cidNumber} mode={selected.draft.mode}
        targetAccount={selected.draft.adminAccount} initialReceipt={selected} onClose={() => setSelected(null)} onSubmitted={onSubmitted}/>}
    </>;
}

export function InstitutionWalletGovernancePanel({ auth, cidNumber, canWrite = true }: GovernanceViewProps) {
    const { state, error, loading, reload } = useGovernanceState({ auth, cidNumber });
    const [target, setTarget] = useState<string | null>(null);
    return <Card title="机构治理">
      {auth.institutionCidNumber === cidNumber && <InstitutionWalletRecoveryPanel auth={auth} mode="GOVERNANCE" onSubmitted={reload}/>}
      <Typography.Paragraph>更换其他管理员的钱包须由原岗位发起提案，依原规则投票并执行。{auth.institutionCode === 'FRG' ? '联邦注册局只使用目标原省专员岗位的选票。' : ''}</Typography.Paragraph>
      <GovernanceStatus loading={loading} error={error} reload={reload}/>
      <Button style={{ marginBottom: 12 }} onClick={reload}>刷新链上钱包状态</Button>
      <Table rowKey="accountId" pagination={false} scroll={{ x: 760 }} dataSource={state?.members ?? []} columns={[
        { title: '姓名', render: (_, member) => memberName(member) },
        { title: '钱包账户', render: (_, member) => <span style={{ wordBreak: 'break-all', fontSize: 11 }}>{tryEncodeSs58(member.signerId)}</span> },
        { title: '操作', render: (_, member) => member.signerId.toLowerCase() === auth.accountId.toLowerCase() ? <Typography.Text type="secondary">本人请从管理员列表更换</Typography.Text>
            : <Button disabled={!canWrite || !state || !institutionWalletRoles(auth, state, member.accountId).length} onClick={() => setTarget(member.accountId)}>提案更换钱包</Button> },
      ]}/>
      {target && <InstitutionWalletChangeModal key={`${auth.accessToken}|${cidNumber}|${target}`} auth={auth} cidNumber={cidNumber} mode="GOVERNANCE" targetAccount={target} onClose={() => setTarget(null)} onSubmitted={reload}/>}
    </Card>;
}
