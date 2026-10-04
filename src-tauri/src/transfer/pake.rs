//! SPAKE2 password-authenticated key exchange for E2E note transfer.
//!
//! The short pairing code is the SPAKE2 password. Both devices derive the same
//! 32-byte session key iff they used the same code; a network attacker without
//! the code gains no offline-guessing advantage (each guess needs a live round)
//! and never sees the code on the wire. SPAKE2 alone yields a key even when the
//! passwords differ, so we add explicit HMAC key-confirmation: a wrong code
//! makes the confirmation MACs mismatch and the transfer aborts.

use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};

type HmacSha256 = Hmac<Sha256>;

const IDENTITY: &[u8] = b"panote-transfer-v1";
const CONFIRM_TAG: &[u8] = b"panote-key-confirm";

/// Normalize a pairing code for use as the SPAKE2 password (case/format-insensitive).
pub fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase()
}

/// Start a symmetric SPAKE2 exchange. Returns (state, outbound message to send).
pub fn start(code: &str) -> (Spake2<Ed25519Group>, Vec<u8>) {
    Spake2::<Ed25519Group>::start_symmetric(
        &Password::new(normalize(code).as_bytes()),
        &Identity::new(IDENTITY),
    )
}

/// Complete the exchange with the peer's message → raw SPAKE2 key.
pub fn finish(state: Spake2<Ed25519Group>, peer_msg: &[u8]) -> anyhow::Result<Vec<u8>> {
    state
        .finish(peer_msg)
        .map_err(|e| anyhow::anyhow!("spake2 finish failed: {e:?}"))
}

/// Derived handshake keys: the ChaCha20 session key plus a confirmation key for
/// each direction. The initiator's and responder's messages are bound into the
/// KDF so the full transcript is authenticated.
pub struct SessionKeys {
    pub session: [u8; 32],
    pub confirm_initiator: [u8; 32],
    pub confirm_responder: [u8; 32],
}

/// Expand the raw SPAKE2 key into session + confirmation keys. `initiator_msg`
/// and `responder_msg` must be ordered identically on both peers.
pub fn derive_keys(spake_key: &[u8], initiator_msg: &[u8], responder_msg: &[u8]) -> SessionKeys {
    let mut info = Vec::with_capacity(initiator_msg.len() + responder_msg.len());
    info.extend_from_slice(initiator_msg);
    info.extend_from_slice(responder_msg);
    let hk = Hkdf::<Sha256>::new(Some(&info), spake_key);
    let mut keys = SessionKeys {
        session: [0u8; 32],
        confirm_initiator: [0u8; 32],
        confirm_responder: [0u8; 32],
    };
    hk.expand(b"session", &mut keys.session).unwrap();
    hk.expand(b"confirm-initiator", &mut keys.confirm_initiator)
        .unwrap();
    hk.expand(b"confirm-responder", &mut keys.confirm_responder)
        .unwrap();
    keys
}

/// Produce the key-confirmation MAC for a given confirmation key.
pub fn confirm_mac(confirm_key: &[u8; 32]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(confirm_key).expect("hmac key");
    mac.update(CONFIRM_TAG);
    mac.finalize().into_bytes().to_vec()
}

/// Constant-time verify a peer's key-confirmation MAC.
pub fn verify_mac(confirm_key: &[u8; 32], tag: &[u8]) -> bool {
    let mut mac = HmacSha256::new_from_slice(confirm_key).expect("hmac key");
    mac.update(CONFIRM_TAG);
    mac.verify_slice(tag).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a full in-process handshake for a pair of codes; return the two
    /// session keys plus whether both key-confirmations verified.
    fn handshake(code_a: &str, code_b: &str) -> Option<([u8; 32], [u8; 32])> {
        let (state_i, msg_i) = start(code_a);
        let (state_r, msg_r) = start(code_b);
        let key_i = finish(state_i, &msg_r).ok()?;
        let key_r = finish(state_r, &msg_i).ok()?;
        let ki = derive_keys(&key_i, &msg_i, &msg_r);
        let kr = derive_keys(&key_r, &msg_i, &msg_r);
        // Initiator confirms with confirm_initiator; responder verifies it, etc.
        if !verify_mac(&kr.confirm_initiator, &confirm_mac(&ki.confirm_initiator)) {
            return None;
        }
        if !verify_mac(&ki.confirm_responder, &confirm_mac(&kr.confirm_responder)) {
            return None;
        }
        Some((ki.session, kr.session))
    }

    #[test]
    fn matching_codes_yield_equal_session_key() {
        let (a, b) = handshake("ABCD-1234", "abcd1234").expect("confirm ok");
        assert_eq!(a, b, "same code (normalized) → same session key");
    }

    #[test]
    fn mismatched_codes_fail_confirmation() {
        // Wrong code: SPAKE2 still finishes, but key confirmation must reject.
        assert!(handshake("ABCD-1234", "WXYZ-9999").is_none());
    }

    #[test]
    fn session_key_is_not_all_zero() {
        let (a, _) = handshake("SECRET", "SECRET").unwrap();
        assert_ne!(a, [0u8; 32]);
    }
}
