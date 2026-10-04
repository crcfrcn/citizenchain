import { invoke } from '../tauri';
import type { MiningDashboard, NetworkOverview } from './types';

// 挖矿 tab 专用 Tauri API。
export const miningApi = {
  getMiningDashboard: () => invoke<MiningDashboard>('get_mining_dashboard'),
  getNetworkOverview: () => invoke<NetworkOverview>('get_network_overview'),
};
