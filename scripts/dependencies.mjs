#!/usr/bin/env node
// CitizenChain开发者和CI按产品声明直接取得工具；取得结果只进入调用方源码外缓存。
import { createHash } from 'node:crypto';
import {
  createWriteStream,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
} from 'node:fs';
import { chmod, open } from 'node:fs/promises';
import { isAbsolute, join, resolve } from 'node:path';
import { pipeline } from 'node:stream/promises';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scripts = fileURLToPath(new URL('.', import.meta.url));
const contract = JSON.parse(readFileSync(join(scripts, 'dependencies.json'), 'utf8'));
const expectedVersion = '35.0';
const expectedSource = 'https://github.com/protocolbuffers/protobuf/releases/tag/v35.0';
const archiveNames = Object.freeze({
  macos: 'protoc-35.0-osx-aarch_64.zip',
  'linux-arm': 'protoc-35.0-linux-aarch_64.zip',
  'linux-amd': 'protoc-35.0-linux-x86_64.zip',
  windows: 'protoc-35.0-win64.zip',
});
function fail(message) { throw new Error(message); }
function safeWork(value) {
  if (!isAbsolute(value)) fail('CitizenChain工具工作目录必须是绝对路径');
  const source = realpathSync(join(scripts, '..'));
  const target = resolve(value);
  if (target === source || target.startsWith(source + '/')) fail('CitizenChain工具不得写入源码目录');
  mkdirSync(target, { recursive: true, mode: 0o700 });
  const actual = realpathSync(target);
  if (actual !== target) fail('CitizenChain工具工作目录禁止符号链接');
  return actual;
}
async function download(url, output) {
  const parsed = new URL(url);
  if (parsed.protocol !== 'https:' || parsed.hostname !== 'github.com' || parsed.username || parsed.password) fail('CitizenChain工具来源无效');
  let last;
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    const partial = `${output}.partial-${process.pid}-${attempt}`;
    try {
      const response = await fetch(url, { redirect: 'follow', signal: AbortSignal.timeout(300_000) });
      const final = new URL(response.url);
      if (!response.ok || !response.body) fail(`CitizenChain工具下载失败：${response.status}`);
      if (final.protocol !== 'https:'
          || !['github.com', 'release-assets.githubusercontent.com'].includes(final.hostname)
          || final.username || final.password) fail('CitizenChain工具重定向来源无效');
      await pipeline(response.body, createWriteStream(partial, { flags: 'wx', mode: 0o600 }));
      renameSync(partial, output);
      return;
    } catch (error) {
      rmSync(partial, { force: true });
      last = error;
    }
  }
  throw last;
}

// 白皮书来源只属于官网；每轮先捕获唯一main提交，再消费同一干净Git快照。
export function prepareWhitepaperSource(workValue) {
  const entry = contract.sources?.citizenweb, url = 'https://github.com/crcfrcn/citizenweb.git';
  if (entry?.url !== url || entry.ref !== 'main' || entry.path !== 'src') fail('白皮书唯一来源声明无效');
  const work = safeWork(workValue), source = join(work, 'citizenweb');
  const git = (args, cwd = work) => {
    const result = spawnSync('git', ['-c', 'credential.helper=', '-c', 'core.hooksPath=/dev/null',
      '-c', 'protocol.file.allow=never', '-c', 'gc.auto=0', '-C', cwd, ...args], {
      encoding: 'utf8', env: { ...process.env, GIT_TERMINAL_PROMPT: '0',
        GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null' },
    });
    if (result.error || result.status !== 0) fail('白皮书Git输入取得或验真失败');
    return result.stdout.trim();
  };
  const reference = git(['ls-remote', '--refs', url, 'refs/heads/main']);
  const match = /^([0-9a-f]{40})\s+refs\/heads\/main$/.exec(reference);
  if (!match) fail('白皮书main未指向唯一真实提交');
  if (!existsSync(source)) {
    mkdirSync(source, { mode: 0o700 }); const owned = lstatSync(source);
    try {
      git(['init', '--quiet'], source); git(['remote', 'add', 'origin', url], source);
      git(['fetch', '--no-tags', '--depth=1', 'origin', match[1]], source);
      git(['checkout', '--quiet', '--detach', match[1]], source);
    } catch (error) {
      const now = lstatSync(source, { throwIfNoEntry: false });
      if (now?.isDirectory() && !now.isSymbolicLink() && now.dev === owned.dev && now.ino === owned.ino) rmSync(source, { recursive: true });
      throw error;
    }
  }
  for (const p of [source, join(source, '.git')]) {
    const info = lstatSync(p);
    if (!info.isDirectory() || info.isSymbolicLink() || realpathSync(p) !== p) fail('白皮书Git目录无效');
  }
  if (resolve(git(['rev-parse', '--show-toplevel'], source)) !== source
      || resolve(git(['rev-parse', '--absolute-git-dir'], source)) !== join(source, '.git')
      || resolve(source, git(['rev-parse', '--git-common-dir'], source)) !== join(source, '.git')
      || git(['rev-parse', '--abbrev-ref', 'HEAD'], source) !== 'HEAD'
      || git(['rev-parse', 'HEAD'], source) !== match[1]
      || git(['remote', 'get-url', 'origin'], source) !== url
      || git(['status', '--porcelain=v1', '--untracked-files=all'], source)) fail('白皮书Git原件来源或内容已改变');
  const input = join(source, 'src/whitepaper.md'), info = lstatSync(input);
  if (!info.isFile() || info.isSymbolicLink() || !info.size) fail('白皮书原件缺失');
  return source;
}

async function main() {
  const [command, toolName, platform, workValue] = process.argv.slice(2);
  if (command !== 'prepare' || toolName !== 'protoc' || !workValue || process.argv.length !== 6) fail('CitizenChain工具参数无效');
  const entry = contract.tools?.protoc?.archives?.[platform];
  const archiveName = archiveNames[platform];
  const expectedURL = archiveName
    ? `https://github.com/protocolbuffers/protobuf/releases/download/v${expectedVersion}/${archiveName}`
    : null;
  const expectedExecutable = platform === 'windows' ? 'bin/protoc.exe' : 'bin/protoc';
  if (contract.schema !== 1 || contract.tools?.protoc?.version !== expectedVersion
      || contract.tools?.protoc?.source !== expectedSource
      || Object.keys(contract.tools.protoc.archives).sort().join(',') !== Object.keys(archiveNames).sort().join(',')
      || !entry || entry.url !== expectedURL || entry.executable !== expectedExecutable
      || !/^[a-f0-9]{64}$/.test(entry.sha256)) fail('CitizenChain protoc声明无效');
  const work = safeWork(workValue);
  const archive = join(work, archiveName);
  const payload = join(work, 'payload');
  rmSync(payload, { recursive: true, force: true });
  if (existsSync(archive)
      && createHash('sha256').update(readFileSync(archive)).digest('hex') !== entry.sha256) {
    rmSync(archive, { force: true });
  }
  if (!existsSync(archive)) await download(entry.url, archive);
  if (createHash('sha256').update(readFileSync(archive)).digest('hex') !== entry.sha256) {
    rmSync(archive, { force: true });
    fail('CitizenChain protoc摘要不符');
  }
  mkdirSync(payload, { mode: 0o700 });
  const result = spawnSync('unzip', ['-q', archive, '-d', payload], { stdio: 'inherit' });
  if (result.error || result.status !== 0) { rmSync(payload, { recursive: true, force: true }); fail('CitizenChain protoc解包失败'); }
  const executable = join(payload, entry.executable);
  if (!existsSync(executable) || !lstatSync(executable).isFile()) fail('CitizenChain protoc可执行文件无效');
  const handle = await open(executable, 'r');
  await handle.close();
  await chmod(executable, 0o700);
  const version = spawnSync(executable, ['--version'], { encoding: 'utf8' });
  if (version.error || version.status !== 0 || version.stdout.trim() !== `libprotoc ${expectedVersion}`) {
    rmSync(payload, { recursive: true, force: true });
    fail('CitizenChain protoc版本验真失败');
  }
  process.stdout.write(executable);
}
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch(error => { process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`); process.exitCode = 1; });
}
