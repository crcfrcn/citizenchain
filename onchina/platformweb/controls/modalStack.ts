// 定义业务弹窗及钱包签名弹窗的层叠顺序。

export const CID_MODAL_Z_INDEX = {
  business: 1000,
  accountScan: 1600,
  securitySignature: 3000,
} as const;
