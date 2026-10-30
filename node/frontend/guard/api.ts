import { invoke } from "../client/tauri";
import type { OtherTabsPayload, WhitepaperDocument } from "../control/types";
import type { RuntimeConstitutionDocument } from "./types";

export const guardApi = {

  getRuntimeConstitutionDocument: () =>
    invoke<RuntimeConstitutionDocument>('get_runtime_constitution_document')
};
