// 定义提案阶段和状态展示，计算只读赞成率及语义标签。

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
