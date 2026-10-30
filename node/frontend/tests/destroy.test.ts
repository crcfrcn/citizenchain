import {it,expectTypeOf} from 'vitest';
import type {ResolutionDestroyDetail} from '../proposal/destroy';
it('既有返回合同使用准确字段及字符串分金额',()=>{expectTypeOf<ResolutionDestroyDetail>().toEqualTypeOf<{proposalId:number;actorCidNumber:string;institutionAccountId:string;amountFen:string}>();});
