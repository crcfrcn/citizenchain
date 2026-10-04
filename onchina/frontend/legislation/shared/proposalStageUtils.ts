// 提案进度的纯展示工具(六阶段定义 / 状态语义色 / 赞成率),操作端进度页与大屏看板共用。
// 数值口径对齐链端 STAGE_LEG_*(10..14)与 STATUS_*(0..4),单源自此,避免两处漂移。

/** 六阶段(对齐链端 STAGE_LEG_*)。 */
export const STAGES: { value: number; label: string }[] = [
  { value: 10, label: '代表机构表决' },
  { value: 11, label: '立法公投' },
  { value: 12, label: '行政签署' },
  { value: 13, label: '三人会签' },
  { value: 14, label: '护宪终审' },
];

/** 提案状态 → 文案 + 语义色(对齐链端 STATUS_*)。 */
export function statusTag(status: number): { text: string; color: string } {
  switch (status) {
    case 0:
      return { text: '投票中', color: 'blue' };
    case 1:
      return { text: '通过', color: 'green' };
    case 2:
      return { text: '否决', color: 'red' };
    case 3:
      return { text: '已执行', color: 'green' };
    case 4:
      return { text: '执行失败', color: 'volcano' };
    default:
      return { text: '—', color: 'default' };
  }
}

/** 赞成率(百分比;无票为 0)。 */
export function approvalPercent(yes: number, no: number): number {
  const total = yes + no;
  return total === 0 ? 0 : Math.round((yes / total) * 100);
}
