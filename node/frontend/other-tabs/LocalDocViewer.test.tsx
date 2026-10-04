import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import ts from 'typescript';

const sourcePath = `${process.cwd()}/other-tabs/LocalDocViewer.tsx`;
const sourceText = readFileSync(sourcePath, 'utf8');
const sourceFile = ts.createSourceFile(
  sourcePath,
  sourceText,
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TSX,
);

function descendants(node: ts.Node) {
  const found: ts.Node[] = [];
  const visit = (child: ts.Node) => {
    found.push(child);
    ts.forEachChild(child, visit);
  };
  ts.forEachChild(node, visit);
  return found;
}

test('正文 HTML 属性使用依赖 html 的稳定引用', () => {
  const declaration = descendants(sourceFile)
    .filter(ts.isVariableDeclaration)
    .find((node) => ts.isIdentifier(node.name) && node.name.text === 'htmlPayload');

  assert.ok(declaration?.initializer && ts.isCallExpression(declaration.initializer));
  assert.equal(declaration.initializer.expression.getText(sourceFile), 'useMemo');
  assert.equal(declaration.initializer.arguments[0]?.getText(sourceFile), '() => ({ __html: html })');
  assert.equal(declaration.initializer.arguments[1]?.getText(sourceFile), '[html]');

  const htmlAttribute = descendants(sourceFile)
    .filter(ts.isJsxAttribute)
    .find((node) => node.name.getText(sourceFile) === 'dangerouslySetInnerHTML');

  assert.ok(htmlAttribute?.initializer && ts.isJsxExpression(htmlAttribute.initializer));
  assert.equal(htmlAttribute.initializer.expression?.getText(sourceFile), 'htmlPayload');
});

test('目录移除和主标题居中处理保持在目录状态更新之前', () => {
  const effect = descendants(sourceFile)
    .filter(ts.isCallExpression)
    .find((node) => node.expression.getText(sourceFile) === 'useEffect' && node.getText(sourceFile).includes('stripInlineToc'));

  assert.ok(effect);
  const effectText = effect.getText(sourceFile);
  const stripIndex = effectText.indexOf('stripInlineToc(article)');
  const titleIndex = effectText.indexOf('applyDocSpecificClasses(article)');
  const stateIndex = effectText.indexOf('setTocItems(roots)');

  assert.ok(stripIndex >= 0);
  assert.ok(titleIndex > stripIndex);
  assert.ok(stateIndex > titleIndex);
});
