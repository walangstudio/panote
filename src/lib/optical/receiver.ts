// Camera to payload: capture frames, decode QR codes in a worker pool, feed the
// fountain decoder, and hand back the reassembled payload.
//
// Adapted from Decimen Optical Transfer v0.5.3 receive/main.ts
// (Copyright (c) 2026 Evan Crawley, AGPL-3.0-or-later; see
// src/lib/vendor/decimen/NOTICE). Kept: per-code crop tracking, the full-scan
// cadence and the stream identity checks. Dropped: diagnostics, the opt-in
// bitmap capture path, camera pickers and the settings UI.

import { LTDecoder } from "$lib/vendor/decimen/shared/fountain";
import {
  classifyFrame,
  fnv1a,
  frameVerdictMessage,
  parseFrame,
  streamIdentity,
  unpackFile,
  verifyFile,
} from "$lib/vendor/decimen/shared/protocol";
import { DecodeWorkerPool, type SymbolBox, type SymbolInfo, type SymbolQuad } from "$lib/vendor/decimen/shared/worker-pool";
import { createDecodeWorker } from "$lib/vendor/decimen/receive/worker-factory";
import { PAYLOAD_NAME } from "./sender";

interface Region extends SymbolBox {
  seen: number;
  decoded: boolean;
  drift?: number;
  quad?: SymbolQuad;
  dim?: number;
}

// Tuning carried over from Decimen, where each value is measured; see its
// receive/main.ts for the reasoning behind every one.
const REGION_TTL_MS = 1500;
const FULL_SCAN_INTERVAL_MS = 1500;
const FULL_SCAN_DEGRADED_MS = 250;
const ACQUISITION_SCAN_MS = 100;
const EXPECTED_REGIONS_DECAY_MS = 10_000;
const REGION_PAD = 0.35;
const MAX_REGIONS = 9;
const CAPTURE_WIDTH = 1280;
/** Above the Rust side's own cap on a sealed transfer, with room for the container. */
const MAX_STREAM_BYTES = 16 * 1024 * 1024;

export interface ReceiverEvents {
  /** Blocks solved so far out of k, once the first frame of a stream lands. */
  progress(solved: number, k: number): void;
  /** The verified payload. Capture has already stopped. */
  complete(payload: Uint8Array): void;
  /** A frame decoded but cannot be used (another app's or format version). */
  notice(message: string): void;
  error(message: string): void;
}

export class OpticalReceiver {
  private stream: MediaStream | null = null;
  private decoder: LTDecoder | null = null;
  private streamKey = "";
  private readonly regions: Region[] = [];
  private readonly grab = document.createElement("canvas");
  private readonly pool: DecodeWorkerPool;
  private frameId = 0;
  private lastFullScan = 0;
  private cropRotate = 0;
  private expectedRegions = 0;
  private expectedRegionsAt = 0;
  private done = false;
  private noticeShown: string | null = null;

  constructor(
    private readonly video: HTMLVideoElement,
    private readonly events: ReceiverEvents,
  ) {
    this.pool = new DecodeWorkerPool(
      createDecodeWorker,
      (bytes, box, info) => this.onDecoded(bytes, box, info),
      (box) => this.noteRegion(box, performance.now(), false),
    );
  }

  async start(): Promise<void> {
    if (!navigator.mediaDevices?.getUserMedia) throw new Error("This device has no camera access.");
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: {
        facingMode: "environment",
        width: { ideal: CAPTURE_WIDTH },
        height: { ideal: Math.round((CAPTURE_WIDTH * 3) / 4) },
      },
    });
    // Closed while the permission prompt was up: stop() already ran and found
    // no stream, so release this one here or the camera stays on.
    if (this.done) {
      stream.getTracks().forEach((t) => t.stop());
      return;
    }
    this.stream = stream;
    this.video.srcObject = stream;
    this.video.muted = true;
    this.video.playsInline = true;
    await this.video.play();
    if (this.done) return; // stop() ran during play() and released the stream
    this.pool.resize(Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1)));
    this.scheduleFrame();
  }

  stop(): void {
    this.done = true;
    this.pool.resize(0);
    this.stream?.getTracks().forEach((t) => t.stop());
    this.stream = null;
    this.video.srcObject = null;
  }

  private scheduleFrame(): void {
    if (this.done) return;
    const v = this.video as HTMLVideoElement & { requestVideoFrameCallback?: (cb: () => void) => number };
    const next = () => {
      if (this.done) return;
      this.captureFrame();
      this.scheduleFrame();
    };
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(next);
    else requestAnimationFrame(next);
  }

  private decodedCount(): number {
    return this.regions.filter((r) => r.decoded).length;
  }

  private noteRegion(box: SymbolBox, now: number, decoded: boolean, info?: SymbolInfo): void {
    for (const r of this.regions) {
      const dx = Math.abs(box.x + box.w / 2 - (r.x + r.w / 2));
      const dy = Math.abs(box.y + box.h / 2 - (r.y + r.h / 2));
      if (dx < Math.max(box.w, r.w) / 2 && dy < Math.max(box.h, r.h) / 2) {
        // A sighting keeps a region alive but never moves it: failed quads are
        // routinely clipped or mis-sized.
        if (!decoded) {
          r.seen = now;
          return;
        }
        r.drift = 0.5 * (r.drift ?? 0) + 0.5 * Math.hypot(dx, dy);
        Object.assign(r, box, { seen: now, decoded: true });
        if (info?.quad) r.quad = info.quad;
        if (info?.modules) r.dim = info.modules;
        return;
      }
    }
    if (!decoded) {
      // Only found a region from a sighting that matches a proven code's size.
      const reference = this.regions.find((r) => r.decoded);
      if (!reference) return;
      const ratio = Math.max(box.w, box.h) / Math.max(reference.w, reference.h);
      if (ratio < 0.5 || ratio > 2) return;
    }
    this.regions.push({ ...box, seen: now, decoded, quad: info?.quad, dim: info?.modules });
    if (this.regions.length > MAX_REGIONS) {
      this.regions.sort((a, b) => Number(b.decoded) - Number(a.decoded) || b.seen - a.seen);
      this.regions.length = MAX_REGIONS;
    }
  }

  private captureFrame(): void {
    const { video, pool, regions } = this;
    const vw = video.videoWidth;
    const vh = video.videoHeight;
    if (!vw || !vh || pool.busyCount === pool.size) return;
    const now = performance.now();

    for (let i = regions.length - 1; i >= 0; i--) {
      if (now - regions[i]!.seen > REGION_TTL_MS) regions.splice(i, 1);
    }
    const live = this.decodedCount();
    if (live >= this.expectedRegions || now - this.expectedRegionsAt > EXPECTED_REGIONS_DECAY_MS) {
      this.expectedRegions = live;
      this.expectedRegionsAt = now;
    }
    const scanInterval =
      live === 0 ? ACQUISITION_SCAN_MS : live < this.expectedRegions ? FULL_SCAN_DEGRADED_MS : FULL_SCAN_INTERVAL_MS;

    if (this.grab.width !== vw || this.grab.height !== vh) {
      this.grab.width = vw;
      this.grab.height = vh;
    }
    const ctx = this.grab.getContext("2d", { willReadFrequently: true })!;
    ctx.drawImage(video, 0, 0);

    if (now - this.lastFullScan > scanInterval) {
      this.lastFullScan = now;
      const img = ctx.getImageData(0, 0, vw, vh);
      pool.submit(
        { id: this.frameId++, buf: img.data.buffer, w: vw, h: vh, ox: 0, oy: 0, full: true },
        [img.data.buffer],
      );
      return;
    }
    // One crop per known code, rotated so a short pool never starves the tail.
    for (let i = 0; i < regions.length; i++) {
      const r = regions[(i + this.cropRotate) % regions.length]!;
      const size = Math.max(r.w, r.h);
      const pad = Math.round(size * REGION_PAD + Math.min(size, 2 * (r.drift ?? 0)));
      const x = Math.max(0, Math.floor(r.x - pad));
      const y = Math.max(0, Math.floor(r.y - pad));
      const w = Math.min(vw - x, Math.ceil(r.w + 2 * pad));
      const h = Math.min(vh - y, Math.ceil(r.h + 2 * pad));
      if (w < 32 || h < 32) continue;
      const img = ctx.getImageData(x, y, w, h);
      const taken = pool.submit(
        { id: this.frameId++, buf: img.data.buffer, w, h, ox: x, oy: y, full: false, quad: r.quad, dim: r.dim },
        [img.data.buffer],
      );
      if (!taken) break;
    }
    this.cropRotate++;
  }

  private onDecoded(bytes: Uint8Array, box?: SymbolBox, info?: SymbolInfo): void {
    if (box) this.noteRegion(box, performance.now(), true, info);
    if (this.done) return;
    const parsed = parseFrame(bytes);
    if (!parsed) {
      const message = frameVerdictMessage(classifyFrame(bytes));
      if (message) this.notice(message);
      return;
    }
    const { header, block } = parsed;
    // The header is attacker-controlled until the payload verifies; never size a
    // decoder from a claim larger than any panote transfer.
    if (header.totalLen > MAX_STREAM_BYTES) {
      this.notice("That stream is larger than a panote transfer can be.");
      return;
    }
    const identity = streamIdentity(header);
    if (!this.decoder || this.streamKey !== identity) {
      this.decoder = new LTDecoder(header.k, header.blockLen, header.sessionId, header.totalLen);
      this.streamKey = identity;
    }
    this.decoder.addFrame(header.seq, block);
    this.events.progress(this.decoder.solvedCount, this.decoder.k);
    if (!this.decoder.isComplete) return;

    const container = this.decoder.assemble()!;
    if (fnv1a(container) !== header.payloadFnv) {
      // A corrupt assembly: start this stream over rather than fail the user.
      this.decoder = null;
      this.streamKey = "";
      return;
    }
    this.stop();
    void this.finish(container);
  }

  /** Each distinct message once: a bad stream repeats it dozens of times a second. */
  private notice(message: string): void {
    if (message === this.noticeShown) return;
    this.noticeShown = message;
    this.events.notice(message);
  }

  private async finish(container: Uint8Array): Promise<void> {
    try {
      const file = await unpackFile(container);
      if (!(await verifyFile(file))) throw new Error("The received data failed verification.");
      if (file.name !== PAYLOAD_NAME) throw new Error("That stream is not a panote transfer.");
      this.events.complete(file.bytes);
    } catch (err) {
      this.events.error(err instanceof Error ? err.message : String(err));
    }
  }
}
