// A fake camera for the receive test: the sender's real QR stream rendered into
// a Y4M video that Chromium plays as the webcam
// (--use-file-for-fake-video-capture). Relative imports: global setup runs in
// Node, outside Vite's $lib alias.
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { LTEncoder } from "../src/lib/vendor/decimen/shared/fountain";
import { blockLength } from "../src/lib/vendor/decimen/shared/frame-capacity";
import { fnv1a, packFile, packFrame } from "../src/lib/vendor/decimen/shared/protocol";
import { rasterizeQr } from "../src/lib/vendor/decimen/shared/qr-raster";
import { createFrameQr, QUIET_ZONE_MODULES } from "../src/lib/vendor/decimen/send/qr-frame";

export const FAKE_CAMERA = join(tmpdir(), "panote-optical-camera.y4m");
const W = 640;
const H = 480;
const FPS = 10;
const CYCLES = 3;

/** What the stream carries: stands in for a sealed panote payload. */
export const OPTICAL_PAYLOAD = Uint8Array.from({ length: 2500 }, (_, i) => (i * 131 + 7) & 0xff);

export async function writeFakeCamera(): Promise<void> {
  // Same container, name and frame size the app's sender uses.
  const { container } = await packFile("panote-transfer.panote", "application/octet-stream", OPTICAL_PAYLOAD);
  const blockLen = blockLength(1000);
  const encoder = new LTEncoder(container, blockLen, 0x1234);
  const header = {
    sessionId: 0x1234, seq: 0, k: encoder.k, blockLen,
    totalLen: container.length, payloadFnv: fnv1a(container), flags: 0,
  };

  const chunks: Buffer[] = [Buffer.from(`YUV4MPEG2 W${W} H${H} F${FPS}:1 Ip A1:1 C420jpeg\n`)];
  const chroma = Buffer.alloc((W / 2) * (H / 2), 128);
  let version: number | undefined;
  for (let seq = 0; seq < encoder.k * CYCLES; seq++) {
    const qr = createFrameQr(packFrame({ ...header, seq }, encoder.encode(seq)), "M", version);
    version ??= qr.version;
    const raster = rasterizeQr(qr.modules.size, qr.modules.data, QUIET_ZONE_MODULES);
    const scale = Math.floor((H - 40) / raster.size);
    const ox = Math.floor((W - raster.size * scale) / 2);
    const oy = Math.floor((H - raster.size * scale) / 2);
    const luma = Buffer.alloc(W * H, 255);
    for (let y = 0; y < raster.size * scale; y++) {
      for (let x = 0; x < raster.size * scale; x++) {
        const dark = raster.pixels[Math.floor(y / scale) * raster.size + Math.floor(x / scale)] === 0xff000000;
        luma[(oy + y) * W + ox + x] = dark ? 0 : 255;
      }
    }
    chunks.push(Buffer.from("FRAME\n"), luma, chroma, chroma);
  }
  writeFileSync(FAKE_CAMERA, Buffer.concat(chunks));
}
