// 法律案语义色状态标签(生效/待生效/废止)。层级/表决类型纯文本标签下沉到
// legislation/shared/labels.ts(叶子层,shared/ 不向上依赖 operator/),此处 re-export 保持既有导入点不变。

import React from 'react';
import { Tag } from 'antd';

export { tierLabel, voteTypeLabel } from '../../shared/labels';

/** 法律状态 → 语义色 Tag(0 待生效 / 1 生效 / 2 废止)。 */
export function statusTag(status: number): React.ReactNode {
  switch (status) {
    case 0:
      return <Tag color="gold">待生效</Tag>;
    case 1:
      return <Tag color="green">生效中</Tag>;
    case 2:
      return <Tag color="default">已废止</Tag>;
    default:
      return <Tag>—</Tag>;
  }
}
