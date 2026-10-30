import {expect,it} from 'vitest';
import {parseQrEnvelope} from '../client/citizenQr';
import fixture from '../../../crates/protocol/fixtures/account_id_code.json';
it('拒绝超大载荷、未知字段、同形账户与错误协议标记',()=>{for(const value of ['x'.repeat(32769),JSON.stringify({...fixture,p:'OTHER'}),JSON.stringify({...fixture,b:{...fixture.b,extra:true}}),JSON.stringify({...fixture,b:{n:'0x'+'Ａ'.repeat(64)}})])expect(()=>parseQrEnvelope(value)).toThrow();});
