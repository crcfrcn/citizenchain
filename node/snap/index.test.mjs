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

// 执行实际接入页脚本，检查官方钱包方法、授权时机和失败状态；不冒充钱包页面验收。
const installPage = ({ href = 'https://localhost:9443/node/snap/install.html', provider, response } = {}) => {
  const html = readFileSync(new URL('./install.html', import.meta.url), 'utf8');
  const source = html.match(/<script>([\s\S]*?)<\/script>/)[1];
  const elements = Object.fromEntries(['rpc', 'add-network', 'install-snap', 'status'].map(id => [id, {
    value: '', textContent: '', disabled: false, handlers: {},
    addEventListener(name, callback) { this.handlers[name] = callback; },
  }]));
  const calls = [];
  const listeners = {};
  const manifest = JSON.parse(readFileSync(new URL('./snap.manifest.json', import.meta.url)));
  const fakeProvider = provider ?? {
    isMetaMask: true,
    async request(request) {
      calls.push(request);
      if (request.method === 'wallet_addEthereumChain') return null;
      const [id, options] = Object.entries(request.params)[0];
      return { [id]: { id, version: options.version, enabled: true, blocked: false } };
    },
  };
  const fetches = [];
  const window = {
    ethereum: fakeProvider,
    addEventListener(name, callback) { listeners[name] = callback; },
    dispatchEvent(event) { calls.push({ event: event.type }); },
  };
  vm.runInNewContext(source, {
    document: { getElementById: id => elements[id] }, window, location: new URL(href), URL,
    Event: class { constructor(type) { this.type = type; } },
    async fetch(url, options) {
      fetches.push({ url: url.href, options });
      return response ?? { ok: true, json: async () => manifest };
    },
  }, { timeout: 1000 });
  return { elements, calls, fetches, listeners, fakeProvider };
};

test('加载接入页不请求账户或安装权限，添加网络使用准确链号和兼容精度', async () => {
  const page = installPage();
  assert.deepEqual(JSON.parse(JSON.stringify(page.calls)), [{ event: 'eip6963:requestProvider' }]);
  page.elements.rpc.value = 'https://localhost:9944';
  await page.elements['add-network'].handlers.click();
  assert.deepEqual(JSON.parse(JSON.stringify(page.calls[1])), {
    method: 'wallet_addEthereumChain',
    params: [{ chainId: '0x7eb', chainName: '公民链',
      nativeCurrency: { name: '公民币', symbol: 'GMB', decimals: 18 }, rpcUrls: ['https://localhost:9944/'] }],
  });
  assert.match(page.elements.status.textContent, /公民链已添加/);
  assert.equal(page.fetches.length, 0);
});

test('接入页拒绝明文、异常地址和嵌入凭据，失败后恢复操作', async () => {
  for (const value of ['', 'invalid', 'http://localhost:9944', 'wss://localhost:9944',
    'https://user:password@localhost:9944', 'https://localhost:9944/?key=token', 'https://localhost:9944/#fragment']) {
    const page = installPage();
    page.elements.rpc.value = value;
    await page.elements['add-network'].handlers.click();
    assert.equal(page.calls.length, 1);
    assert.match(page.elements.status.textContent, /HTTPS/);
    assert.equal(page.elements['add-network'].disabled, false);
    assert.equal(page.elements['install-snap'].disabled, false);
    assert.doesNotMatch(page.elements.status.textContent, /password|token/);
  }
  const page = installPage({ href: 'http://localhost:9443/node/snap/install.html' });
  page.elements.rpc.value = 'https://localhost:9944';
  await page.elements['add-network'].handlers.click();
  assert.equal(page.calls.length, 1);
  assert.equal(page.elements.status.textContent, '请通过 HTTPS 打开本页面。');
});

test('接入页只在用户操作后按同源清单的准确 npm 名称与版本安装', async () => {
  const page = installPage();
  assert.equal(page.fetches.length, 0);
  await page.elements['install-snap'].handlers.click();
  assert.deepEqual(JSON.parse(JSON.stringify(page.calls[1])), {
    method: 'wallet_requestSnaps', params: { 'npm:@crcfrcn/citizenchain-fees': { version: '1.0.0' } },
  });
  assert.equal(page.fetches[0].url, 'https://localhost:9443/node/snap/snap.manifest.json');
  assert.deepEqual(JSON.parse(JSON.stringify(page.fetches[0].options)), { credentials: 'omit', redirect: 'error' });
  assert.match(page.elements.status.textContent, /公民链费用已安装/);
});

test('本地 Snap 只允许显式选择的 HTTPS 回环来源，公网不能切换为 local', async () => {
  const local = installPage({ href: 'https://localhost:9443/node/snap/install.html?snap=local' });
  await local.elements['install-snap'].handlers.click();
  assert.ok(local.calls[1].params['local:https://localhost:9443/node/snap/']);
  const remote = installPage({ href: 'https://www.crcfrcn.com/node/snap/install.html?snap=local' });
  await remote.elements['install-snap'].handlers.click();
  assert.equal(remote.calls.length, 1);
  assert.doesNotMatch(remote.elements.status.textContent, /已安装/);
});

test('不可读或无效清单不会发起安装，异常安装回执不能显示成功', async () => {
  for (const response of [{ ok: false }, { ok: true, json: async () => ({}) },
    { ok: true, json: async () => ({ version: 'latest', source: { location: { npm: {} } } }) }]) {
    const page = installPage({ response });
    await page.elements['install-snap'].handlers.click();
    assert.equal(page.calls.length, 1);
    assert.doesNotMatch(page.elements.status.textContent, /已安装/);
  }
  const page = installPage({ provider: { isMetaMask: true, request: async () => ({}) } });
  await page.elements['install-snap'].handlers.click();
  assert.doesNotMatch(page.elements.status.textContent, /已安装/);
});

test('EIP-6963 找到 MetaMask 时不用其他注入钱包，缺失钱包明确提示', async () => {
  const page = installPage({ provider: {} });
  page.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'other.wallet' }, provider: {
    request: async () => { throw new Error('不得选择其他钱包'); },
  } } });
  await page.elements['install-snap'].handlers.click();
  assert.match(page.elements.status.textContent, /已安装 MetaMask/);
  const requests = [];
  page.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider: {
    request: async request => { requests.push(request); return null; },
  } } });
  page.elements.rpc.value = 'https://localhost:9944';
  await page.elements['add-network'].handlers.click();
  assert.equal(requests[0].method, 'wallet_addEthereumChain');
});

test('等待中的钱包请求不能重复发送，取消和第三方异常不回显敏感内容', async () => {
  let release;
  let count = 0;
  const page = installPage({ provider: { isMetaMask: true, request: () => {
    count++;
    return new Promise(resolve => { release = resolve; });
  } } });
  page.elements.rpc.value = 'https://localhost:9944';
  const pending = page.elements['add-network'].handlers.click();
  await page.elements['add-network'].handlers.click();
  await page.elements['install-snap'].handlers.click();
  assert.equal(count, 1);
  assert.equal(page.elements['add-network'].disabled, true);
  release(null);
  await pending;
  for (const [code, expected] of [[4001, '你已取消请求。'], [-32002, 'MetaMask 中已有待确认请求，请先处理。'],
    [undefined, '操作未完成，请检查 RPC 是否可用、费用插件是否已发布，并在 MetaMask 中查看原因。']]) {
    const rejected = installPage({ provider: { isMetaMask: true, request: async () => {
      throw { code, message: '不可输出的第三方错误内容' };
    } } });
    rejected.elements.rpc.value = 'https://localhost:9944';
    await rejected.elements['add-network'].handlers.click();
    assert.equal(rejected.elements.status.textContent, expected);
    assert.equal(rejected.elements['add-network'].disabled, false);
  }
});
