import {expect,it,vi} from 'vitest';
import {councilApi} from '../institution/council';
import {proposalApi} from '../proposal/api';
const mock=vi.hoisted(()=>({invoke:vi.fn()}));vi.mock('../client/tauri',()=>({invoke:mock.invoke}));
it('机构入口和提案入口按同一CID协作且分别使用所属命令',async()=>{mock.invoke.mockResolvedValueOnce({cidNumber:'44'}).mockResolvedValueOnce({items:[],hasMore:false,warning:null});await expect(councilApi.getInstitutionDetail('44')).resolves.toMatchObject({cidNumber:'44'});await expect(proposalApi.getInstitutionProposalPage('44',0,20)).resolves.toEqual({items:[],hasMore:false,warning:null});expect(mock.invoke.mock.calls).toEqual([['get_institution_detail',{cidNumber:'44'}],['get_institution_proposal_page',{cidNumber:'44',startId:0,count:20}]]);});
