import {expect,it} from 'vitest';
import {formatBalance,formatAmount} from '../client/format';
import {accountIdToSs58,normalizeSs58Address} from '../client/ss58';
import {sanitizeError} from '../client/tauri';
it.each([['0','0.00 元'],['-1','-0.01 元'],['123456789012345678901','1,234,567,890,123,456,789.01 元']])('分转元保持整数精度 %s',(input,output)=>expect(formatBalance(input)).toBe(output));
it('空金额和单位保留原显示合同',()=>{expect(formatAmount(null)).toBeNull();expect(formatAmount('  ')).toBeNull();expect(formatAmount('123456.00 GMB')).toBe('123,456.00 GMB');expect(()=>formatBalance('1.1')).toThrow();});
it('账户编码与严格地址验证往返一致',()=>{for(let n=0;n<32;n++){const id='0x'+n.toString(16).padStart(2,'0').repeat(32),address=accountIdToSs58(id);expect(normalizeSs58Address(' '+address+' ')).toBe(address);expect(()=>normalizeSs58Address(address.slice(0,-1)+(address.endsWith('1')?'2':'1'))).toThrow();}});
it.each(['','0x01','0x'+'AA'.repeat(32)])('账户ID拒绝非规范输入%s',input=>expect(accountIdToSs58(input)).toBe('无效账户 ID'));
it('错误显示保留500字符上限',()=>{expect(sanitizeError(new Error('中'.repeat(501)))).toBe('中'.repeat(500)+'...');expect(sanitizeError('失败')).toBe('失败');});
