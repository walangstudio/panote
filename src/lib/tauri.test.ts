import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  noteCreate,
  noteDelete,
  noteGet,
  noteList,
  noteUpdate,
  noteReceiveAccept,
  noteReceiveReject,
  noteSend,
  notesSend,
  pendingTransfersList,
  pendingOffersList,
  transferOfferRespond,
  peersScan,
  generatePairingCode,
  knownPeersList,
  startReceiving,
  stopReceiving,
  isReceiving,
} from "./tauri";

const input = { kind: "document" as const, title: "t", content: {}, tags: [] };

describe("tauri bindings", () => {
  beforeEach(() => vi.clearAllMocks());

  it("noteCreate → note_create", async () => {
    await noteCreate(input);
    expect(invoke).toHaveBeenCalledWith("note_create", { input });
  });

  it("noteUpdate → note_update", async () => {
    await noteUpdate("id1", input);
    expect(invoke).toHaveBeenCalledWith("note_update", { id: "id1", input });
  });

  it("noteDelete → note_delete", async () => {
    await noteDelete("id1");
    expect(invoke).toHaveBeenCalledWith("note_delete", { id: "id1" });
  });

  it("noteList → note_list", async () => {
    await noteList();
    expect(invoke).toHaveBeenCalledWith("note_list");
  });

  it("noteGet → note_get", async () => {
    await noteGet("id1");
    expect(invoke).toHaveBeenCalledWith("note_get", { id: "id1" });
  });

  it("peersScan → peers_scan", async () => {
    await peersScan();
    expect(invoke).toHaveBeenCalledWith("peers_scan");
  });

  it("noteSend → note_send with passphrase", async () => {
    await noteSend("note1", "peer1", "secret");
    expect(invoke).toHaveBeenCalledWith("note_send", {
      noteId: "note1",
      peerId: "peer1",
      passphrase: "secret",
    });
  });

  it("pendingTransfersList → pending_transfers_list", async () => {
    await pendingTransfersList();
    expect(invoke).toHaveBeenCalledWith("pending_transfers_list");
  });

  it("noteReceiveAccept → note_receive_accept with passphrase", async () => {
    await noteReceiveAccept("t1", "secret");
    expect(invoke).toHaveBeenCalledWith("note_receive_accept", {
      transferId: "t1",
      passphrase: "secret",
    });
  });

  it("noteReceiveReject → note_receive_reject", async () => {
    await noteReceiveReject("t1");
    expect(invoke).toHaveBeenCalledWith("note_receive_reject", {
      transferId: "t1",
    });
  });

  it("generatePairingCode → generate_pairing_code", async () => {
    await generatePairingCode();
    expect(invoke).toHaveBeenCalledWith("generate_pairing_code");
  });

  it("knownPeersList → known_peers_list", async () => {
    await knownPeersList();
    expect(invoke).toHaveBeenCalledWith("known_peers_list");
  });

  it("notesSend → notes_send", async () => {
    await notesSend(["n1", "n2"], "peer1", "CODE");
    expect(invoke).toHaveBeenCalledWith("notes_send", {
      noteIds: ["n1", "n2"],
      peerId: "peer1",
      passphrase: "CODE",
    });
  });

  it("pendingOffersList → pending_offers_list", async () => {
    await pendingOffersList();
    expect(invoke).toHaveBeenCalledWith("pending_offers_list");
  });

  it("transferOfferRespond → transfer_offer_respond", async () => {
    await transferOfferRespond("offer-1", "K4X7P2");
    expect(invoke).toHaveBeenCalledWith("transfer_offer_respond", {
      offerId: "offer-1",
      passphrase: "K4X7P2",
    });
  });

  it("startReceiving → start_receiving", async () => {
    await startReceiving();
    expect(invoke).toHaveBeenCalledWith("start_receiving");
  });

  it("stopReceiving → stop_receiving", async () => {
    await stopReceiving();
    expect(invoke).toHaveBeenCalledWith("stop_receiving");
  });

  it("isReceiving → is_receiving", async () => {
    await isReceiving();
    expect(invoke).toHaveBeenCalledWith("is_receiving");
  });
});
