
// 设置页类型，对齐后端 src/settings 返回值。

export type RewardAccount = {
  accountId: string | null;
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
  | 'downloading'
  | 'ready'
  | 'installing'
  | 'error';

// 更新元数据仅来自当前平台正式Release，下载及安装状态由后端维护。
export type DesktopInstaller = {
  currentVersion: string;
  version: string;
  downloadUrl: string;
  size: number;
  filename: string;
  releaseId: number;
  assetId: number;
};

export type DesktopUpdateProgress = {
  status: 'idle' | 'downloading' | 'ready' | 'installing' | 'error';
  version: string | null;
  downloaded: number;
  total: number;
  error: string | null;
};

export type DesktopUpdateInfo = {
  status: DesktopUpdateStatus;
  currentVersion: string | null;
  latestVersion: string | null;
  downloaded?: number;
  total?: number;
  error: string | null;
};

// 首页节点状态类型，对齐后端 src/home 的 identity/rpc/process 返回值。

export type NodeStatus = {
  running: boolean;
  state: string;
  pid: number | null;
  lastError: string | null;
};

export type ChainStatus = {
  blockHeight: number | null;
  finalizedHeight: number | null;
  syncing: boolean | null;
  specVersion: number | null;
  nodeVersion: string;
};

export type NodeIdentity = {
  peerId: string | null;
  role: string | null;
  genesisHash: string | null;
};

export type TotalIssuance = {
  totalIssuance: string | null;
};

export type TotalStake = {
  totalStake: string | null;
};

// 其他 tab 内容类型，对齐后端 src/other/other_tabs。

export type LocalDoc = {
  key: 'whitepaper';
  markdown: string;
};


export type WhitepaperDocument = {
  markdown: string;
};


type OtherRuntimeConstitutionTabItem = {
  key: 'constitution';
  title: string;
  contentType: 'runtimeConstitution';
};


type OtherTextTabItem = {
  key: string;
  title: string;
  contentType: 'text';
  text: string;
};


export type OtherTabItem = OtherRuntimeConstitutionTabItem | OtherTextTabItem;


export type OtherTabsPayload = {
  tabs: OtherTabItem[];
};