// 调用原生挖矿收入和网络概览接口。

import { invoke } from "../client/tauri";
import type { MiningDashboard, NetworkOverview } from "./types";


// 挖矿 tab 专用 Tauri API。
export const miningApi = {
  getMiningDashboard: () => invoke<MiningDashboard>('get_mining_dashboard'),
  getNetworkOverview: () => invoke<NetworkOverview>('get_network_overview'),
};
