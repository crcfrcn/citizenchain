// 验证清算行端点、登记和锁钥命令及失败不重放。

import {beforeEach, expect, it, vi} from 'vitest';
import {offchainApi} from '../offchain/api';
const mocks=vi.hoisted(()=>({invoke:vi.fn()}));
vi.mock('../client/tauri',()=>({invoke:mocks.invoke,sanitizeError:String}));
const api=offchainApi as unknown as Record<string,(...args:unknown[])=>Promise<unknown>>;
beforeEach(()=>mocks.invoke.mockReset());
it("testClearingBankEndpointConnectivity保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["testClearingBankEndpointConnectivity"](...["test.example",443,"peer"])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("test_clearing_bank_endpoint_connectivity", {"domain":"test.example","port":443,"expectedPeerId":"peer"}); });
it.each([
 {method:'buildRegisterClearingBankRequest',command:'build_register_clearing_bank_request',args:['public','44','ROLE_B','peer','test.example',443],payload:{signerPublicKey:'public',actorCidNumber:'44',actorRoleCode:'ROLE_B',peerId:'peer',rpcDomain:'test.example',rpcPort:443}},
 {method:'submitRegisterClearingBank',command:'submit_register_clearing_bank',args:['request','public','hash','44','ROLE_B','peer','test.example',443,3,0,'response'],payload:{requestId:'request',expectedPublicKey:'public',expectedPayloadHash:'hash',actorCidNumber:'44',actorRoleCode:'ROLE_B',peerId:'peer',rpcDomain:'test.example',rpcPort:443,signNonce:3,signBlockNumber:0,responseJson:'response'}},
 {method:'buildUpdateClearingBankEndpointRequest',command:'build_update_clearing_bank_endpoint_request',args:['public','44','ROLE_B','test.example',443],payload:{signerPublicKey:'public',actorCidNumber:'44',actorRoleCode:'ROLE_B',newDomain:'test.example',newPort:443}},
 {method:'submitUpdateClearingBankEndpoint',command:'submit_update_clearing_bank_endpoint',args:['request','public','hash','44','ROLE_B','test.example',443,3,0,'response'],payload:{requestId:'request',expectedPublicKey:'public',expectedPayloadHash:'hash',actorCidNumber:'44',actorRoleCode:'ROLE_B',newDomain:'test.example',newPort:443,signNonce:3,signBlockNumber:0,responseJson:'response'}},
 {method:'buildUnregisterClearingBankRequest',command:'build_unregister_clearing_bank_request',args:['public','44','ROLE_B'],payload:{signerPublicKey:'public',actorCidNumber:'44',actorRoleCode:'ROLE_B'}},
 {method:'submitUnregisterClearingBank',command:'submit_unregister_clearing_bank',args:['request','public','hash','44','ROLE_B',3,0,'response'],payload:{requestId:'request',expectedPublicKey:'public',expectedPayloadHash:'hash',actorCidNumber:'44',actorRoleCode:'ROLE_B',signNonce:3,signBlockNumber:0,responseJson:'response'}},
])('$method把明确所选岗位传至原生构建和提交，不使用默认岗位',async({method,command,args,payload})=>{
 const receipt={receipt:'合成回执'};mocks.invoke.mockResolvedValueOnce(receipt);await expect(api[method](...args)).resolves.toBe(receipt);expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith(command,payload);
});
it("lockClearingDecrypt保留唯一IPC合同且返回真实回执", async () => { const receipt = { receipt: '合成回执' }; mocks.invoke.mockResolvedValueOnce(receipt); await expect(api["lockClearingDecrypt"](...["public"])).resolves.toBe(receipt); expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("lock_clearing_decrypt", {"signerPublicKey":"public"}); });
it('IPC失败保留拒绝状态且不自动重放', async()=>{const failure=new Error('合成失败');mocks.invoke.mockRejectedValueOnce(failure);await expect(api["testClearingBankEndpointConnectivity"](...["test.example",443,"peer"])).rejects.toBe(failure);expect(mocks.invoke).toHaveBeenCalledOnce();});


import {fireEvent, render, screen, waitFor} from '@testing-library/react';
import {ClearingBankDeclareNodePage} from '../offchain/node-register';
vi.mock('../desktop/signing',()=>({CitizenSignatureModal:({open,onScan}:{open:boolean;onScan:(v:string)=>void})=>open?<button onClick={()=>onScan('fixture-response')}>扫描回执</button>:null}));

it('同人同钱包的不同岗位分别选择，换绑失效的授权不出现',async()=>{
 const accountId='0x'+'01'.repeat(32);
 const roles=['R1','R2','R3'].map(roleCode=>({roleCode,roleName:roleCode,seats:1,active:true,termRequired:false,permissions:[{moduleTag:'offchain',actionCode:50,operation:0}],assignments:[{personCid:'person',accountId,bindingRevision:'2',active:true,effective:true}]}));
 mocks.invoke.mockImplementation(async(command:string)=>{
  if(command==='query_local_peer_id')return 'peer';
  if(command==='get_role_access')return roles.map(r=>({cidNumber:'fixture',roleCode:r.roleCode,personCid:'person',accountId,bindingRevision:r.roleCode==='R3'?'1':'2'}));
  if(command==='get_institution_roles')return {cidNumber:'fixture',roles};
 });
 const view=render(<ClearingBankDeclareNodePage cidNumber="fixture" cidFullName="合成机构" onBack={()=>{}} onSuccess={()=>{}}/>);
 try{
  const select=await screen.findByRole('combobox');
  expect([...select.querySelectorAll('option')].map(x=>x.value)).toEqual(['','person:R1','person:R2']);
  fireEvent.change(select,{target:{value:'person:R2'}});
  expect((select as HTMLSelectElement).value).toBe('person:R2');
 }finally{view.unmount();}
});

it('岗位读取失败明确显示错误且不提供可签名授权',async()=>{
 mocks.invoke.mockImplementation(async(command:string)=>{if(command==='get_institution_roles')throw new Error('岗位读取失败');return [];});
 const view=render(<ClearingBankDeclareNodePage cidNumber="fixture" cidFullName="合成机构" onBack={()=>{}} onSuccess={()=>{}}/>);
 try{
  await waitFor(()=>expect(screen.getByText('Error: 岗位读取失败')).toBeTruthy());
  expect((screen.getByRole('button',{name:'扫码签名并提交'}) as HTMLButtonElement).disabled).toBe(true);
 }finally{view.unmount();}
});


it('节点登记从岗位选择到构建及回执提交始终使用同一岗位',async()=>{
 const accountId='0x'+'01'.repeat(32);
 const roles=['R1','R2'].map(roleCode=>({roleCode,roleName:roleCode,seats:1,active:true,termRequired:false,permissions:[{moduleTag:'offchain',actionCode:50,operation:0}],assignments:[{personCid:'person',accountId,bindingRevision:'2',active:true,effective:true}]}));
 mocks.invoke.mockImplementation(async(command:string)=>{
  if(command==='query_local_peer_id')return 'peer';
  if(command==='get_role_access')return roles.map(r=>({cidNumber:'fixture',roleCode:r.roleCode,personCid:'person',accountId,bindingRevision:'2'}));
  if(command==='get_institution_roles')return{cidNumber:'fixture',roles};
  if(command==='test_clearing_bank_endpoint_connectivity')return{allOk:true,checks:[]};
  if(command==='build_register_clearing_bank_request')return{requestId:'request',requestJson:'fixture-request',expectedPayloadHash:'hash',signNonce:3,signBlockNumber:0};
  if(command==='submit_register_clearing_bank')throw new Error('合成提交拒绝');
 });
 const view=render(<ClearingBankDeclareNodePage cidNumber="fixture" cidFullName="合成机构" onBack={()=>{}} onSuccess={()=>{}}/>);
 try{
  fireEvent.change(await screen.findByRole('combobox'),{target:{value:'person:R2'}});
  fireEvent.change(screen.getByRole('textbox'),{target:{value:'rpc.example.com'}});
  fireEvent.click(screen.getByRole('button',{name:'运行连通性自测'}));
  await waitFor(()=>expect((screen.getByRole('button',{name:'扫码签名并提交'}) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole('button',{name:'扫码签名并提交'}));
  fireEvent.click(await screen.findByRole('button',{name:'扫描回执'}));
  await screen.findByText('Error: 合成提交拒绝');
  expect(mocks.invoke).toHaveBeenCalledWith('build_register_clearing_bank_request',expect.objectContaining({actorCidNumber:'fixture',actorRoleCode:'R2',signerPublicKey:accountId}));
  expect(mocks.invoke).toHaveBeenCalledWith('submit_register_clearing_bank',expect.objectContaining({actorCidNumber:'fixture',actorRoleCode:'R2',expectedPublicKey:accountId,requestId:'request',responseJson:'fixture-response'}));
  expect(mocks.invoke.mock.calls.filter(([c])=>c==='submit_register_clearing_bank')).toHaveLength(1);
 }finally{view.unmount();}
});
