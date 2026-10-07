#!/usr/bin/env node
import { remoteEnvironment as productRemoteEnvironment } from '../../../../build.mjs';
if(process.env.GITHUB_ACTIONS==='true'&&String(process.env.GITHUB_WORKFLOW||'').startsWith('citizenchain.'))Object.assign(process.env,productRemoteEnvironment());
import { spawnSync as runExactProcess } from 'node:child_process';

function validateCandidate() {
  const value=process.env;
if(!/^[0-9a-f]{40}$/.test(value.SOURCE_SHA||'')||!/^[1-9][0-9]*$/.test(value.CI_RUN_ID||'')||!/^\d+\.\d{1,2}\.\d{1,2}$/.test(value.SOFTWARE_VERSION||'')||value.VERSION_TAG!=='citizenchain-linux-arm-v'+value.SOFTWARE_VERSION)throw Error('准确Release候选无效');
}

// 本文件只执行 citizenchain.linux-arm.release 的 build-desktop Job；阶段编号由本仓唯一 Workflow 固定，禁止接收其它身份。
export const EXACT_REMOTE_JOB_IDENTITY = Object.freeze({"pipeline":"citizenchain.linux-arm.release","job":"build-desktop"});

function requireExactRemoteJobEnvironment() {
  const expected = 'crcfrcn/citizenchain';
  if (!expected || process.env.GITHUB_REPOSITORY !== expected) {
    throw new Error('准确远端Job仓库身份无效');
  }
}
const workflowSteps = Object.freeze({"0":{"shell":"bash","source":"set -euo pipefail\ntest \"$(git rev-parse HEAD)\" = \"$GMB_SOURCE_SHA\"\nnode $GITHUB_WORKSPACE/scripts/node/release/linux-arm/index.mjs node-version apply \"$GMB_SOFTWARE_VERSION\"\n"},"1":{"shell":"bash","source":"set -euo pipefail\nif [ \"${RUNNER_ARCH}\" != \"${EXPECTED_RUNNER_ARCH}\" ]; then\n  echo \"::error::runner 架构不符合 ${EXPECTED_RUNNER_ARCH}: 当前为 ${RUNNER_ARCH}\"\n  exit 1\nfi\n"},"2":{"shell":"bash","source":"set -euo pipefail\nnode <<'NODE'\nconst fs = require('fs');\nconst targetSource = process.env.CITIZENCHAIN_MANUAL_BUNDLE_TARGETS;\nconst bundleTargets = (targetSource || '')\n  .split(',')\n  .map((target) => target.trim())\n  .filter(Boolean);\nif (bundleTargets.length === 0) {\n  throw new Error('缺少本 matrix 的 Tauri bundle targets。');\n}\n// 候选版本已在当前 runner 工作区应用；这里只设置本 matrix 的 bundle 目标。\nconst tauriPath = 'node/tauri.conf.json';\nconst tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));\nconst version = tauri.version;\ntauri.bundle = tauri.bundle || {};\n// 中文注释：Release 生成安装包与 updater 正式文件；正式签名在同一 runner 的后续步骤完成。\ntauri.bundle.targets = bundleTargets;\n// 中文注释：显式关闭 Tauri 自动 updater 归档；本 workflow 按现有平台文件格式签名并生成清单。\ndelete tauri.bundle.createUpdaterArtifacts;\n// 中文注释：平台 Release 可以独立成功，因此每个安装包只读取自己的更新清单；\n// 不能再用一个根 version 同时描述版本可能不同的四个目标。\ntauri.plugins = tauri.plugins || {};\ntauri.plugins.updater = tauri.plugins.updater || {};\ntauri.plugins.updater.endpoints = [\n  `https://github.com/crcfrcn/citizenchain/releases/download/${process.env.GMB_VERSION_TAG}/${process.env.CITIZENCHAIN_UPDATER_MANIFEST_NAME}`,\n];\nfs.writeFileSync(tauriPath, `${JSON.stringify(tauri, null, 2)}\n`);\nconsole.log(`桌面端版本(源码): ${version}`);\nconsole.log(`Tauri bundle targets: ${bundleTargets.join(',')}`);\nNODE\n"},"3":{"shell":"bash","source":"node $GITHUB_WORKSPACE/scripts/node/release/linux-arm/index.mjs linux-deps"},"4":{"shell":"bash","source":"case \"$RUNNER_OS/$RUNNER_ARCH\" in\n  Linux/ARM64) platform=linux-arm ;;\n  Linux/X64) platform=linux-amd ;;\n  *) echo 'CitizenChain Linux protoc宿主不受支持' >&2; exit 1 ;;\nesac\nprotoc_executable=\"$(node scripts/dependencies.mjs prepare protoc \"$platform\" \"$RUNNER_TEMP/citizenchain-protoc/$platform\")\"\n{\n  echo \"LLVM_CONFIG_PATH=$(command -v llvm-config)\"\n  echo \"LIBCLANG_PATH=$(llvm-config --libdir)\"\n  echo \"PROTOC=$protoc_executable\"\n} >> \"$GITHUB_ENV\"\n"},"5":{"shell":"bash","source":"node $GITHUB_WORKSPACE/scripts/node/release/linux-arm/index.mjs node-version lock \"$GMB_SOFTWARE_VERSION\""},"6":{"shell":"bash","source":"npm --prefix node/frontend ci\nnpm --prefix node/frontend run build\n"},"7":{"shell":"bash","source":"# 中文注释：安装包只携带节点软件；首次启动按冻结 chainspec 本地初始化并联网同步。\nrm -rf node/resources/genesis-state\n"},"8":{"shell":"bash","source":"node frontend/node_modules/@tauri-apps/cli/tauri.js build --ci -- --locked --config \"$GITHUB_WORKSPACE/config.toml\""},"9":{"shell":"bash","source":"set -euo pipefail\nmkdir -p \"$RUNNER_TEMP/citizenchain-release\"\nversioned_installer=\"${INSTALLER_NAME%.deb}-v${GMB_SOFTWARE_VERSION}.deb\"\nversioned_updater=\"${UPDATER_ASSET_NAME%.AppImage}-v${GMB_SOFTWARE_VERSION}.AppImage\"\ndeb_file=\"$(find ${CARGO_TARGET_DIR}/release/bundle/deb -name '*.deb' | head -1)\"\nif [ -z \"$deb_file\" ]; then\n  echo \"::error::未找到 Linux deb 安装包。\"\n  exit 1\nfi\nactual_deb_arch=\"$(dpkg-deb -f \"$deb_file\" Architecture)\"\nif [ \"$actual_deb_arch\" != \"$EXPECTED_DEB_ARCH\" ]; then\n  echo \"::error::deb 架构不符合 ${EXPECTED_DEB_ARCH}: 当前为 ${actual_deb_arch}\"\n  exit 1\nfi\ncp \"$deb_file\" \"$RUNNER_TEMP/citizenchain-release/${versioned_installer}\"\nappimage_file=\"\"\nif [ -d ${CARGO_TARGET_DIR}/release/bundle/appimage ]; then\n  appimage_file=\"$(find ${CARGO_TARGET_DIR}/release/bundle/appimage -name '*.AppImage' | head -1)\"\nfi\nif [ -z \"$appimage_file\" ]; then\n  echo \"::error::Release 缺少 Linux updater AppImage。\"\n  exit 1\nfi\ncp \"$appimage_file\" \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}\"\n(cd node && node frontend/node_modules/@tauri-apps/cli/tauri.js signer sign \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}\")\nsignature=\"$(tr -d '\r\n' < \"$RUNNER_TEMP/citizenchain-release/${versioned_updater}.sig\")\"\ntest -n \"$signature\"\nnode - \"$versioned_updater\" \"$signature\" <<'NODE'\nconst fs = require('node:fs');\nconst [asset, signature] = process.argv.slice(2);\nfs.writeFileSync(process.env.RUNNER_TEMP + '/citizenchain-release/citizenchain-node-latest-LinuxARM.json', `${JSON.stringify({\n  version: process.env.GMB_SOFTWARE_VERSION,\n  notes: `CitizenChain LinuxARM v${process.env.GMB_SOFTWARE_VERSION}。`,\n  pub_date: new Date().toISOString(),\n  platforms: { 'linux-aarch64': {\n    signature,\n    url: `https://github.com/${process.env.GITHUB_REPOSITORY}/releases/download/${process.env.GMB_VERSION_TAG}/${asset}`,\n  } },\n}, null, 2)}\n`);\nNODE\nls -lh \"$RUNNER_TEMP/citizenchain-release\"\n"}});

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
