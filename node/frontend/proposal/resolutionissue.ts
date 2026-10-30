/** 节点既有决议发行详情返回合同；本模块仅声明类型。 */
export type IssuanceAllocationItem = { recipientAccountId: string; amountFen: string };
export type ResolutionIssuanceDetail = { proposalId: number; actorCidNumber: string; proposerRoleCode: string; proposerAccountId: string; reason: string; totalAmountFen: string; allocations: IssuanceAllocationItem[] };
