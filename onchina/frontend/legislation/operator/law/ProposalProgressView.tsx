// 提案进度看板(操作端)。输入提案 ID → getProposalState → 复用共享 ProposalTallyPanel 呈现
// 六阶段 + 当前代表机构 + 计票 + 状态。只读投影，不做计票判定。

import React, { useState } from 'react';
import { Alert, Button, InputNumber, Space, Spin } from 'antd';
import type { AdminAuth } from '../../../auth/types';
import { getProposalState } from '../../api';
import type { LegProposalState } from '../../types';
import { ProposalTallyPanel } from '../../shared/ProposalTallyPanel';

interface Props {
  auth: AdminAuth;
}

export function ProposalProgressView({ auth }: Props) {
  const [proposalId, setProposalId] = useState<number | null>(null);
  const [state, setState] = useState<LegProposalState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const query = async () => {
    if (proposalId === null) {
      return;
    }
    setLoading(true);
    setError(null);
    setState(null);
    try {
      const result = await getProposalState(auth, proposalId);
      setState(result);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : '查询提案进度失败');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div>
      <Space wrap>
        <span>提案 ID:</span>
        <InputNumber
          min={0}
          value={proposalId ?? undefined}
          onChange={(v) => setProposalId(v ?? null)}
        />
        <Button onClick={query} loading={loading} disabled={proposalId === null}>
          查询进度
        </Button>
      </Space>

      {error && <Alert style={{ marginTop: 12 }} type="error" message={error} showIcon />}
      {loading && (
        <div style={{ marginTop: 12 }}>
          <Spin />
        </div>
      )}

      {state && (
        <div style={{ marginTop: 16 }}>
          <ProposalTallyPanel state={state} />
        </div>
      )}
    </div>
  );
}
