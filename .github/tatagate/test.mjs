import { gateToolInterfaces } from '../../scripts/resources.mjs';
const { toolEnvironment } = gateToolInterfaces;
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
test('公开链真源只读准确SHA，拒绝main、网络、重定向、超限及伪造坐标',async()=>{
 const sha='a'.repeat(40),path='runtime/src/lib.rs';
 assert.equal(await readPublicChain(path,sha,async(url,options)=>{
  assert.equal(url,'https://raw.githubusercontent.com/crcfrcn/citizenchain/'+sha+'/'+path);
  assert.equal(options.redirect,'error');assert.equal(options.credentials,'omit');assert.equal(options.headers.Authorization,undefined);
  return new Response('source');
 }),'source');
 for(const [path,sha]of [[null,null],['../private','a'.repeat(40)],['runtime/src/lib.rs','main'],['runtime/src/lib.rs','a'.repeat(39)]]){
  await assert.rejects(readPublicChain(path,sha,()=>assert.fail('非法坐标禁止联网')));
 }
 for(const request of [async()=>{throw Error('synthetic network failure');},async()=>new Response('',{status:302}),async()=>new Response('',{status:404}),async()=>new Response(Buffer.alloc(2*1024**2+1)),async()=>new Response(new Uint8Array([255]))])await assert.rejects(readPublicChain(path,sha,request),/准确提交真源读取失败/u);
});

// 用隔离的合成Git提交验证门禁读取真实初始内容；不修改产品仓或调用仓库保存/推送。
test('保留源码不按每文件汉字数量判定，真实第一方临时注释仍拒绝', async () => {
  const [{ mkdtempSync, mkdirSync, writeFileSync, rmSync }, { join }, { testRoot: tmpdir }, { execFileSync }, { validateQuality }] = await Promise.all([
    import('node:fs'), import('node:path'), import('../../scripts/build.mjs'), import('node:child_process'), import('./index.mjs'),
  ]);
  const root = mkdtempSync(join(tmpdir(), 'tatagate-quality-'));
  const env = { ...toolEnvironment(), HOME: process.env.HOME, LANG: 'C', LC_ALL: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
    GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid' };
  const git = (...args) => execFileSync(env.PRODUCT_GIT_BIN, ['-C', root, ...args], { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
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
  const env = { ...toolEnvironment(), HOME: process.env.HOME, LANG: 'C', LC_ALL: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null',
    GIT_AUTHOR_NAME: 'Fixture', GIT_AUTHOR_EMAIL: 'fixture@example.invalid',
    GIT_COMMITTER_NAME: 'Fixture', GIT_COMMITTER_EMAIL: 'fixture@example.invalid' };
  const git = (...args) => execFileSync(env.PRODUCT_GIT_BIN, ['-C', root, ...args], { env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim();
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
test('链Python RPC只接受严格HTTPS输入',async()=>{const {BUILD_SOURCES,testRoot}=await import('../../scripts/build.mjs');const {mkdtempSync,writeFileSync,rmSync}=await import('node:fs');const {join}=await import('node:path');const {spawnSync}=await import('node:child_process');const dir=mkdtempSync(join(testRoot(),'rpc-python-'));try{const file=join(dir,'check.py');writeFileSync(file,BUILD_SOURCES.constitution);for(const [url,rejected]of [['https://example.invalid',false],['http'+ '://localhost',true],['ws'+ '://localhost',true],['https://user:pass@example.invalid',true],['',true]]){const r=spawnSync(process.env.PRODUCT_TEST_PYTHON,['-B','-c','import runpy,sys; module=runpy.run_path(sys.argv[1]); module["RpcTop"](sys.argv[2],None)',file,url],{encoding:'utf8'});assert.equal(r.status!==0,rejected,r.stderr);}}finally{rmSync(dir,{recursive:true,force:true});}});
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

// 实际Workflow只进入本仓完整资源与门禁入口；不以Rustup shim或PATH替身交付Cargo。
test('远端链门禁只调用本仓完整入口，命令失败据实传播',async()=>{
 const [{readFileSync},{runInNewContext}]=await Promise.all([import('node:fs'),import('node:vm')]);
 const source=readFileSync(new URL('../workflows/tatagate.yml',import.meta.url),'utf8');
 const step=source.split('      - name: 本仓资源准备与完整塔塔门禁\n')[1];assert.ok(step);
 const block=step.split('        run: |\n')[1];assert.ok(block);
 const script=block.split('\n').filter(line=>line.startsWith('          ')).map(line=>line.slice(10)).join('\n');
 const calls=[],failure=Error('合成命令失败');let reject=false;
 const context={process:{execPath:process.execPath},require(id){assert.equal(id,'node:child_process');return{execFileSync(command,args,options){calls.push({command,args,options});if(reject)throw failure;}};}};
 runInNewContext(script,context);assert.equal(calls.length,1);assert.equal(calls[0].command,process.execPath);
 assert.deepEqual(Array.from(calls[0].args),['.github/tatagate/index.mjs','remote']);assert.equal(calls[0].options.stdio,'inherit');
 reject=true;assert.throws(()=>runInNewContext(script,{...context}),error=>error===failure);assert.equal(calls.length,2);
 assert.doesNotMatch(script,/rustup|which|TATAGATE_CARGO/u);
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
  const { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync, unlinkSync } = await import('node:fs');
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
    unlinkSync(target);
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


// 正负输入均经过本仓真实门禁的完整Git跟踪扫描，不裁剪补丁或增加生产测试出口。
test('官方补丁只处理准确原上下文，未使用补丁和工具声明的来源摘要及其它文字仍严格检查', async () => {
  const { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } = await import('node:fs');
  const { join, isAbsolute } = await import('node:path');
  const { execFileSync } = await import('node:child_process');
  const { createHash } = await import('node:crypto');
  const { testRoot } = await import('../../scripts/build.mjs');
  const { validatePlatformNaming } = await import('./index.mjs');
  const gitBin = process.env.PRODUCT_GIT_BIN;
  assert.ok(typeof gitBin === 'string' && isAbsolute(gitBin), '测试需要当前获准Git绝对入口');
  const root = mkdtempSync(join(testRoot(), 'tatagate-patch-'));
  const env = { HOME: process.env.HOME, PATH: process.env.PATH, LANG: 'C', LC_ALL: 'C',
    GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: '/dev/null' };
  const git = (...args) => execFileSync(gitBin,
    ['-c', 'credential.helper=', '-c', 'core.hooksPath=/dev/null', '-C', root, ...args],
    { env, stdio: ['ignore', 'pipe', 'pipe'] });
  const source = readFileSync(new URL('../../scripts/resources.mjs', import.meta.url), 'utf8');
  const tools = JSON.parse(source.match(/^const toolDefinitions=(\[.*\]);$/mu)[1]);
  const patch = JSON.parse(source.match(/^const flutterPatch=(".*");$/mu)[1]);
  assert.equal(tools.filter(tool => tool.id === 'flutter').length, 0);
  const marker = ['macos', 'arm64'].join(' '), legacy = ['macos', 'arm64'].join('_');
  const comment = ' /// ios device or ' + marker + '.';
  const hash = body => createHash('sha256').update(body).digest('hex');
  const declaration = values => 'const toolDefinitions=' + JSON.stringify(values) + ';\n';
  const patchDeclaration = body => 'const flutterPatch=' + JSON.stringify(body) + ';\n';
  const combined = (values = tools, body = patch) => declaration(values) + patchDeclaration(body);
  // 合成工具只用于扫描边界；不登记、下载或运行Flutter，也不声称产品实际使用该工具。
  const flutter = { id: 'flutter', version: '3.47.2',
    source: 'https://storage.googleapis.com/flutter_infra_release/releases/releases_macos.json',
    archive: { url: 'https://storage.googleapis.com/flutter_infra_release/releases/stable/macos/flutter_'
      + legacy + '_3.47.2-stable.zip',
      sha256: 'f456fd6733053d9301828a2e702d6cbec872923126809aa8c48eb0a696d6cc01',
      root: 'flutter', executable: 'bin/flutter', kind: 'extract' },
    patch: { path: 'flutter.patch', sha256: hash(patch),
      source: 'https://github.com/flutter/flutter/commit/d3b14c876900e553bc736ca19295fc09e3853e8e' } };
  const file = join(root, 'scripts', 'resources.mjs');
  const accept = body => {
    writeFileSync(file, body);
    assert.doesNotThrow(() => validatePlatformNaming(root));
    assert.equal(readFileSync(file, 'utf8'), body, '扫描不得改写物理源码');
  };
  const reject = body => {
    writeFileSync(file, body);
    assert.throws(() => validatePlatformNaming(root), /禁用平台命名/u);
    assert.equal(readFileSync(file, 'utf8'), body);
  };
  const changedTool = change => { const tool = structuredClone(flutter); change(tool); return combined([tool]); };
  const changedPatch = change => {
    const body = change(patch), tool = structuredClone(flutter);
    tool.patch.sha256 = hash(body);
    return combined([tool], body);
  };
  try {
    git('init', '--quiet', '--initial-branch=main');
    mkdirSync(join(root, 'scripts')); mkdirSync(join(root, '.github', 'tatagate'), { recursive: true });
    writeFileSync(join(root, '.github', 'tatagate', 'contracts.json'), JSON.stringify(gateContract()));
    writeFileSync(file, source); git('add', '--all');
    accept(source);
    accept(combined());
    accept(patchDeclaration(patch) + declaration(tools));
    accept(combined([flutter]));
    accept(patchDeclaration(patch) + declaration([flutter]));
    for (const body of [
      patch + '\n', patch.replace('fixed source', 'other source'),
      patch.replace('d3b14c876900e553bc736ca19295fc09e3853e8e', '0'.repeat(40)),
      patch.replace('Future<void> lipoDylibs', 'Future<void> changed'),
      patch.replaceAll('native_assets_host.dart', 'other.dart'),
      patch.replace(comment, '+' + comment.slice(1)), patch + patch,
      patch + '\n+// ' + marker + '\n',
    ]) reject(combined(tools, body));
    // 伪造登记摘要也不能放行其它新增行、原上下文、旧名字或改变核实位置。
    for (const invalid of [
      changedPatch(body => body.replace('Future<void> lipoDylibs', 'Future<void> changed')),
      changedPatch(body => body.replaceAll('native_assets_host.dart', 'other.dart')),
      changedPatch(body => body.replace('@@ -66,7 +66,8 @@', '@@ -67,7 +67,8 @@')),
      changedPatch(body => body.replace(comment, '+' + comment.slice(1))),
      changedPatch(body => body + '\n+// ' + marker + '\n'),
      changedPatch(body => body + '\n // ' + marker + '\n'),
      changedPatch(body => body + '\n-// ' + marker + '\n'),
      changedPatch(body => body + '\n' + body),
      changedTool(tool => { tool.patch.source = 'https://example.invalid/commit/' + 'a'.repeat(40); }),
      changedTool(tool => { tool.patch.source = tool.patch.source.replace('https:', 'http:'); }),
      changedTool(tool => { tool.patch.source = 'https://github.com/flutter/flutter/commit/' + '0'.repeat(40); }),
      changedTool(tool => { tool.patch.sha256 = '0'.repeat(64); }),
      changedTool(tool => { tool.patch.path = 'other.patch'; }),
      changedTool(tool => { tool.patch.extra = 'unexpected'; }),
      changedTool(tool => { tool.archive.url = tool.archive.url.replace('storage.googleapis.com', 'example.invalid'); }),
      changedTool(tool => { tool.archive.url = tool.archive.url.replace('https:', 'http:'); }),
      changedTool(tool => { tool.version = '0.0.0'; }),
      changedTool(tool => { tool.archive.url = tool.archive.url.replace('-stable.zip', '-other.zip'); }),
      changedTool(tool => { tool.source = 'https://example.invalid/releases.json'; }),
      changedTool(tool => { tool.archive.root = 'other'; }),
      changedTool(tool => { tool.archive.executable = 'other'; }),
      changedTool(tool => { tool.archive.kind = 'native-source'; }),
      changedTool(tool => { tool.archive.sha256 = 'invalid'; }),
      changedTool(tool => { tool.title = legacy; }),
      changedTool(tool => { tool.archive.extra = legacy; }),
      combined([flutter, flutter]), combined([...tools, tools[0]]),
      combined([null]), combined([[]]), combined([{ id: 1 }]), combined([{ id: 'invalid id' }]), combined([]),
      combined() + declaration(tools),
      combined() + declaration(tools).replace('const toolDefinitions=', 'const toolDefinitions = '),
      combined() + patchDeclaration(patch),
      combined() + patchDeclaration(patch).replace('const flutterPatch=', 'let flutterPatch = '),
      combined().replace('"id":', '"id":"other","id":'),
      combined().replace('"id":', '"\\u0069d":'),
      combined().replace('fixed source', 'fixed\\u0020source'),
      combined().replace('const toolDefinitions=', 'const toolDefinitions = '),
      declaration(tools) + 'const flutterPatch=' + JSON.stringify(patch).slice(0, -1) + ';\n',
      'const toolDefinitions=[invalid];\n' + patchDeclaration(patch),
      'const toolDefinitions=' + JSON.stringify({ tools }) + ';\n' + patchDeclaration(patch),
      'const toolDefinitions=' + JSON.stringify([flutter]).replace('"flutter"', '"flutt\\u0065r"') + ';\n' + patchDeclaration(patch),
      combined() + '// ' + marker + '\n',
    ]) reject(invalid);
    for (const alias of gateContract().platform_forbidden_values) reject(combined() + '// ' + alias);
    accept(combined());
    const other = join(root, 'other.mjs'); writeFileSync(other, combined()); git('add', '--all');
    assert.throws(() => validatePlatformNaming(root), /禁用平台命名/u);
    rmSync(other); git('add', '--all');
    mkdirSync(join(root, legacy)); writeFileSync(join(root, legacy, 'source.mjs'), 'export const fixture=true;\n');
    git('add', '--all');
    assert.throws(() => validatePlatformNaming(root), /禁用平台目录/u);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

// 用真实门禁函数检查登记与执行回执；这些用例在整项实现后统一运行。
test('本仓Git测试集合不得漏项、增项、重复或混入门禁自身', async () => {
  const { validateNodeInventory } = await import('./index.mjs');
  const paths = ['scripts/build.mjs', 'scripts/build.test.mjs', 'test/api.spec.mjs', '.github/tatagate/test.mjs'];
  const registered = ['scripts/build.test.mjs', 'test/api.spec.mjs'];
  assert.deepEqual(validateNodeInventory(paths, registered,undefined,()=>''), registered);
  for (const listed of [registered.slice(1), [...registered, 'missing.test.mjs'], [...registered, registered[0]], []]) {
    assert.throws(() => validateNodeInventory(paths, listed,undefined,()=>''));
  }
  assert.throws(() => validateNodeInventory([...paths, 'scripts/new.test.mjs'], registered,undefined,()=>''));
  assert.throws(() => validateNodeInventory([...paths, paths[0]], registered,undefined,()=>''));
});
test('成功退出但零用例、失败、取消或跳过不能作为完整测试回执', async () => {
  const { successfulTestSummary } = await import('./index.mjs');
  const counts = { tests: 2, passed: 2, failed: 0, skipped: 0, todo: 0, cancelled: 0 };
  assert.equal(successfulTestSummary({ success: true, counts }), true);
  for (const change of [{ tests: 0 }, { passed: 0 }, { failed: 1 }, { skipped: 1 }, { todo: 1 }, { cancelled: 1 }]) {
    assert.equal(Boolean(successfulTestSummary({ success: true, counts: { ...counts, ...change } })), false);
  }
  assert.equal(Boolean(successfulTestSummary({ success: false, counts })), false);
  assert.equal(Boolean(successfulTestSummary({ success: true })), false);
});
test('资源中的注释文本与正则字面量不是第一方代码注释', async () => {
  const { commentText, hasFirstPartyTemporaryComments } = await import('./index.mjs');
  const source = 'const patch = "// TODO upstream\\n/* FIXME original */";\nconst literal = /\\/\\/ XXX/;\n// 正常中文实现说明\n';
  assert.equal(hasFirstPartyTemporaryComments('scripts/resources.mjs', source), false);
  assert.match(commentText('module.mjs', source), /正常中文实现说明/u);
  assert.equal(hasFirstPartyTemporaryComments('module.mjs', source + '// TODO first party\n'), true);
  assert.equal(hasFirstPartyTemporaryComments('module.rs', 'let raw = r##"// TODO raw"##;\n// 中文说明\n'), false);
  assert.equal(hasFirstPartyTemporaryComments('module.rs', "fn bind<'a>() {} // FIXME actual\n"), true);
});

// 真实词法和消费者闭合，模板表达式中的真实注释仍参与检查。
test('注释检查区分模板正文、模板表达式、Python文串和真实行尾注释',async()=>{
 const {hasFirstPartyTemporaryComments}=await import('./index.mjs');
 assert.equal(hasFirstPartyTemporaryComments('module.mjs','const value=`// TODO text ${1}`;'),false);
 assert.equal(hasFirstPartyTemporaryComments('module.mjs','const value=`text ${(()=>{ // FIXME actual\n return 1; })()}`;'),true);
 assert.equal(hasFirstPartyTemporaryComments('module.py','value="""# TODO text"""\nvalue=1 # FIXME actual\n'),true);
 assert.equal(hasFirstPartyTemporaryComments('module.py','value="""# TODO text"""\n'),false);
});
// 使用真实Node运行器和实际门禁Reporter；不以伪造汇总对象代替最终执行回执。
test('实际NodeReporter拒绝漏文件、零用例、跳过和失败',async()=>{
 const [{mkdtempSync,writeFileSync,rmSync},{join},{testRoot},{spawnSync},{fileURLToPath}]=await Promise.all([import('node:fs'),import('node:path'),import('../../scripts/build.mjs'),import('node:child_process'),import('node:url')]);
 const work=mkdtempSync(join(testRoot(),'gate-reporter-')),file=join(work,'case.test.mjs'),reporter=fileURLToPath(new URL('./index.mjs',import.meta.url));
 try{
  for(const [body,extra,success]of [
   ['import test from "node:test";test("正常夹具",()=>{});',[],true],
   ['export const noTests=true;',[],false],
   ['import test from "node:test";test.skip("跳过夹具",()=>{});',[],false],
   ['import test from "node:test";test("失败夹具",()=>{throw Error("synthetic failure")});',[],false],
   ['import test from "node:test";test("漏项夹具",()=>{});',[join(work,'missing.test.mjs')],false],
  ]){
   // 子Node必须是独立运行器；保留产品工具输入，只移除父运行器的内部测试上下文。
   const childEnvironment={...process.env,TATAGATE_NODE_TESTS:JSON.stringify([file,...extra])};delete childEnvironment.NODE_TEST_CONTEXT;
   writeFileSync(file,body);const result=spawnSync(process.execPath,['--test','--test-reporter='+reporter,file],{env:childEnvironment,encoding:'utf8',timeout:30000,maxBuffer:2*1024**2});
   assert.equal(result.error,undefined);assert.equal(result.signal,null);assert.equal(result.status===0,success,result.stdout+result.stderr);
  }
 }finally{rmSync(work,{recursive:true,force:true});}
});

test('实际语言回执拒绝零用例、跳过、失败及不完整终态',async()=>{
 const {validateLanguageResult}=await import('./index.mjs');
 const vitest={success:true,numTotalTests:2,numPassedTests:2,numFailedTests:0,numPendingTests:0,numTodoTests:0};assert.equal(validateLanguageResult('vitest',JSON.stringify(vitest)),true);
 for(const change of [{numTotalTests:0,numPassedTests:0},{numPassedTests:1},{numPendingTests:1},{success:false}])assert.throws(()=>validateLanguageResult('vitest',JSON.stringify({...vitest,...change})));
 const flutter=JSON.stringify({type:'testDone',result:'success',skipped:false,hidden:false})+'\n'+JSON.stringify({type:'done',success:true});assert.equal(validateLanguageResult('flutter',flutter),true);
 for(const invalid of ['',JSON.stringify({type:'done',success:true}),flutter.replace('"skipped":false','"skipped":true'),flutter.replace('"success":true','"success":false')])assert.throws(()=>validateLanguageResult('flutter',invalid));
 assert.equal(validateLanguageResult('cargo','test result: ok. 2 passed; 0 failed; 0 ignored;'),true);
 for(const invalid of ['', 'test result: ok. 0 passed; 0 failed; 0 ignored;', 'test result: ok. 2 passed; 0 failed; 1 ignored;'])assert.throws(()=>validateLanguageResult('cargo',invalid));
});

test('代码变化必须同步所属文档与非空回归差异',async()=>{
 const {validateChangeEvidence}=await import('./index.mjs');
 assert.equal(validateChangeEvidence(['src/main.mjs','Owned.md','scripts/main.test.mjs'],['Owned.md']),true);
 assert.equal(validateChangeEvidence(['Owned.md'],['Owned.md']),true);
 for(const paths of [['src/main.mjs'],['src/main.mjs','Owned.md'],['src/main.mjs','Foreign.md','scripts/main.test.mjs']])assert.throws(()=>validateChangeEvidence(paths,['Owned.md']));
 assert.throws(()=>validateChangeEvidence(['src/main.mjs','Owned.md','scripts/main.test.mjs'],['Owned.md'],{changed:path=>path!=='Owned.md'}));
});

// 调用真实结果核验接口；合成协议事件仅验证核验器，不能作为产品功能通过证据。
test('功能映射必须闭合，拒绝遗漏类型、重复来源和路径越界',async()=>{
 const {validateFunctionalContract,gateContract}=await import('./index.mjs');const list=structuredClone(gateContract().functions);
 assert.equal(validateFunctionalContract(list),true);
 for(const invalid of [[],[...list,list[0]],list.map((item,index)=>index?item:{...item,path:'../foreign.test.mjs'}),list.map((item,index)=>index?item:{...item,target:'/foreign/Cargo.toml'}),list.map((item,index)=>index?item:{...item,runner:'skip'}),list.map((item,index)=>index?item:{...item,unknown:true})])assert.throws(()=>validateFunctionalContract(invalid));
 const value=structuredClone(gateContract());value.functions=value.functions.filter(item=>item.path!==value.node_tests[0]);assert.throws(()=>gateContract(value));
});
test('Vitest必须逐一完成本仓具名文件，错路径、漏跑和重复结果均拒绝',async()=>{
 const {functionalFiles}=await import('./index.mjs');const root='/owned/source',paths=['test/one.test.ts','test/two.test.ts'];
 const suite=name=>({name:root+'/'+name,assertionResults:[{status:'passed'}]}),value={success:true,numTotalTests:2,numPassedTests:2,numFailedTests:0,numPendingTests:0,numTodoTests:0,testResults:paths.map(suite)};
 assert.deepEqual(functionalFiles('vitest',JSON.stringify(value),paths,[root]),paths.map(path=>({path,tests:1})));
 for(const change of [{testResults:[suite(paths[0])]},{testResults:[suite(paths[0]),suite(paths[0])]},{testResults:[suite(paths[0]),{...suite(paths[1]),name:'/foreign/'+paths[1]}]},{testResults:[suite(paths[0]),{...suite(paths[1]),assertionResults:[]}]},{testResults:[suite(paths[0]),{...suite(paths[1]),assertionResults:[{status:'skipped'}]}]}])assert.throws(()=>functionalFiles('vitest',JSON.stringify({...value,...change}),paths,[root]));
});
test('Flutter加载事件不能代替实际用例，每个本仓套件均需成功',async()=>{
 const {functionalFiles}=await import('./index.mjs');const path='test/feature_test.dart',events=[{type:'suite',suite:{id:1,path:'/owned/source/'+path}},{type:'testStart',test:{id:1,suiteID:1,name:'真实协议夹具',hidden:false}},{type:'testDone',testID:1,hidden:false,result:'success',skipped:false},{type:'done',success:true}];
 const text=value=>value.map(row=>JSON.stringify(row)).join('\n');
 assert.deepEqual(functionalFiles('flutter',text(events),[path],['/owned/source']),[{path,tests:1}]);
 for(const value of [events.filter(item=>item.type!=='testDone'),events.map(item=>item.type==='testDone'?{...item,skipped:true}:item),events.map(item=>item.type==='suite'?{...item,suite:{...item.suite,path:'/foreign/'+path}}:item),[events[0],events[1],events[2],events[2],events[3]]])assert.throws(()=>functionalFiles('flutter',text(value),[path],['/owned/source']));
 assert.throws(()=>functionalFiles('flutter',text(events),[path,'test/missing_test.dart'],['/owned/source']));
});
test('Rust功能结果必须属于准确包，完整摘要不等于具名用例执行',async()=>{
 const {functionalRustCases}=await import('./index.mjs'),items=[{path:'src/auth.rs',package:'owned-package',cases:['reject_expired']},{path:'tests/boundary.rs',package:'owned-package',cases:['reject_foreign']}];
 const value='test auth::reject_expired ... ok\ntest reject_foreign ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored;';
 assert.deepEqual(functionalRustCases(value,items),items.map(item=>({path:item.path,cases:item.cases})));
 for(const invalid of [value.replace('test reject_foreign ... ok\n',''),value.replace('0 ignored','1 ignored'),value.replace('2 passed','0 passed')])assert.throws(()=>functionalRustCases(invalid,items));
 assert.throws(()=>functionalRustCases(value,[items[0],{...items[1],package:'foreign-package'}]));
 assert.throws(()=>functionalRustCases(value,[items[0],{...items[1],cases:['reject_expired']} ]));
});

// 固定协调参数不承载产品产物；目录身份及空状态必须真实验证。
test('门禁请求协调目录拒绝相对、源码和无效目录',async()=>{
 const {validateGateRequestWork}=await import('./index.mjs');
 assert.throws(()=>validateGateRequestWork('/owned/source','relative/work'));
 assert.throws(()=>validateGateRequestWork('/owned/source','/nonexistent/owned/gate-work'));
});

// 回读本仓实际已跟踪测试来源；完整映射不可空跑、漏登记或混入不存在的入口。
test('本仓真实功能源码清单与登记准确闭合',async()=>{const {validateFunctionalInventory}=await import('./index.mjs');const {fileURLToPath}=await import('node:url');const {readdirSync,lstatSync}=await import('node:fs');const {join,relative}=await import('node:path');const root=fileURLToPath(new URL('../../',import.meta.url)).replace(/\/$/u,''),paths=[];function visit(dir){for(const n of readdirSync(dir)){if(['.git','target','node_modules'].includes(n))continue;const p=join(dir,n),s=lstatSync(p);if(s.isDirectory())visit(p);else if(s.isFile())paths.push(relative(root,p));}}visit(root);assert.equal(validateFunctionalInventory(root,gateContract().functions,paths).length,gateContract().functions.length);assert.throws(()=>validateFunctionalInventory(root,gateContract().functions.slice(1),paths),/本仓功能测试存在遗漏/);assert.throws(()=>validateFunctionalInventory(root,[...gateContract().functions,{function:'不存在',path:'missing.test.mjs',runner:'node',target:'.'}],paths),/本仓功能测试存在遗漏/);});