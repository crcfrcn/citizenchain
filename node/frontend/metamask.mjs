// 唯一接入页真源；直接运行输出 HTML，导入只提供页面内容，不运行测试。
export const installHtml = String.raw`<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta http-equiv="Content-Security-Policy" content="default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self'; connect-src 'none'; base-uri 'none'; object-src 'none'">
  <title>公民链 · MetaMask</title>
  <style>
    :root { color-scheme: light dark; font-family: system-ui, sans-serif; }
    body { max-width: 480px; margin: 8vh auto; padding: 24px; line-height: 1.6; }
    header { display: flex; align-items: center; gap: 16px; }
    header img { width: 64px; height: 64px; border-radius: 14px; }
    h1 { margin: 0; font-size: 26px; }
    header p { margin: 0; }
    label { display: block; margin-top: 28px; }
    input, button { box-sizing: border-box; width: 100%; padding: 13px; font: inherit; border-radius: 8px; }
    input { margin: 8px 0 16px; border: 1px solid #8b9693; background: transparent; }
    button { margin-bottom: 12px; border: 0; background: #008b70; color: white; cursor: pointer; }
    button:disabled { opacity: .6; cursor: wait; }
    button:focus-visible, input:focus-visible, a:focus-visible { outline: 3px solid #008b70; outline-offset: 3px; }
    #status { min-height: 3em; overflow-wrap: anywhere; }
    a { color: inherit; }
  </style>
</head>
<body>
  <header>
    <img src="/icons/gmb.png" alt="公民币 GMB 图标">
    <div><h1>公民链</h1><p>Chain ID 2027 · 公民币 GMB</p></div>
  </header>
  <p>添加并切换公民链后，可直接在 MetaMask 中查看余额、估算网络费并发起转账。</p>
  <p>电脑请使用已安装 MetaMask 扩展的浏览器；手机请在 MetaMask App 的“探索”内置浏览器中打开本页面。两端需要分别添加网络。</p>
  <p>转账费率：0.1%；最低手续费：0.10 GMB，按分四舍五入。MetaMask 的网络费估算可能包含缓冲，实际手续费以交易回执为准。</p>
  <label for="rpc">公民链 RPC 地址</label>
  <input id="rpc" type="url" inputmode="url" autocomplete="off" spellcheck="false" value="https://nrcrpc.crcfrcn.com/" readonly>
  <button id="add-network" type="button">添加并切换公民链</button>
  <p id="status" role="status" aria-live="polite"></p>
  <a href="https://www.crcfrcn.com/" rel="noreferrer">公民链官网</a>
  <script>
    (() => {
      'use strict';
      const rpc = document.getElementById('rpc');
      const add = document.getElementById('add-network');
      const status = document.getElementById('status');
      let announcedProvider;
      let busy = false;
      const localErrors = new WeakSet();

      // 只发现钱包；页面加载不请求账户或签名权限。
      window.addEventListener('eip6963:announceProvider', (event) => {
        const detail = event.detail;
        if (detail?.info?.rdns === 'io.metamask' && typeof detail?.provider?.request === 'function') {
          announcedProvider ??= detail.provider;
        }
      });
      window.dispatchEvent(new Event('eip6963:requestProvider'));

      // 每次点击读取当前钱包，允许两端钱包在页面加载后完成注入。
      const wallet = () => {
        if (location.protocol !== 'https:') throw new Error('请通过 HTTPS 打开本页面。');
        const provider = announcedProvider ?? (window.ethereum?.isMetaMask ? window.ethereum : undefined);
        if (typeof provider?.request !== 'function') throw new Error('未发现 MetaMask。电脑请使用已安装 MetaMask 扩展的浏览器；手机请在 MetaMask App 的内置浏览器中打开本页面。');
        return provider;
      };

      // 永久公共 RPC 固定；拒绝其他域名、路径、明文或附带凭据的地址。
      const rpcUrl = () => {
        let url;
        try { url = new URL(rpc.value.trim()); } catch { throw new Error('请输入有效的 HTTPS RPC 地址。'); }
        if (url.protocol !== 'https:' || url.origin !== 'https://nrcrpc.crcfrcn.com' || url.pathname !== '/' || url.username || url.password || url.search || url.hash) {
          throw new Error('RPC 仅允许固定的 HTTPS 地址 https://nrcrpc.crcfrcn.com/，不能使用其他地址或附带参数。');
        }
        return url.href;
      };
      const locally = (action) => {
        try { return action(); } catch (error) { localErrors.add(error); throw error; }
      };

      add.addEventListener('click', async () => {
        if (busy) return;
        busy = true;
        add.disabled = true;
        let added = false;
        status.textContent = '请在 MetaMask 中确认添加网络。';
        try {
          const provider = locally(wallet);
          const url = locally(rpcUrl);
          const result = await provider.request({
            method: 'wallet_addEthereumChain',
            params: [{
              chainId: '0x7eb', chainName: '公民链',
              nativeCurrency: { name: '公民币', symbol: 'GMB', decimals: 18 },
              rpcUrls: [url],
            }],
          });
          if (result !== null) throw new Error('无效的添加结果');
          added = true;
          status.textContent = '公民链已添加，请在 MetaMask 中确认切换网络。';
          // 添加成功并不代表已切换；分别校验两步回执及钱包当前链号。
          const switched = await provider.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: '0x7eb' }] });
          if (switched !== null || await provider.request({ method: 'eth_chainId' }) !== '0x7eb') {
            throw new Error('无效的切换结果');
          }
          status.textContent = '已切换到公民链。可在 MetaMask 中查看 GMB 余额并发起转账。';
        } catch (error) {
          const prefix = added ? '公民链已添加，切换未完成。' : '';
          status.textContent = prefix + (error?.code === 4001 ? '你已取消请求。'
            : error?.code === -32002 ? 'MetaMask 中已有待确认请求，请先处理。'
            : error?.code === -32601 ? '当前钱包不支持该操作，请检查 MetaMask 版本。'
            : localErrors.has(error) ? error.message : '操作未完成，请检查 RPC 是否可用，并在 MetaMask 中查看原因。');
        } finally {
          busy = false;
          add.disabled = false;
        }
      });
    })();
  </script>
</body>
</html>
`;

const directEntry = process.argv[1] === import.meta.filename &&
  !process.execArgv.some(value => /^(?:-e|-p|--eval|--print)(?:=|$)/u.test(value));
const inlineTestEntry = directEntry && Boolean(process.env.NODE_TEST_CONTEXT) && process.argv.length === 2;
if (directEntry && !inlineTestEntry) {
  if (process.argv.length !== 2) {
    console.error('用法：node node/frontend/metamask.mjs（向标准输出生成接入页）');
    process.exitCode = 1;
  } else process.stdout.write(installHtml);
}

// 测试位于正式代码之后，仅 node --test 直接执行本文件时加载 Node 测试依赖。
if(inlineTestEntry){
  const { default: assert } = await import('node:assert/strict');
  const { readFileSync } = await import('node:fs');
  const { execFileSync } = await import('node:child_process');
  const { default: test } = await import('node:test');
  const { default: vm } = await import('node:vm');
  const html = installHtml;
  const plain = value => JSON.parse(JSON.stringify(value));

  // 执行实际接入页脚本；钱包替身只验证请求合同，不冒充 MetaMask 实机验收。
  const page = ({ href = 'https://localhost:9443/', provider, request, pageHtml = html } = {}) => {
    const elements = Object.fromEntries(['rpc', 'add-network', 'status'].map(id => [id, {
      value: id === 'rpc' ? pageHtml.match(/id="rpc"[^>]*value="([^"]+)"/)[1] : '',
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
    vm.runInNewContext(pageHtml.match(/<script>([\s\S]*?)<\/script>/)[1], {
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
    assert.match(html, /src="\/icons\/gmb\.png"/);
    assert.doesNotMatch(html, /\/crates\/icons\//);
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

  // 子进程移除测试上下文，准确模拟生产生成入口，避免把测试调度误作页面输出。
  test('正式生成入口只输出唯一 HTML，不混入测试代码或测试报告', () => {
    const { NODE_TEST_CONTEXT, ...environment } = process.env;
    const generated = execFileSync(process.execPath, [import.meta.filename], { env: environment, encoding: 'utf8' });
    assert.equal(generated, installHtml);
    assert.doesNotMatch(generated, /TAP version|node:test|inlineTestEntry/);
  });

  test('普通导入与测试上下文中的导入均无输出和测试注册副作用', () => {
    const source = `const page = await import(${JSON.stringify(import.meta.url)}); if (typeof page.installHtml !== 'string' || !page.installHtml.startsWith('<!doctype html>')) throw Error('页面导出无效'); process.stdout.write('loaded');`;
    const { NODE_TEST_CONTEXT, ...environment } = process.env;
    for (const env of [environment, { ...environment, NODE_TEST_CONTEXT: 'child-v8' }]) {
      const output = execFileSync(process.execPath, ['--input-type=module', '--eval', source], { env, encoding: 'utf8' });
      assert.equal(output, 'loaded');
    }
  });

  // 仅测试入口处理生成视图；三个准确绑定点同时替换，不改正式CLI或钱包业务脚本。
  const isolatedOrigin = 'https://metamask-test.crcfrcn.com';
  const isolatePage = (input, origin) => {
    if (origin !== isolatedOrigin) throw Error('隔离入口只允许已确认的测试HTTPS域');
    if (input !== html) throw Error('隔离页面绑定模板漂移');
    const formalOrigin = 'https://nrcrpc.crcfrcn.com';
    const bindings = [
      [`value="${formalOrigin}/" readonly`, `value="${origin}/" readonly`],
      [`url.origin !== '${formalOrigin}'`, `url.origin !== '${origin}'`],
      [`RPC 仅允许固定的 HTTPS 地址 ${formalOrigin}/`, `RPC 仅允许固定的 HTTPS 地址 ${origin}/`],
      ['<h1>公民链</h1>', '<h1>公民链 · 隔离测试链</h1>'],
    ];
    if (typeof input !== 'string' || input.split(formalOrigin).length !== 4) throw Error('隔离页面绑定数量不符');
    let output = input;
    for (const [before, after] of bindings) {
      if (output.split(before).length !== 2) throw Error('隔离页面绑定位置不符');
      output = output.replace(before, after);
    }
    return output;
  };

  test('隔离生成视图沿用同一钱包脚本，展示地址、校验和提示绑定同一测试域', async () => {
    const pageHtml = isolatePage(html, isolatedOrigin), p = page({ href: isolatedOrigin + '/', pageHtml });
    assert.match(pageHtml, /公民链 · 隔离测试链/);
    assert.doesNotMatch(pageHtml, /nrcrpc\.crcfrcn\.com/);
    assert.equal(p.elements.rpc.value, isolatedOrigin + '/');
    await p.click();
    assert.deepEqual(p.calls.map(call => call.method), ['wallet_addEthereumChain', 'wallet_switchEthereumChain', 'eth_chainId']);
    assert.deepEqual(p.calls[0].params[0], { chainId: '0x7eb', chainName: '公民链', nativeCurrency: { name: '公民币', symbol: 'GMB', decimals: 18 }, rpcUrls: [isolatedOrigin + '/'] });
    assert.match(p.elements.status.textContent, /^已切换到公民链/);
  });

  test('正式与隔离页面互相拒绝对方RPC，隔离页面拒绝越界地址', async () => {
    const formal = page(); formal.elements.rpc.value = isolatedOrigin + '/'; await formal.click(); assert.deepEqual(formal.calls, []);
    const pageHtml = isolatePage(html, isolatedOrigin);
    for (const rpc of ['https://nrcrpc.crcfrcn.com/', 'https://other.example/', isolatedOrigin + '/rpc', isolatedOrigin + '/?key=fixture', isolatedOrigin + '/#fragment', 'http://metamask-test.crcfrcn.com/', 'https://metamask-test.crcfrcn.com:8443/']) {
      const p = page({ href: isolatedOrigin + '/', pageHtml }); p.elements.rpc.value = rpc; await p.click();
      assert.deepEqual(p.calls, []); assert.match(p.elements.status.textContent, /HTTPS/);
      assert.equal(p.elements['add-network'].disabled, false);
    }
  });

  test('测试绑定拒绝未经确认的域、缺失标记、重复标记和模板漂移', () => {
    for (const origin of ['https://nrcrpc.crcfrcn.com', isolatedOrigin + '/', 'http://metamask-test.crcfrcn.com', 'https://other.example']) assert.throws(() => isolatePage(html, origin), /只允许/);
    for (const input of [html.replace('value="https://nrcrpc.crcfrcn.com/" readonly', 'value="https://nrcrpc.crcfrcn.com/"'), html + 'https://nrcrpc.crcfrcn.com', html.replace('<h1>公民链</h1>', '<h1>另一个页面</h1>'), html.replace('wallet_addEthereumChain', 'wallet_requestSnaps')]) assert.throws(() => isolatePage(input, isolatedOrigin), /绑定/);
  });

  test('隔离HTML生成入口仅写入调用方持有的本产品测试现场', async () => {
    const output = process.env.CITIZENCHAIN_TEST_PAGE_OUTPUT;
    if (output === undefined) { assert.equal(typeof isolatePage(html, isolatedOrigin), 'string'); return; }
    const { fixedWork, checkScratchPath } = await import('../../scripts/build.mjs');
    const { dirname, join } = await import('node:path');
    const { writeFileSync, realpathSync } = await import('node:fs');
    const work = ['build', 'test'].map(fixedWork).find(root => output === join(root, 'metamask', 'install.html'));
    if (!work || realpathSync(import.meta.filename) !== import.meta.filename) throw Error('隔离HTML输出越出本产品固定现场');
    checkScratchPath(dirname(output));
    const owner = JSON.parse(readFileSync(join(work, '.active.json'), 'utf8'));
    if (owner.work !== work || owner.product_id !== 'citizenchain' || owner.state !== 'running' || !process.env.PRODUCT_WORK_LEASE || owner.nonce !== process.env.PRODUCT_WORK_LEASE || !Number.isSafeInteger(owner.pid) || owner.pid <= 0) throw Error('隔离HTML缺少当前调用方所有权');
    process.kill(owner.pid, 0);
    writeFileSync(output, isolatePage(html, isolatedOrigin), { flag: 'wx', mode: 0o444 });
    assert.equal(readFileSync(output, 'utf8'), isolatePage(html, isolatedOrigin));
  });
}
