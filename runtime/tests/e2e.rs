//! 真实节点的只读端到端：TLS -> RPC -> 同一finalized状态 -> 实际Runtime API与候选代码。
//! 第5步由控制台交付隔离节点、公开证书和准确候选WASM；条件不足必须失败。
use std::{env, fs, path::Path, process::Command};

const DRIVER: &str = r#"
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {pathToFileURL} from 'node:url';
const owner=await import(pathToFileURL(process.env.CITIZENCHAIN_RUNTIME_SCRIPT).href);
const endpoint=new URL(process.env.CITIZENCHAIN_TEST_RPC);
assert.ok(endpoint.protocol==='https:' && endpoint.hostname==='127.0.0.1' &&
  endpoint.port && endpoint.port!=='9944' && endpoint.pathname==='/' &&
  !endpoint.search && !endpoint.username && !endpoint.password && !endpoint.hash,
  '仅允许无凭据的隔离回环HTTPS根端点');
const text=readFileSync(process.env.CITIZENCHAIN_TEST_CERTIFICATE,'utf8').trim();
assert.ok(/^(?:[0-9a-fA-F]{2})+$/.test(text),'公开测试证书格式无效');const certificate=Buffer.from(text,'hex');
const rpc=(method,params)=>owner.rpcRequest(endpoint.href,method,params,{certificate});
const deadline=setTimeout(()=>{process.stderr.write('端到端读取超时\n');process.exit(1);},60000);
try{
  const base=await owner.fetchRuntimeBase(rpc,process.env.CITIZENCHAIN_TEST_GENESIS);
  const at=base.blockHash;
  const identity=await rpc('state_call',['ChainIdentityApi_genesis_hash','0x',at]);
  assert.equal(identity,process.env.CITIZENCHAIN_TEST_GENESIS);
  const codeHash=await rpc('state_getStorageHash',['0x3a636f6465',at]);
  assert.equal(codeHash,process.env.CITIZENCHAIN_TEST_CODE_HASH,'测试节点未运行本次准确候选');
  const version=owner.readRuntimeWasmVersion(readFileSync(process.env.CITIZENCHAIN_TEST_WASM));
  assert.equal(base.specVersion,version.specVersion);
  assert.equal(base.transactionVersion,version.transactionVersion);
  console.log('同一finalized锚点的实际Runtime身份、代码与双版本一致',at);
}finally{clearTimeout(deadline);}
"#;

#[test]
#[ignore = "第5步由控制台显式交付隔离HTTPS节点、公开证书及准确候选后执行"]
fn finalized_runtime_api_and_exact_wasm_match_over_verified_https() {
    let required =
        |name: &str| env::var(name).unwrap_or_else(|_| panic!("缺少独立端到端输入：{name}"));
    let node = required("CITIZENCHAIN_TEST_NODEJS");
    assert!(Path::new(&node).is_absolute() && Path::new(&node).is_file());
    let endpoint = required("CITIZENCHAIN_TEST_RPC");
    let certificate = required("CITIZENCHAIN_TEST_CERTIFICATE");
    let wasm = required("CITIZENCHAIN_TEST_WASM");
    let metadata = fs::symlink_metadata(&wasm).expect("候选必须可读");
    assert!(
        metadata.is_file()
            && !metadata.file_type().is_symlink()
            && metadata.len() <= 64 * 1024 * 1024
    );
    assert_eq!(
        Path::new(&wasm).extension().and_then(|v| v.to_str()),
        Some("wasm")
    );
    let code = fs::read(&wasm).expect("读取准确候选WASM");
    let status = Command::new(node)
        .args(["--input-type=module", "--eval", DRIVER])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env(
            "CITIZENCHAIN_RUNTIME_SCRIPT",
            concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/runtime.mjs"),
        )
        .env("CITIZENCHAIN_TEST_RPC", endpoint)
        .env("CITIZENCHAIN_TEST_CERTIFICATE", certificate)
        .env("CITIZENCHAIN_TEST_WASM", wasm)
        .env(
            "CITIZENCHAIN_TEST_GENESIS",
            format!("0x{}", hex::encode(primitives::genesis::GENESIS_HASH)),
        )
        .env(
            "CITIZENCHAIN_TEST_CODE_HASH",
            format!("0x{}", hex::encode(sp_io::hashing::blake2_256(&code))),
        )
        .status()
        .expect("通过控制台交付的Node执行端到端驱动");
    assert!(status.success(), "真实端到端未通过");
}
