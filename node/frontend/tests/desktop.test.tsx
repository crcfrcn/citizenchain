import {expect,it,vi} from 'vitest';
import {fireEvent,render,screen} from '@testing-library/react';
import {CitizenSignatureModal} from "../desktop/signing";
vi.mock('@gmb/scanner-react',()=>({ScannerView:()=> <p>签名内容</p>}));
it('隐藏弹窗不挂载，内部点击不会取消，外部和关闭按钮保留取消动作',()=>{const onCancel=vi.fn(),props={title:'签名',qrValue:'合成请求',onScan:vi.fn(),onScanError:vi.fn(),onCancel};const view=render(<CitizenSignatureModal {...props} open={false}/>);expect(view.container.childElementCount).toBe(0);view.rerender(<CitizenSignatureModal {...props} open/>);fireEvent.click(screen.getByText('签名内容'));expect(onCancel).not.toHaveBeenCalled();fireEvent.click(view.container.querySelector('.citizen-signature-modal-overlay')!);fireEvent.click(screen.getByRole('button',{name:'×'}));expect(onCancel).toHaveBeenCalledTimes(2);});
