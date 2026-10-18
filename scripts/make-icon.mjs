import { deflateSync } from 'node:zlib';
import { writeFileSync } from 'node:fs';

/**
 * 產生 app-icon.png（1024x1024 RGBA）作為 tauri icon 的來源。
 * 不依賴任何圖片函式庫：手寫 PNG 編碼（IHDR/IDAT/IEND + CRC32）。
 * 圖形：深色圓角底 + 三條「清單」白條，其中一條帶橘色標記（象徵被標記的問題列）。
 */

const SIZE = 1024;

const BG = [15, 23, 42, 255]; // #0f172a
const BAR = [226, 232, 240, 255]; // #e2e8f0
const ACCENT = [249, 115, 22, 255]; // #f97316

const px = new Uint8Array(SIZE * SIZE * 4);

function setPx(x, y, c) {
  if (x < 0 || y < 0 || x >= SIZE || y >= SIZE) return;
  const i = (y * SIZE + x) * 4;
  px[i] = c[0];
  px[i + 1] = c[1];
  px[i + 2] = c[2];
  px[i + 3] = c[3];
}

function fillRoundRect(x0, y0, w, h, radius, c) {
  for (let y = y0; y < y0 + h; y++) {
    for (let x = x0; x < x0 + w; x++) {
      const dx = Math.min(x - x0, x0 + w - 1 - x);
      const dy = Math.min(y - y0, y0 + h - 1 - y);
      if (dx < radius && dy < radius) {
        const ox = radius - dx;
        const oy = radius - dy;
        if (ox * ox + oy * oy > radius * radius) continue;
      }
      setPx(x, y, c);
    }
  }
}

// 底
fillRoundRect(32, 32, SIZE - 64, SIZE - 64, 180, BG);

// 三條清單
const barX = 224;
const barH = 96;
const r = barH / 2;
fillRoundRect(barX, 288, 576, barH, r, BAR);
fillRoundRect(barX, 464, 448, barH, r, BAR);
fillRoundRect(barX, 640, 512, barH, r, BAR);

// 橘色標記：第二條的左側圓點 + 第三條尾端方塊
fillRoundRect(112, 464, barH, barH, r, ACCENT);
fillRoundRect(704, 640, barH, barH, 24, ACCENT);

// ---- PNG 編碼 ----
const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  Buffer.from(px.buffer, y * SIZE * 4, SIZE * 4).copy(raw, y * (SIZE * 4 + 1) + 1);
}

const crcTable = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = -1;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // truecolor + alpha
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);

writeFileSync('app-icon.png', png);
console.log(`app-icon.png written (${SIZE}x${SIZE}, ${png.length} bytes)`);
