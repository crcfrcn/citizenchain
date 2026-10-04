import { invoke } from '../tauri';
import type { ChainStatus, NodeIdentity, NodeStatus, TotalIssuance, TotalStake } from './types';

// 首页节点面板专用 Tauri API。
export const homeNodeApi = {
  getNodeStatus: () => invoke<NodeStatus>('get_node_status'),
  startNode: () => invoke<NodeStatus>('start_node'),
  stopNode: () => invoke<NodeStatus>('stop_node'),
  getChainStatus: () => invoke<ChainStatus>('get_chain_status'),
  getNodeIdentity: () => invoke<NodeIdentity>('get_node_identity'),
  getTotalIssuance: () => invoke<TotalIssuance>('get_total_issuance'),
  getTotalStake: () => invoke<TotalStake>('get_total_stake'),
};
