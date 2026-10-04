import type { DesktopUpdateInfo } from './types';

export function shouldShowDesktopUpdateDot(
  updateInfo: Pick<DesktopUpdateInfo, 'status'>,
): boolean {
  // 红点只表达“仍有更新待处理”，安装中也保留提示状态。
  return updateInfo.status === 'available' || updateInfo.status === 'installing';
}
