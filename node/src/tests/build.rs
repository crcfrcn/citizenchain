use super::{comparison_path, local_build_work, path_within, same_path};
use std::path::{Path, PathBuf};

// 同一夹具既可由Rust测试调用，也可从控制台直接交给官方Node定向验收。
const POSTGRES_TEST: &str = r#"
import assert from 'node:assert/strict';
import {mkdirSync, mkdtempSync, writeFileSync, existsSync, rmSync, realpathSync, readFileSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
const root=process.cwd();
const {preparePostgres,postgresVersion,npmCLI,nodeTools,platformCargoEnvironment,macLibraries,runBuildProcess}=await import(pathToFileURL(join(root,'scripts/build.mjs')));
const parent=join(root,'target/test');mkdirSync(parent,{recursive:true});
const work=mkdtempSync(join(parent,'postgres-'));
function distribution(path){
 for(const name of ['bin/postgres','bin/initdb','lib/library','share/postgresql/postgres.bki','COPYRIGHT']){
  const file=join(path,name);mkdirSync(join(file,'..'),{recursive:true});writeFileSync(file,'fixture');
 }
}
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
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.11 (Homebrew)\n'),'17.11');
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.110'),'17.110');
 assert.equal(postgresVersion('postgres (PostgreSQL) 17.11 unexpected'),undefined);count++;
 const installed=join(work,'installed');distribution(installed);
 const version=async()=>({stdout:'postgres (PostgreSQL) 17.11 (Homebrew)\n'});
 const cache=join(work,'cache');
 let calls=[];
 const reuse=async(file,args)=>{calls.push(file);assert.deepEqual(args,['--version']);return version();};
 assert.equal(await preparePostgres(work,{CITIZENCHAIN_PG_DIST:installed},{run:reuse,candidates:[],cache}),realpathSync(installed));
 assert.equal(calls.length,1);count++;
 calls=[];
 assert.equal(await preparePostgres(work,{}, {run:reuse,candidates:[installed],cache}),realpathSync(installed));
 assert.equal(calls.length,1);assert.equal(existsSync(cache),false);count++;
 distribution(cache);
 assert.equal(await preparePostgres(work,{}, {run:reuse,candidates:[],cache}),realpathSync(cache));count++;
 await assert.rejects(preparePostgres(work,{CITIZENCHAIN_PG_DIST:join(work,'missing')},{run:reuse,candidates:[],cache}),/指定/);count++;
 await assert.rejects(preparePostgres(work,{}, {run:async()=>{throw Error('broken executable');},candidates:[installed],cache}),/broken executable/);count++;
 const fresh=join(work,'fresh');let downloads=0,dist;
 const build=async(file,args)=>{
  if(file.endsWith('/curl')){downloads++;assert.ok(args.at(-1).endsWith('/v17.11/postgresql-17.11.tar.bz2'));writeFileSync(args[args.indexOf('--output')+1],'archive');}
  else if(file.endsWith('/tar')){const source=join(args.at(-1),'postgresql-17.11');mkdirSync(source);writeFileSync(join(source,'COPYRIGHT'),'license');}
  else if(file.endsWith('/configure')){dist=args[0].slice('--prefix='.length);}
  else if(file.endsWith('/make')&&args[0]==='install')distribution(dist);
  return {stdout:'postgres (PostgreSQL) 17.11\n'};
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
  for(const file of [...files,exe])await run('/usr/bin/codesign',['--force','--sign','-',file]);
  rmSync(source,{recursive:true});await run(exe,[]);count++;
 }
 const {automationInputs}=await import(pathToFileURL(join(root,'scripts/runtime.mjs')));
 const base={genesisHash:'0x'+'1'.repeat(64),blockHash:'0x'+'2'.repeat(64),specName:'citizenchain',specVersion:7,transactionVersion:3};
 const prepared=await automationInputs({readTarget:async()=>({base,target:{specVersion:8,transactionVersion:4}}),readRevision:()=>'a'.repeat(40)});
 assert.deepEqual(prepared,{version:'8',transaction_version:'4',runtime_base:JSON.stringify(base),source_sha:'a'.repeat(40)});
 const {describeAction}=await import(pathToFileURL(join(root,'scripts/config.mjs')));
 assert.deepEqual(await describeAction({flow:'release',platform:'wasm',inputs:prepared}),{repository:'crcfrcn/citizenchain',workflow:'wasm.yml',ref:'main',inputs:prepared});count++;
 let revisions=0;
 await assert.rejects(automationInputs({readTarget:async()=>{throw Error('RPC unavailable');},readRevision:()=>{revisions++;return 'x';}}),/RPC unavailable/);
 assert.equal(revisions,0);
 assert.deepEqual((await describeAction({flow:'release',platform:'macos'})).inputs,{});count++;
 const workflow=readFileSync(join(root,'.github/workflows/macos.yml'),'utf8');
 assert.ok(workflow.includes('node scripts/build.mjs postgres macos'));assert.ok(!workflow.includes('ftp.postgresql.org'));count++;
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
