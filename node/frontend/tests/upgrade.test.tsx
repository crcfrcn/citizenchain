import {beforeEach, expect, it, vi} from 'vitest';
import {runtimeUpgradeApi} from '../upgrade/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke}));
const api=runtimeUpgradeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>mocks.invoke.mockReset());
it("getPowDifficultyParams保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getPowDifficultyParams"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_pow_difficulty_params"); });
it("buildProposeUpgradeRequest保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["buildProposeUpgradeRequest"](...["public","44","candidate.wasm","原因",{"paramsVersion":1,"algorithmVersion":1,"targetBlockTimeMs":360000,"adjustmentInterval":10,"maxAdjustUpFactor":2,"maxAdjustDownDivisor":2}])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("build_propose_upgrade_request", {"signerPublicKey":"public","actorCidNumber":"44","wasmPath":"candidate.wasm","reason":"原因","powParams":{"paramsVersion":1,"algorithmVersion":1,"targetBlockTimeMs":360000,"adjustmentInterval":10,"maxAdjustUpFactor":2,"maxAdjustDownDivisor":2}}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["getPowDifficultyParams"](...[])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});
