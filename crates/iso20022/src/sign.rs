//! The signature on a message between an integrante and its red.
//!
//! Independent project. Not affiliated with, nor endorsed by, the Banco Central del Ecuador.

use std::collections::HashMap;
use std::fmt;

use ed25519_dalek::{Signature, Signer as _, SigningKey, Verifier as _, VerifyingKey};
use veinte_types::Bic;

use crate::{BusinessMessage, DecodeError, decode, encode};

/// [assumption] Catálogo Operacional §5, Tabla 11, says an order is accepted only after the
/// signature check, and does not name an algorithm. This profile uses Ed25519 over the sender
/// BIC, a zero byte, the receiver BIC, a zero byte, and the encoded message. The crate builds
/// no key and stores none.
fn canonical(from: Bic, to: Bic, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(from.as_str().len() + to.as_str().len() + body.len() + 2);
    out.extend_from_slice(from.as_str().as_bytes());
    out.push(0);
    out.extend_from_slice(to.as_str().as_bytes());
    out.push(0);
    out.extend_from_slice(body);
    out
}

#[derive(Clone)]
pub struct Signer {
    bic: Bic,
    key: SigningKey,
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer({})", self.bic)
    }
}

#[derive(Debug)]
pub struct EntropyError;

impl fmt::Display for EntropyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the operating system did not provide entropy")
    }
}

impl std::error::Error for EntropyError {}

impl Signer {
    pub fn from_bytes(bic: Bic, bytes: [u8; 32]) -> Self {
        Self { bic, key: SigningKey::from_bytes(&bytes) }
    }

    pub fn generate(bic: Bic) -> Result<Self, EntropyError> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes).map_err(|_| EntropyError)?;
        Ok(Self::from_bytes(bic, bytes))
    }

    pub fn bic(&self) -> Bic {
        self.bic
    }

    pub fn verifying_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    pub fn seal(&self, to: Bic, message: &BusinessMessage) -> Envelope {
        let body = encode(message);
        let signature = self.key.sign(&canonical(self.bic, to, &body)).to_bytes();
        Envelope { from: self.bic, to, body, signature }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: Bic,
    pub to: Bic,
    pub body: Vec<u8>,
    signature: [u8; 64],
}

impl fmt::Debug for Envelope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Envelope({} -> {}, {} bytes)", self.from, self.to, self.body.len())
    }
}

/// [assumption] The catálogo requires a signature and does not publish a framing. The bytes are
/// the sender length, the sender BIC, the receiver length, the receiver BIC, the 64 signature
/// bytes, and the encoded message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvelopeError {
    Truncated,
    Party,
}

impl fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Truncated => "truncated envelope",
            Self::Party => "the envelope names a party that is not a BIC",
        })
    }
}

impl std::error::Error for EnvelopeError {}

impl Envelope {
    pub fn from_parts(from: Bic, to: Bic, body: Vec<u8>, signature: [u8; 64]) -> Self {
        Self { from, to, body, signature }
    }

    pub fn signature(&self) -> &[u8; 64] {
        &self.signature
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        push_party(&mut out, self.from);
        push_party(&mut out, self.to);
        out.extend_from_slice(&self.signature);
        out.extend_from_slice(&self.body);
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, EnvelopeError> {
        let mut at = 0;
        let from = take_party(bytes, &mut at)?;
        let to = take_party(bytes, &mut at)?;
        let sig_end = at.checked_add(64).ok_or(EnvelopeError::Truncated)?;
        let mut signature = [0u8; 64];
        signature.copy_from_slice(bytes.get(at..sig_end).ok_or(EnvelopeError::Truncated)?);
        at = sig_end;
        Ok(Self { from, to, signature, body: bytes[at..].to_vec() })
    }
}

fn push_party(out: &mut Vec<u8>, bic: Bic) {
    let text = bic.as_str().as_bytes();
    out.push(u8::try_from(text.len()).expect("a BIC fits in one length byte"));
    out.extend_from_slice(text);
}

fn take_party(bytes: &[u8], at: &mut usize) -> Result<Bic, EnvelopeError> {
    let len = *bytes.get(*at).ok_or(EnvelopeError::Truncated)? as usize;
    *at += 1;
    let end = at.checked_add(len).ok_or(EnvelopeError::Truncated)?;
    if end > bytes.len() {
        return Err(EnvelopeError::Truncated);
    }
    let text = std::str::from_utf8(&bytes[*at..end]).map_err(|_| EnvelopeError::Party)?;
    *at = end;
    text.parse().map_err(|_| EnvelopeError::Party)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejected {
    UnknownSender,
    BadSignature,
    /// [assumption] The signed sender and receiver are the header BICs. Tabla 15 requires those
    /// BICs and Tabla 11 requires a signature; the catálogo does not say the two agree.
    Unbound,
}

impl fmt::Display for Rejected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnknownSender => "unknown sender",
            Self::BadSignature => "bad signature",
            Self::Unbound => "the signature does not match the header",
        })
    }
}

impl std::error::Error for Rejected {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustError {
    MalformedKey,
}

impl fmt::Display for TrustError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the key is not an Ed25519 point")
    }
}

impl std::error::Error for TrustError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    Rejected(Rejected),
    Message(DecodeError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(err) => write!(f, "{err}"),
            Self::Message(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for OpenError {}

#[derive(Debug, Clone, Default)]
pub struct Keyring {
    keys: HashMap<Bic, VerifyingKey>,
}

impl Keyring {
    pub fn trust(&mut self, bic: Bic, verifying_key: [u8; 32]) -> Result<(), TrustError> {
        let key = VerifyingKey::from_bytes(&verifying_key).map_err(|_| TrustError::MalformedKey)?;
        self.keys.insert(bic, key);
        Ok(())
    }

    pub fn open<'a>(&self, envelope: &'a Envelope) -> Result<&'a [u8], Rejected> {
        let key = self.keys.get(&envelope.from).ok_or(Rejected::UnknownSender)?;
        let signature = Signature::from_bytes(&envelope.signature);
        key.verify(&canonical(envelope.from, envelope.to, &envelope.body), &signature)
            .map_err(|_| Rejected::BadSignature)?;
        Ok(&envelope.body)
    }
}

pub fn open_message(keyring: &Keyring, envelope: &Envelope) -> Result<BusinessMessage, OpenError> {
    let body = keyring.open(envelope).map_err(OpenError::Rejected)?;
    let message = decode(body).map_err(OpenError::Message)?;
    if message.header.from != envelope.from || message.header.to != envelope.to {
        return Err(OpenError::Rejected(Rejected::Unbound));
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::sample;

    #[test]
    fn a_sealed_message_opens_and_a_changed_byte_does_not() {
        let (message, _) = sample::order();
        let signer = Signer::generate(message.header.from).unwrap();
        let mut keyring = Keyring::default();
        let envelope = signer.seal(message.header.to, &message);
        assert_eq!(keyring.open(&envelope), Err(Rejected::UnknownSender));
        keyring.trust(signer.bic(), signer.verifying_key()).unwrap();
        assert_eq!(open_message(&keyring, &envelope).unwrap(), message);
        let mut mislabeled = message.clone();
        mislabeled.header.to = "CBETECEQ".parse().unwrap();
        let crossed = signer.seal(message.header.to, &mislabeled);
        assert_eq!(open_message(&keyring, &crossed), Err(OpenError::Rejected(Rejected::Unbound)));
        let mut body = envelope.body.clone();
        body[40] ^= 0x01;
        let mut signature = *envelope.signature();
        signature[0] ^= 0x01;
        let tampered = Envelope::from_parts(envelope.from, envelope.to, body, signature);
        assert_eq!(keyring.open(&tampered), Err(Rejected::BadSignature));
        let mut redirected = envelope;
        redirected.to = "CBETECEQ".parse().unwrap();
        assert_eq!(keyring.open(&redirected), Err(Rejected::BadSignature));
    }

    #[test]
    fn a_sealed_envelope_survives_the_wire() {
        let (message, _) = sample::order();
        let signer = Signer::generate(message.header.from).unwrap();
        let sealed = signer.seal(message.header.to, &message);
        let parsed = Envelope::parse(&sealed.to_bytes()).unwrap();
        assert_eq!(parsed.signature(), sealed.signature());
        assert_eq!(parsed, sealed);
        let mut keyring = Keyring::default();
        keyring.trust(signer.bic(), signer.verifying_key()).unwrap();
        assert_eq!(open_message(&keyring, &parsed).unwrap(), message);
    }

    #[test]
    fn a_malformed_key_is_its_own_error() {
        let mut keyring = Keyring::default();
        let bic = "BALFECEQ".parse().unwrap();
        assert_eq!(keyring.trust(bic, [2; 32]), Err(TrustError::MalformedKey));
    }
}
