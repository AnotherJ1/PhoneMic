// 从 logo.svg 渲染各尺寸 PNG。一次性资产生成脚本（非运行时依赖）。
//   node render.js   （需先 npm i sharp）
// 产物：
//   web favicon: ../web/favicon-32.png / favicon-180.png / favicon-192.png / favicon-512.png
//   ico 源:      ico/16.png 32 48 64 128 256  （交给 go-winres 合成 .ico + .syso）
const sharp = require('sharp');
const fs = require('fs');
const path = require('path');

const svg = path.join(__dirname, 'logo.svg');
const webDir = path.join(__dirname, '..', 'web');
const icoDir = path.join(__dirname, 'ico');
fs.mkdirSync(icoDir, { recursive: true });

const web = [
  [32, path.join(webDir, 'favicon-32.png')],
  [180, path.join(webDir, 'favicon-180.png')],  // apple-touch
  [192, path.join(webDir, 'favicon-192.png')],
  [512, path.join(webDir, 'favicon-512.png')],
];
const ico = [16, 32, 48, 64, 128, 256].map(s => [s, path.join(icoDir, `${s}.png`)]);

(async () => {
  for (const [size, out] of [...web, ...ico]) {
    await sharp(svg, { density: 384 }).resize(size, size).png().toFile(out);
    console.log('wrote', out, `(${size}px)`);
  }
})();
