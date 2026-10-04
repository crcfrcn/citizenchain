#!/usr/bin/env node

// 公民产品 Logo 唯一派生器。
// 唯一设计真源是 citizenchain/node/resources/icons/logo.png；产品目录里的 PNG/ICNS/ICO 都只是生成物。

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { access, mkdir, readFile, writeFile, lstat, realpath } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { deflateSync, inflateSync } from 'node:zlib';

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const masterPath = path.join(repoRoot, 'node/resources/icons/logo.png');
const productRoots = new Map([['citizenchain', repoRoot]]);
for (const product of ['citizenapp', 'citizenwallet']) {
  const key = '--' + product + '-root';
  const occurrences = process.argv.filter(value => value === key).length;
  if (occurrences > 1) throw Error('Logo产品根参数重复');
  if (occurrences) {
    const index = process.argv.indexOf(key), value = process.argv[index + 1];
    if (!value || !path.isAbsolute(value) || path.resolve(value) !== value || value === repoRoot
      || value.startsWith(repoRoot + path.sep) || repoRoot.startsWith(value + path.sep)) throw Error('Logo产品根须为明确独立的绝对路径');
    productRoots.set(product, value);
  }
}
const outputPath = relativePath => {
  const [product, ...parts] = relativePath.split('/');
  if (!parts.length || parts.some(value => !value || value === '.' || value === '..')) return null;
  const base = productRoots.get(product);
  return base ? path.join(base, ...parts) : null;
};
const checkOnly = process.argv.includes('--check');
const sanitizeMaster = process.argv.includes('--sanitize-master');
const teal = [24, 120, 125, 255];

const crcTable = Array.from({ length: 256 }, (_, initial) => {
  let value = initial;
  for (let bit = 0; bit < 8; bit += 1) {
    value = (value & 1) !== 0 ? 0xedb88320 ^ (value >>> 1) : value >>> 1;
  }
  return value >>> 0;
});

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, 'ascii');
  const output = Buffer.alloc(12 + data.length);
  output.writeUInt32BE(data.length, 0);
  typeBuffer.copy(output, 4);
  data.copy(output, 8);
  output.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 8 + data.length);
  return output;
}

function decodePng(buffer) {
  assert.deepEqual(buffer.subarray(0, 8), Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  let offset = 8;
  let width = 0;
  let height = 0;
  const idat = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString('ascii', offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = data.readUInt32BE(0);
      height = data.readUInt32BE(4);
      assert.equal(data[8], 8, '母版必须是 8-bit PNG');
      assert.equal(data[9], 6, '母版必须是 RGBA PNG');
      assert.equal(data[12], 0, '母版不允许隔行扫描');
    } else if (type === 'IDAT') {
      idat.push(data);
    } else if (type === 'IEND') {
      break;
    }
    offset += 12 + length;
  }
  assert(width > 0 && height > 0 && width === height, 'Logo 母版必须是正方形');
  const packed = inflateSync(Buffer.concat(idat));
  const stride = width * 4;
  const pixels = Buffer.alloc(stride * height);
  let sourceOffset = 0;
  for (let y = 0; y < height; y += 1) {
    const filter = packed[sourceOffset];
    sourceOffset += 1;
    for (let x = 0; x < stride; x += 1) {
      const raw = packed[sourceOffset + x];
      const left = x >= 4 ? pixels[y * stride + x - 4] : 0;
      const up = y > 0 ? pixels[(y - 1) * stride + x] : 0;
      const upLeft = x >= 4 && y > 0 ? pixels[(y - 1) * stride + x - 4] : 0;
      let value;
      if (filter === 0) value = raw;
      else if (filter === 1) value = raw + left;
      else if (filter === 2) value = raw + up;
      else if (filter === 3) value = raw + Math.floor((left + up) / 2);
      else if (filter === 4) {
        const estimate = left + up - upLeft;
        const leftDistance = Math.abs(estimate - left);
        const upDistance = Math.abs(estimate - up);
        const diagonalDistance = Math.abs(estimate - upLeft);
        value = raw + (leftDistance <= upDistance && leftDistance <= diagonalDistance
          ? left
          : upDistance <= diagonalDistance ? up : upLeft);
      } else throw new Error(`不支持的 PNG filter：${filter}`);
      pixels[y * stride + x] = value & 0xff;
    }
    sourceOffset += stride;
  }
  return { width, height, pixels };
}

function encodePng(image, includeAlpha = true) {
  const header = Buffer.alloc(13);
  header.writeUInt32BE(image.width, 0);
  header.writeUInt32BE(image.height, 4);
  header[8] = 8;
  header[9] = includeAlpha ? 6 : 2;
  const sourceStride = image.width * 4;
  const targetStride = image.width * (includeAlpha ? 4 : 3);
  const packed = Buffer.alloc((targetStride + 1) * image.height);
  for (let y = 0; y < image.height; y += 1) {
    const rowOffset = y * (targetStride + 1);
    packed[rowOffset] = 0;
    if (includeAlpha) {
      image.pixels.copy(packed, rowOffset + 1, y * sourceStride, (y + 1) * sourceStride);
    } else {
      for (let x = 0; x < image.width; x += 1) {
        const sourceOffset = y * sourceStride + x * 4;
        const targetOffset = rowOffset + 1 + x * 3;
        image.pixels.copy(packed, targetOffset, sourceOffset, sourceOffset + 3);
      }
    }
  }
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', header),
    pngChunk('IDAT', deflateSync(packed, { level: 9 })),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

function transparentImage(width, height = width) {
  return { width, height, pixels: Buffer.alloc(width * height * 4) };
}

function resizePremultiplied(source, width, height = width) {
  const target = transparentImage(width, height);
  for (let y = 0; y < height; y += 1) {
    const sourceY = (y + 0.5) * source.height / height - 0.5;
    const y0 = Math.max(0, Math.min(source.height - 1, Math.floor(sourceY)));
    const y1 = Math.max(0, Math.min(source.height - 1, y0 + 1));
    const fy = Math.max(0, Math.min(1, sourceY - Math.floor(sourceY)));
    for (let x = 0; x < width; x += 1) {
      const sourceX = (x + 0.5) * source.width / width - 0.5;
      const x0 = Math.max(0, Math.min(source.width - 1, Math.floor(sourceX)));
      const x1 = Math.max(0, Math.min(source.width - 1, x0 + 1));
      const fx = Math.max(0, Math.min(1, sourceX - Math.floor(sourceX)));
      const weights = [(1 - fx) * (1 - fy), fx * (1 - fy), (1 - fx) * fy, fx * fy];
      const offsets = [
        (y0 * source.width + x0) * 4,
        (y0 * source.width + x1) * 4,
        (y1 * source.width + x0) * 4,
        (y1 * source.width + x1) * 4,
      ];
      let alpha = 0;
      const premultiplied = [0, 0, 0];
      for (let sample = 0; sample < 4; sample += 1) {
        const sampleAlpha = source.pixels[offsets[sample] + 3] / 255;
        alpha += sampleAlpha * weights[sample];
        for (let channel = 0; channel < 3; channel += 1) {
          premultiplied[channel] += source.pixels[offsets[sample] + channel]
            * sampleAlpha * weights[sample];
        }
      }
      const targetOffset = (y * width + x) * 4;
      if (alpha > 0) {
        for (let channel = 0; channel < 3; channel += 1) {
          target.pixels[targetOffset + channel] = Math.round(premultiplied[channel] / alpha);
        }
      }
      target.pixels[targetOffset + 3] = Math.round(alpha * 255);
    }
  }
  return target;
}

function composite(target, source, offsetX, offsetY) {
  for (let y = 0; y < source.height; y += 1) {
    for (let x = 0; x < source.width; x += 1) {
      const sourceOffset = (y * source.width + x) * 4;
      const targetOffset = ((y + offsetY) * target.width + x + offsetX) * 4;
      const sourceAlpha = source.pixels[sourceOffset + 3] / 255;
      const targetAlpha = target.pixels[targetOffset + 3] / 255;
      const outputAlpha = sourceAlpha + targetAlpha * (1 - sourceAlpha);
      for (let channel = 0; channel < 3; channel += 1) {
        const value = outputAlpha === 0 ? 0 : (
          source.pixels[sourceOffset + channel] * sourceAlpha
          + target.pixels[targetOffset + channel] * targetAlpha * (1 - sourceAlpha)
        ) / outputAlpha;
        target.pixels[targetOffset + channel] = Math.round(value);
      }
      target.pixels[targetOffset + 3] = Math.round(outputAlpha * 255);
    }
  }
  return target;
}

function placeOnCanvas(master, canvasSize, logoSize, background = null) {
  const target = transparentImage(canvasSize);
  if (background) {
    for (let offset = 0; offset < target.pixels.length; offset += 4) {
      target.pixels.set(background, offset);
    }
  }
  const logo = resizePremultiplied(master, logoSize);
  const inset = Math.floor((canvasSize - logoSize) / 2);
  return composite(target, logo, inset, inset);
}

function platformIcon(master, size) {
  const source = resizePremultiplied(master, size);
  const target = transparentImage(size);
  for (let offset = 0; offset < target.pixels.length; offset += 4) {
    target.pixels.set(teal, offset);
    const red = source.pixels[offset];
    const motifAlpha = Math.max(0, Math.min(1, (red - 140) / 70))
      * source.pixels[offset + 3] / 255;
    for (let channel = 0; channel < 3; channel += 1) {
      target.pixels[offset + channel] = Math.round(
        source.pixels[offset + channel] * motifAlpha
          + teal[channel] * (1 - motifAlpha),
      );
    }
    target.pixels[offset + 3] = 255;
  }
  return target;
}

function adaptiveForeground(master, size) {
  // Android 只把米白图案放在前景安全区，青色背景由 adaptive icon 背景层铺满。
  // 因而圆形、方形、圆角方形等系统遮罩都不会露出透明空白。
  const source = resizePremultiplied(master, Math.round(size * 0.66));
  const motif = transparentImage(source.width);
  for (let offset = 0; offset < source.pixels.length; offset += 4) {
    const red = source.pixels[offset];
    const alpha = Math.max(0, Math.min(1, (red - 140) / 70))
      * source.pixels[offset + 3] / 255;
    motif.pixels[offset] = source.pixels[offset];
    motif.pixels[offset + 1] = source.pixels[offset + 1];
    motif.pixels[offset + 2] = source.pixels[offset + 2];
    motif.pixels[offset + 3] = Math.round(alpha * 255);
  }
  const target = transparentImage(size);
  const inset = Math.floor((size - motif.width) / 2);
  return composite(target, motif, inset, inset);
}

function touchesTransparency(image, x, y, radius = 2) {
  for (let deltaY = -radius; deltaY <= radius; deltaY += 1) {
    for (let deltaX = -radius; deltaX <= radius; deltaX += 1) {
      const sampleX = x + deltaX;
      const sampleY = y + deltaY;
      if (sampleX < 0 || sampleY < 0 || sampleX >= image.width || sampleY >= image.height) return true;
      if (image.pixels[(sampleY * image.width + sampleX) * 4 + 3] === 0) return true;
    }
  }
  return false;
}

function isCleanTeal(pixels, offset) {
  const red = pixels[offset];
  const green = pixels[offset + 1];
  const blue = pixels[offset + 2];
  return green > red + 20 && blue > red + 20 && red < 120 && green < 190 && blue < 195;
}

function invalidOuterEdgeCount(image) {
  let count = 0;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      const alpha = image.pixels[offset + 3];
      if (alpha === 0) {
        if (image.pixels[offset] !== 0 || image.pixels[offset + 1] !== 0
          || image.pixels[offset + 2] !== 0) count += 1;
        continue;
      }
      if (touchesTransparency(image, x, y, 6) && !isCleanTeal(image.pixels, offset)) count += 1;
    }
  }
  return count;
}

function sanitizeMasterOuterEdge(image) {
  const result = { ...image, pixels: Buffer.from(image.pixels) };
  const occupiedRows = [];
  for (let y = 0; y < image.height; y += 1) {
    let occupied = false;
    for (let x = 0; x < image.width && !occupied; x += 1) {
      occupied = image.pixels[(y * image.width + x) * 4 + 3] > 0;
    }
    if (occupied) occupiedRows.push(y);
  }
  assert(occupiedRows.length > 2, 'Logo 母版没有有效主体');

  // 旧母版的最上、最下像素行混入了不透明白线。用相邻主体行建立一层低透明度
  // 青色抗锯齿，既去掉白点，也不使用二值硬切造成锯齿。
  for (const [edgeY, innerY] of [
    [occupiedRows[0], occupiedRows[0] + 1],
    [occupiedRows.at(-1), occupiedRows.at(-1) - 1],
  ]) {
    for (let x = 0; x < image.width; x += 1) {
      const edgeOffset = (edgeY * image.width + x) * 4;
      const innerOffset = (innerY * image.width + x) * 4;
      result.pixels.fill(0, edgeOffset, edgeOffset + 4);
      if (image.pixels[innerOffset + 3] === 0) continue;
      image.pixels.copy(result.pixels, edgeOffset, innerOffset, innerOffset + 3);
      result.pixels[edgeOffset + 3] = Math.round(image.pixels[innerOffset + 3] * 0.25);
    }
  }

  // 透明像素中的隐藏 RGB 也必须清零，否则某些平台缩放器仍会把隐藏白色采样进边缘。
  // 外轮廓不得刷成单一色带；每个边缘像素沿圆角中心方向取相邻内侧的原图青色，
  // 因而纹理和明暗能连续衔接。只替换 RGB，原 Alpha 完全不变，圆角仍保持平滑。
  const alphaShape = { ...result, pixels: Buffer.from(result.pixels) };
  const centerX = (image.width - 1) / 2;
  const centerY = (image.height - 1) / 2;
  for (let y = 0; y < image.height; y += 1) {
    for (let x = 0; x < image.width; x += 1) {
      const offset = (y * image.width + x) * 4;
      if (alphaShape.pixels[offset + 3] === 0) {
        result.pixels.fill(0, offset, offset + 4);
      } else if (touchesTransparency(alphaShape, x, y, 6)) {
        const directionX = centerX - x;
        const directionY = centerY - y;
        const distance = Math.hypot(directionX, directionY);
        let sourceOffset = -1;
        for (let step = 1; step <= 32; step += 1) {
          const sampleX = Math.round(x + directionX / distance * step);
          const sampleY = Math.round(y + directionY / distance * step);
          const candidate = (sampleY * image.width + sampleX) * 4;
          if (alphaShape.pixels[candidate + 3] > 0
            && !touchesTransparency(alphaShape, sampleX, sampleY, 6)
            && isCleanTeal(alphaShape.pixels, candidate)) {
            sourceOffset = candidate;
            break;
          }
        }
        assert(sourceOffset >= 0, `Logo 外缘无法找到相邻内侧青色：${x},${y}`);
        alphaShape.pixels.copy(result.pixels, offset, sourceOffset, sourceOffset + 3);
      }
    }
  }
  return result;
}

function icnsChunk(type, png) {
  const chunk = Buffer.alloc(8 + png.length);
  chunk.write(type, 0, 'ascii');
  chunk.writeUInt32BE(chunk.length, 4);
  png.copy(chunk, 8);
  return chunk;
}

function encodeIcns(pngBySize) {
  const chunks = [
    ['icp4', 16], ['icp5', 32], ['icp6', 64], ['ic07', 128],
    ['ic08', 256], ['ic09', 512], ['ic10', 1024],
  ].map(([type, size]) => icnsChunk(type, pngBySize.get(size)));
  const output = Buffer.concat([Buffer.alloc(8), ...chunks]);
  output.write('icns', 0, 'ascii');
  output.writeUInt32BE(output.length, 4);
  return output;
}

function encodeIco(pngBySize) {
  const sizes = [16, 24, 32, 48, 64, 128, 256];
  const headerSize = 6 + sizes.length * 16;
  const header = Buffer.alloc(headerSize);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  let dataOffset = headerSize;
  const images = [];
  sizes.forEach((size, index) => {
    const image = pngBySize.get(size);
    const entry = 6 + index * 16;
    header[entry] = size === 256 ? 0 : size;
    header[entry + 1] = size === 256 ? 0 : size;
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(image.length, entry + 8);
    header.writeUInt32LE(dataOffset, entry + 12);
    dataOffset += image.length;
    images.push(image);
  });
  return Buffer.concat([header, ...images]);
}

const outputs = new Map();
const addPng = (relativePath, image, includeAlpha = true) => {
  outputs.set(relativePath, encodePng(image, includeAlpha));
};

async function buildOutputs() {
  const masterBuffer = await readFile(masterPath);
  let master = decodePng(masterBuffer);
  assert.equal(master.width, 1024, 'Logo 母版必须固定为 1024×1024');
  if (sanitizeMaster) {
    master = sanitizeMasterOuterEdge(master);
    await writeFile(masterPath, encodePng(master));
  }
  assert.equal(invalidOuterEdgeCount(master), 0, 'Logo 母版外轮廓颜色或透明像素 RGB 不干净');

  const iosIconNames = new Map([
    ['Icon-App-20x20@1x.png', 20], ['Icon-App-20x20@2x.png', 40],
    ['Icon-App-20x20@3x.png', 60], ['Icon-App-29x29@1x.png', 29],
    ['Icon-App-29x29@2x.png', 58], ['Icon-App-29x29@3x.png', 87],
    ['Icon-App-40x40@1x.png', 40], ['Icon-App-40x40@2x.png', 80],
    ['Icon-App-40x40@3x.png', 120], ['Icon-App-60x60@2x.png', 120],
    ['Icon-App-60x60@3x.png', 180], ['Icon-App-76x76@1x.png', 76],
    ['Icon-App-76x76@2x.png', 152], ['Icon-App-83.5x83.5@2x.png', 167],
    ['Icon-App-1024x1024@1x.png', 1024],
  ]);
  const android = [
    ['mdpi', 1], ['hdpi', 1.5], ['xhdpi', 2], ['xxhdpi', 3], ['xxxhdpi', 4],
  ];
  for (const product of ['citizenapp', 'citizenwallet']) {
    const androidResources = product === 'citizenwallet' ? `${product}/resources/android` : `${product}/android/app/src/main/res`;
    const iosResources = product === 'citizenwallet' ? `${product}/resources/ios` : `${product}/ios/Runner`;
    outputs.set(`${androidResources}/values/colors.xml`, Buffer.from(
      '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
        + '    <color name="ic_launcher_background">#18787D</color>\n</resources>\n',
    ));
    for (const [name, size] of iosIconNames) {
      // iOS AppIcon 禁止透明。青色背景铺满整个方形画布，米白图案从母版提取；
      // 禁止把透明圆角用纯色补洞，否则会在原图与补色之间形成异色环。
      addPng(`${iosResources}/Assets.xcassets/AppIcon.appiconset/${name}`,
        platformIcon(master, size), false);
    }
    for (const [name, size] of [['CitizenLaunchLogo.png', 160], ['CitizenLaunchLogo@2x.png', 320], ['CitizenLaunchLogo@3x.png', 480]]) {
      addPng(`${iosResources}/Assets.xcassets/CitizenLaunchLogo.imageset/${name}`,
        placeOnCanvas(master, size, Math.round(size * 0.8)));
    }
    const launchContents = `${JSON.stringify({
      images: [
        { idiom: 'universal', filename: 'CitizenLaunchLogo.png', scale: '1x' },
        { idiom: 'universal', filename: 'CitizenLaunchLogo@2x.png', scale: '2x' },
        { idiom: 'universal', filename: 'CitizenLaunchLogo@3x.png', scale: '3x' },
      ],
      info: { version: 1, author: 'xcode' },
    }, null, 2)}\n`;
    outputs.set(`${iosResources}/Assets.xcassets/CitizenLaunchLogo.imageset/Contents.json`, Buffer.from(launchContents));

    for (const [density, scale] of android) {
      const resourceRoot = `${androidResources}/mipmap-${density}`;
      const launchSize = Math.round(120 * scale);
      const legacySize = Math.round(48 * scale);
      const foregroundSize = Math.round(108 * scale);
      addPng(`${resourceRoot}/launch_image.png`, placeOnCanvas(master, launchSize, Math.round(launchSize * 0.8)));
      const legacy = platformIcon(master, legacySize);
      addPng(`${resourceRoot}/ic_launcher.png`, legacy, false);
      addPng(`${resourceRoot}/ic_launcher_round.png`, legacy, false);
      addPng(`${resourceRoot}/ic_launcher_foreground.png`, adaptiveForeground(master, foregroundSize));
    }
  }

  addPng('citizenwallet/resources/icons/citizen-logo.png', resizePremultiplied(master, 256));

  const desktopSizes = [16, 24, 32, 48, 64, 128, 256, 512, 1024];
  // Tauri 的 generate_context! 会在编译期拒绝 RGB PNG，即使图像视觉上完全不透明，
  // 桌面 PNG 也必须保留 RGBA 色彩类型。ICNS/ICO 继续复用同一组 RGBA PNG，禁止
  // 为不同打包格式另造第二套像素派生路径。
  const desktopPng = new Map(desktopSizes.map((size) => [size, encodePng(platformIcon(master, size))]));
  // icons/logo.png 是母版，只读取；派生文件不得覆盖它。
  const desktopNames = new Map([
    ['32x32.png', 32], ['32x32@2x.png', 64], ['128x128.png', 128],
    ['128x128@2x.png', 256], ['256x256.png', 256], ['256x256@2x.png', 512],
    ['512x512.png', 512], ['512x512@2x.png', 1024], ['icon.png', 1024],
  ]);
  for (const [name, size] of desktopNames) {
    const png = desktopPng.get(size);
    // PNG 固定头之后，IHDR 数据的第 10 字节是 color type；6 表示 RGBA。
    // 该断言钉住 Tauri 的真实编译前提，避免生成器与错误 RGB 派生物一起“自洽通过”。
    assert.equal(png[25], 6, `CitizenChain Tauri 图标 ${name} 必须是 RGBA PNG`);
    outputs.set(`citizenchain/node/resources/icons/${name}`, png);
  }
  outputs.set('citizenchain/node/resources/icons/icon.icns', encodeIcns(desktopPng));
  outputs.set('citizenchain/node/resources/icons/icon.ico', encodeIco(desktopPng));
}

async function writeOrCheck() {
  const mismatches = [];
  for (const [relativePath, expected] of outputs) {
    const absolutePath = outputPath(relativePath);
    if (!absolutePath) continue;
    if (checkOnly) {
      let actual;
      try { actual = await readFile(absolutePath); } catch { mismatches.push(`${relativePath}（缺失）`); continue; }
      if (!actual.equals(expected)) mismatches.push(relativePath);
    } else {
      await mkdir(path.dirname(absolutePath), { recursive: true });
      await writeFile(absolutePath, expected);
    }
  }
  if (productRoots.has('citizenapp')) {
    const hash = bytes => createHash('sha256').update(bytes).digest('hex');
    const files = Object.fromEntries([...outputs].filter(([name]) => name.startsWith('citizenapp/'))
      .map(([name, bytes]) => [name.slice('citizenapp/'.length), hash(bytes)]).sort(([a], [b]) => a.localeCompare(b)));
    const manifest = JSON.stringify({ schema: 1, product: 'citizenapp',
      source: { repository: 'crcfrcn/citizenchain', path: 'node/resources/icons/logo.png',
        sha256: hash(await readFile(masterPath)) }, files }, null, 2) + '\n';
    const target = path.join(productRoots.get('citizenapp'), 'scripts/logo-assets.json');
    if (checkOnly) {
      const actual = JSON.parse(await readFile(target, 'utf8')), expected = JSON.parse(manifest);
      assert.deepEqual(actual, expected, '公民Logo受控清单未与唯一派生器同步');
    } else await writeFile(target, manifest);
  }
  if (mismatches.length > 0) {
    throw new Error(`以下 Logo 派生物未由 citizenchain/node/resources/icons/logo.png 生成：\n${mismatches.map((item) => `- ${item}`).join('\n')}`);
  }
  if (checkOnly) {
    const forbiddenLegacyAssets = [
      'citizenchain/branding',
      'citizenchain/node/icons',
      'citizenchain/resources',
      'docs/logo.svg',
      'docs/logo256.png',
      'citizenapp/ios/Runner/Base.lproj/LaunchScreen.storyboard',
      'citizenapp/ios/Runner/Assets.xcassets/LaunchImage.imageset',
      'citizenwallet/resources/ios/Base.lproj/LaunchScreen.storyboard',
      'citizenwallet/assets',
      'citizenwallet/android/app/src/main/res',
      'citizenwallet/ios/Runner/Assets.xcassets',
      'citizenwallet/resources/ios/Assets.xcassets/LaunchImage.imageset',
    ];
    const legacyAssets = [];
    for (const relativePath of forbiddenLegacyAssets) {
      try {
        const absolutePath = outputPath(relativePath);
        if (!absolutePath) continue;
        await access(absolutePath);
        legacyAssets.push(relativePath);
      } catch {
        // 不存在才是唯一真源目标态。
      }
    }
    if (legacyAssets.length > 0) {
      throw new Error(`以下旧 Logo 资源必须删除：\n${legacyAssets.map((item) => `- ${item}`).join('\n')}`);
    }
  }
  process.stdout.write(checkOnly
    ? `Logo 唯一真源检查通过：${outputs.size} 个派生文件。\n`
    : `已从 citizenchain/node/resources/icons/logo.png 生成 ${outputs.size} 个 Logo 派生文件。\n`);
}

for (const base of productRoots.values()) {
  let current = path.parse(base).root;
  for (const part of path.relative(current, base).split(path.sep)) {
    current = path.join(current, part); const info = await lstat(current);
    if (!info.isDirectory() || info.isSymbolicLink()) throw Error('Logo产品根缺失或经过链接');
  }
  if (await realpath(base) !== base) throw Error('Logo产品根存在路径别名');
}
await buildOutputs();
await writeOrCheck();
