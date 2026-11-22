// 按提案编号读取并展示链上阶段、代表机构和计票进度。

import React, { useState } from 'react';
import { Alert, Button, InputNumber, Space, Spin } from 'antd';
import type { AdminAuth } from '@gmb/onchina-platform/authentication/types';
import { getProposalState } from '../proposals/api';
import type { LegProposalState } from '../proposals/types';
import { ProposalTallyPanel } from '../proposals/ProposalTallyPanel';

interface Props {
  auth: AdminAuth;
}

export function ProposalProgressView({ auth }: Props) {
  const [proposalId, setProposalId] = useState<string | null>(null);
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
        <InputNumber<string>
          stringMode
          min="0"
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
