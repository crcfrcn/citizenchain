// 提案进度纯展示件——阶段、状态、规则、当前代表机构和计票。
// 只读投影(接收已取好的 LegProposalState,不含取数/鉴权),操作端进度页与大屏看板共用。

import React from 'react';
import { Progress, Space, Steps, Tag } from 'antd';
import type { LegProposalState } from '../types';
import { representativeRuleLabel } from './labels';
import { STAGES, approvalPercent, statusTag } from './proposalStageUtils';

interface Props {
  state: LegProposalState;
}

export function ProposalTallyPanel({ state }: Props) {
  const stageIndex = STAGES.findIndex((s) => s.value === state.stage);
  const status = statusTag(state.status);

  return (
    <div>
      <Space wrap style={{ marginBottom: 12 }}>
        <Tag color={status.color}>{status.text}</Tag>
        <span>表决规则:{representativeRuleLabel(state.representativeRule)}</span>
        <span>当前代表机构:第 {state.currentBody + 1} 个</span>
        {state.needsGuard && <Tag color="volcano">需护宪终审</Tag>}
        {state.representativeRule === 2 && <Tag color="gold">特别案公投</Tag>}
      </Space>

      <Steps
        size="small"
        current={stageIndex < 0 ? 0 : stageIndex}
        items={STAGES.map((s) => ({ title: s.label }))}
      />

      <div style={{ marginTop: 16 }}>
        <div>
          当前代表机构计票(赞成 {state.representativeTally.yes} / 反对 {state.representativeTally.no}):
        </div>
        <Progress percent={approvalPercent(state.representativeTally.yes, state.representativeTally.no)} />
        {state.representativeRule === 2 && (
          <>
            <div style={{ marginTop: 8 }}>
              公投计票(赞成 {state.referendumTally.yes} / 反对 {state.referendumTally.no}):
            </div>
            <Progress
              percent={approvalPercent(state.referendumTally.yes, state.referendumTally.no)}
            />
          </>
        )}
      </div>
    </div>
  );
}
