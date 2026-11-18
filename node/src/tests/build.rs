use super::{comparison_path, local_build_work, path_within, same_path};
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
const MACOS_RELEASE_TEST: &str = r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createHash} from 'node:crypto';
import childProcess, {execFileSync} from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {mock} from 'node:test';
const root=process.cwd();
const {recordAssets,publish}=await import(pathToFileURL(join(root,'.github/release.mjs')));
const {product}=await import(pathToFileURL(join(root,'scripts/config.mjs')));
const parent=join(root,'target/test');fs.mkdirSync(parent,{recursive:true});
const work=fs.mkdtempSync(join(parent,'macos-release-'));
const context={sha:'a'.repeat(40),ref:'refs/heads/main',runId:7};
const outputs={};const core={setOutput:(name,value)=>{outputs[name]=value;}};
let count=0;
try{
 Object.assign(process.env,{GITHUB_WORKSPACE:work,CITIZENCHAIN_RELEASE_VERSION:'1.2.3',CITIZENCHAIN_SOURCE_SHA:context.sha,PLATFORM:'macos'});
 const artifact=join(work,'target/build/macos/artifact'),app=join(artifact,'公民链.app');
 const contents=join(app,'Contents'),resources=join(contents,'Resources'),executable=join(contents,'MacOS/check');
 fs.mkdirSync(resources,{recursive:true});fs.mkdirSync(join(contents,'MacOS'));
 fs.writeFileSync(join(contents,'Info.plist'),'<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>org.citizenchain.release-test</string><key>CFBundleExecutable</key><string>check</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>');
 const c=join(work,'check.c');fs.writeFileSync(c,'int main(void){return 0;}\n');
 execFileSync('/usr/bin/cc',[c,'-o',executable]);fs.chmodSync(executable,0o751);
 fs.writeFileSync(join(resources,'payload'),'original payload');fs.writeFileSync(join(resources,'empty'),'');
 fs.symlinkSync('payload',join(resources,'alias'));
 execFileSync('/usr/bin/codesign',['--force','--sign','-',app],{stdio:['ignore','pipe','pipe']});
 await recordAssets({context,core});
 assert.deepEqual(fs.readdirSync(artifact),['公民链.zip']);
 const archive=join(artifact,'公民链.zip'),bytes=fs.readFileSync(archive),assets=JSON.parse(outputs.assets);
 assert.deepEqual(assets,[{name:'citizenchain-macOS-v1.2.3.zip',label:'公民链·macOS·v1.2.3.zip',size:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')}]);count++;
 const extracted=join(work,'round-trip');fs.mkdirSync(extracted);
 execFileSync('/usr/bin/ditto',['-x','-k',archive,extracted]);
 const restored=join(extracted,'公民链.app');
 execFileSync('/usr/bin/codesign',['--verify','--deep','--strict',restored],{stdio:['ignore','pipe','pipe']});
 assert.equal(fs.statSync(join(restored,'Contents/MacOS/check')).mode&0o7777,0o751);
 assert.equal(fs.readlinkSync(join(restored,'Contents/Resources/alias')),'payload');
 assert.equal(fs.readFileSync(join(restored,'Contents/Resources/payload'),'utf8'),'original payload');
 assert.equal(fs.statSync(join(restored,'Contents/Resources/empty')).size,0);count++;
 const releaseWork=join(work,'runner');fs.mkdirSync(join(releaseWork,'artifact'),{recursive:true});
 fs.copyFileSync(archive,join(releaseWork,'artifact/公民链.zip'));fs.appendFileSync(join(releaseWork,'artifact/公民链.zip'),'tampered');
 Object.assign(process.env,{RUNNER_TEMP:releaseWork,SOURCE_ASSETS:outputs.assets});
 let ghCalls=0;const actualExec=childProcess.execFileSync;
 mock.method(childProcess,'execFileSync',(command,...args)=>{if(command==='gh'){ghCalls++;throw Error('测试禁止访问GitHub');}return actualExec(command,...args);});
 syncBuiltinESMExports();
 await assert.rejects(publish({context,github:{},core}),/正式产物字节不符/);
 assert.equal(ghCalls,0);mock.restoreAll();syncBuiltinESMExports();count++;
 fs.rmSync(archive);execFileSync('/usr/bin/ditto',[restored,app]);
 fs.writeFileSync(join(app,'Contents/Resources/payload'),'changed after signing');
 await assert.rejects(recordAssets({context,core}),/codesign/);
 assert.ok(!fs.existsSync(archive));count++;
 for(const [platform,output,name,extension]of [
  ['windows','公民链.exe','citizenchain-Windows-v1.2.3.exe','.exe'],
  ['linux-arm','公民链.deb','citizenchain-LinuxARM-v1.2.3.deb','.deb'],
  ['linux-amd','公民链.deb','citizenchain-LinuxAMD-v1.2.3.deb','.deb'],
 ]){
  process.env.PLATFORM=platform;const dir=join(work,'target/build',platform,'artifact');fs.mkdirSync(dir,{recursive:true});
  fs.writeFileSync(join(dir,output),'asset');await recordAssets({context,core});
  const [asset]=JSON.parse(outputs.assets);assert.equal(asset.name,name);assert.equal(asset.label,'公民链·'+product.platforms[platform].name+'·v1.2.3'+extension);count++;
 }
 Object.assign(process.env,{PLATFORM:'wasm',CITIZENCHAIN_RELEASE_VERSION:'4',CITIZENCHAIN_RUNTIME_TRANSACTION_VERSION:'3',CITIZENCHAIN_RUNTIME_BASE:JSON.stringify({genesisHash:'fixture-genesis',blockHash:'fixture-block',specName:'citizenchain',specVersion:3,transactionVersion:2})});
 const wasmDir=join(work,'target/build/wasm/artifact');fs.mkdirSync(wasmDir,{recursive:true});
 const names=['runtime.wasm','runtime.compact.wasm','runtime.compact.compressed.wasm'];
 for(const name of names)fs.writeFileSync(join(wasmDir,name),'fixture bytes');
 await recordAssets({context,core});assert.deepEqual(JSON.parse(outputs.assets).map(a=>a.name),names);assert.equal(outputs.version,'4');
 const runtime=JSON.parse(outputs.runtime);assert.equal(runtime.spec_version,4);assert.equal(runtime.transaction_version,3);count++;
 const workflow=fs.readFileSync(join(root,'.github/workflows/macos.yml'),'utf8');
 assert.ok(workflow.includes('steps.assets.outputs.assets'));assert.ok(workflow.includes('needs.build.outputs.assets'));
 assert.ok(workflow.includes('actions/upload-artifact@'));assert.ok(workflow.includes('actions/download-artifact@'));
 assert.ok(workflow.indexOf('await recordAssets({context,core})')<workflow.indexOf('上传已验收的唯一附件'));
 assert.ok(workflow.indexOf('上传已验收的唯一附件')<workflow.indexOf('清理本轮编译现场'));
 assert.ok(workflow.includes('await publish({context,github,core})'));assert.ok(!workflow.includes('检查正式附件交付能力'));count++;
 console.log('macOS发布定向回归通过：'+count+'组');
}finally{mock.restoreAll();syncBuiltinESMExports();fs.rmSync(work,{recursive:true,force:true});}
"#;

#[cfg(target_os = "macos")]
#[test]
fn macos_release_archive_preserves_signed_app_and_rejects_corruption() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("node package belongs to repository");
    let output =
        std::process::Command::new(std::env::var_os("NODE").unwrap_or_else(|| "node".into()))
            .args(["--input-type=module", "-e", MACOS_RELEASE_TEST])
            .current_dir(root)
            .output()
            .expect("execute declared Node");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

// 同一夹具既可由Rust测试调用，也可从控制台直接交给官方Node定向验收。
const POSTGRES_TEST: &str = r#"
import assert from 'node:assert/strict';
import {mkdirSync, mkdtempSync, writeFileSync, existsSync, rmSync, realpathSync, readFileSync, copyFileSync} from 'node:fs';
import {join,sep,win32} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=process.cwd();
const {inside,checkTools,preparePostgres,postgresVersion,npmCLI,nodeTools,platformCargoEnvironment,macLibraries,runBuildProcess,signLocalResources,appleSigning}=await import(pathToFileURL(join(root,'scripts/build.mjs')));
const parent=join(root,'target/test');mkdirSync(parent,{recursive:true});
const work=mkdtempSync(join(parent,'postgres-'));
function distribution(path){
 for(const name of ['bin/postgres','bin/initdb','bin/pg_config','lib/library','share/postgresql/postgres.bki','COPYRIGHT']){
  const file=join(path,name);mkdirSync(join(file,'..'),{recursive:true});writeFileSync(file,'fixture');
 }
}
// 同一模拟命令处理器必须识别POSIX与Windows路径，不能用固定斜杠判断命令名。
const executable=file=>win32.basename(file);
let count=0;
try{
 const npm=npmCLI();const npmRun=spawnSync(process.execPath,[npm,'--version'],{encoding:'utf8'});
 assert.equal(npmRun.status,0);assert.match(npmRun.stdout,/^\d+\.\d+\.\d+/);count++;
 const diagnostic=spawnSync(process.execPath,['--input-type=module','-e',`import {runBuildProcess} from './scripts/build.mjs';await runBuildProcess(process.execPath,['-e',"process.stderr.write('final diagnostic')"],process.env,process.cwd(),{diagnostics:true});`],{cwd:root,encoding:'utf8'});
 assert.equal(diagnostic.status,0);assert.equal(diagnostic.stdout,'final diagnostic');count++;
 const tools=nodeTools();const env=platformCargoEnvironment('macos',work,{NODE:tools.node,DYLD_FALLBACK_LIBRARY_PATH:'/usr/lib'});
 assert.equal(env.NODE,tools.node);assert.equal(env.DYLD_FALLBACK_LIBRARY_PATH,'/usr/lib');
 if(process.platform==='darwin'){
  const forwarded=spawnSync(env.NODE,['-e',"console.log(process.env.DYLD_FALLBACK_LIBRARY_PATH)"],{env,encoding:'utf8'});
  assert.equal(forwarded.status,0);assert.equal(forwarded.stdout.trim(),'/usr/lib');
 }count++;
 assert.ok(inside('C:\\repo','C:\\repo\\child',false,win32));
 assert.ok(!inside('C:\\repo','D:\\repo\\child',true,win32));
 assert.ok(!inside('C:\\repo','C:\\repo',false,win32));
 assert.ok(inside('C:\\repo','C:\\repo',true,win32));
 const captured=[];
 await checkTools({CARGO:'cargo',PROTOC:'protoc'},async command=>{
  captured.push(command);
  return {stdout:command==='cargo'?'cargo 1.97.1 (fixture)':'libprotoc 35.0\n'};
 });
 assert.deepEqual(captured,['cargo','protoc']);
 await assert.rejects(checkTools({CARGO:'cargo'},async()=>({stdout:'cargo 1.97.1 (fixture)'})),/protoc/);count++;
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.11 (Homebrew)\n'),'17.11');
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.110'),'17.110');
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.11 unexpected'),undefined);count++;
 const installed=join(work,'installed');distribution(installed);
 const version=async(file,args)=>({stdout:args[0]==='--version'?'postgres (PostgreSQL) 17.11 (Homebrew)\n':[join(file,'../..','bin'),join(file,'../..','lib'),join(file,'../..','share/postgresql')].join('\n')});
 const cache=join(work,'cache');
 let calls=[];
 const reuse=async(file,args)=>{calls.push(file);assert.deepEqual(args,executable(file)==='pg_config'?['--bindir','--pkglibdir','--sharedir']:['--version']);return version(file,args);};
 await reuse(win32.join(installed,'bin','pg_config'),['--bindir','--pkglibdir','--sharedir']);
 assert.equal(calls.length,1);calls=[];count++;
 assert.equal(await preparePostgres(work,{CITIZENCHAIN_PG_DIST:installed},{run:reuse,candidates:[],cache}),realpathSync(installed));
 assert.equal(calls.length,2);count++;
 calls=[];
 assert.equal(await preparePostgres(work,{}, {run:reuse,candidates:[installed],cache}),realpathSync(installed));
 assert.equal(calls.length,2);assert.equal(existsSync(cache),false);count++;
 distribution(cache);
 assert.equal(await preparePostgres(work,{}, {run:reuse,candidates:[],cache}),realpathSync(cache));count++;
 const external=async(file,args)=>args[0]==='--version'?version(file,args):{stdout:'/usr/lib\n/usr/lib\n/usr/lib'};
 await assert.rejects(preparePostgres(work,{CITIZENCHAIN_PG_DIST:installed},{run:external,candidates:[],cache}),/不可独立搬迁/);
 assert.equal(await preparePostgres(work,{}, {run:async(file,args)=>file.startsWith(installed+sep)?external(file,args):reuse(file,args),candidates:[installed],cache}),realpathSync(cache));count++;
 await assert.rejects(preparePostgres(work,{CITIZENCHAIN_PG_DIST:join(work,'missing')},{run:reuse,candidates:[],cache}),/指定/);count++;
 await assert.rejects(preparePostgres(work,{}, {run:async()=>{throw Error('broken executable');},candidates:[installed],cache}),/broken executable/);count++;
 const fresh=join(work,'fresh');let downloads=0,dist;
 const build=async(file,args)=>{
  if(executable(file)==='curl'){downloads++;assert.ok(args.at(-1).endsWith('/v17.11/postgresql-17.11.tar.bz2'));writeFileSync(args[args.indexOf('--output')+1],'archive');}
  else if(executable(file)==='tar'){const source=join(args.at(-1),'postgresql-17.11');mkdirSync(source);writeFileSync(join(source,'COPYRIGHT'),'license');}
  else if(executable(file)==='configure'){dist=args[0].slice('--prefix='.length);}
  else if(executable(file)==='make'&&args[0]==='install')distribution(dist);
  return executable(file)==='pg_config'?version(file,args):{stdout:'postgres (PostgreSQL) 17.11\n'};
 };
 assert.equal(await preparePostgres(work,{}, {run:build,candidates:[],cache:fresh}),fresh);
 assert.equal(downloads,1);assert.equal(existsSync(fresh+'.lock'),false);count++;
 const archived=join(work,'archived');writeFileSync(archived+'.tar.bz2','cached archive');
 await preparePostgres(work,{}, {run:build,candidates:[],cache:archived});assert.equal(downloads,1);count++;
 const failed=join(work,'failed');
 await assert.rejects(preparePostgres(work,{}, {run:async()=>{throw Error('download failed');},candidates:[],cache:failed}),/download failed/);
 assert.equal(existsSync(failed),false);assert.equal(existsSync(failed+'.tar.bz2'),false);assert.equal(existsSync(failed+'.lock'),false);count++;
 const wrong=join(work,'wrong');distribution(wrong);
 await assert.rejects(preparePostgres(work,{CITIZENCHAIN_PG_DIST:wrong},{run:async()=>({stdout:'postgres (PostgreSQL) 18.6'}),candidates:[],cache}),/版本/);count++;
 if(process.platform==='darwin'){
  const source=join(work,'original'),resources=join(work,'App/Contents/Resources'),exe=join(work,'App/Contents/MacOS/citizenchain');
  mkdirSync(source);mkdirSync(join(resources,'postgres/macos/lib'),{recursive:true});mkdirSync(join(exe,'..'),{recursive:true});
  writeFileSync(join(source,'b.c'),'int b(void){return 7;}');
  writeFileSync(join(source,'a.c'),'extern int b(void); int a(void){return b();}');
  writeFileSync(join(source,'main.c'),'extern int a(void); int main(void){return a()==7?0:1;}');
  const env={HOME:process.env.HOME,PATH:'/usr/bin:/bin'};
  const run=(file,args)=>runBuildProcess(file,args,env,root,{capture:true});
  await run('/usr/bin/clang',['-dynamiclib',join(source,'b.c'),'-install_name','@loader_path/libb.dylib','-o',join(source,'libb.dylib')]);
  await run('/usr/bin/clang',['-dynamiclib',join(source,'a.c'),'-install_name',join(source,'liba.dylib'),'-L'+source,'-lb','-o',join(source,'liba.dylib')]);
  await run('/usr/bin/clang',[join(source,'main.c'),'-L'+source,'-la','-o',exe]);
  const files=await macLibraries(resources,exe,env);
  assert.ok(existsSync(join(resources,'postgres/macos/lib/libb.dylib')));
  const signing=appleSigning(files,resources);
  assert.deepEqual(signing.at(-1),{path:'.',entitlements:join(root,'node/Entitlements.plist')});
  assert.ok(signing.some(item=>item.path==='Contents/Resources/postgres/macos/lib/libb.dylib'&&item.entitlements===null));
  await signLocalResources([...files,exe],env);
  await run('/usr/bin/codesign',['--verify','--strict',exe]);
  await assert.rejects(signLocalResources([join(work,'missing')],env),/codesign/);
  const portable=join(work,'Portable/Contents/Resources'),portableExe=join(work,'Portable/Contents/MacOS/main');
  mkdirSync(join(portable,'postgres/macos/lib'),{recursive:true});mkdirSync(join(portableExe,'..'),{recursive:true});
  for(const name of ['liba.dylib','libb.dylib'])copyFileSync(join(source,name),join(portable,'postgres/macos/lib',name));
  await run('/usr/bin/clang',[join(source,'main.c'),'-L'+source,'-la','-o',portableExe]);
  const bad=join(work,'Bad/Contents/Resources'),badExe=join(work,'Bad/Contents/MacOS/main');
  mkdirSync(join(bad,'postgres/macos/lib'),{recursive:true});mkdirSync(join(badExe,'..'),{recursive:true});
  copyFileSync(portableExe,badExe);copyFileSync(join(source,'liba.dylib'),join(bad,'postgres/macos/lib/liba.dylib'));
  copyFileSync(join(source,'libb.dylib'),join(bad,'postgres/macos/lib/libb.dylib'));
  await run('/usr/bin/install_name_tool',['-id','/foreign/liba.dylib',join(bad,'postgres/macos/lib/liba.dylib')]);
  rmSync(source,{recursive:true});await run(exe,[]);
  await assert.rejects(macLibraries(bad,badExe,env),/无法明确定位/);
  const relocated=await macLibraries(portable,portableExe,env);await signLocalResources([...relocated,portableExe],env);await run(portableExe,[]);count++;
 }
 const {describeAction}=await import(pathToFileURL(join(root,'scripts/config.mjs')));
 assert.deepEqual((await describeAction({flow:'release',platform:'macos'})).inputs,{});count++;
 const workflow=readFileSync(join(root,'.github/workflows/macos.yml'),'utf8');
 const shared=readFileSync(join(root,'.github/release.mjs'),'utf8');
 assert.ok(workflow.includes('await prepareTools()'));
 assert.ok(shared.includes('node scripts/build.mjs postgres macos'));
 assert.ok(!workflow.includes('ftp.postgresql.org'));count++;
 console.log('PostgreSQL资源回归通过：'+count+'组');
}finally{rmSync(work,{recursive:true,force:true});}
"#;

#[test]
fn postgres_resources_reuse_before_official_download() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("node package belongs to repository");
    let output =
        std::process::Command::new(std::env::var_os("NODE").unwrap_or_else(|| "node".into()))
            .args(["--input-type=module", "-e", POSTGRES_TEST])
            .current_dir(root)
            .output()
            .expect("execute declared Node");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn comparison_keeps_directory_boundaries_and_different_targets() {
    let root = PathBuf::from(if cfg!(windows) {
        r"C:\repo\target\build\windows"
    } else {
        "/repo/target/build/windows"
    });
    assert!(path_within(&root.join("work/out"), &root));
    assert!(!path_within(
        &root.with_file_name("windows-other").join("out"),
        &root
    ));
    assert!(!same_path(&root.join("work/../out"), &root.join("out")));
    assert!(!same_path(&root.join("link/out"), &root.join("real/out")));
}

#[test]
fn local_macos_work_is_limited_to_the_compiler_generated_directory() {
    let root = PathBuf::from(if cfg!(windows) { r"C:\repo" } else { "/repo" });
    let build = root.join("target/build");
    assert!(local_build_work(
        &root,
        &build.join("local-Abc123"),
        "macos"
    ));
    assert!(!local_build_work(
        &root,
        &build.join("local-Abc123"),
        "windows"
    ));
    assert!(!local_build_work(
        &root,
        &build.join("local-Abc123/nested"),
        "macos"
    ));
    assert!(!local_build_work(
        &root,
        &root.join("target/build-other/local-Abc123"),
        "macos"
    ));
    for name in [
        "local-",
        "local-short",
        "local-toolong",
        "local-中文汉字",
        "other-Abc123",
    ] {
        assert!(!local_build_work(&root, &build.join(name), "macos"));
    }
}

#[cfg(windows)]
#[test]
fn windows_disk_and_unc_prefixes_compare_without_relaxing_ownership() {
    assert!(same_path(
        Path::new(r"C:\repo\target\out"),
        Path::new(r"\\?\C:\repo\target\out")
    ));
    assert!(same_path(
        Path::new(r"c:\repo\target\out"),
        Path::new(r"\\?\C:\repo\target\out")
    ));
    assert!(same_path(
        Path::new(r"\\server\share\repo\out"),
        Path::new(r"\\?\UNC\server\share\repo\out")
    ));
    assert!(path_within(
        Path::new(r"\\?\C:\repo\target\out"),
        Path::new(r"C:\repo\target")
    ));
    assert!(!same_path(
        Path::new(r"D:\repo\out"),
        Path::new(r"\\?\C:\repo\out")
    ));
    assert!(!same_path(
        Path::new(r"C:\repo\out"),
        Path::new(r"\\.\C:\repo\out")
    ));
    assert!(!path_within(
        Path::new(r"\\?\C:\repo\target-other\out"),
        Path::new(r"C:\repo\target")
    ));
    assert!(!same_path(
        Path::new(r"C:\repo\link\out"),
        Path::new(r"\\?\C:\repo\real\out")
    ));
    assert!(!Path::new(r"C:repo\out").is_absolute());
}

#[cfg(windows)]
#[test]
fn windows_comparison_preserves_native_path_code_units() {
    use std::os::windows::ffi::OsStringExt;
    let raw = [67, 58, 92, 0xd800];
    let mut extended = vec![92, 92, 63, 92];
    extended.extend_from_slice(&raw);
    let plain = PathBuf::from(std::ffi::OsString::from_wide(&raw));
    let extended = PathBuf::from(std::ffi::OsString::from_wide(&extended));
    let other = PathBuf::from(std::ffi::OsString::from_wide(&[67, 58, 92, 0xd801]));
    assert!(same_path(&plain, &extended));
    assert!(!same_path(&plain, &other));
}

#[cfg(not(windows))]
#[test]
fn non_windows_paths_keep_the_original_representation() {
    let path = Path::new(r"\\?\C:\repo\out");
    assert_eq!(comparison_path(path), path);
    assert!(!same_path(path, Path::new(r"C:\repo\out")));
}

const CI_TEST: &str = r#"
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import childProcess,{execFileSync} from 'node:child_process';
import {syncBuiltinESMExports} from 'node:module';
import {mock} from 'node:test';
const root=process.cwd(),originalRuntime=fs.readFileSync(join(root,'runtime/src/lib.rs'),'utf8');
const originalHook=fs.readFileSync(join(root,'runtime/build.rs'),'utf8');
const sha=execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim();
const platform=process.platform==='darwin'?'macos':process.platform==='win32'?'windows':process.arch==='arm64'?'linux-arm':'linux-amd';
const {product}=await import(pathToFileURL(join(root,'scripts/config.mjs')));
const {prepareTestRuntime,readRuntimeWasmVersion}=await import(pathToFileURL(join(root,'scripts/runtime.mjs')));
const {test:ciTest,confirmRunDeleted}=await import(pathToFileURL(join(root,'.github/release.mjs')));
const {platformCargoEnvironment}=await import(pathToFileURL(join(root,'scripts/build.mjs')));
const parent=join(root,'target/test');fs.mkdirSync(parent,{recursive:true});
const work=fs.mkdtempSync(join(parent,'ci-wasm-'));
const sourceVersion=originalRuntime.match(/pub const VERSION:[\s\S]*?\n\};/u)[0];
const spec=Number(sourceVersion.match(/\bspec_version:\s*(\d+)/u)[1]);
const transaction=Number(sourceVersion.match(/\btransaction_version:\s*(\d+)/u)[1]);
const u32=n=>{const b=Buffer.alloc(4);b.writeUInt32LE(n);return b;};
const leb=n=>{const b=[];do{const v=n&127;n>>>=7;b.push(v|(n?128:0));}while(n);return Buffer.from(b);};
const text=s=>Buffer.concat([Buffer.from([s.length*4]),Buffer.from(s)]);
function wasm(s=spec,t=transaction){
 const v=Buffer.concat([text('citizenchain'),text('fixture'),u32(0),u32(s),u32(0),Buffer.from([0]),u32(t),Buffer.from([0])]);
 const payload=Buffer.concat([leb(15),Buffer.from('runtime_version'),v]);
 return Buffer.concat([Buffer.from([0,97,115,109,1,0,0,0]),Buffer.from([0]),leb(payload.length),payload]);
}
const files=['runtime/src/lib.rs','runtime/build.rs','Cargo.toml','Cargo.lock','config.toml'];
const saved={...process.env};let count=0;
try{
 // GET确认只接收404；有界等待不重复DELETE，不吞掉权限、服务或网络错误。
 for(const readable of [0,2,99]){
  let calls=0;const delays=[];
  const github={rest:{actions:{getWorkflowRun:async params=>{assert.equal(params.run_id,7);calls++;if(calls>readable)throw Object.assign(Error('gone'),{status:404});return {};},deleteWorkflowRun:()=>{throw Error('不得重删');}}}};
  const action=confirmRunDeleted({github,repo:{owner:'fixture',repo:'fixture'},run_id:7,wait:async ms=>{delays.push(ms);}});
  if(readable===99){await assert.rejects(action,/旧Run删除未确认/);assert.equal(calls,6);assert.deepEqual(delays,[250,500,1000,2000,4000]);}
  else {await action;assert.equal(calls,readable+1);assert.deepEqual(delays,[250,500].slice(0,readable));}count++;
 }
 for(const status of [403,500,undefined]){
  let calls=0;const error=Object.assign(Error('failure'),{status});
  await assert.rejects(confirmRunDeleted({github:{rest:{actions:{getWorkflowRun:async()=>{calls++;throw error;}}}},repo:{},run_id:7,wait:()=>{throw Error('不得等待非404错误');}}),e=>e===error);
  assert.equal(calls,1);count++;
 }
 // 实际执行四份Workflow原脚本的成功/失败分支，保护活动任务和其它平台。
 const AsyncFunction=Object.getPrototypeOf(async function(){}).constructor;
 for(const p of ['macos','windows','linux-arm','linux-amd'])for(const success of [true,false]){
  const workflow=product.platforms[p].workflow,path='.github/workflows/'+workflow;
  const current={id:100,run_number:3,path,event:'workflow_dispatch',head_branch:'main',head_sha:sha,run_attempt:1,status:'in_progress'};
  const previous=[{...current,id:10,run_number:1,status:'completed',conclusion:'success'},
   {...current,id:20,run_number:2,status:'completed',conclusion:'failure'},
   {...current,id:200,run_number:4,status:'queued'},
   {...current,id:300,run_number:1,path:'.github/workflows/other.yml',status:'completed',conclusion:'failure'}];
  const removed=new Set(),pending=new Map(),deleted=[];
  const listWorkflowRuns=()=>{},listReleases=()=>{};
  const github={paginate:async fn=>fn===listWorkflowRuns?[current,...previous.filter(r=>!removed.has(r.id))]:fn===listReleases?[]:assert.fail('未知集合'),rest:{actions:{listWorkflowRuns,
   getWorkflowRun:async({run_id})=>{if(removed.has(run_id)){const n=pending.get(run_id)||0;if(n){pending.set(run_id,n-1);return {data:previous.find(r=>r.id===run_id)};}throw Object.assign(Error('gone'),{status:404});}return {data:run_id===100?current:previous.find(r=>r.id===run_id)};},
   deleteWorkflowRun:async({run_id})=>{assert.equal(run_id,success?10:20);assert.ok(!removed.has(run_id));deleted.push(run_id);removed.add(run_id);pending.set(run_id,1);}},repos:{listReleases}}};
  Object.assign(process.env,{GITHUB_WORKSPACE:root,PLATFORM:p,GITHUB_RUN_ATTEMPT:'1',JOB_RESULTS:JSON.stringify({prepare:'success',build:success?'success':'failure',release:success?'success':'skipped'})});
  const yaml=fs.readFileSync(join(root,path),'utf8'),start=yaml.indexOf('name: 清理本平台已完成历史');assert.ok(start>=0);
  const marker=yaml.indexOf('script: |',start);assert.ok(marker>start);
  const code=yaml.slice(yaml.indexOf('\n',marker)+1).split('\n').map(line=>line.replace(/^ {10}/u,'')).join('\n');
  const summary={addHeading(){return this;},addRaw(){return this;},async write(){}};
  await new AsyncFunction('github','context','core',code)(github,{repo:{owner:'fixture',repo:'fixture'},runId:100,sha},{summary});
  assert.deepEqual(deleted,[success?10:20]);assert.ok(!removed.has(200)&&!removed.has(300));count++;
 }
 // macOS真实脚本：单项已404但列表延迟，必须有界回读且不重删；原保留断言继续裁决。
 for(const success of [true,false])for(const mode of [2,99,'403','500','network']){
  const path='.github/workflows/macos.yml',oldId=success?10:20;
  const current={id:100,run_number:3,path,event:'workflow_dispatch',head_branch:'main',head_sha:sha,run_attempt:1,status:'in_progress'};
  const previous=[{...current,id:10,run_number:1,status:'completed',conclusion:'success'},
   {...current,id:20,run_number:2,status:'completed',conclusion:'failure'},
   {...current,id:200,run_number:4,status:'queued'},
   {...current,id:300,run_number:1,path:'.github/workflows/other.yml',status:'completed',conclusion:'failure'}];
  const removed=new Set(),deleted=[],delays=[];let reads=0,confirmed=0,written=false;
  const failure=Object.assign(Error('list failure'),{status:mode==='network'?undefined:Number(mode)});
  const listWorkflowRuns=()=>{},listReleases=()=>{};
  const github={paginate:async fn=>{
   if(fn===listReleases)return [];
   assert.equal(fn,listWorkflowRuns);
   if(!removed.size)return [current,...previous];
   reads++;if(typeof mode==='string'&&reads===2)throw failure;
   const stale=typeof mode==='string'||reads<=mode;
   return [current,...previous.filter(r=>stale||!removed.has(r.id))];
  },rest:{actions:{listWorkflowRuns,
   getWorkflowRun:async({run_id})=>{if(removed.has(run_id)){confirmed++;throw Object.assign(Error('gone'),{status:404});}return {data:run_id===100?current:previous.find(r=>r.id===run_id)};},
   deleteWorkflowRun:async({run_id})=>{assert.equal(run_id,oldId);assert.ok(!removed.has(run_id));deleted.push(run_id);removed.add(run_id);}},repos:{listReleases}}};
  Object.assign(process.env,{GITHUB_WORKSPACE:root,PLATFORM:'macos',GITHUB_RUN_ATTEMPT:'1',JOB_RESULTS:JSON.stringify({prepare:'success',build:success?'success':'failure',release:success?'success':'skipped'})});
  const yaml=fs.readFileSync(join(root,path),'utf8'),marker=yaml.indexOf('script: |',yaml.indexOf('name: 清理本平台已完成历史'));
  assert.ok(marker>=0);
  const code=yaml.slice(yaml.indexOf('\n',marker)+1).split('\n').map(line=>line.replace(/^ {10}/u,'')).join('\n');
  const summary={addHeading(){return this;},addRaw(){return this;},async write(){written=true;}};
  const action=new AsyncFunction('github','context','core','setTimeout',code)(github,{repo:{owner:'fixture',repo:'fixture'},runId:100,sha},{summary},(fn,ms)=>{delays.push(ms);fn();return 0;});
  if(mode===2){await action;assert.equal(reads,3);assert.deepEqual(delays,[250,500]);assert.equal(written,true);}
  else if(mode===99){await assert.rejects(action,/历史删除回读未通过/);assert.equal(reads,6);assert.deepEqual(delays,[250,500,1000,2000,4000]);assert.equal(written,false);}
  else {await assert.rejects(action,error=>error===failure);assert.equal(reads,2);assert.deepEqual(delays,[250]);assert.equal(written,false);}
  assert.equal(confirmed,1);assert.deepEqual(deleted,[oldId]);assert.ok(!removed.has(200)&&!removed.has(300));count++;
 }
 const environment={...saved,PLATFORM:platform,CITIZENCHAIN_SOURCE_SHA:sha,NODE:process.execPath,CARGO:'cargo',PROTOC:saved.PROTOC||'protoc',CARGO_TARGET_DIR:join(work,'cache'),WASM_FILE:join(work,'untrusted.wasm'),SKIP_WASM_BUILD:'1'};
 function runner(mode,events){return async(command,args,env,cwd)=>{
  if(args[0]==='--version')return {stdout:command===env.CARGO?'cargo '+product.tools.rust+' fixture':'libprotoc '+product.tools.protoc};
  if(command==='git'){
   if(args[0]==='rev-parse')return {stdout:mode==='sha'?'b'.repeat(40):sha};
   if(args[0]==='diff'){if(mode==='dirty')throw Error('Runtime input changed');return {};}
   assert.deepEqual(args,['ls-files','-z']);return {stdout:files.join('\0')+'\0'};
  }
  assert.equal(command,env.CARGO);assert.ok(args.includes('build'));events.push('build');
  assert.equal(env.WASM_FILE,undefined);assert.equal(env.SKIP_WASM_BUILD,undefined);
  assert.equal(fs.readFileSync(join(cwd,'runtime/src/lib.rs'),'utf8'),originalRuntime);
  assert.match(fs.readFileSync(join(cwd,'runtime/build.rs'),'utf8'),/WasmBuilder::build_using_defaults/);
  if(mode==='retain')throw Object.assign(Error('tool active'),{retainWork:true});
  const output=join(env.CARGO_TARGET_DIR,'release/wbuild/runtime');fs.mkdirSync(output,{recursive:true});
  fs.writeFileSync(join(output,'runtime.wasm'),mode==='invalid'?Buffer.from([0]):wasm(mode==='version'?spec+1:spec));return {};
 };}
 for(const mode of ['ok','sha','dirty','version','invalid','retain']){
  const directory=join(work,mode);fs.mkdirSync(directory);const events=[];
  const result=prepareTestRuntime({platform,work:directory,environment,run:runner(mode,events)});
  if(mode==='ok'){
   const path=await result;assert.deepEqual(readRuntimeWasmVersion(fs.readFileSync(path)),{specName:'citizenchain',specVersion:spec,transactionVersion:transaction});
   assert.ok(!fs.existsSync(join(directory,'runtime/source')));
   await assert.rejects(prepareTestRuntime({platform,work:directory,environment,run:runner(mode,[])}),/EEXIST/);
   assert.ok(fs.existsSync(path));
   const native=platformCargoEnvironment(platform,directory,{...environment,WASM_FILE:path});assert.equal(native.WASM_FILE,undefined);assert.equal(native.SKIP_WASM_BUILD,'1');
  }else {
   await assert.rejects(result);
   if(mode==='retain'){assert.ok(fs.existsSync(join(directory,'.retain-runtime-work')));assert.ok(fs.existsSync(join(directory,'runtime/source')));}
   else assert.ok(!fs.existsSync(join(directory,'runtime')));
  }
  if(['sha','dirty'].includes(mode))assert.equal(events.length,0);count++;
 }
 // 真实test()接线的前端→fixture→Rust顺序；所有外部命令仅为本子进程内的模拟。
 for(const failFrontend of [false,true]){
  const directory=join(work,failFrontend?'frontend-failure':'pipeline');fs.mkdirSync(directory);const events=[];
  Object.assign(process.env,environment,{PLATFORM:platform,CITIZENCHAIN_WORK_DIR:directory,GITHUB_WORKSPACE:root,CITIZENCHAIN_RELEASE_VERSION:'1.2.3'});
  mock.method(childProcess,'spawnSync',(command,args,options)=>{
   const ok=stdout=>({status:0,signal:null,stdout,stderr:''});
   if(args[0]==='--version')return ok(command===process.execPath?'v'+product.tools.node:command==='cargo'?'cargo '+product.tools.rust+' fixture':'libprotoc '+product.tools.protoc);
   if(command==='git')return ok(args[0]==='rev-parse'?sha:args[0]==='ls-files'?files.join('\0')+'\0':'');
   if(command==='cargo'){events.push('fixture');const out=join(options.env.CARGO_TARGET_DIR,'release/wbuild/runtime');fs.mkdirSync(out,{recursive:true});fs.writeFileSync(join(out,'runtime.wasm'),wasm());return ok('');}
   if(command==='bash'){
    const script=args[args.length-1];
    if(script.includes('--workspace')){
     events.push('rust');assert.equal(options.env.SKIP_WASM_BUILD,'1');assert.ok(fs.existsSync(options.env.WASM_FILE));
     assert.ok(!options.env.WASM_FILE.startsWith(options.env.CARGO_TARGET_DIR));
     assert.match(script,/-p nodeguard --features test-support --lib -- --ignored/u);assert.ok(!script.includes('unset SKIP_WASM_BUILD WASM_FILE'));return ok('');
    }
    events.push('frontend');assert.match(script,/unset SKIP_WASM_BUILD WASM_FILE DOCS_RS/u);return failFrontend?{...ok(''),status:1}:ok('');
   }
   return ok('');
  });syncBuiltinESMExports();
  try {if(failFrontend)await assert.rejects(ciTest());else await ciTest();}
  finally {mock.restoreAll();syncBuiltinESMExports();}
  assert.deepEqual(events,failFrontend?['frontend']:['frontend','fixture','rust']);count++;
 }
 assert.equal(fs.readFileSync(join(root,'runtime/src/lib.rs'),'utf8'),originalRuntime);
 assert.equal(fs.readFileSync(join(root,'runtime/build.rs'),'utf8'),originalHook);
 console.log('CI测试WASM与删除回读回归通过：'+count+'组');
}finally{mock.restoreAll();syncBuiltinESMExports();for(const k of Object.keys(process.env))if(!(k in saved))delete process.env[k];Object.assign(process.env,saved);fs.rmSync(work,{recursive:true,force:true});}
"#;

#[test]
fn ci_test_wasm_and_history_readback_preserve_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("node package belongs to repository");
    let output =
        std::process::Command::new(std::env::var_os("NODE").unwrap_or_else(|| "node".into()))
            .args(["--input-type=module", "-e", CI_TEST])
            .current_dir(root)
            .output()
            .expect("execute declared Node");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
