use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello {
        device_name: String,
    },
    /// Sender opens a transfer and starts the SPAKE2 handshake (initiator msg).
    TransferOffer {
        from_peer: String,
        offer_id: String,
        note_count: u32,
        /// SPAKE2 initiator message (E2E). Empty from legacy senders.
        #[serde(default)]
        pake_msg: Vec<u8>,
    },
    /// Recipient replies with its SPAKE2 message and key-confirmation MAC.
    /// No pairing code is ever sent on the wire.
    TransferAccept {
        offer_id: String,
        pake_msg: Vec<u8>,
        confirm: Vec<u8>,
    },
    /// Sender's key-confirmation MAC — mutual auth before any note is sent.
    PakeConfirm {
        confirm: Vec<u8>,
    },
    /// One note encrypted under the PAKE-derived session key (E2E flow).
    SessionNote {
        nonce: Vec<u8>,
        ct: Vec<u8>,
    },
    /// Legacy single-note transfer: blob encrypted with a passphrase-derived key.
    /// Kept for the manual "pending transfer" path; superseded by the PAKE flow.
    SendNote {
        from_peer: String,
        transfer_salt: Vec<u8>,
        transfer_nonce: Vec<u8>,
        transfer_ct: Vec<u8>,
    },
    Ack {
        transfer_id: String,
    },
    Reject {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(msg: &Message) -> Message {
        let bytes = serde_json::to_vec(msg).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn hello_roundtrip() {
        let msg = Message::Hello {
            device_name: "Alice".into(),
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(decoded, Message::Hello { device_name } if device_name == "Alice"));
    }

    #[test]
    fn send_note_roundtrip() {
        let msg = Message::SendNote {
            from_peer: "bob.local".into(),
            transfer_salt: vec![1, 2, 3],
            transfer_nonce: vec![4, 5, 6],
            transfer_ct: vec![7, 8, 9],
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(decoded, Message::SendNote { from_peer, .. } if from_peer == "bob.local"));
    }

    #[test]
    fn ack_roundtrip() {
        let msg = Message::Ack {
            transfer_id: "tid-123".into(),
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(decoded, Message::Ack { transfer_id } if transfer_id == "tid-123"));
    }

    #[test]
    fn reject_roundtrip() {
        let msg = Message::Reject {
            reason: "bad passphrase".into(),
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(decoded, Message::Reject { reason } if reason == "bad passphrase"));
    }

    #[test]
    fn transfer_offer_roundtrip() {
        let msg = Message::TransferOffer {
            from_peer: "alice".into(),
            offer_id: "offer-1".into(),
            note_count: 3,
            pake_msg: vec![9, 9, 9],
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(
            decoded,
            Message::TransferOffer { note_count: 3, .. }
        ));
    }

    #[test]
    fn transfer_offer_legacy_no_pake_msg_defaults_empty() {
        let bad = r#"{"type":"transfer_offer","from_peer":"a","offer_id":"o","note_count":1}"#;
        let decoded: Message = serde_json::from_str(bad).unwrap();
        assert!(matches!(decoded, Message::TransferOffer { pake_msg, .. } if pake_msg.is_empty()));
    }

    #[test]
    fn transfer_accept_roundtrip() {
        let msg = Message::TransferAccept {
            offer_id: "offer-1".into(),
            pake_msg: vec![1, 2],
            confirm: vec![3, 4],
        };
        let decoded = roundtrip(&msg);
        assert!(
            matches!(decoded, Message::TransferAccept { confirm, .. } if confirm == vec![3, 4])
        );
    }

    #[test]
    fn session_note_roundtrip() {
        let msg = Message::SessionNote {
            nonce: vec![1],
            ct: vec![2, 3],
        };
        let decoded = roundtrip(&msg);
        assert!(matches!(decoded, Message::SessionNote { ct, .. } if ct == vec![2, 3]));
    }

    #[test]
    fn unknown_type_deserialize_fails() {
        let bad = r#"{"type":"unknown_type","foo":"bar"}"#;
        assert!(serde_json::from_str::<Message>(bad).is_err());
    }
}
