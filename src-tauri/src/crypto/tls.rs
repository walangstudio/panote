use rcgen::{generate_simple_self_signed, CertifiedKey};
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    DigitallySignedStruct, Error as TlsError, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

/// Generate a new self-signed certificate for this device.
/// Returns `(cert_der, key_der_pkcs8)`.
pub fn generate_self_signed() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let CertifiedKey { cert, key_pair } =
        generate_simple_self_signed(vec!["panote.local".to_string()])?;
    Ok((cert.der().to_vec(), key_pair.serialize_der()))
}

/// SHA-256 fingerprint of a certificate's DER bytes.
pub fn cert_fingerprint(cert_der: &[u8]) -> [u8; 32] {
    Sha256::digest(cert_der).into()
}

/// Hex string representation of a fingerprint (for display / storage).
#[allow(dead_code)]
pub fn fingerprint_hex(fp: &[u8; 32]) -> String {
    fp.iter().map(|b| format!("{b:02x}")).collect()
}

/// Build a `rustls::ServerConfig` from DER-encoded cert + key.
pub fn server_config(
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
    provider: Arc<CryptoProvider>,
) -> anyhow::Result<rustls::ServerConfig> {
    let cert = CertificateDer::from(cert_der);
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_der));
    Ok(rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)?)
}

/// Build a `rustls::ClientConfig` using a `TofuVerifier`.
pub fn client_config(
    verifier: Arc<TofuVerifier>,
    provider: Arc<CryptoProvider>,
) -> anyhow::Result<rustls::ClientConfig> {
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth())
}

// ---- TOFU verifier ----

/// In-memory TOFU store: maps peer hostname → cert fingerprint.
/// On first connection: records fingerprint and accepts.
/// On subsequent connections: accepts only if fingerprint matches.
///
/// TODO: persist to SQLite so TOFU state survives restarts.
pub struct TofuVerifier {
    store: Mutex<HashMap<String, [u8; 32]>>,
    provider: Arc<CryptoProvider>,
}

impl TofuVerifier {
    pub fn new(provider: Arc<CryptoProvider>) -> Self {
        Self {
            store: Mutex::new(HashMap::new()),
            provider,
        }
    }

    /// Pre-load a known peer fingerprint (e.g., from DB on startup).
    #[allow(dead_code)]
    pub fn preload(&self, hostname: &str, fingerprint: [u8; 32]) {
        self.store.lock().unwrap().insert(hostname.to_string(), fingerprint);
    }

    /// Check if a fingerprint matches the stored one for `hostname`.
    /// Returns `true` if accepted (new peer or fingerprint matches).
    /// Returns `false` if the fingerprint changed (potential MITM).
    pub fn verify_and_record(&self, hostname: &str, cert_der: &[u8]) -> bool {
        let fp = cert_fingerprint(cert_der);
        let mut store = self.store.lock().unwrap();
        match store.get(hostname) {
            None => {
                // First time seeing this peer — accept and record (TOFU)
                store.insert(hostname.to_string(), fp);
                true
            }
            Some(&known) => known == fp,
        }
    }

    /// Return all stored (hostname, fingerprint_hex) pairs.
    #[allow(dead_code)]
    pub fn known_peers(&self) -> Vec<(String, String)> {
        self.store
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), fingerprint_hex(v)))
            .collect()
    }
}

impl std::fmt::Debug for TofuVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TofuVerifier").finish()
    }
}

impl ServerCertVerifier for TofuVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let hostname = server_name.to_str().to_string();
        if self.verify_and_record(&hostname, end_entity.as_ref()) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(TlsError::General(format!(
                "TOFU fingerprint mismatch for {hostname} — possible MITM"
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_self_signed_produces_nonempty_der() {
        let (cert_der, key_der) = generate_self_signed().unwrap();
        assert!(!cert_der.is_empty());
        assert!(!key_der.is_empty());
    }

    #[test]
    fn cert_fingerprint_is_32_bytes() {
        let (cert_der, _) = generate_self_signed().unwrap();
        let fp = cert_fingerprint(&cert_der);
        assert_eq!(fp.len(), 32);
    }

    #[test]
    fn cert_fingerprint_is_deterministic() {
        let (cert_der, _) = generate_self_signed().unwrap();
        assert_eq!(cert_fingerprint(&cert_der), cert_fingerprint(&cert_der));
    }

    #[test]
    fn different_certs_have_different_fingerprints() {
        let (cert1, _) = generate_self_signed().unwrap();
        let (cert2, _) = generate_self_signed().unwrap();
        assert_ne!(cert_fingerprint(&cert1), cert_fingerprint(&cert2));
    }

    #[test]
    fn fingerprint_hex_is_64_chars() {
        let fp = [0xabu8; 32];
        let hex = fingerprint_hex(&fp);
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn fingerprint_hex_known_value() {
        let fp = [0u8; 32];
        let hex = fingerprint_hex(&fp);
        assert_eq!(hex, "0".repeat(64));
    }

    #[test]
    fn tofu_first_connection_accepted() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert_der, _) = generate_self_signed().unwrap();

        assert!(verifier.verify_and_record("alice.local", &cert_der));
    }

    #[test]
    fn tofu_same_cert_accepted_on_reconnect() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert_der, _) = generate_self_signed().unwrap();

        assert!(verifier.verify_and_record("alice.local", &cert_der));
        assert!(verifier.verify_and_record("alice.local", &cert_der)); // same cert
    }

    #[test]
    fn tofu_changed_cert_rejected() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert1, _) = generate_self_signed().unwrap();
        let (cert2, _) = generate_self_signed().unwrap();

        assert!(verifier.verify_and_record("alice.local", &cert1));
        assert!(!verifier.verify_and_record("alice.local", &cert2)); // different cert = reject
    }

    #[test]
    fn tofu_different_peers_independent() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert1, _) = generate_self_signed().unwrap();
        let (cert2, _) = generate_self_signed().unwrap();

        assert!(verifier.verify_and_record("alice.local", &cert1));
        assert!(verifier.verify_and_record("bob.local", &cert2));
        // Swapped: alice presenting bob's cert should fail
        assert!(!verifier.verify_and_record("alice.local", &cert2));
    }

    #[test]
    fn tofu_preload_then_verify() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert_der, _) = generate_self_signed().unwrap();
        let fp = cert_fingerprint(&cert_der);

        verifier.preload("charlie.local", fp);
        assert!(verifier.verify_and_record("charlie.local", &cert_der));
    }

    #[test]
    fn tofu_preload_wrong_cert_rejected() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert1, _) = generate_self_signed().unwrap();
        let (cert2, _) = generate_self_signed().unwrap();
        let fp = cert_fingerprint(&cert1);

        verifier.preload("charlie.local", fp);
        assert!(!verifier.verify_and_record("charlie.local", &cert2)); // cert2 ≠ cert1
    }

    #[test]
    fn known_peers_lists_recorded_entries() {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let verifier = TofuVerifier::new(provider);
        let (cert, _) = generate_self_signed().unwrap();

        verifier.verify_and_record("peer-a.local", &cert);
        let peers = verifier.known_peers();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].0, "peer-a.local");
        assert_eq!(peers[0].1.len(), 64); // hex fingerprint
    }
}
