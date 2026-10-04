// CID 前端弹窗 z-index 统一表。业务弹窗保持在底层,
// 公民钱包扫码签名弹窗必须盖住所有编辑/确认弹窗。
export const CID_MODAL_Z_INDEX = {
  business: 1000,
  accountScan: 1600,
  securitySignature: 3000,
} as const;
