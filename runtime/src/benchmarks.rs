// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

frame_benchmarking::define_benchmarks!(
    [frame_benchmarking, BaselineBench::<Runtime>]
    [frame_system, SystemBench::<Runtime>]
    [frame_system_extensions, SystemExtensionsBench::<Runtime>]
    [pallet_balances, Balances]
    [pallet_revive, Revive]
    [pallet_timestamp, Timestamp]
    [prbbankissue, ProvincialBankInterest]
    [fullnodeissue, FullnodeIssuance]
    [citizenissue, CitizenIssuance]
    [resolutionissue, ResolutionIssuance]
    [identity, CitizenIdentity]
    [difficulty, PowDifficulty]
    [onchain, OnchainTransaction]
    [address, AddressRegistry]
    // public/privateadmin 已无外部 extrinsic 和 WeightInfo，仅由 entity 内部同步管理员账户集合。
    [destroy, ResolutionDestroy]
    [grandpakey, GrandpaKeyChange]
    [publicentity, PublicManage]
    [privateentity, PrivateManage]
    // personalentity / personaladmin benchmark 用例待 follow-up;当前 benchmarks.rs 是空骨架,
    // 不挂载到 list_benchmarks 避免 Benchmarking trait 缺失编译错误。
    // [personaladmin, PersonalAdmins]
    [multisig, MultisigTransfer]
    [internalvote, InternalVote]
    [jointvote, JointVote]
    [votingengine, VotingEngine]
    [legislationvote, LegislationVote]
    [electionvote, ElectionVote]
    [square, SquarePost]
    [upgrade, RuntimeUpgrade]
);
