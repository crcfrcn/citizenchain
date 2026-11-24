//! 第5步隔离候选的真实HTTPS只读验收；无写链操作。

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
