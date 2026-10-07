import assert from 'node:assert/strict';
import test from 'node:test';
import { gateContract, validateWorkflowSource, validateVectorGroup, validatePalletRegistry, readPublicChain } from './index.mjs';

// 本仓登记必须准确闭合；路径、重复和未知工具版本不得被默默接受。
test('本仓门禁登记拒绝漂移和重复', () => {
  const contract = structuredClone(gateContract());
  assert.equal(gateContract(contract), contract);
  for (const change of [
    value => { value.schema = 2; },
    value => { value.node_tests.push(value.node_tests[0]); },
    value => { value.node_tests = ['../outside.test.mjs']; },
    value => { value.tools.node = '0.0.0'; },
    value => { value.checks.push('undeclared'); },
    value => { value.workflows.push('other.ios.ci'); },
  ]) {
    const value = structuredClone(contract); change(value);
    assert.throws(() => gateContract(value));
  }
});
test('产品CI与Release仍只接受自己的三维手动入口', () => {
  const { repository } = gateContract();
  const id = repository + '.macos.ci';
  const workflow = 'name: ' + id + '\non:\n  workflow_dispatch:\nconcurrency:\n  group: ' + id
    + '\njobs:\n  flow:\n    steps:\n      - run: allowed=new Set(["' + id + '"])\n';
  assert.equal(validateWorkflowSource(workflow,repository+'-macos-ci.yml'),id);
  for (const invalid of [workflow.replace(repository+'.','other.'),workflow.replace('workflow_dispatch','push'),
    workflow.replace('group: '+id,'group: other'),workflow.replace('  flow:','  other:')]) {
    assert.throws(()=>validateWorkflowSource(invalid,repository+'-macos-ci.yml'));
  }
});
const group={ keys:['name'],values:['hex'],top:['domain'],complete:true };
const vectors={domain:'GMB',vectors:[{name:'a',hex:'AB'},{name:'b',hex:'CD'}]};
test('金标按语义键归一比较并阻断重复、缺项和漂移', () => {
  assert.equal(validateVectorGroup(vectors,{...vectors,vectors:[{name:'b',hex:'cd'},{name:'a',hex:'ab'}]},group),2);
  for (const mirror of [
    {...vectors,domain:'other'}, {...vectors,vectors:[vectors.vectors[0]]},
    {...vectors,vectors:[vectors.vectors[0],vectors.vectors[0]]},
    {...vectors,vectors:[{name:'a',hex:'EE'},vectors.vectors[1]]},
    {...vectors,vectors:[{name:'a'},vectors.vectors[1]]},
    {...vectors,vectors:[]},
  ]) assert.throws(()=>validateVectorGroup(vectors,mirror,group));
  assert.equal(validateVectorGroup(vectors,{...vectors,vectors:[vectors.vectors[0]]},{...group,complete:false}),1);
});
test('Pallet不得错指、为空或重复',()=>{
  const chain='#[runtime::pallet_index(1)]\n pub type Balances = PalletBalances;\n';
  assert.equal(validatePalletRegistry(chain,'static const int balancesPallet = 1;'),1);
  for (const dart of ['', 'static const int balancesPallet = 2;', 'static const int otherPallet = 1;',
    'static const int balancesPallet = 1;\nstatic const int balancesPallet = 1;']) {
    assert.throws(()=>validatePalletRegistry(chain,dart));
  }
  assert.throws(()=>validatePalletRegistry(chain+chain));
});
test('公开链真源只读准确SHA，拒绝网络、重定向、超限及伪造坐标',async()=>{
  const sha='a'.repeat(40);
  const reference={ref:'refs/heads/main',object:{type:'commit',sha,url:'https://api.github.com/repos/crcfrcn/citizenchain/git/commits/'+sha}};
  assert.equal(await readPublicChain(null,null,async(url,options)=>{
    assert.equal(url,'https://api.github.com/repos/crcfrcn/citizenchain/git/ref/heads/main');
    assert.equal(options.redirect,'error'); assert.equal(options.credentials,'omit');
    assert.equal(options.headers.Authorization,undefined);
    return new Response(JSON.stringify(reference));
  }),sha);
  assert.equal(await readPublicChain('runtime/src/lib.rs',sha,async()=>new Response('source')),'source');
  for (const request of [
    async()=>{throw new Error('private response forbidden');},
    async()=>new Response('',{status:302}), async()=>new Response('',{status:404}),
    async()=>new Response(Buffer.alloc(2*1024*1024+1)),
    async()=>new Response(new Uint8Array([255])),
  ]) await assert.rejects(readPublicChain('runtime/src/lib.rs',sha,request),/准确提交真源读取失败/u);
  for (const value of [
    {...reference,ref:'refs/heads/other'}, {...reference,object:{...reference.object,sha:'main'}},
    {...reference,object:{...reference.object,type:'tag'}},
    {...reference,object:{...reference.object,url:'https://example.org/commit'}},
  ]) await assert.rejects(readPublicChain(null,null,async()=>new Response(JSON.stringify(value))));
  await assert.rejects(readPublicChain('../private',sha,()=>assert.fail('非法路径禁止联网')));
});

// 用隔离的合成Git提交验证门禁读取真实初始内容；不修改产品仓或调用仓库保存/推送。
test('保留源码不按每文件汉字数量判定，真实第一方临时注释仍拒绝', async () => {
  const [{ mkdtempSync, mkdirSync, writeFileSync, rmSync }, { join }, { testRoot: tmpdir }, { execFileSync }, { validateQuality }] = await Promise.all([
    import('node:fs'), import('node:path'), import('../../scripts/build.mjs'), import('node:child_process'), import('./index.mjs'),
  ]);
  const root = mkdtempSync(join(tmpdir(), 'tatagate-quality-'));
  const env = { HOME: process.env.HOME, PATH: '/usr/bin:/bin', LANG: 'C', LC_ALL: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
    GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid' };
  const git = (...args) => execFileSync('/usr/bin/git', ['-C', root, ...args], { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  try {
    git('init', '--quiet', '--initial-branch=main');
    mkdirSync(join(root, 'test'));
    writeFileSync(join(root, 'test', 'example.test.mjs'), 'export const fixture = true;\n');
    const base = git('hash-object', '-w', '-t', 'tree', '/dev/null');
    for (const [source, rejected] of [
      ['export const value = 1;\n', false],
      ['// Retained implementation explanation.\nexport const value = 1;\n', false],
      ['// Generated file; do not edit.\nexport const value = 1;\n', false],
      ['// HACK: unfinished first-party implementation.\nexport const value = 1;\n', true],
    ]) {
      writeFileSync(join(root, 'source.mjs'), source);
      git('add', '--all');
      const head = git('commit-tree', git('write-tree'), '-m', 'synthetic quality input');
      git('update-ref', 'refs/heads/main', head);
      const run = () => validateQuality(root, base, head, gateContract().repository);
      if (rejected) await assert.rejects(run(), /第一方|产品实现代码保留临时注释/u);
      else await assert.doesNotReject(run());
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// 候选文只作为合成测试数据；准确负向断言可识别，同文伪装和运行地址继续被阻断。
test('协议拒绝断言只归属本仓登记测试中的真实代码', async () => {
  const { protocolAssertionLines } = await import('./index.mjs');
  const path = gateContract().node_tests.find(value => /(?:test|tests)[./_-]/u.test(value) && value.endsWith('.mjs'));
  assert.ok(path);
  const statement = ['assert.doesNotMatch(source, /\\/', 'v', '1(?:\\/|\\b)/);'].join('');
  const line = '  ' + statement;
  assert.deepEqual(protocolAssertionLines(path, 'test(() => {\n' + line + '\n});\n'), [line]);
  for (const source of [
    '/*\n' + line + '\n*/', '`\n' + line + '\n`', JSON.stringify(statement),
    '/*\n' + line + '\n*/\ntest(() => {\n' + line + '\n});',
    statement.replace('doesNotMatch', 'match'), statement.replace('source', 'other'),
    'const endpoint = "https://example.invalid/' + 'v' + '9";',
  ]) assert.deepEqual(protocolAssertionLines(path, source), []);
  assert.deepEqual(protocolAssertionLines('source.mjs', line), []);
  assert.deepEqual(protocolAssertionLines('unregistered.test.mjs', line), []);
});

// 执行本仓真实Shell增量防护，检查CLI/浏览器边界、拒绝断言、真实残留和大输入通道。
test('增量防护执行真实归属判断并支持超过argv单项限制的输入', async () => {
  const [{ mkdtempSync, mkdirSync, writeFileSync, rmSync }, { join, dirname }, { testRoot: tmpdir }, { execFileSync, spawnSync }, { checkGuardrails }] = await Promise.all([
    import('node:fs'), import('node:path'), import('../../scripts/build.mjs'), import('node:child_process'), import('./index.mjs'),
  ]);
  const root = mkdtempSync(join(tmpdir(), 'tatagate-guard-'));
  const env = { HOME: process.env.HOME, PATH: '/usr/bin:/bin', LANG: 'C', LC_ALL: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
    GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid' };
  const git = (...args) => execFileSync('/usr/bin/git', ['-C', root, ...args], { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  let output;
  const execute = (command, args, options) => {
    output = spawnSync(command, args, { ...options, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], maxBuffer: 4 * 1024 * 1024, timeout: 20_000 });
    return output;
  };
  try {
    git('init', '--quiet', '--initial-branch=main');
    const base = git('hash-object', '-w', '-t', 'tree', '/dev/null');
    const testPath = gateContract().node_tests.find(value => /(?:test|tests)[./_-]/u.test(value) && value.endsWith('.mjs'));
    assert.ok(testPath);
    const statement = ['assert.doesNotMatch(source, /\\/', 'v', '1(?:\\/|\\b)/);'].join('');
    const log = ['console', '.log("result");\n'].join('');
    const unfinished = '// ' + ['TO', 'DO'].join('') + ': unfinished\n';
    const protocol = 'const endpoint = "https://example.invalid/' + 'v' + '9";\n';
    for (const [path, source, rejected, message] of [
      ['scripts/fixture.mjs', log, false],
      ['scripts/fixture.mjs', log + 'const known = "' + ['QR', '_V1'].join('').repeat(64000) + '";\n', false],
      ['lib/fixture.rs', 'fn only_qr_' + 'v1_is_versioned() {}\nfn with_safe_default_' + 'protocol_' + 'versions() {}\nconst KEY: &str = "grandpa_' + 'schema_' + 'version";\n', false],
      ['scripts/fixture.mjs', 'const source = "https://github.com/protocolbuffers/protobuf/releases/tag/' + 'v35.0";\n', false],
      ['scripts/runtime/release/build-wasm/test.mjs', 'const tag = "citizenchain-wasm-' + 'v2";\n', false],
      ['scripts/runtime/release/build-wasm/test.mjs', 'const tag = "citizenchain-wasm-' + 'v3";\n', true, '版本化标识'],
      ['node/frontend/local-docs.generated.ts', '    "markdown": "https://example.invalid/' + 'v12",\n', false],
      ['node/frontend/local-docs.generated.ts', 'const endpoint = "https://example.invalid/' + 'v12";\n', true, '版本化标识'],
      ['scripts/fixture.mjs', log + 'const large = "' + 'x'.repeat(256 * 1024) + '";\n', false],
      ['lib/browser.js', log, true, '开发残留'],
      ['scripts/fixture.mjs', ['debug', 'ger;\n'].join(''), true, '开发残留'],
      ['scripts/fixture.mjs', unfinished, true, '开发残留'],
      [testPath, 'test(() => {\n  ' + statement + '\n});\n', false],
      [testPath, 'test(() => {\n  ' + statement + '\n});\n' + protocol, true, '版本化标识'],
      [testPath, '`\n  ' + statement + '\n`\n', true, '版本化标识'],
      ['unregistered.test.mjs', '  ' + statement + '\n', true, '版本化标识'],
    ]) {
      for (const entry of ['scripts', 'lib', 'test', 'node', 'unregistered.test.mjs']) rmSync(join(root, entry), { recursive: true, force: true });
      mkdirSync(dirname(join(root, path)), { recursive: true });
      writeFileSync(join(root, path), source);
      git('add', '--all');
      const head = git('commit-tree', git('write-tree'), '-m', 'synthetic guard input');
      git('update-ref', 'refs/heads/main', head);
      const run = () => checkGuardrails(root, { ...env, BASE_REF: base }, execute);
      if (rejected) {
        assert.throws(run, /增量防护未通过/u);
        assert.ok((output.stdout + output.stderr).includes(message));
      } else assert.doesNotThrow(run);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// 中文注释：保留准确上游法律注释原件；真正运行地址、其它域名和字符串伪装继续拒绝。
test('准确上游许可证注释不等于明文网络入口', async () => {
  const { insecureTransportLines } = await import('./index.mjs');
  const url = 'ht' + 'tp://www.apache.org/licenses/LICENSE-2.0';
  for (const suffix of ['www.apache.org/licenses/LICENSE-2.0', 'opensource.org/licenses/MIT', 'unlicense.org']) {
    const license = 'ht' + 'tp://' + suffix;
    assert.deepEqual(insecureTransportLines('runtime/src/genesis.rs', '// <' + license + '>'), []);
    assert.ok(insecureTransportLines('source.rs', '// ' + license + '/unknown').length > 0);
    assert.ok(insecureTransportLines('source.rs', 'let url = "' + license + '";').length > 0);
  }
  assert.ok(insecureTransportLines('source.rs', 'let url = "' + url + '";').length > 0);
  assert.ok(insecureTransportLines('source.rs', '// ' + url.replace('www.apache.org', 'example.invalid')).length > 0);
});

// 中文注释：执行实际 Python RPC 配置验证，拒绝明文及隐式缺省；不发网络请求。
test('链 Python RPC 只接受严格 HTTPS 输入', async () => {
  const { spawnSync } = await import('node:child_process');
  const script = new URL('../../scripts/check-constitution-genesis.py', import.meta.url).pathname;
  const code = 'import runpy,sys; module=runpy.run_path(sys.argv[1]); module["RpcTop"](sys.argv[2],None)';
  for (const [url, rejected] of [['https://example.invalid', false], ['http' + '://localhost', true], ['ws' + '://localhost', true], ['https://user:pass@example.invalid', true], ['', true]]) {
    const result = spawnSync('/usr/bin/python3', ['-B', '-c', code, script, url], { encoding: 'utf8' });
    assert.equal(result.status !== 0, rejected, result.stderr);
  }
});

// 中文注释：组织重构原件完整保留，路径错配及任何字节变化都不能免检。
test('保留原件以完整字节摘要闭合', async () => {
  const { retainedOriginalSource } = await import('./index.mjs');
  const { readFileSync } = await import('node:fs');
  for (const path of ["node/vendor/src/environment.rs", "node/vendor/src/import.rs", "runtime/issuance/onchain-issuance/src/benchmarks.rs", "runtime/governance/resolution-destroy/src/weights.rs", "runtime/governance/runtime-upgrade/src/weights.rs", "runtime/issuance/citizen-issuance/src/weights.rs", "runtime/issuance/fullnode-issuance/src/weights.rs", "runtime/issuance/provincialbank-interest/src/weights.rs", "runtime/issuance/resolution-issuance/src/weights.rs", "runtime/misc/pow-difficulty/src/weights.rs", "runtime/transaction/multisig/src/weights.rs", "runtime/votingengine/election-vote/src/weights.rs", "runtime/votingengine/internal-vote/src/weights.rs", "runtime/votingengine/joint-vote/src/weights.rs", "runtime/votingengine/legislation-vote/src/weights.rs", "runtime/votingengine/src/weights.rs"]) {
    const source = readFileSync(new URL('../../' + path, import.meta.url), 'utf8');
    assert.equal(retainedOriginalSource(path, source), source);
    assert.equal(retainedOriginalSource(path, source + '\n'), '');
    assert.equal(retainedOriginalSource('other/' + path, source), '');
  }
});

// 中文注释：真实 JSONC 配置原注释保留，其它 JSON 及畸形正文继续拒绝。
test('OnChina 原 TypeScript 配置按 JSONC 验真', async () => {
  const { parseSyntaxJSON } = await import('./index.mjs');
  const { readFileSync } = await import('node:fs');
  const path = 'onchina/frontend/tsconfig.json';
  assert.ok(parseSyntaxJSON(path, readFileSync(new URL('../../' + path, import.meta.url), 'utf8')).compilerOptions);
  const input = '{/* note */"value":"literal,}/*stay*/",}';
  assert.equal(parseSyntaxJSON(path, input).value, 'literal,}/*stay*/');
  assert.throws(() => parseSyntaxJSON('other.json', input));
  assert.throws(() => parseSyntaxJSON(path, '{"value":}'));
  assert.throws(() => parseSyntaxJSON(path, '{/* unclosed'));
});

// 中文注释：机构命名账户和个人多签使用各自真实派生输入，仍拒绝缺项、重复与密码学漂移。
test('账户派生金标覆盖机构与个人的真实语义键', async () => {
  const { readFileSync } = await import('node:fs');
  const canonical = JSON.parse(readFileSync(new URL("../../runtime/primitives/tests/fixtures/account_derive_vectors.json", import.meta.url), 'utf8'));
  const options = { keys: {"InstitutionMain": ["kind", "cid_number"], "InstitutionFee": ["kind", "cid_number"], "InstitutionSafetyFund": ["kind", "cid_number"], "InstitutionHe": ["kind", "cid_number"], "InstitutionStake": ["kind", "cid_number"], "InstitutionClearing": ["kind", "cid_number"], "InstitutionNamed": ["kind", "cid_number", "account_name"], "Personal": ["kind", "creator_account_id", "account_name"]}, values: ['account_id'], top: ['domain','ss58_format'], complete: true };
  assert.equal(validateVectorGroup(canonical, structuredClone(canonical), options), canonical.vectors.length);
  for (const mutate of [
    d => { delete d.vectors.find(v => v.kind === 'Personal').creator_account_id; },
    d => { d.vectors.find(v => v.kind === 'InstitutionNamed').account_name = 'changed'; },
    d => { d.vectors.find(v => v.kind === 'Personal').account_id = '0x00'; },
    d => { d.vectors.push(structuredClone(d.vectors.find(v => v.kind === 'Personal'))); },
    d => { d.vectors[0].kind = 'Unknown'; },
  ]) { const mirror = structuredClone(canonical); mutate(mirror); assert.throws(() => validateVectorGroup(canonical, mirror, options)); }
});

// Rustup 的命令入口是 shim；远端必须交付登记工具链的 Cargo 原件，解析失败不得继续运行门禁。
test('远端链门禁取得准确Cargo原件并拒绝解析失败', async () => {
  const [{ mkdtempSync, mkdirSync, writeFileSync, readFileSync, rmSync, existsSync }, { join }, { testRoot: tmpdir }, { spawnSync }] = await Promise.all([
    import('node:fs'), import('node:path'), import('../../scripts/build.mjs'), import('node:child_process'),
  ]);
  const work = mkdtempSync(join(tmpdir(), 'chain-cargo-delivery-'));
  try {
    const source = readFileSync(new URL('../workflows/tatagate.yml', import.meta.url), 'utf8');
    const step = source.split('      - name: 本仓完整塔塔门禁\n')[1];
    const script = step.split('        run: |\n')[1].split('\n').map(line => line.startsWith('          ') ? line.slice(10) : line).join('\n');
    const bin = join(work, 'bin'); mkdirSync(bin);
    const cargo = join(work, 'toolchain', 'bin', 'cargo'); mkdirSync(join(cargo, '..'), { recursive: true });
    writeFileSync(cargo, '#!/bin/bash\nexit 0\n', { mode: 0o755 });
    const rustup = join(bin, 'rustup');
    writeFileSync(rustup, '#!/bin/bash\n[[ "$*" == "which --toolchain 1.97.1 cargo" ]] || exit 79\nprintf "%s\\n" "$REAL_CARGO"\n', { mode: 0o755 });
    writeFileSync(join(bin, 'node'), '#!/bin/bash\n[[ "$*" == ".github/tatagate/index.mjs remote" ]] || exit 80\nprintf "%s" "$TATAGATE_CARGO" > "$TRACE"\n', { mode: 0o755 });
    const trace = join(work, 'trace');
    const run = () => spawnSync('/bin/bash', ['-c', script], { encoding: 'utf8', env: { PATH: bin + ':/usr/bin:/bin', REAL_CARGO: cargo, TRACE: trace } });
    const valid = run(); assert.equal(valid.status, 0, valid.stderr); assert.equal(readFileSync(trace, 'utf8'), cargo);
    rmSync(trace); writeFileSync(rustup, '#!/bin/bash\nexit 71\n', { mode: 0o755 });
    assert.equal(run().status, 71); assert.equal(existsSync(trace), false);
  } finally { rmSync(work, { recursive: true, force: true }); }
});

// 准确坐标、首次提交、唯一无父根和带父覆盖分别验证，防止历史清理扩大强推范围。
test('历史清理仅接受唯一无父新根并完整检查全部内容', async () => {
  const { pushBaseSHA } = await import('./index.mjs');
  const headSHA = 'a'.repeat(40), before = 'b'.repeat(40), empty = '4b825dc642cb6eb9a060e54bf8d69288fbee4904';
  const reset = { forced: true, before, headSHA, parents: headSHA, commitCount: '1' };
  assert.equal(pushBaseSHA({ forced: false, before, headSHA }), before);
  assert.equal(pushBaseSHA({ forced: false, before: '0'.repeat(40), headSHA }), empty);
  assert.equal(pushBaseSHA(reset), empty);
  for (const invalid of [
    { ...reset, parents: headSHA + ' ' + before }, { ...reset, commitCount: '2' },
    { ...reset, parents: '' }, { ...reset, forced: 'true' },
    { ...reset, before: 'main' }, { ...reset, headSHA: 'main' },
    { ...reset, headSHA: '0'.repeat(40) }, { ...reset, before: '0'.repeat(40) },
  ]) assert.throws(() => pushBaseSHA(invalid));
});

// 执行真实环境过滤函数；准确编译器交付保留，凭据和其他产品路径不得继承。
test('链门禁子进程保留宿主链接器和SDK并隔离无关环境', async () => {
  const { readFile } = await import('node:fs/promises');
  const source = await readFile(new URL('./index.mjs', import.meta.url), 'utf8');
  const body = source.slice(source.indexOf('function environment(root, work) {'), source.indexOf('export async function executeGate('));
  const environment = new Function('process', 'resolve', 'contract', 'mkdirSync', body + '\nreturn environment;')({ env: {
    PATH: '/controlled/bin', DEVELOPER_DIR: '/verified/Developer', SDKROOT: '/verified/SDK',
    CC: '/verified/clang', CXX: '/verified/clang++', CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER: '/verified/clang',
    GITHUB_TOKEN: 'forbidden', OTHER_PRODUCT_ROOT: '/other', DYLD_INSERT_LIBRARIES: '/untrusted',
  } }, (...parts) => parts.join('/'), { repository: 'citizenchain' }, () => {});
  const env = environment('/chain', '/task');
  assert.equal(env.CC, '/verified/clang'); assert.equal(env.CXX, '/verified/clang++');
  assert.equal(env.CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER, env.CC);
  assert.equal(env.SDKROOT, '/verified/SDK'); assert.equal(env.DEVELOPER_DIR, '/verified/Developer');
  for (const key of ['GITHUB_TOKEN', 'OTHER_PRODUCT_ROOT', 'DYLD_INSERT_LIBRARIES']) assert.equal(env[key], undefined);
});

// 本仓target是唯一源码内生成边界；嵌套或链接旁路仍必须拒绝。
test('产品门禁允许自有根target并拒绝嵌套与链接输出', async () => {
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } = await import('node:fs');
  const { join } = await import('node:path');
  const { testRoot } = await import('../../scripts/build.mjs');
  const { assertNoProductOutputDirectories, gateContract } = await import('./index.mjs');
  const fixture = mkdtempSync(join(testRoot(), 'target-boundary-'));
  const root = join(fixture, 'source'), target = join(root, 'target');
  mkdirSync(root);
  try {
    mkdirSync(join(target, 'test', 'build'), { recursive: true });
    writeFileSync(join(target, 'test', 'build', 'generated.txt'), 'generated fixture');
    assert.doesNotThrow(() => assertNoProductOutputDirectories(root, gateContract().repository));
    const nested = join(root, 'source', 'target');
    mkdirSync(nested, { recursive: true });
    assert.throws(() => assertNoProductOutputDirectories(root, gateContract().repository), /生成状态目录/u);
    rmSync(join(root, 'source'), { recursive: true });
    rmSync(target, { recursive: true });
    const outside = join(fixture, 'outside'); mkdirSync(outside);
    symlinkSync(outside, target, 'dir');
    assert.throws(() => assertNoProductOutputDirectories(root, gateContract().repository), /生成状态目录/u);
    rmSync(target);
    writeFileSync(target, 'ordinary file');
    assert.throws(() => assertNoProductOutputDirectories(root, gateContract().repository), /生成状态目录/u);
  } finally { rmSync(fixture, { recursive: true, force: true }); }
});

// 所属根文档验收只读本仓，负向夹具在本产品target内，不借其它仓库资料。
test('所属根技术文档拒绝缺失、空文件、链接、副本与错误文件类型', async () => {
  const { validateProductDocuments } = await import('./index.mjs');
  const { mkdtempSync, writeFileSync, unlinkSync, symlinkSync, mkdirSync, rmSync } = await import('node:fs');
  const { join } = await import('node:path');
  const { testRoot } = await import('../../scripts/build.mjs');
  const root = mkdtempSync(join(testRoot(), 'product-documents-'));
  const names = ["CitizenChainNode.md", "CitizenChainRuntime.md", "CitizenChainOnChina.md"];
  try {
    assert.throws(() => validateProductDocuments(root), /根技术文档/u);
    for (const name of names) writeFileSync(join(root, name), '产品技术文档\n');
    writeFileSync(join(root, 'README.md'), '产品简介\n');
    assert.equal(validateProductDocuments(root), true);
    const file = join(root, names[0]);
    writeFileSync(file, '');
    assert.throws(() => validateProductDocuments(root), /根技术文档/u);
    unlinkSync(file); symlinkSync(join(root, 'README.md'), file);
    assert.throws(() => validateProductDocuments(root), /根技术文档/u);
    unlinkSync(file); mkdirSync(file);
    assert.throws(() => validateProductDocuments(root), /根技术文档/u);
    rmSync(file, { recursive: true }); writeFileSync(file, '产品技术文档\n');
    writeFileSync(join(root, 'Extra.md'), '第二技术文档\n');
    assert.throws(() => validateProductDocuments(root), /额外技术文档/u);
    unlinkSync(join(root, 'Extra.md'));
    const readme = join(root, 'README.md'); unlinkSync(readme); symlinkSync(file, readme);
    assert.throws(() => validateProductDocuments(root), /非空普通原件/u);
    unlinkSync(readme); writeFileSync(readme, '产品简介\n');
    assert.equal(validateProductDocuments(root), true);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// 文档迁出后保留同等资料扫描，测试只使用合成材料。
test('根技术文档机密扫描保留正文、令牌和转义快照拒绝', async () => {
  const { hasSecretMaterial } = await import('./index.mjs');
  const header = type => '-----BEGIN ' + type + 'PRIVATE KEY-----';
  const footer = type => '-----END ' + type + 'PRIVATE KEY-----';
  for (const type of ['', 'RSA ', 'EC ', 'OPENSSH ']) {
    const begin = header(type), end = footer(type);
    assert.equal(hasSecretMaterial('识别格式 ' + JSON.stringify(begin)), false);
    assert.equal(hasSecretMaterial(begin + '\\n\\(fixtureData.base64EncodedString())\\n' + end), false);
    const shaped = begin + '\n' + 'A'.repeat(96) + '\n' + end;
    assert.equal(hasSecretMaterial(shaped), true);
    assert.equal(hasSecretMaterial(shaped.replaceAll('\n', '\\n')), true);
    assert.equal(hasSecretMaterial(JSON.stringify({ original: shaped })), true);
    const escaped = JSON.stringify({ original: shaped }).replace('BEGIN', '\\u0042EGIN');
    assert.equal(hasSecretMaterial(escaped), true);
    assert.equal(hasSecretMaterial(JSON.stringify({ original: JSON.stringify(shaped).replace('BEGIN', '\\u0042EGIN') })), true);
    assert.equal(hasSecretMaterial(JSON.stringify({ [shaped]: '合成键名' }).replace('BEGIN', '\\u0042EGIN')), true);
    const snapshot = '<!-- PATCH_DATA\n' + escaped + '\nPATCH_DATA -->';
    assert.equal(hasSecretMaterial(snapshot), true);
    assert.equal(hasSecretMaterial(begin + '\n' + 'A'.repeat(32)), true);
  }
  for (const [prefix, length] of [['AKIA', 16], ['github_pat_', 20], ['ghp_', 30], ['sk_live_', 16]]) {
    assert.equal(hasSecretMaterial(prefix + 'A'.repeat(length)), true);
    assert.equal(hasSecretMaterial(JSON.stringify({ example: prefix + 'A'.repeat(length) })), true);
  }
  assert.equal(hasSecretMaterial('格式说明，没有凭据正文'), false);
  assert.equal(hasSecretMaterial(header('') + '\nfixture-only\n' + footer('')), false);
  assert.throws(() => hasSecretMaterial('<!-- PATCH_DATA\n{}'), /快照结构/u);
  assert.throws(() => hasSecretMaterial('<!-- PATCH_DATA\ninvalid\nPATCH_DATA -->'), /快照结构/u);
  assert.throws(() => hasSecretMaterial(null), /输入必须/u);
});
