// 统一立法层级、表决类型和代表规则的文本标签。

export function tierLabel(tier: number): string {
  switch (tier) {
    case 0:
      return '宪法';
    case 1:
      return '国家';
    case 2:
      return '省';
    case 3:
      return '市';
    default:
      return '—';
  }
}

/** 表决类型(0 常规 / 1 常规教育 / 2 重要 / 3 重要教育 / 4 特别)。 */
export function voteTypeLabel(voteType: number): string {
  switch (voteType) {
    case 0:
      return '常规案';
    case 1:
      return '常规教育案';
    case 2:
      return '重要案';
    case 3:
      return '重要教育案';
    case 4:
      return '特别案';
    default:
      return '—';
  }
}

/** 投票引擎三类强类型数学规则，不包含教育等立法业务分类。 */
export function representativeRuleLabel(rule: number): string {
  switch (rule) {
    case 0: return '常规规则';
    case 1: return '重要规则';
    case 2: return '特别规则';
    default: return `未知规则(${rule})`;
  }
}
