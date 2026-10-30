import {expect,it} from 'vitest';
import {accountIdToSs58,normalizeSs58Address} from '../client/ss58';
// 第5步固定机器预算验收；未提供预算明确跳过，普通单测不冒充性能达标。
const budget=process.env.CITIZENCHAIN_FRONTEND_PERF_MS;
it.skipIf(budget===undefined)('一千次地址编解码成本符合显式预算',()=>{const maximum=Number(budget);expect(Number.isFinite(maximum)&&maximum>0).toBe(true);const start=performance.now();for(let n=0;n<1000;n++){const address=accountIdToSs58('0x'+(n%256).toString(16).padStart(2,'0').repeat(32));expect(normalizeSs58Address(address)).toBe(address);}expect(performance.now()-start).toBeLessThanOrEqual(maximum);});
