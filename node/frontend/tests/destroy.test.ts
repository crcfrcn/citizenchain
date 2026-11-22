// 验证决议销毁详情保留准确字段和字符串分金额。

import {it,expectTypeOf} from 'vitest';
import type {ResolutionDestroyDetail} from '../proposal/destroy';
it('既有返回合同使用准确字段及字符串分金额',()=>{expectTypeOf<ResolutionDestroyDetail>().toEqualTypeOf<{proposalId:string;actorCidNumber:string;institutionAccountId:string;amountFen:string}>();});
