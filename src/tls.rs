//! TLS for PostgreSQL connections, with rustls and the ring provider.

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use tokio_postgres_rustls::MakeRustlsConnect;

use crate::db::Tls;

pub fn connector(mode: Tls, ca_file: &str) -> Result<MakeRustlsConnect, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?;

    let config = match mode {
        Tls::Verify => builder
            .with_root_certificates(roots(ca_file)?)
            .with_no_client_auth(),
        // Like libpq, `Prefer` and `Require` encrypt but do not check the server certificate.
        _ => builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Unchecked(provider)))
            .with_no_client_auth(),
    };
    Ok(MakeRustlsConnect::new(config))
}

/// The trusted authorities of the system, and the ones in the PEM file.
fn roots(ca_file: &str) -> Result<rustls::RootCertStore, String> {
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        // Skip a bad certificate of the system and use the others.
        let _ = roots.add(cert);
    }
    let ca_file = ca_file.trim();
    if !ca_file.is_empty() {
        let mut added = 0;
        let certs = CertificateDer::pem_file_iter(ca_file)
            .map_err(|e| format!("The CA certificate file did not open: {e}"))?;
        for cert in certs {
            let cert =
                cert.map_err(|e| format!("The CA certificate file is not valid PEM: {e}"))?;
            roots
                .add(cert)
                .map_err(|e| format!("The CA certificate file has a bad certificate: {e}"))?;
            added += 1;
        }
        if added == 0 {
            return Err("The CA certificate file holds no certificates.".into());
        }
    }
    if roots.is_empty() {
        return Err("This computer has no trusted certificates. Add a CA certificate file.".into());
    }
    Ok(roots)
}

/// Accepts all certificates, but checks the handshake signatures.
#[derive(Debug)]
struct Unchecked(Arc<CryptoProvider>);

impl ServerCertVerifier for Unchecked {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
