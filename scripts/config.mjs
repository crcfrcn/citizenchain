// 本产品配置需求的公开入口；只登记名称、类型和规则，不保存配置值。
export const manifest=Object.freeze({
  "schema": 1,
  "product_id": "citizenchain",
  "configuration": {
    "id": "citizenchain",
    "kind": "chain",
    "title": "公民链配置",
    "items": [
      {
        "name": "NRC_PUBLICKEY",
        "description": "国储会管理员冷签公钥",
        "accessLevel": "protected",
        "kind": "authorization",
        "format": "hex32"
      },
      {
        "name": "CHAIN_GENESISHASH",
        "description": "公民链创世块哈希",
        "accessLevel": "public",
        "kind": "不适用",
        "format": "不适用"
      },
      {
        "name": "APPLE_PASSWORD",
        "description": "苹果公证专用密码",
        "accessLevel": "protected",
        "kind": "不适用",
        "format": "不适用"
      }
    ]
  }
});
export function describeConfiguration(){return JSON.parse(JSON.stringify(manifest));}

// 平台、工具及产物模板只声明数据；编译和GitHub任务由各自入口实现。
export const product=Object.freeze({id:'citizenchain',title:'公民链',repository:'crcfrcn/citizenchain',entry:'scripts/build.mjs',
  platforms:Object.freeze({
    macos:{name:'macOS',workflow:'macos.yml',host:'darwin',arch:'arm64',target:'aarch64-apple-darwin',bundle:'dmg',extension:'.dmg',artifact:'citizenchain-macOS-v{version}.dmg'},
    windows:{name:'Windows',workflow:'windows.yml',host:'win32',arch:'x64',target:'x86_64-pc-windows-msvc',bundle:'nsis',extension:'.exe',artifact:'citizenchain-Windows-v{version}.exe'},
    'linux-arm':{name:'LinuxARM',workflow:'linuxarm.yml',host:'linux',arch:'arm64',target:'aarch64-unknown-linux-gnu',bundle:'deb',extension:'.deb',artifact:'citizenchain-LinuxARM-v{version}.deb'},
    'linux-amd':{name:'LinuxAMD',workflow:'linuxamd.yml',host:'linux',arch:'x64',target:'x86_64-unknown-linux-gnu',bundle:'deb',extension:'.deb',artifact:'citizenchain-LinuxAMD-v{version}.deb'},
    wasm:{name:'WASM',entry:'scripts/runtime.mjs',workflow:'wasm.yml',host:'linux',arch:'x64',target:'wasm32v1-none',files:['runtime.wasm','runtime.compact.wasm','runtime.compact.compressed.wasm']},
  }),
  tools:Object.freeze({node:'25.2.1',rust:'1.97.1',protoc:'35.0',postgres:'17.11',xcode:'27.0'}),
  locks:Object.freeze(['Cargo.lock','crates/protocol/Cargo.lock','crates/scanner/package-lock.json','node/frontend/package-lock.json','onchina/platform/frontend/package-lock.json']),
});

// 节点及 Runtime 发布的公开合同；连接地址和所有机密只由本轮授权输入交付。
export const runtimeUpgrade=Object.freeze({platform:'wasm',files:product.platforms.wasm.files,compressed:'runtime.compact.compressed.wasm',
  secretRefs:[{name:'NRC_PUBLICKEY'},{name:'NODE_WSS'}]});
export const deployment=Object.freeze({platform:'linux-arm',repository:product.repository,product:product.id,workflow:'linuxarm.yml',
  work:'/var/lib/citizenchain-deploy',service:'citizenchain-node.service',package:'citizenchain',executable:'/usr/bin/citizenchain',
  helper:'/usr/local/sbin/citizenchain-deploy',unit:'citizenchain-deploy',data:'/opt/citizenchain/data',
  serviceFile:'/etc/systemd/system/citizenchain-node.service',architecture:'arm64',sshUser:'ubuntu',ready:'deploy-ready:arm64:release'});

// 只返回声明数据；派发与观察归通用调用方，版本及交付归本仓YAML。
export function describeAction({flow,platform,inputs={}}){
 const workflow=product.platforms[platform]?.workflow;
 if(flow==='release'&&workflow)return {repository:product.repository,workflow,ref:'main',inputs};
 throw Error('本产品未声明该流程：'+flow+'·'+platform);
}
