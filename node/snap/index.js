(() => {
'use strict';

// 费率为 Runtime 永久常量 0.1%；金额、最低费与四舍五入均使用整数分。
const NATIVE_TO_ETH_RATIO = 10n ** 16n;
const MAX_NATIVE_AMOUNT = (1n << 128n) - 1n;

exports.onTransaction = async ({ transaction, chainId }) => {
  if (chainId !== 'eip155:2027') return null;

  let payable;
  try {
    if (!transaction || typeof transaction !== 'object' || Array.isArray(transaction)) {
      throw new Error('缺少有效交易');
    }
    const value = transaction?.value ?? '0x0';
    // 拒绝非规范数量、超过 EVM 宽度、非整分或超出原生账本的金额。
    if (typeof value !== 'string' || !/^0x(?:0|[1-9a-fA-F][0-9a-fA-F]{0,63})$/.test(value)) {
      throw new Error('交易金额格式无效');
    }
    const wei = BigInt(value);
    if (wei % NATIVE_TO_ETH_RATIO !== 0n) throw new Error('交易金额必须为整分');
    const amount = wei / NATIVE_TO_ETH_RATIO;
    if (amount > MAX_NATIVE_AMOUNT) throw new Error('交易金额超出账本范围');
    // 与 calculate_onchain_fee 的整数四舍五入一致，最低收取 10 分。
    const rounded = (amount + 500n) / 1000n;
    const fee = rounded < 10n ? 10n : rounded;
    payable = `${fee / 100n}.${(fee % 100n).toString().padStart(2, '0')} GMB`;
  } catch (error) {
    // 无效金额不能展示为零费用；错误仍占用同一个应付手续费字段。
    payable = `无法计算：${error.message}`;
  }

  // 使用官方序列化 UI 协议，确认窗口严格只包含用户要求的两项。
  return {
    content: {
      type: 'panel',
      children: [
        { type: 'text', value: '适用费率：0.1%' },
        { type: 'text', value: `本笔应付手续费：${payable}` },
      ],
    },
  };
};
})();
