//! Narrow trust policy for the existing self-signed origin certificate.
//! Its CA=true extension makes ordinary WebPKI end-entity validation unsuitable.
//! Accept only the recorded DER digest, hostname and certificate validity window;
//! still cryptographically verify the TLS 1.2/1.3 handshake signatures with ring.
use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
    CertificateError, DigitallySignedStruct, Error, SignatureScheme,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const NOT_BEFORE: u64 = 1791642254; // 2026-10-10T14:24:14Z; exact pinned certificate
pub const NOT_AFTER: u64 = 2107002254; // 2036-10-07T14:24:14Z
#[derive(Debug)]
struct OriginVerifier;
impl ServerCertVerifier for OriginVerifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        _: &[u8],
        now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, Error> {
        if !matches!(name,ServerName::DnsName(n) if n.as_ref()==crate::account::HOST) {
            return Err(Error::InvalidCertificate(CertificateError::NotValidForName));
        }
        if format!("{:x}", Sha256::digest(cert.as_ref())) != crate::account::ROOT_SHA256 {
            return Err(Error::InvalidCertificate(CertificateError::UnknownIssuer));
        }
        if now.as_secs() < NOT_BEFORE {
            return Err(Error::InvalidCertificate(CertificateError::NotValidYet));
        }
        if now.as_secs() > NOT_AFTER {
            return Err(Error::InvalidCertificate(CertificateError::Expired));
        }
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
pub fn config() -> rustls::ClientConfig {
    rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_safe_default_protocol_versions()
        .expect("TLS 1.2/1.3")
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(OriginVerifier))
        .with_no_client_auth()
}
#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    #[test]
    fn exact_pin_host_and_validity_are_all_required() {
        let b64: String = include_str!("../account-root.pem")
            .lines()
            .filter(|l| !l.starts_with("---"))
            .collect();
        let der = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .unwrap();
        let cert = CertificateDer::from(der);
        let name = ServerName::try_from(crate::account::HOST).unwrap();
        let t = UnixTime::since_unix_epoch(std::time::Duration::from_secs(NOT_BEFORE + 100));
        assert!(OriginVerifier
            .verify_server_cert(&cert, &[], &name, &[], t)
            .is_ok());
        assert!(OriginVerifier
            .verify_server_cert(&CertificateDer::from(vec![0, 1]), &[], &name, &[], t)
            .is_err());
        assert!(OriginVerifier
            .verify_server_cert(
                &cert,
                &[],
                &ServerName::try_from("example.com").unwrap(),
                &[],
                t
            )
            .is_err());
        for seconds in [NOT_BEFORE - 1, NOT_AFTER + 1] {
            assert!(OriginVerifier
                .verify_server_cert(
                    &cert,
                    &[],
                    &name,
                    &[],
                    UnixTime::since_unix_epoch(std::time::Duration::from_secs(seconds))
                )
                .is_err());
        }
    }
}
