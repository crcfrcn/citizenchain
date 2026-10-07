import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import test from 'node:test';
import vm from 'node:vm';

const require = createRequire(import.meta.url);
const { onTransaction } = require('./index.js');
const ratio = 10n ** 16n;
const show = (fen, extra = {}) => onTransaction({
  chainId: 'eip155:2027',
  transaction: { value: `0x${(fen * ratio).toString(16)}`, ...extra },
});
const feeText = (result) => {
  assert.equal(result.content.type, 'panel');
  assert.equal(result.content.children.length, 2);
  assert.deepEqual(result.content.children[0], { type: 'text', value: '适用费率：0.1%' });
  return result.content.children[1].value;
};

// 小额、最低费边界、半分取整与大额均对照 Runtime 的业务金额。
test('应付手续费遵循最低 10 分和四舍五入，仅展示两项', async () => {
  for (const [amount, expected] of [
    [0n, '0.10'], [1n, '0.10'], [200n, '0.10'],
    [10499n, '0.10'], [10500n, '0.11'], [100000n, '1.00'],
    [(1n << 128n) - 1n, '3402823669209384634633746074317682.11'],
  ]) {
    assert.equal(feeText(await show(amount)), `本笔应付手续费：${expected} GMB`);
  }
});

// 金额编辑必须立即重新计算；钱包 gas 缓冲、费用上限和交易类型不能改写业务费。
test('每次交易独立计算，gas 和优先费字段不影响业务报价', async () => {
  assert.equal(feeText(await show(200n)), '本笔应付手续费：0.10 GMB');
  assert.equal(feeText(await show(100000n, {
    type: '0x2', gas: '0xffff', maxFeePerGas: '0xffff', maxPriorityFeePerGas: '0xffff',
  })), '本笔应付手续费：1.00 GMB');
  assert.equal(feeText(await show(200n)), '本笔应付手续费：0.10 GMB');
});

test('其他链不显示公民链报价', async () => {
  for (const chainId of ['eip155:1', 'eip155:2028', undefined]) {
    assert.equal(await onTransaction({ chainId, transaction: { value: '0x0' } }), null);
  }
});

// 错误不得冒充免费交易或产生第三项 UI。
test('异常编码、非整分和原生金额越界明确拒绝计算', async () => {
  for (const value of ['0x', '0x00', '0X0', '10', '-1', 0, '0x1', `0x${'f'.repeat(65)}`,
    `0x${((1n << 128n) * ratio).toString(16)}`]) {
    const result = await onTransaction({ chainId: 'eip155:2027', transaction: { value } });
    assert.match(feeText(result), /^本笔应付手续费：无法计算：/);
  }
});

test('缺省 value 按零值调用收最低费', async () => {
  assert.equal(feeText(await onTransaction({ chainId: 'eip155:2027', transaction: {} })),
    '本笔应付手续费：0.10 GMB');
});

test('缺失交易或错误结构不能被当成零金额调用', async () => {
  for (const transaction of [undefined, null, [], '0x0']) {
    assert.equal(feeText(await onTransaction({ chainId: 'eip155:2027', transaction })),
      '本笔应付手续费：无法计算：缺少有效交易');
  }
});

// 只读现有真源，防止展示常量和链上金额单位被无意改成另一套制度。
test('费率、最低费、原生精度与 Runtime 真源一致', () => {
  const policy = readFileSync(new URL('../../runtime/primitives/src/fee_policy.rs', import.meta.url), 'utf8');
  const units = readFileSync(new URL('../../runtime/primitives/src/core_const.rs', import.meta.url), 'utf8');
  assert.match(policy, /ONCHAIN_FEE_RATE[^;]*1_000_000/);
  assert.match(policy, /ONCHAIN_MIN_FEE[^;]*10/);
  assert.match(units, /NATIVE_TO_ETH_RATIO == 10_000_000_000_000_000/);
  assert.match(units, /ETHEREUM_CHAIN_ID: u64 = 2027/);
});

// bundle 无 Node 模块、网络或钱包账户权限；在只有 exports 的隔离上下文执行实际入口。
test('发布入口无需依赖或宿主能力，清单摘要覆盖实际代码', async () => {
  const source = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
  const module = { exports: {} };
  const sandbox = { module, exports: module.exports };
  vm.runInNewContext(source, sandbox, { timeout: 1000 });
  const result = await sandbox.exports.onTransaction({
    chainId: 'eip155:2027', transaction: { value: `0x${(200n * ratio).toString(16)}` },
  });
  assert.equal(result.content.children[1].value, '本笔应付手续费：0.10 GMB');
  const manifest = JSON.parse(readFileSync(new URL('./snap.manifest.json', import.meta.url)));
  const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url)));
  assert.equal(manifest.source.shasum, createHash('sha256').update(source).digest('base64'));
  assert.equal(manifest.version, pkg.version);
  assert.equal(manifest.source.location.npm.packageName, pkg.name);
  assert.equal(manifest.source.location.npm.filePath, 'index.js');
  assert.deepEqual(manifest.initialPermissions, { 'endowment:transaction-insight': {} });
  assert.deepEqual(pkg.files, ['index.js', 'snap.manifest.json']);
  assert.equal(pkg.dependencies, undefined);
});
