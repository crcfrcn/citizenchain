import {beforeEach, expect, it, vi} from 'vitest';
import {miningApi} from '../mining/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke}));
const api=miningApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>mocks.invoke.mockReset());
it("getMiningDashboard保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getMiningDashboard"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_mining_dashboard"); });
it("getNetworkOverview保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getNetworkOverview"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_network_overview"); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["getMiningDashboard"](...[])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});
