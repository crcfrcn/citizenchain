import { Channel } from '@tauri-apps/api/core';
import { invoke } from "../client/tauri";
import type { BootnodeKey, BootnodeOption, DesktopInstaller, DesktopUpdateProgress, GrandpaKey, NodeMode, NodeModeState, OnChinaPlatformState, RewardAccount, ChainStatus, NodeIdentity, NodeStatus, TotalIssuance, TotalStake, OtherTabsPayload, WhitepaperDocument } from "./types";
import type { RuntimeConstitutionDocument } from "../guard/types";


// 设置页专用 Tauri API。
export const settingsApi = {
  getNodeMode: () => invoke<NodeModeState>('get_node_mode'),
  setNodeMode: (mode: NodeMode) => invoke<NodeModeState>('set_node_mode', { mode }),
  getOnChinaPlatform: () => invoke<OnChinaPlatformState>('get_onchina_platform'),
  startOnChinaPlatform: () => invoke<OnChinaPlatformState>('start_onchina_platform'),
  stopOnChinaPlatform: () => invoke<OnChinaPlatformState>('stop_onchina_platform'),
  getRewardAccount: () => invoke<RewardAccount>('get_reward_account'),
  setRewardAccount: (ss58Address: string, unlockPassword: string) =>
    invoke<RewardAccount>('set_reward_account', {
      ss58Address,
      unlockPassword: unlockPassword,
    }),
  getLocalMinerSs58Address: () =>
    invoke<string | null>('get_local_miner_ss58_address'),
  getBootnodeKey: () => invoke<BootnodeKey>('get_bootnode_key'),
  getGrandpaKey: () => invoke<GrandpaKey>('get_grandpa_key'),
  setBootnodeKey: (nodeKey: string, unlockPassword: string) =>
    invoke<BootnodeKey>('set_bootnode_key', { nodeKey, unlockPassword }),
  setGrandpaKey: (key: string, unlockPassword: string) =>
    invoke<GrandpaKey>('set_grandpa_key', { key, unlockPassword }),
  getGenesisBootnodeOptions: () =>
    invoke<BootnodeOption[]>('get_genesis_bootnode_options'),
  checkDesktopUpdate: () => invoke<DesktopInstaller | null>('check_desktop_update'),
  desktopUpdateStatus: () => invoke<DesktopUpdateProgress>('desktop_update_status'),
  downloadDesktopUpdate: (onProgress: (progress: DesktopUpdateProgress) => void) => {
    const channel = new Channel<DesktopUpdateProgress>();
    channel.onmessage = onProgress;
    return invoke<void>('download_desktop_update', { onProgress: channel });
  },
  installDesktopUpdate: () => invoke<void>('install_desktop_update'),
};



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


export const otherTabsApi = {

  getOtherTabsContent: () => invoke<OtherTabsPayload>('get_other_tabs_content'),

  getWhitepaperDocument: () => invoke<WhitepaperDocument>('get_whitepaper_document')
};
