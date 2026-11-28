//! 验证隔离节点的HTTPS只读入口和逐钱包收支分页；无写链操作。

#[test]
#[ignore = "第5步交付隔离HTTPS候选后显式执行"]
fn isolated_node_https_genesis_and_health() {
    let endpoint = std::env::var("CITIZENCHAIN_TEST_RPC").expect("缺少隔离候选HTTPS入口");
    let url = reqwest::Url::parse(&endpoint).expect("隔离候选入口格式无效");
    assert!(
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "隔离入口必须是无凭据或参数的HTTPS根地址"
    );
    assert!(
        url.host_str() == Some("127.0.0.1") && url.port().is_some_and(|port| port != 9944),
        "隔离候选只能使用独立回环端口"
    );
    let node = std::env::var("CITIZENCHAIN_TEST_NODEJS").expect("缺少已安装Node入口");
    assert!(std::path::Path::new(&node).is_absolute());
    // 复用产品公开HTTPS客户端并交付公开测试CA；不改变系统信任或关闭主机名核验。
    let status = std::process::Command::new(node)
        .args(["--input-type=module", "--eval", r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {X509Certificate} from 'node:crypto';
import {pathToFileURL} from 'node:url';
const {rpcRequest}=await import(pathToFileURL(process.env.CITIZENCHAIN_RUNTIME_SCRIPT).href);
const text=readFileSync(process.env.CITIZENCHAIN_TEST_CERTIFICATE,'utf8').trim();
assert.ok(/^(?:[0-9a-fA-F]{2})+$/.test(text));
const certificate=new X509Certificate(Buffer.from(text,'hex')).toString();
const rpc=(method,params)=>rpcRequest(process.env.CITIZENCHAIN_TEST_RPC,method,params,{certificate});
assert.equal(await rpc('chain_getBlockHash',[0]),process.env.CITIZENCHAIN_TEST_GENESIS);
const health=await rpc('system_health',[]);
assert.equal(typeof health.isSyncing,'boolean');
console.log('隔离节点的原创世与真实System健康RPC通过');
"#])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("CITIZENCHAIN_TEST_RPC", endpoint)
        .env("CITIZENCHAIN_TEST_CERTIFICATE",
            std::env::var("CITIZENCHAIN_TEST_CERTIFICATE").expect("缺少公开测试证书"))
        .env("CITIZENCHAIN_RUNTIME_SCRIPT", concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/runtime.mjs"))
        .env("CITIZENCHAIN_TEST_GENESIS", format!("0x{}", hex::encode(primitives::genesis::GENESIS_HASH)))
        .status().expect("执行公开HTTPS只读客户端");
    assert!(status.success(), "隔离节点HTTPS只读验收失败");
}

#[test]
fn transaction_history_pagination_preserves_block_items_and_boundaries() {
    use crate::rpc::{transaction_history_page,HistoryBlock};
    use nodeonchain::types::{TransactionRecord,TransactionCursor};
    let block=|number:u32|Ok(HistoryBlock{event_count:25,records:(0..25).map(|index|(index,TransactionRecord{
        block_number:number,timestamp:1,tx_hash:Some(format!("{number}:{index}")),event_index:index,account_id:"0xsynthetic".into(),direction:"sent".into(),mode:"transfer".into(),transfer_count:1,amount_fen:"100".into(),fee_fen:"10".into(),state:"success".into()
    })).collect()});
    let first=transaction_history_page(1,None,block).unwrap();
    assert_eq!(first.records.len(),20);assert_eq!(first.records[0].tx_hash.as_deref(),Some("1:24"));assert_eq!(first.records[19].tx_hash.as_deref(),Some("1:5"));
    let cursor=first.cursor.unwrap();assert_eq!(cursor.block_number,1);assert_eq!(cursor.event_index,Some(4));
    let next=transaction_history_page(1,Some(cursor),block).unwrap();
    assert_eq!(next.records.len(),5);assert_eq!(next.records[0].tx_hash.as_deref(),Some("1:4"));assert_eq!(next.records[4].tx_hash.as_deref(),Some("1:0"));assert!(next.cursor.is_none());
    assert!(transaction_history_page(1,Some(TransactionCursor{block_number:2,event_index:None}),block).is_err());
    assert!(transaction_history_page(1,Some(TransactionCursor{block_number:1,event_index:Some(25)}),block).is_err());
}

#[test]
fn transaction_history_pagination_keeps_both_wallet_sides_at_page_boundary() {
    use crate::rpc::{transaction_history_page,HistoryBlock};
    use nodeonchain::types::TransactionRecord;
    let block=|number:u32|{
        let mut records=Vec::new();
        for index in 0..21 {
            for direction in if index==1 {vec!["received","sent"]} else {vec!["received"]} {
                records.push((index,TransactionRecord{block_number:number,timestamp:1,tx_hash:None,event_index:index,account_id:direction.into(),direction:direction.into(),mode:"transfer".into(),transfer_count:1,amount_fen:"100".into(),fee_fen:"0".into(),state:"success".into()}));
            }
        }
        Ok(HistoryBlock{event_count:21,records})
    };
    // 第20条和21条属于同一事件的两个钱包，必须一起交付而不跳过其一。
    let first=transaction_history_page(1,None,block).unwrap();assert_eq!(first.records.len(),21);
    assert_eq!(first.records.iter().filter(|row|row.event_index==1).count(),2);
    let cursor=first.cursor.unwrap();assert_eq!(cursor.event_index,Some(0));
    let second=transaction_history_page(1,Some(cursor),block).unwrap();assert_eq!(second.records.len(),1);assert_eq!(second.records[0].event_index,0);assert!(second.cursor.is_none());
}

#[test]
fn transaction_history_empty_pages_are_bounded_and_do_not_hide_read_failures() {
    use crate::rpc::{transaction_history_page,HistoryBlock};
    let mut read=Vec::new();
    let page=transaction_history_page(100,None,|number|{read.push(number);Ok(HistoryBlock{event_count:1,records:vec![]})}).unwrap();
    assert_eq!(read,(85..=100).rev().collect::<Vec<_>>());assert_eq!(page.cursor.unwrap().block_number,84);
    assert!(transaction_history_page(10,None,|_|Err("合成归档读取失败".into())).is_err());
    let page=transaction_history_page(0,None,|_|panic!("创世起点没有需要读取的交易")).unwrap();assert!(page.records.is_empty());assert!(page.cursor.is_none());
}
