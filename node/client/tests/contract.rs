//! 验证System账户存储键的固定布局。

use crate::query::system_account_key;

#[test]
fn system_account_key_has_correct_length() {
    let account_id = "0xa5423e483bba281da84b99620a670718d5a7eceb5ae720f7d492e8b5c2570d84";
    let key = system_account_key(account_id).unwrap();
    // 0x 前缀 + (16+16+16+32)*2 hex 字符 = 2 + 160 = 162
    assert_eq!(key.len(), 162);
}

#[test]
fn transaction_history_reads_actual_batch_receipts_without_guard_dependencies(){
    use codec::{Decode,Encode};
    use sp_core::{Pair,sr25519};
    use sp_runtime::{BuildStorage,traits::Dispatchable};
    for atomic in [false,true]{
        let storage=runtime::RuntimeGenesisConfig::default().build_storage().unwrap();
        sp_io::TestExternalities::new(storage).execute_with(||{
            let pair=sr25519::Pair::from_seed(&[79;32]);let payer=chain_signing::account_id_from_public(pair.public());
            let recipient=sp_runtime::AccountId32::new([203;32]);
            runtime::System::set_block_number(1);
            runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),payer.clone().into(),50_000).unwrap();
            runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),recipient.clone().into(),111).unwrap();
            runtime::System::reset_events();runtime::System::note_finished_initialize();
            // 从Runtime公开调用编码构造合成批次，不改变Runtime或守卫。
            let make=|to:sp_runtime::AccountId32,amount:u128|(4u8,0u8,to,amount,Vec::<u8>::new()).encode();
            let mut encoded=vec![27,if atomic{2}else{4}];encoded.extend(codec::Compact(3u32).encode());
            for (to,amount) in [(recipient.clone(),100u128),(sp_runtime::AccountId32::new([204;32]),1),(recipient,10_500)]{encoded.extend(make(to,amount));}
            let call=runtime::RuntimeCall::decode(&mut encoded.as_slice()).unwrap();
            let body=vec![chain_signing::build_signed_extrinsic_local(call.clone(),sp_core::H256::repeat_byte(1),0,&pair).into()];
            let result=call.dispatch(runtime::RuntimeOrigin::signed(payer));runtime::System::note_applied_extrinsic(&result,Default::default());
            let raw=runtime::System::events().encode();let metadata=runtime::Runtime::metadata_at_version(16).unwrap();
            let sent=crate::codec::transaction_activity(&body,&raw,metadata.as_slice(),&[pair.public().0,pair.public().0]).unwrap();
            assert_eq!(sent.len(),1);assert_eq!(sent[0].amount_fen,10_601);assert_eq!(sent[0].transfer_count,3);
            assert_eq!(sent[0].state,if atomic{"rolled_back"}else{"partial"});assert_eq!(sent[0].fee_fen,if atomic{0}else{21});
            let incoming=crate::codec::transaction_activity(&body,&raw,metadata.as_slice(),&[[203;32]]).unwrap();
            if atomic{assert!(incoming.is_empty());}else{assert_eq!(incoming.len(),1);assert_eq!(incoming[0].amount_fen,10_600);assert_eq!(incoming[0].fee_fen,0);}
            assert!(crate::codec::transaction_activity(&body,&raw,metadata.as_slice(),&[[205;32]]).unwrap().is_empty());
        });
    }
}

#[test]
fn transaction_history_reads_signed_v4_with_declared_parent_types(){
    use codec::{Decode,Encode};
    use sp_core::{Pair,sr25519};
    use sp_runtime::{BuildStorage,traits::Dispatchable,traits::TransactionExtension};
    let storage=runtime::RuntimeGenesisConfig::default().build_storage().unwrap();
    sp_io::TestExternalities::new(storage).execute_with(||{
        let pair=sr25519::Pair::from_seed(&[80;32]);let payer=chain_signing::account_id_from_public(pair.public());let recipient=sp_runtime::AccountId32::new([206;32]);
        runtime::System::set_block_number(1);runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),payer.clone().into(),50_000).unwrap();runtime::Balances::force_set_balance(runtime::RuntimeOrigin::root(),recipient.clone().into(),111).unwrap();runtime::System::reset_events();runtime::System::note_finished_initialize();
        let call=runtime::RuntimeCall::decode(&mut (4u8,0u8,recipient,1000u128,Vec::<u8>::new()).encode().as_slice()).unwrap();
        let material=chain_signing::build_signing_material_from_call(call.clone(),sp_core::H256::repeat_byte(1),0,3,2);let signature=chain_signing::sign_material_with_pair(&material,&pair);
        let legacy=sp_runtime::generic::UncheckedExtrinsic::<runtime::TransactionAddress,runtime::RuntimeCall,runtime::Signature,runtime::TxExtension>::new_signed(material.call,sp_runtime::MultiAddress::Id(payer.clone()),signature.into(),material.tx_ext);
        let body=vec![sp_runtime::OpaqueExtrinsic::try_from_encoded_extrinsic(&legacy.encode()).unwrap()];
        let result=call.dispatch(runtime::RuntimeOrigin::signed(payer));runtime::System::note_applied_extrinsic(&result,Default::default());
        let published=runtime::Runtime::metadata_at_version(15).unwrap();let frame_metadata::RuntimeMetadata::V15(mut parent)=crate::codec::decode_runtime_metadata(published.as_slice()).unwrap() else{panic!("需要V15元数据")};
        // 用SDK公开的历史类型声明父交易格式；不引用守卫或重算收费规则。
        let address=parent.types.types.iter().find(|ty|ty.ty.path.segments.last().is_some_and(|name|name=="MultiAddress")).expect("公开地址类型").id;
        let signature=parent.types.types.iter().find(|ty|ty.ty.path.segments.last().is_some_and(|name|name=="MultiSignature")).expect("公开签名类型").id;
        parent.extrinsic.version=4;parent.extrinsic.address_ty=address.into();parent.extrinsic.signature_ty=signature.into();parent.extrinsic.signed_extensions.retain(|extension|extension.identifier!=<chain_signing::mldsa::Authenticate<runtime::Runtime> as TransactionExtension<runtime::RuntimeCall>>::IDENTIFIER);
        let metadata=frame_metadata::RuntimeMetadataPrefixed(frame_metadata::META_RESERVED,frame_metadata::RuntimeMetadata::V15(parent)).encode();
        let records=crate::codec::transaction_activity(&body,&runtime::System::events().encode(),&metadata,&[pair.public().0]).unwrap();assert_eq!(records.len(),1);assert_eq!(records[0].direction,"sent");assert_eq!(records[0].amount_fen,1000);assert_eq!(records[0].fee_fen,10);
    });
}
