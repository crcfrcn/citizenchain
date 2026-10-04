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
