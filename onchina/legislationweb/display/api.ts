// 读取无需登录且由后端节点绑定确定的立法看板快照。

import { request } from '@gmb/onchina-platform/http';
import type { DisplayBoard } from './types';

/** 拉取本节点大屏看板快照(名册 × 活跃立法提案 × 逐席投票)。 */
export function getDisplayBoard(): Promise<DisplayBoard> {
  return request<DisplayBoard>('/api/public/legislation/display/board');
}
