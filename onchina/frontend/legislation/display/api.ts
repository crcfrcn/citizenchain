// 大屏看板取数(免登录只读)。经 publicRequest 拉本节点绑定机构的看板快照;
// 机构由后端按节点绑定确定,前端不传任何参数(fail-closed 在后端)。

import { publicRequest } from '../../utils/http';
import type { DisplayBoard } from './types';

/** 拉取本节点大屏看板快照(名册 × 活跃立法提案 × 逐席投票)。 */
export function getDisplayBoard(): Promise<DisplayBoard> {
  return publicRequest<DisplayBoard>('/api/public/legislation/display/board');
}
