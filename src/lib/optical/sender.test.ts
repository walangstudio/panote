import { describe, expect, it } from "vitest";
import { LTDecoder } from "$lib/vendor/decimen/shared/fountain";
import { fnv1a, parseFrame, unpackFile, verifyFile } from "$lib/vendor/decimen/shared/protocol";
import { createFrameQr } from "$lib/vendor/decimen/send/qr-frame";
import { ECC, frameSource, PAYLOAD_NAME } from "./sender";

function payload(size: number): Uint8Array {
  const bytes = new Uint8Array(size);
  let x = 0x9e3779b9;
  for (let i = 0; i < size; i++) {
    x ^= x << 13;
    x ^= x >>> 17;
    x ^= x << 5;
    bytes[i] = x & 0xff;
  }
  return bytes;
}

/** What the receiver does with every frame it manages to read. */
async function receive(frames: Uint8Array[]) {
  let decoder: LTDecoder | null = null;
  for (const frame of frames) {
    const parsed = parseFrame(frame);
    if (!parsed) throw new Error("frame did not parse");
    decoder ??= new LTDecoder(parsed.header.k, parsed.header.blockLen, parsed.header.sessionId, parsed.header.totalLen);
    decoder.addFrame(parsed.header.seq, parsed.block);
    if (decoder.isComplete) {
      const container = decoder.assemble()!;
      expect(fnv1a(container)).toBe(parsed.header.payloadFnv);
      const file = await unpackFile(container);
      expect(await verifyFile(file)).toBe(true);
      return file;
    }
  }
  return null;
}

describe("optical sender stream", () => {
  it("one clean sweep delivers the payload", async () => {
    const sealed = payload(5000);
    const source = await frameSource(sealed);
    const file = await receive(Array.from({ length: source.k }, () => source.next()));
    expect(file?.name).toBe(PAYLOAD_NAME);
    expect(file?.bytes).toEqual(sealed);
  });

  it("survives a camera that misses a third of the frames", async () => {
    const sealed = payload(20_000);
    const source = await frameSource(sealed);
    let r = 7;
    const caught: Uint8Array[] = [];
    for (let i = 0; i < source.k * 10; i++) {
      const frame = source.next();
      r = (r * 1103515245 + 12345) & 0x7fffffff;
      if (r % 3 !== 0) caught.push(frame);
    }
    const file = await receive(caught);
    expect(file?.bytes).toEqual(sealed);
  });

  it("every frame fits a QR code at the chosen error correction", async () => {
    const source = await frameSource(payload(3000));
    const versions = new Set(Array.from({ length: source.k + 2 }, () => createFrameQr(source.next(), ECC, undefined).version));
    expect(versions.size).toBe(1);
  });
});
