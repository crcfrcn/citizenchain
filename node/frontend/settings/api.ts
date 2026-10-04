import { invoke } from '../tauri';
import type {
  BootnodeKey,
  BootnodeOption,
  GrandpaKey,
  NodeMode,
  NodeModeState,
  OnChinaPlatformState,
  RewardAccount,
} from './types';

// 设置页专用 Tauri API。
export const settingsApi = {
  getNodeMode: () => invoke<NodeModeState>('get_node_mode'),
  setNodeMode: (mode: NodeMode) => invoke<NodeModeState>('set_node_mode', { mode }),
  getOnChinaPlatform: () => invoke<OnChinaPlatformState>('get_onchina_platform'),
  startOnChinaPlatform: () => invoke<OnChinaPlatformState>('start_onchina_platform'),
  stopOnChinaPlatform: () => invoke<OnChinaPlatformState>('stop_onchina_platform'),
  getRewardAccount: () => invoke<RewardAccount>('get_reward_account'),
  setRewardAccount: (ss58_address: string, unlockPassword: string) =>
    invoke<RewardAccount>('set_reward_account', {
      ss58_address,
      unlock_password: unlockPassword,
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
  prepareDesktopUpdate: () => invoke<void>('prepare_desktop_update'),
};
