import { invoke } from '../tauri';
import type { OtherTabsPayload, RuntimeConstitutionDocument } from './types';

// 白皮书/公民宪法等其他 tab 专用 Tauri API。
export const otherTabsApi = {
  getOtherTabsContent: () => invoke<OtherTabsPayload>('get_other_tabs_content'),
  getRuntimeConstitutionDocument: () =>
    invoke<RuntimeConstitutionDocument>('get_runtime_constitution_document'),
};
