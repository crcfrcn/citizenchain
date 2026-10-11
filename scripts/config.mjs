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
        "environment": "rtupg",
        "name": "NRC_SIGNER_PUBLIC_KEY",
        "title": "NRC_SIGNER_PUBLIC_KEY",
        "description": "公民协议开发升级冷签公钥",
        "revealable": true,
        "accessLevel": "public"
      },
      {
        "environment": "rtupg",
        "name": "CHAIN_GENESIS_HASH",
        "title": "CHAIN_GENESIS_HASH",
        "description": "公民链固定创世块哈希",
        "revealable": true,
        "accessLevel": "public"
      }
    ]
  }
});
export function describeConfiguration(){return JSON.parse(JSON.stringify(manifest));}
