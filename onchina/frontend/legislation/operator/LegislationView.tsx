// 立法与表决操作端页面壳(Phase 2B)。立法机构管理员登录后的入口:
// 顶部院身份 + 立法角色,下方按能力位分区(本级法律 / 发起提案 / 表决与进度)占位,2C–2F 逐块接入。
// 视觉复用注册局同款 glassCard 毛玻璃卡片,与控制台其它模块一致。

import React, { useState } from 'react';
import { Button, Card, Input, InputNumber, Space, Tag, message } from 'antd';
import type { AdminAuth } from '../../auth/types';
import { glassCardStyle, glassCardHeadStyle } from '../../core/cardStyles';
import { LawListTable } from './law/LawListTable';
import { LawDetailView } from './law/LawDetailView';
import { ProposeMenu } from './law/ProposeMenu';
import { ProposalProgressView } from './law/ProposalProgressView';
import { castRepresentativeVote } from '../api';
import { submitChainSign, useChainSign } from '../../core/useChainSign';

interface Props {
  auth: AdminAuth;
}

/** 行政层级标签 → 中文。 */
function tierLabel(level?: string | null): string {
  switch (level) {
    case 'NATIONAL':
      return '国家级';
    case 'PROVINCE':
      return '省级';
    case 'CITY':
      return '市级';
    default:
      return '—';
  }
}

/** 由能力位派生立法角色文案(单源自后端能力位下发,前端只镜像展示)。 */
function roleTag(auth: AdminAuth): { text: string; color: string } {
  const canPropose = !!auth.capabilities?.canProposeLegislation;
  const canVote = !!auth.capabilities?.canCastRepresentativeVote;
  if (canPropose && canVote) {
    return { text: '发起机构 · 发起 + 代表机构表决', color: 'geekblue' };
  }
  if (!canPropose && canVote) {
    return { text: '复议/终审机构 · 只参加代表机构表决', color: 'purple' };
  }
  if (canPropose && !canVote) {
    return { text: '提案机构 · 向表决院提案', color: 'green' };
  }
  return { text: '只读', color: 'default' };
}

/** 当前代表机构表决；后端再次按机构角色校验。 */
function RepresentativeVotePanel({ auth }: Props) {
  const [proposalId, setProposalId] = useState<number | null>(null);
  const [voterRoleCode, setVoterRoleCode] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const { signChain, chainSignModal } = useChainSign('代表机构表决链交易签名');

  const vote = async (approve: boolean) => {
    if (proposalId === null || !voterRoleCode.trim()) return;
    setSubmitting(true);
    try {
      const prepared = await castRepresentativeVote(
        auth,
        proposalId,
        voterRoleCode.trim(),
        approve,
      );
      const signed = await signChain(prepared.request_id, prepared.sign_request);
      const submitted = await submitChainSign(
        auth,
        prepared.request_id,
        signed.account_id,
        signed.signature,
      );
      message.success(`表决交易已提交：${submitted.tx_hash}`);
    } catch (error: unknown) {
      message.error(error instanceof Error ? error.message : '表决提交失败');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div>
      <Space wrap>
        <span>提案 ID:</span>
        <InputNumber
          min={0}
          value={proposalId ?? undefined}
          onChange={(value) => setProposalId(value ?? null)}
        />
        <Input
          placeholder="投票岗位码"
          value={voterRoleCode}
          onChange={(event) => setVoterRoleCode(event.target.value)}
          style={{ width: 220 }}
        />
        <Button
          type="primary"
          loading={submitting}
          disabled={proposalId === null || !voterRoleCode.trim()}
          onClick={() => vote(true)}
        >
          赞成
        </Button>
        <Button
          danger
          loading={submitting}
          disabled={proposalId === null || !voterRoleCode.trim()}
          onClick={() => vote(false)}
        >
          反对
        </Button>
      </Space>
      {chainSignModal}
    </div>
  );
}

/** 立法与表决操作端页面壳。 */
export function LegislationView({ auth }: Props) {
  const [selectedLawId, setSelectedLawId] = useState<number | null>(null);
  const role = roleTag(auth);
  const scope =
    [auth.scope_province_name, auth.scope_city_name].filter(Boolean).join(' · ') || '全国';
  const canPropose = !!auth.capabilities?.canProposeLegislation;
  const canVote = !!auth.capabilities?.canCastRepresentativeVote;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 16 }}>
      <Card
        style={glassCardStyle}
        headStyle={glassCardHeadStyle}
        title={
          <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
            <span style={{ fontSize: 20, fontWeight: 700 }}>
              {auth.cid_short_name ?? auth.institution_code}
            </span>
            <Tag color={role.color}>{role.text}</Tag>
          </div>
        }
      >
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: 24, color: 'rgba(0,0,0,0.65)' }}>
          <span>层级:{tierLabel(auth.admin_level)}</span>
          <span>辖区:{scope}</span>
          <span>机构码:{auth.institution_code}</span>
        </div>
      </Card>

      <Card style={glassCardStyle} headStyle={glassCardHeadStyle} title="本级法律">
        {selectedLawId === null ? (
          <LawListTable auth={auth} onOpen={setSelectedLawId} />
        ) : (
          <LawDetailView
            auth={auth}
            lawId={selectedLawId}
            onBack={() => setSelectedLawId(null)}
          />
        )}
      </Card>

      {canPropose && (
        <Card style={glassCardStyle} headStyle={glassCardHeadStyle} title="发起提案">
          <ProposeMenu auth={auth} />
        </Card>
      )}

      <Card style={glassCardStyle} headStyle={glassCardHeadStyle} title="表决与进度">
        {canVote && <RepresentativeVotePanel auth={auth} />}
        <div style={{ marginTop: canVote ? 16 : 0 }}>
          <ProposalProgressView auth={auth} />
        </div>
      </Card>
    </div>
  );
}
