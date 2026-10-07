#!/usr/bin/env node
import { remoteEnvironment as productRemoteEnvironment } from '../../../../build.mjs';
if(process.env.GITHUB_ACTIONS==='true'&&String(process.env.GITHUB_WORKFLOW||'').startsWith('citizenchain.'))Object.assign(process.env,productRemoteEnvironment());
import { spawnSync as runExactProcess } from 'node:child_process';

function validateCandidate() {
  const value=process.env;
if(!/^[0-9a-f]{40}$/.test(value.SOURCE_SHA||'')||!/^[1-9][0-9]*$/.test(value.CI_RUN_ID||'')||!/^\d+\.\d{1,2}\.\d{1,2}$/.test(value.SOFTWARE_VERSION||'')||value.VERSION_TAG!=='citizenchain-windows-v'+value.SOFTWARE_VERSION)throw Error('准确Release候选无效');
}

// 本文件只执行 citizenchain.windows.release 的 build-desktop Job；阶段编号由本仓唯一 Workflow 固定，禁止接收其它身份。
export const EXACT_REMOTE_JOB_IDENTITY = Object.freeze({"pipeline":"citizenchain.windows.release","job":"build-desktop"});

function requireExactRemoteJobEnvironment() {
  const expected = 'crcfrcn/citizenchain';
  if (!expected || process.env.GITHUB_REPOSITORY !== expected) {
    throw new Error('准确远端Job仓库身份无效');
  }
}
const workflowSteps = Object.freeze({"0":{"shell":"bash","source":"set -euo pipefail\ntest \"$(git rev-parse HEAD)\" = \"$GMB_SOURCE_SHA\"\nnode $GITHUB_WORKSPACE/scripts/node/release/windows/index.mjs node-version apply \"$GMB_SOFTWARE_VERSION\"\n"},"1":{"shell":"bash","source":"set -euo pipefail\nif [ \"${RUNNER_ARCH}\" != \"${EXPECTED_RUNNER_ARCH}\" ]; then\n  echo \"::error::runner 架构不符合 ${EXPECTED_RUNNER_ARCH}: 当前为 ${RUNNER_ARCH}\"\n  exit 1\nfi\n"},"2":{"shell":"bash","source":"set -euo pipefail\nnode <<'NODE'\nconst fs = require('fs');\nconst targetSource = process.env.CITIZENCHAIN_MANUAL_BUNDLE_TARGETS;\nconst bundleTargets = (targetSource || '')\n  .split(',')\n  .map((target) => target.trim())\n  .filter(Boolean);\nif (bundleTargets.length === 0) {\n  throw new Error('缺少本 matrix 的 Tauri bundle targets。');\n}\n// 候选版本已在当前 runner 工作区应用；这里只设置本 matrix 的 bundle 目标。\nconst tauriPath = 'node/tauri.conf.json';\nconst tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));\nconst version = tauri.version;\ntauri.bundle = tauri.bundle || {};\n// 中文注释：Release 生成安装包与 updater 正式文件；updater 在同一 runner 内签名。\ntauri.bundle.targets = bundleTargets;\n// 中文注释：按现有 Windows NSIS 文件格式在后续步骤签名并生成清单。\ndelete tauri.bundle.createUpdaterArtifacts;\n// 中文注释：平台 Release 可以独立成功，因此每个安装包只读取自己的更新清单；\n// 不能再用一个根 version 同时描述版本可能不同的四个目标。\ntauri.plugins = tauri.plugins || {};\ntauri.plugins.updater = tauri.plugins.updater || {};\ntauri.plugins.updater.endpoints = [\n  `https://github.com/crcfrcn/citizenchain/releases/download/${process.env.GMB_VERSION_TAG}/${process.env.CITIZENCHAIN_UPDATER_MANIFEST_NAME}`,\n];\nfs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\\n`);\nconsole.log(`桌面端版本(源码): ${version}`);\nconsole.log(`Tauri bundle targets: ${bundleTargets.join(',')}`);\nNODE\n"},"3":{"shell":"bash","source":"set -euo pipefail\ncase \"$RUNNER_OS/$RUNNER_ARCH\" in\n  macOS/ARM64) platform=macos ;;\n  Windows/X64) platform=windows ;;\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node scripts/dependencies.mjs prepare protoc \"$platform\" \"$RUNNER_TEMP/citizenchain-protoc/$platform\")\"\nprintf 'PROTOC=%s\\n' \"$protoc_executable\" >> \"$GITHUB_ENV\"\n"},"4":{"shell":"bash","source":"choco install llvm -y\necho \"LIBCLANG_PATH=C:\\Program Files\\LLVM\\lib\" >> \"$env:GITHUB_ENV\"\n"},"5":{"shell":"bash","source":"node $GITHUB_WORKSPACE/scripts/node/release/windows/index.mjs node-version lock \"$GMB_SOFTWARE_VERSION\""},"6":{"shell":"bash","source":"npm --prefix node/frontend ci\nnpm --prefix node/frontend run build\n"},"7":{"shell":"bash","source":"# 中文注释：安装包只携带节点软件；首次启动按冻结 chainspec 本地初始化并联网同步。\nrm -rf node/resources/genesis-state\n"},"8":{"shell":"bash","source":"node frontend/node_modules/@tauri-apps/cli/tauri.js build --ci -- --locked --config \"$GITHUB_WORKSPACE/config.toml\""},"9":{"shell":"pwsh","source":"$installerName = \"$env:CITIZENCHAIN_CONTEXT_MATRIX_INSTALLER_NAME\"\n$versionedName = $installerName -replace '\\.exe, \"-v$env:GMB_SOFTWARE_VERSION.exe\"\nNew-Item -ItemType Directory -Force \"$env:RUNNER_TEMP/citizenchain-release\" | Out-Null\n$exe = Get-ChildItem -Path \"$env:CARGO_TARGET_DIR/release/bundle/nsis\" -Filter \"*.exe\" | Select-Object -First 1\nif ($null -eq $exe) {\n  Write-Error \"未找到 Windows exe 安装包。\"\n  exit 1\n}\nCopy-Item $exe.FullName (Join-Path \"$env:RUNNER_TEMP/citizenchain-release\" $versionedName)\n# 中文注释：NSIS exe 同时作为 Windows updater 正式文件；后续步骤在同一 runner 内签名。\nGet-ChildItem \"$env:RUNNER_TEMP/citizenchain-release\" | Format-Table Name, Length\n"},"10":{"shell":"bash","source":"set -euo pipefail\nupdater=\"$(find \"$RUNNER_TEMP/citizenchain-release\" -maxdepth 1 -type f -name '*.exe' -print)\"\ntest \"$(printf '%s\\n' \"$updater\" | sed '/^$/d' | wc -l | tr -d ' ')\" = 1\n(cd node && node frontend/node_modules/@tauri-apps/cli/tauri.js signer sign \"$GITHUB_WORKSPACE/$updater\")\nsignature=\"$(tr -d '\\r\\n' < \"${updater}.sig\")\"\ntest -n \"$signature\"\nnode - \"$(basename \"$updater\")\" \"$signature\" <<'NODE'\nconst fs = require('node:fs');\nconst [asset, signature] = process.argv.slice(2);\nfs.writeFileSync(process.env.RUNNER_TEMP + '/citizenchain-release/citizenchain-node-latest-Windows.json', `${JSON.stringify({\n  version: process.env.GMB_SOFTWARE_VERSION,\n  notes: `CitizenChain Windows v${process.env.GMB_SOFTWARE_VERSION}。`,\n  pub_date: new Date().toISOString(),\n  platforms: { 'windows-x86_64': {\n    signature,\n    url: `https://github.com/${process.env.GITHUB_REPOSITORY}/releases/download/${process.env.GMB_VERSION_TAG}/${asset}`,\n  } },\n}, null, 2)}\\n`);\nNODE\n"}});
function runExactWorkflowStep(index) {
  requireExactRemoteJobEnvironment();
  if (!/^(?:0|[1-9][0-9]*)$/.test(String(index || '')) || !Object.hasOwn(workflowSteps, String(index))) {
    throw new Error('准确远端Job阶段无效');
  }
  const step = workflowSteps[String(index)];
  const command = step.shell === 'pwsh' ? 'pwsh' : (process.platform === 'win32' ? 'bash' : '/bin/bash');
  const args = step.shell === 'pwsh'
    ? ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', step.source]
    : ['--noprofile', '--norc', '-e', '-o', 'pipefail', '-c', step.source];
  const result = runExactProcess(command, args, { cwd: process.cwd(), env: process.env, stdio: 'inherit' });
  if (result.error) throw new Error('准确远端Job阶段无法启动');
  if (result.status !== 0) process.exitCode = Number.isInteger(result.status) ? result.status : 1;
}

requireExactRemoteJobEnvironment();
validateCandidate();
if (process.argv[2] !== 'workflow-step') throw new Error('准确Release Job只接受workflow-step');
runExactWorkflowStep(process.argv[3]);
