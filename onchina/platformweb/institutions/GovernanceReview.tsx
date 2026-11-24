// 汇总机构治理草稿、当前链状态和提交审阅。

import { IdentityCidText } from '../codes/identityCid';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Alert, Button, Descriptions, Modal, Select, Space, Spin, Typography } from 'antd';
import type { AdminAuth } from '../authentication/types';
import { notice } from '../controls/notice';
import { CID_MODAL_Z_INDEX } from '../controls/modalStack';
import { submitChainSign, useChainSign } from '../qr/useChainSign';
import { getGovernanceState, governanceProposers, buildGovernance, prepareInstitutionGovernance, type GovernanceChanges, type GovernanceState } from './governance';
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
    }, [auth.bindingRevision, auth.personCid, auth.roleCode, auth.institutionCidNumber, auth.accessToken, auth.accountId, cidNumber, revision]);
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
    useEffect(() => () => { operation.current?.abort(); }, [auth.bindingRevision, auth.personCid, auth.roleCode, auth.institutionCidNumber, auth.accessToken, auth.accountId, state.cidNumber]);
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
            const fingerprint = (s: GovernanceState) => JSON.stringify([s.roles, s.representative]);
            if (fingerprint(fresh) !== fingerprint(state))
                throw new Error('链上治理数据已变化，请刷新后重新核对');
            if (!governanceProposers(auth, fresh).some(role => role.roleCode === roleCode))
                throw new Error('所选发起岗位已不可用');
            const input = buildGovernance(fresh, draft.changes, roleCode);
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
