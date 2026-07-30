import { deflateSync } from 'node:zlib';

let _crcTable: Uint32Array | undefined;
function crcTable(): Uint32Array {
	if (_crcTable) return _crcTable;
	const t = new Uint32Array(256);
	for (let n = 0; n < 256; n++) {
		let c = n;
		for (let k = 0; k < 8; k++) {
			c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
		}
		t[n] = c;
	}
	_crcTable = t;
	return t;
}

function crc32(buf: Buffer): number {
	const table = crcTable();
	let c = 0xFFFFFFFF;
	for (let i = 0; i < buf.length; i++) {
		c = table[(c ^ buf[i]!) & 0xFF]! ^ (c >>> 8);
	}
	return (c ^ 0xFFFFFFFF) >>> 0;
}

function makePngChunk(type: string, data: Buffer): Buffer {
	const lenB = Buffer.alloc(4);
	lenB.writeUInt32BE(data.length);
	const typeB = Buffer.from(type, 'ascii');
	const crcInput = Buffer.concat([typeB, data]);
	const crc = crc32(crcInput);
	const crcB = Buffer.alloc(4);
	crcB.writeUInt32BE(crc);
	return Buffer.concat([lenB, typeB, data, crcB]);
}

export function buildPng(w: number, h: number): Buffer {
	// Build raw RGB pixel data
	const rawRowSize = 1 + w * 3;
	const raw = Buffer.alloc(rawRowSize * h);
	for (let y = 0; y < h; y++) {
		const off = y * rawRowSize;
		raw[off] = 0; // filter: None
		for (let x = 0; x < w; x++) {
			raw[off + 1 + x * 3] = 255;     // R
			raw[off + 1 + x * 3 + 1] = 0;   // G
			raw[off + 1 + x * 3 + 2] = 0;   // B
		}
	}
	const compressed = deflateSync(raw);

	// IHDR chunk
	const ihdrData = Buffer.alloc(13);
	ihdrData.writeUInt32BE(w, 0);
	ihdrData.writeUInt32BE(h, 4);
	ihdrData[8] = 8;  // bit depth
	ihdrData[9] = 2;  // color type RGB
	ihdrData[10] = 0; // compression
	ihdrData[11] = 0; // filter
	ihdrData[12] = 0; // interlace

	return Buffer.concat([
		Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), // PNG signature
		makePngChunk('IHDR', ihdrData),
		makePngChunk('IDAT', compressed),
		makePngChunk('IEND', Buffer.alloc(0)),
	]);
}
