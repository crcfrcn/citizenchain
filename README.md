# CitizenChain

公民链，包含节点、链上 Runtime 与 OnChina 机构服务。三个组件在同一完整产品仓库中维护。

本仓库独立自有代码采用 MIT；上游组件、衍生修改及组合分发遵循各自适用条款，见 [许可证及范围](LICENSE)。

## 编译与 GitHub 自动化

`scripts/build.mjs` 是 macOS、Windows、LinuxARM、LinuxAMD 节点及 WASM 的唯一产品编译入口。五个平台分别使用 `target/build/<平台>`；每次任务只清理自己的现场。

在对应原生宿主上可运行 `node scripts/build.mjs native-build <平台>`；成功产物留在回执所指的本平台工作目录，消费后运行 `node scripts/build.mjs finish <平台>`。WASM 可在具备其 Rust 目标的宿主上单独构建。节点构建需要对应平台的 PostgreSQL 原件路径 `CITIZENCHAIN_PG_DIST`。

GitHub Actions 保留 `macos.yml`、`windows.yml`、`linuxarm.yml`、`linuxamd.yml`、`wasm.yml` 五个独立手动流程。各流程编译本平台、核验本轮产物、先创建草稿 GitHub Release，回读资产摘要后才转为正式版。一个平台失败不取消其他平台。

四个平台各交付一个完整安装包：macOS为DMG、Windows为EXE、LinuxARM和LinuxAMD各为DEB；首次安装和更新下载同一个包。macOS沿用本机系统签名身份，远端通过 GitHub Actions Secrets 提供 `APPLE_CERTIFICATE_BASE64`、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_PROFILE_BASE64`。完整交付及更新方式见 [节点技术文档](CitizenChainNode.md)。
