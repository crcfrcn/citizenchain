import {beforeEach, expect, it, vi} from 'vitest';
import {homeNodeApi} from '../control/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke}));
const api=homeNodeApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>mocks.invoke.mockReset());
it("getNodeStatus保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getNodeStatus"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_node_status"); });
it("startNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["startNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("start_node"); });
it("stopNode保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["stopNode"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("stop_node"); });
it("getTotalIssuance保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["getTotalIssuance"](...[])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_total_issuance"); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["getNodeStatus"](...[])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});

import {shouldShowDesktopUpdateDot} from '../control/updateIndicator';
it.each(['checking','unavailable','error'] as const)('状态%s不显示新版标记', status => { expect(shouldShowDesktopUpdateDot({status})).toBe(false); });
it('已发现新版持续显示标记', () => { expect(shouldShowDesktopUpdateDot({status:'available'})).toBe(true); });
