import type { DesktopUpdateInfo } from "./types";


export function shouldShowDesktopUpdateDot(
  updateInfo: Pick<DesktopUpdateInfo, 'status'> & Partial<Pick<DesktopUpdateInfo, 'latestVersion'>>,
): boolean {
  // 下载和等待安装期间保留新版标记，安装后由新应用版本重新判断。
  return ['available', 'downloading', 'ready', 'installing'].includes(updateInfo.status)
    || (updateInfo.status === 'error' && !!updateInfo.latestVersion);
}
