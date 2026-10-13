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
        "name": "NRC_PUBLIC_KEY",
        "title": "NRC_PUBLIC_KEY",
        "description": "国储会现任管理员公钥；开发升级与节点实例部署共用的冷签信任锚",
        "revealable": true,
        "accessLevel": "protected",
        "kind": "authorization",
        "format": "hex32"
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
