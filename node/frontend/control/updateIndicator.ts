import type { DesktopUpdateInfo } from "./types";


export function shouldShowDesktopUpdateDot(
  updateInfo: Pick<DesktopUpdateInfo, 'status'>,
): boolean {
  // 打开下载后仍提示新版，直到用户安装新版本再打开应用。
  return updateInfo.status === 'available';
}
