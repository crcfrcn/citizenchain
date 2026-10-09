import {fixedScratch} from './target.mjs';
const directEntry = process.argv[1] === import.meta.filename && !process.execArgv.some(value => /^(?:-e|-p|--eval|--print)(?:=|$)/u.test(value));
const inlineTestEntry = directEntry && Boolean(process.env.NODE_TEST_CONTEXT) && process.argv.length === 2;
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdtempSync } from 'node:fs';
import { temporaryRoot,checkWork } from './build.mjs';
import { prepareWhitepaperSource } from './resources.mjs';
export async function generateDocs({work=process.env.CITIZENCHAIN_WORK_DIR,project=process.env.CITIZENCHAIN_PROJECT_ROOT,sourceDirectory}={}){
 checkWork(work);if(typeof project!=="string"||!path.isAbsolute(project)||path.resolve(project)!==project||!project.startsWith(work+path.sep)||fs.realpathSync(project)!==project)throw Error("文档工程必须属于本轮工作根");
const scriptDir = path.dirname(fileURLToPath(import.meta.url)); // citizenchain/scripts
const chainRoot = path.resolve(scriptDir, '..');                // citizenchain
const frontendRoot = path.resolve(project, 'node/frontend');  // citizenchain/node/frontend
// 白皮书和图片读取官网所有者的本轮Git原件；不读取父目录或产品邻仓。
const inputWork = fs.realpathSync(fixedScratch(path.join(work, 'whitepaper-')));
let webSource;
try { webSource = sourceDirectory || prepareWhitepaperSource(inputWork); }
catch (error) { fs.rmSync(inputWork, { recursive: true }); throw error; }
try {
const repoRoot = webSource;
const outputPath = path.resolve(frontendRoot, 'local-docs.generated.ts');

const sources = [
  {
    key: 'whitepaper',
    title: '白皮书',
    sourcePath: 'src/whitepaper.md',
    // 白皮书正文迁入官网源码目录后，图片与正文统一归入 citizenweb/src/assets 资源目录。
  }
];

const mimeTypes = new Map([
  ['.png', 'image/png'],
  ['.jpg', 'image/jpeg'],
  ['.jpeg', 'image/jpeg'],
  ['.webp', 'image/webp'],
  ['.gif', 'image/gif'],
  ['.svg', 'image/svg+xml'],
]);

const images=[];
function toDataUri(absPath){const ext=path.extname(absPath).toLowerCase(),mime=mimeTypes.get(ext);if(!mime||!fs.existsSync(absPath))return null;const bytes=fs.readFileSync(absPath),digest=crypto.createHash('sha256').update(bytes).digest('hex'),name='whitepaper-'+digest.slice(0,16)+ext;const directory=path.join(project,'icons');fs.mkdirSync(directory,{recursive:true});const file=path.join(directory,name);if(fs.existsSync(file)&&!fs.readFileSync(file).equals(bytes))throw Error('白皮书图片原件漂移');fs.writeFileSync(file,bytes);let entry=images.find(e=>e.name===name);if(!entry){entry={name,marker:'__WHITEPAPER_IMAGE_'+images.length+'__',variable:'whitepaperImage'+images.length};images.push(entry);}return entry.marker;}

function resolveRelativeAsset(sourceAbs, item, assetPath) {
  if (/^(?:[a-z]+:|#|\/)/i.test(assetPath)) return null;
  const decodedPath = decodeURIComponent(assetPath);
  const primary = path.resolve(path.dirname(sourceAbs), decodedPath);
  if (fs.existsSync(primary)) return primary;

  for (const fallbackRoot of item.assetFallbackRoots ?? []) {
    const fallback = path.resolve(repoRoot, fallbackRoot, decodedPath);
    if (fs.existsSync(fallback)) return fallback;
  }

  return primary;
}

function embedLocalImages(markdown, sourceAbs, item) {
  const htmlImgPattern = /(<img\b[^>]*\bsrc=["'])([^"']+)(["'][^>]*>)/gi;
  const markdownImgPattern = /(!\[[^\]]*\]\()([^)]+)(\))/g;

  // 白皮书图片归本轮统一icons目录，由Vite导入后的实际URL替换正文标记。
  return markdown
    .replace(htmlImgPattern, (match, prefix, assetPath, suffix) => {
      const absAssetPath = resolveRelativeAsset(sourceAbs, item, assetPath);
      if (!absAssetPath) return match;
      const dataUri = toDataUri(absAssetPath);
      return dataUri ? `${prefix}${dataUri}${suffix}` : match;
    })
    .replace(markdownImgPattern, (match, prefix, assetPath, suffix) => {
      const absAssetPath = resolveRelativeAsset(sourceAbs, item, assetPath.trim());
      if (!absAssetPath) return match;
      const dataUri = toDataUri(absAssetPath);
      return dataUri ? `${prefix}${dataUri}${suffix}` : match;
    });
}

const docs = sources.map((item) => {
  const abs = path.resolve(repoRoot, item.sourcePath);
  const markdown = embedLocalImages(fs.readFileSync(abs, 'utf8'), abs, item);
  return {
    key: item.key,
    title: item.title,
    sourcePath: item.sourcePath,
    markdown,
    sha256: crypto.createHash('sha256').update(markdown).digest('hex'),
  };
});

fs.mkdirSync(path.dirname(outputPath), { recursive: true });
fs.writeFileSync(
  outputPath,
  [
    '// 本文件由 citizenchain/scripts/docs.mjs 自动生成，仅归本轮工程。',
    ...images.map(e=>'import '+e.variable+' from '+JSON.stringify('../../icons/'+e.name+'?url')+';'),
    '// 本文件只内置白皮书；公民宪法由链上 runtime API 返回。',
    '',
    'export type LocalDocKey = "whitepaper";',
    '',
    'export type LocalDoc = {',
    '  key: LocalDocKey;',
    '  title: string;',
    '  sourcePath: string;',
    '  sha256: string;',
    '  markdown: string;',
    '};',
    '',
    'export const LOCAL_DOCS = ['+docs.map(doc=>'{'+Object.entries(doc).map(([key,value])=>JSON.stringify(key)+':'+(key==='markdown'?images.reduce((expr,e)=>expr+'.split('+JSON.stringify(e.marker)+').join('+e.variable+')',JSON.stringify(value)):JSON.stringify(value))).join(',')+'}').join(',')+'] as const satisfies readonly LocalDoc[];',
    '',
  ].join('\n'),
  'utf8',
);

console.log(`generated ${path.relative(project, outputPath)}`);
} finally { fs.rmSync(inputWork, { recursive: true, force:true }); }

}
if(!inlineTestEntry&&directEntry){void generateDocs().catch(e=>{console.error(e.message);process.exitCode=1;});}

// 文档入口在取得官网输入前拒绝源码输出，保护资源归属。
if(inlineTestEntry){void(async()=>{const {test}=await import('node:test');const {default:assert}=await import('node:assert/strict');const {testRoot}=await import('./build.mjs');test('文档输出源根与未声明工作根在获取输入前拒绝',async()=>{await assert.rejects(generateDocs({work:'relative',project:'relative'}),/工作根/);});
test('文档生成将图片原件放入统一目录，并生成可编译的实际导入，结束清理官网现场',async()=>{
 const work=fs.mkdtempSync(path.join(testRoot(),'docs-')),project=path.join(work,'project'),source=path.join(work,'website'),bytes=fs.readFileSync(new URL('../icons/logo.png',import.meta.url));
 try{fs.mkdirSync(project);fs.mkdirSync(path.join(source,'src'),{recursive:true});fs.writeFileSync(path.join(source,'src','image.png'),bytes);fs.writeFileSync(path.join(source,'src','whitepaper.md'),'# 正文\n![图标](image.png)\n<img src="image.png">');
 await generateDocs({work,project,sourceDirectory:source});const names=fs.readdirSync(path.join(project,'icons'));assert.equal(names.length,1);assert.deepEqual(fs.readFileSync(path.join(project,'icons',names[0])),bytes);const output=fs.readFileSync(path.join(project,'node/frontend/local-docs.generated.ts'),'utf8');assert.ok(output.includes('import whitepaperImage0 from "../../icons/'+names[0]+'?url";'));assert.ok(output.includes('.split("__WHITEPAPER_IMAGE_0__").join(whitepaperImage0)'));assert.equal(fs.readdirSync(work).filter(name=>name.startsWith('whitepaper-')).length,0);
 }finally{fs.rmSync(work,{recursive:true,force:true});}
});})();}
