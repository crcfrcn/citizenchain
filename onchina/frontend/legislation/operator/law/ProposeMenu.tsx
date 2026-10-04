// 发起菜单(唯一发起入口)。候选(可发起立法动作 × 表决类型)单源自后端
// /api/legislation/proposable(参议会/非立法机构返回空 → 不渲染)。选表决类型 + 立法/修法/废法 → 开编辑器。

import React, { useEffect, useState } from 'react';
import { Alert, Button, Select, Space } from 'antd';
import type { AdminAuth } from '../../../auth/types';
import { getProposable } from '../../api';
import type { LawActionInput, ProposableCandidate } from '../../types';
import { voteTypeLabel } from './labels';
import { LawEditorModal } from './LawEditorModal';

interface Props {
  auth: AdminAuth;
}

interface EditorTarget {
  lawAction: LawActionInput;
  tier: number;
  voteType: number;
}

export function ProposeMenu({ auth }: Props) {
  const [candidates, setCandidates] = useState<ProposableCandidate[]>([]);
  const [voteType, setVoteType] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [editor, setEditor] = useState<EditorTarget | null>(null);

  useEffect(() => {
    let cancelled = false;
    getProposable(auth)
      .then((data) => {
        if (cancelled) return;
        setCandidates(data);
        const law = data.find((c) => c.category === 'law');
        if (law && law.voteTypes.length > 0) {
          setVoteType(law.voteTypes[0]);
        }
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : '加载发起候选失败');
        }
      });
    return () => {
      cancelled = true;
    };
  }, [auth.access_token, auth.institution_code]);

  const lawCandidate = candidates.find((c) => c.category === 'law');

  if (error) {
    return <Alert type="error" message={error} showIcon />;
  }
  if (!lawCandidate) {
    return <div style={{ color: 'rgba(0,0,0,0.45)' }}>本机构无可发起的法律案类型。</div>;
  }

  const openEditor = (lawAction: LawActionInput) => {
    if (voteType === null) {
      return;
    }
    setEditor({ lawAction, tier: lawCandidate.tier, voteType });
  };

  return (
    <div>
      <Space wrap>
        <span>表决类型:</span>
        <Select
          style={{ width: 160 }}
          value={voteType ?? undefined}
          onChange={setVoteType}
          options={lawCandidate.voteTypes.map((vt) => ({ value: vt, label: voteTypeLabel(vt) }))}
        />
        <Button type="primary" disabled={voteType === null} onClick={() => openEditor('enact')}>
          发起立法
        </Button>
        <Button disabled={voteType === null} onClick={() => openEditor('amend')}>
          发起修法
        </Button>
        <Button danger disabled={voteType === null} onClick={() => openEditor('repeal')}>
          发起废法
        </Button>
      </Space>
      {editor && (
        <LawEditorModal
          open
          auth={auth}
          lawAction={editor.lawAction}
          tier={editor.tier}
          voteType={editor.voteType}
          onClose={() => setEditor(null)}
        />
      )}
    </div>
  );
}
