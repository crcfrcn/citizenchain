import {it,expectTypeOf} from 'vitest';
import type {ResolutionIssuanceDetail} from '../proposal/resolutionissue';
it('既有返回合同使用准确字段及字符串分金额',()=>{expectTypeOf<ResolutionIssuanceDetail>().toEqualTypeOf<{proposalId:number;actorCidNumber:string;proposerRoleCode:string;proposerAccountId:string;reason:string;totalAmountFen:string;allocations:{recipientAccountId:string;amountFen:string}[]}>();});
