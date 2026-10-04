// 设置页类型，对齐后端 src/settings 返回值。

export type RewardAccount = {
  account_id: string | null;
  ss58_address: string | null;
};

export type BootnodeKey = {
  nodeKey: string | null;
  peerId: string | null;
  cidShortName: string | null;
};

export type GrandpaKey = {
  key: string | null;
  cidShortName: string | null;
};

export type BootnodeOption = {
  cidShortName: string;
  peerId: string;
};

export type NodeMode = 'archive' | 'normal';

export type NodeModeStatus = 'active' | 'pending';

export type NodeModeOption = {
  mode: NodeMode;
  label: string;
  implementationStatus: NodeModeStatus;
  enabled: boolean;
  description: string;
};

export type NodeModeState = {
  selectedMode: NodeMode;
  effectiveMode: NodeMode;
  options: NodeModeOption[];
};

export type OnChinaPlatformState = {
  running: boolean;
  status: 'stopped' | 'starting' | 'enabled' | 'error';
  statusLabel: string;
  url: string;
  detail?: string | null;
};

export type DesktopUpdateStatus =
  | 'checking'
  | 'available'
  | 'unavailable'
  | 'installing'
  | 'error';

export type DesktopUpdateInfo = {
  status: DesktopUpdateStatus;
  currentVersion: string | null;
  latestVersion: string | null;
  error: string | null;
};
