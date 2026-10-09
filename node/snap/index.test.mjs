import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const html = readFileSync(new URL('./install.html', import.meta.url), 'utf8');
const source = html.match(/<script>([\s\S]*?)<\/script>/)[1];
const plain = value => JSON.parse(JSON.stringify(value));

// 执行实际接入页脚本；钱包替身只验证请求合同，不冒充 MetaMask 实机验收。
const page = ({ href = 'https://localhost:9443/node/snap/install.html', provider, request } = {}) => {
  const elements = Object.fromEntries(['rpc', 'add-network', 'status'].map(id => [id, {
    value: id === 'rpc' ? html.match(/id="rpc"[^>]*value="([^"]+)"/)[1] : '',
    textContent: '', disabled: false, handlers: {},
    addEventListener(name, callback) { this.handlers[name] = callback; },
  }]));
  const calls = [], events = [], listeners = {};
  const fakeProvider = provider === undefined ? {
    isMetaMask: true,
    async request(input) {
      calls.push(plain(input));
      return request ? request(input) : input.method === 'eth_chainId' ? '0x7eb' : null;
    },
  } : provider;
  // 保留可变的注入对象，覆盖钱包缺失及页面加载后的真实注入时序。
  const window = {
    ethereum: fakeProvider,
    addEventListener(name, callback) { listeners[name] = callback; },
    dispatchEvent(event) { events.push(event.type); },
  };
  vm.runInNewContext(source, {
    document: { getElementById: id => elements[id] },
    window,
    location: new URL(href), URL, Event: class { constructor(type) { this.type = type; } },
  }, { timeout: 1000 });
  return { elements, calls, events, listeners, window, click: () => elements['add-network'].handlers.click() };
};

test('页面只有一个添加并切换按钮，展示费率、最低费与估算边界', () => {
  assert.equal((html.match(/<button\b/g) ?? []).length, 1);
  assert.match(html, /转账费率：0\.1%/);
  assert.match(html, /最低手续费：0\.10 GMB/);
  assert.match(html, /按分四舍五入/);
  assert.match(html, /网络费估算可能包含缓冲/);
  assert.match(html, /crates\/icons\/gmb\.png/);
  assert.doesNotMatch(html, /wallet_requestSnaps|snap\.manifest|npm:|安装公民链费用|支持 Snaps/);
  assert.match(html, /connect-src 'none'/);
  assert.match(html, /role="status" aria-live="polite"/);
});

test('页面加载仅发现钱包，默认公共 HTTPS RPC，不请求账户或网络权限', () => {
  const p = page();
  assert.deepEqual(p.calls, []);
  assert.deepEqual(p.events, ['eip6963:requestProvider']);
  assert.equal(p.elements.rpc.value, 'https://nrcrpc.crcfrcn.com/');
  assert.match(html, /id="rpc"[^>]*readonly/);
});

test('仅 window.ethereum 注入时按顺序添加、切换并回读链号，名称与币种精度准确', async () => {
  const p = page(); await p.click();
  assert.deepEqual(p.calls, [
    { method: 'wallet_addEthereumChain', params: [{ chainId: '0x7eb', chainName: '公民链',
      nativeCurrency: { name: '公民币', symbol: 'GMB', decimals: 18 }, rpcUrls: ['https://nrcrpc.crcfrcn.com/'] }] },
    { method: 'wallet_switchEthereumChain', params: [{ chainId: '0x7eb' }] },
    { method: 'eth_chainId' },
  ]);
  assert.match(p.elements.status.textContent, /^已切换到公民链/);
  assert.equal(p.elements['add-network'].disabled, false);
});

test('固定 HTTPS RPC 规范化去除周围空格', async () => {
  const p = page(); p.elements.rpc.value = ' https://nrcrpc.crcfrcn.com '; await p.click();
  assert.deepEqual(p.calls[0].params[0].rpcUrls, ['https://nrcrpc.crcfrcn.com/']);
});

test('旧域名、其他域名、非根路径、端口及异常地址在请求钱包之前拒绝', async () => {
  for (const value of ['', 'invalid', 'https://rpc.crcfrcn.com/', 'https://other.example/',
    'https://nrcrpc.crcfrcn.com/api', 'https://nrcrpc.crcfrcn.com:8443/',
    'http://nrcrpc.crcfrcn.com/', 'wss://nrcrpc.crcfrcn.com/',
    'https://user:password@nrcrpc.crcfrcn.com', 'https://nrcrpc.crcfrcn.com/?key=token', 'https://nrcrpc.crcfrcn.com/#fragment']) {
    const p = page(); p.elements.rpc.value = value; await p.click();
    assert.deepEqual(p.calls, []);
    assert.match(p.elements.status.textContent, /HTTPS/);
    assert.doesNotMatch(p.elements.status.textContent, /password|token/);
    assert.equal(p.elements['add-network'].disabled, false);
  }
});

test('页面自身必须 HTTPS，文件与明文页面不请求钱包', async () => {
  for (const href of ['http://localhost:9443/install.html', 'file:///install.html']) {
    const p = page({ href }); await p.click(); assert.deepEqual(p.calls, []);
    assert.equal(p.elements.status.textContent, '请通过 HTTPS 打开本页面。');
  }
});

test('钱包缺失、非 MetaMask 或 request 无效时给出两端引导且不请求权限', async () => {
  for (const provider of [null, {}, { isMetaMask: false, request() { throw Error('不得选择其他钱包'); } },
    { isMetaMask: true, request: null }]) {
    const p = page({ provider }); await p.click();
    assert.deepEqual(p.calls, []);
    assert.equal(p.elements.status.textContent, '未发现 MetaMask。电脑请使用已安装 MetaMask 扩展的浏览器；手机请在 MetaMask App 的内置浏览器中打开本页面。');
    assert.equal(p.elements['add-network'].disabled, false);
  }
});

test('优先使用 EIP-6963 的 MetaMask，忽略其他钱包及异常公告', async () => {
  const p = page({ provider: { isMetaMask: true, request() { throw Error('不得使用回退提供者'); } } });
  p.listeners['eip6963:announceProvider']({});
  p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'other.wallet' }, provider: { request() { throw Error('不得选择其他钱包'); } } } });
  p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider: {} } });
  const calls = [];
  p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider: {
    async request(input) { calls.push(input.method); return input.method === 'eth_chainId' ? '0x7eb' : null; },
  } } });
  await p.click();
  assert.deepEqual(calls, ['wallet_addEthereumChain', 'wallet_switchEthereumChain', 'eth_chainId']);
  assert.match(p.elements.status.textContent, /^已切换/);
});

test('仅 EIP-6963 公告且没有 window.ethereum 时完成同一添加合同', async () => {
  const p = page(), provider = p.window.ethereum;
  delete p.window.ethereum;
  p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider } });
  assert.deepEqual(p.calls, []);
  await p.click();
  assert.deepEqual(p.calls, [
    { method: 'wallet_addEthereumChain', params: [{ chainId: '0x7eb', chainName: '公民链',
      nativeCurrency: { name: '公民币', symbol: 'GMB', decimals: 18 }, rpcUrls: ['https://nrcrpc.crcfrcn.com/'] }] },
    { method: 'wallet_switchEthereumChain', params: [{ chainId: '0x7eb' }] },
    { method: 'eth_chainId' },
  ]);
  assert.match(p.elements.status.textContent, /^已切换到公民链/);
});

test('window.ethereum 延迟注入后可重试，首次缺失不产生钱包请求', async () => {
  const p = page(), provider = p.window.ethereum;
  delete p.window.ethereum;
  await p.click();
  assert.deepEqual(p.calls, []);
  assert.match(p.elements.status.textContent, /未发现 MetaMask/);
  assert.equal(p.elements['add-network'].disabled, false);
  p.window.ethereum = provider;
  assert.deepEqual(p.calls, []);
  await p.click();
  assert.deepEqual(p.calls.map(call => call.method), ['wallet_addEthereumChain', 'wallet_switchEthereumChain', 'eth_chainId']);
  assert.match(p.elements.status.textContent, /^已切换到公民链/);
});

test('EIP-6963 延迟公告后可重试，公告本身不请求网络或账户权限', async () => {
  const p = page(), provider = p.window.ethereum;
  delete p.window.ethereum;
  await p.click();
  assert.deepEqual(p.calls, []);
  assert.match(p.elements.status.textContent, /未发现 MetaMask/);
  p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider } });
  assert.deepEqual(p.calls, []);
  await p.click();
  assert.deepEqual(p.calls.map(call => call.method), ['wallet_addEthereumChain', 'wallet_switchEthereumChain', 'eth_chainId']);
  assert.match(p.elements.status.textContent, /^已切换到公民链/);
});

test('仅 EIP-6963 钱包取消添加或切换后保留真实状态并允许重试', async () => {
  for (const cancelledMethod of ['wallet_addEthereumChain', 'wallet_switchEthereumChain']) {
    let cancelled = false;
    const p = page({ request(input) {
      if (!cancelled && input.method === cancelledMethod) { cancelled = true; throw { code: 4001 }; }
      return input.method === 'eth_chainId' ? '0x7eb' : null;
    } }), provider = p.window.ethereum;
    delete p.window.ethereum;
    p.listeners['eip6963:announceProvider']({ detail: { info: { rdns: 'io.metamask' }, provider } });
    await p.click();
    assert.equal(p.elements.status.textContent, cancelledMethod === 'wallet_addEthereumChain'
      ? '你已取消请求。' : '公民链已添加，切换未完成。你已取消请求。');
    assert.deepEqual(p.calls.map(call => call.method), cancelledMethod === 'wallet_addEthereumChain'
      ? ['wallet_addEthereumChain'] : ['wallet_addEthereumChain', 'wallet_switchEthereumChain']);
    assert.equal(p.elements['add-network'].disabled, false);
    const count = p.calls.length;
    await p.click();
    assert.deepEqual(p.calls.slice(count).map(call => call.method), ['wallet_addEthereumChain', 'wallet_switchEthereumChain', 'eth_chainId']);
    assert.match(p.elements.status.textContent, /^已切换到公民链/);
  }
});

test('取消添加不请求切换，不回显钱包原始错误', async () => {
  const p = page({ request() { throw { code: 4001, message: 'sensitive-wallet-detail' }; } });
  await p.click(); assert.equal(p.calls.length, 1);
  assert.equal(p.elements.status.textContent, '你已取消请求。');
});

test('取消切换明确保留已添加的事实，不虚报成功', async () => {
  const p = page({ request(input) { if (input.method === 'wallet_switchEthereumChain') throw { code: 4001 }; return null; } });
  await p.click(); assert.equal(p.calls.length, 2);
  assert.equal(p.elements.status.textContent, '公民链已添加，切换未完成。你已取消请求。');
});

test('添加结果不是 null 时不得继续切换或显示成功', async () => {
  for (const reply of [undefined, {}, true, '0x7eb']) {
    const p = page({ request: () => reply }); await p.click();
    assert.equal(p.calls.length, 1); assert.doesNotMatch(p.elements.status.textContent, /已添加|已切换/);
  }
});

test('切换回执异常或实际链号不匹配时不显示切换成功', async () => {
  for (const reply of [undefined, {}, true]) {
    const p = page({ request: input => input.method === 'wallet_switchEthereumChain' ? reply : null });
    await p.click(); assert.equal(p.calls.length, 2);
    assert.match(p.elements.status.textContent, /已添加，切换未完成/);
  }
  for (const chainId of ['0x1', undefined, {}, 2027]) {
    const p = page({ request: input => input.method === 'eth_chainId' ? chainId : null });
    await p.click(); assert.equal(p.calls.length, 3); assert.doesNotMatch(p.elements.status.textContent, /^已切换/);
  }
});

test('钱包待处理、不支持与其他失败有固定中文提示且不暴露错误', async () => {
  for (const [code, message] of [[-32002, /已有待确认/], [-32601, /检查 MetaMask 版本/], [-1, /检查 RPC/]]) {
    const p = page({ request() { throw { code, message: 'sensitive-wallet-detail', localMessage: 'sensitive-wallet-detail' }; } });
    await p.click(); assert.match(p.elements.status.textContent, message);
    assert.doesNotMatch(p.elements.status.textContent, /sensitive-wallet-detail/);
    assert.equal(p.elements['add-network'].disabled, false);
  }
});

test('已有操作时抑制重复点击，完成后可以再次执行', async () => {
  let release;
  const p = page({ request: input => input.method === 'wallet_addEthereumChain'
    ? new Promise(resolve => { release = resolve; }) : input.method === 'eth_chainId' ? '0x7eb' : null });
  const first = p.click(); assert.equal(p.elements['add-network'].disabled, true);
  await p.click(); assert.equal(p.calls.length, 1); release(null); await first;
  assert.equal(p.elements['add-network'].disabled, false);
  const second = p.click(); assert.equal(p.calls.length, 4); release(null); await second;
  assert.equal(p.calls.length, 6);
});

// 文本只展示 Runtime 真源的制度，不在页面维护第二套收费实现。
test('链号、EVM 精度、固定费率与最低手续费仍与 Runtime 真源一致', () => {
  const root = new URL(process.env.CITIZENCHAIN_SOURCE_ROOT
    ? `${process.env.CITIZENCHAIN_SOURCE_ROOT}/` : '../../', import.meta.url);
  const policy = readFileSync(new URL('runtime/primitives/src/fee_policy.rs', root), 'utf8');
  const units = readFileSync(new URL('runtime/primitives/src/core_const.rs', root), 'utf8');
  assert.match(policy, /ONCHAIN_FEE_RATE[^;]*1_000_000/);
  assert.match(policy, /ONCHAIN_MIN_FEE[^;]*10/);
  assert.match(units, /NATIVE_TO_ETH_RATIO == 10_000_000_000_000_000/);
  assert.match(units, /ETHEREUM_CHAIN_ID: u64 = 2027/);
});
