// 呈现法律生效状态标签并导出共用层级和表决标签。

import React from 'react';
import { Tag } from 'antd';

export { tierLabel, voteTypeLabel } from '../proposals/labels';

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
