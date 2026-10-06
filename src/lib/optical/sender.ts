// Animated QR stream of one payload, painted onto a canvas.
//
// Adapted from Decimen Optical Transfer v0.5.3 send/main.ts
// (Copyright (c) 2026 Evan Crawley, AGPL-3.0-or-later; see
// src/lib/vendor/decimen/NOTICE). Single code, no export, no settings UI.

import { LTEncoder } from "$lib/vendor/decimen/shared/fountain";
import { blockLength, fitsInOneStream } from "$lib/vendor/decimen/shared/frame-capacity";
import { fnv1a, packFile, packFrame, type FrameHeader } from "$lib/vendor/decimen/shared/protocol";
import { rasterizeQr } from "$lib/vendor/decimen/shared/qr-raster";
import { createFrameQr, QUIET_ZONE_MODULES, type EccLevel } from "$lib/vendor/decimen/send/qr-frame";

// ponytail: fixed for laptop webcams and older phones, which miss dense fast
// codes. Notes are kilobytes, so a whole sweep still takes well under a second.
// Expose Decimen's fps/frame-size knobs if large payloads ever need speed.
export const STREAM_FPS = 15;
export const FRAME_BYTES = 1000;
export const ECC: EccLevel = "M";
export const PAYLOAD_NAME = "panote-transfer.panote";

export interface FrameSource {
  /** Source blocks; one carousel sweep shows each of them once. */
  k: number;
  next(): Uint8Array;
}

/** Wrap the sealed payload in Decimen's file container and fountain-code it. */
export async function frameSource(payload: Uint8Array): Promise<FrameSource> {
  const { container } = await packFile(PAYLOAD_NAME, "application/octet-stream", payload);
  if (!fitsInOneStream(container.length, FRAME_BYTES)) throw new Error("Too much to send in one stream.");
  const blockLen = blockLength(FRAME_BYTES);
  const sessionId = (Math.floor(Math.random() * 0xffff) + 1) & 0xffff;
  const encoder = new LTEncoder(container, blockLen, sessionId);
  const header: FrameHeader = {
    sessionId,
    seq: 0,
    k: encoder.k,
    blockLen,
    totalLen: container.length,
    payloadFnv: fnv1a(container),
    flags: 0,
  };
  let seq = 0;
  return {
    k: encoder.k,
    next() {
      const frame = packFrame({ ...header, seq }, encoder.encode(seq));
      seq++;
      return frame;
    },
  };
}

/** Paint frames at STREAM_FPS until the returned stop() is called. The canvas
 *  raster is an integer multiple of the code so modules stay crisp. */
export function playStream(canvas: HTMLCanvasElement, source: FrameSource): () => void {
  const staging = document.createElement("canvas");
  const interval = 1000 / STREAM_FPS;
  let version: number | undefined; // every frame the same QR version, so the code never jumps size
  let nextAt = performance.now();
  let raf = 0;
  let stopped = false;

  const paint = () => {
    const qr = createFrameQr(source.next(), ECC, version);
    version ??= qr.version;
    const raster = rasterizeQr(qr.modules.size, qr.modules.data, QUIET_ZONE_MODULES);
    const img = new ImageData(new Uint8ClampedArray(raster.pixels.buffer), raster.size, raster.size);
    if (staging.width !== img.width) {
      staging.width = staging.height = img.width;
      const dpr = window.devicePixelRatio || 1;
      const scale = Math.max(1, Math.floor((canvas.clientWidth * dpr) / img.width));
      canvas.width = canvas.height = img.width * scale;
      // Display at exactly the raster's size, at most one module short of the
      // layout box, so the browser never rescales the modules.
      canvas.style.width = canvas.style.height = `${canvas.width / dpr}px`;
    }
    staging.getContext("2d")!.putImageData(img, 0, 0);
    const ctx = canvas.getContext("2d")!;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(staging, 0, 0, canvas.width, canvas.height);
  };

  const tick = (now: number) => {
    if (stopped) return;
    raf = requestAnimationFrame(tick);
    if (now < nextAt) return;
    // A stall (hidden window) leaves a backlog no camera saw; restart the cadence.
    nextAt = now - nextAt > interval ? now + interval : nextAt + interval;
    paint();
  };
  paint();
  raf = requestAnimationFrame(tick);
  // A screen that sleeps mid-stream ends the transfer. Best effort: without the
  // API, or if refused, the stream runs anyway.
  type WakeLock = { release(): Promise<void> };
  const wake = (navigator as Navigator & { wakeLock?: { request(t: "screen"): Promise<WakeLock> } }).wakeLock
    ?.request("screen")
    .catch(() => undefined);
  return () => {
    stopped = true;
    cancelAnimationFrame(raf);
    void wake?.then((lock) => lock?.release());
  };
}
