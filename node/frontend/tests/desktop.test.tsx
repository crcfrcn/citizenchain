// 验证签名弹窗的挂载、内部点击和外部取消交互。

import {expect,it,vi} from 'vitest';
import {fireEvent,render,screen,within} from '@testing-library/react';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {CitizenSignatureModal} from "../desktop/signing";
import {TransferForm} from '../onchain/TransferForm';
// 仅替换摄像头组件，其余公开导出使用真实实现。
vi.mock('@gmb/scanner-react', async (importOriginal) => ({
  ...await importOriginal<typeof import('@gmb/scanner-react')>(),
  ScannerView: () => <p>签名内容</p>,
}));
it('隐藏弹窗不挂载，内部点击不会取消，外部和关闭按钮保留取消动作',()=>{const onCancel=vi.fn(),props={title:'签名',qrValue:'合成请求',onScan:vi.fn(),onScanError:vi.fn(),onCancel};const view=render(<CitizenSignatureModal {...props} open={false}/>);expect(view.container.childElementCount).toBe(0);view.rerender(<CitizenSignatureModal {...props} open/>);fireEvent.click(screen.getByText('签名内容'));expect(onCancel).not.toHaveBeenCalled();fireEvent.click(view.container.querySelector('.citizen-signature-modal-overlay')!);fireEvent.click(screen.getByRole('button',{name:'×'}));expect(onCancel).toHaveBeenCalledTimes(2);});

it('批量默认组及新增组的真实扫码弹层均高于批量弹窗，关闭扫码后保留原弹窗',()=>{
 const style=document.createElement('style');style.textContent=readFileSync(resolve(process.cwd(),'desktop/global.css'),'utf8');document.head.append(style);
 const view=render(<TransferForm activeWallet={{name:'合成钱包',signMode:'cold',accountId:'0x'+'11'.repeat(32)}} balance="100000" onSubmit={vi.fn()}/>);
 try{
  fireEvent.click(screen.getByRole('button',{name:'批量转账'}));fireEvent.click(screen.getByRole('button',{name:'添加'}));
  for(const index of [1,2]){
   fireEvent.click(screen.getByTitle(`扫描第${index}组地址`));
   const camera=view.container.querySelector('.address-scan-modal-mask')!,batch=view.container.querySelector('.wallet-manager-modal-mask')!;
   expect(camera.parentElement).toBe(batch.parentElement);
   expect(Number(getComputedStyle(camera).zIndex)).toBeGreaterThan(Number(getComputedStyle(batch).zIndex));
   expect(within(camera as HTMLElement).getByText('签名内容')).toBeTruthy();
   fireEvent.click(within(camera as HTMLElement).getByRole('button',{name:'取消'}));
   expect(view.container.querySelector('.address-scan-modal-mask')).toBeNull();expect(screen.getByRole('dialog')).toBeTruthy();
   expect(screen.getByLabelText(`第${index}组金额`)).toBeTruthy();
  }
 }finally{view.unmount();style.remove();}
});
