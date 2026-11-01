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

pub struct Signer {
    bic: Bic,
    key: SigningKey,
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer({})", self.bic)
    }
}

impl Signer {
    pub fn from_bytes(bic: Bic, bytes: [u8; 32]) -> Self {
        Self { bic, key: SigningKey::from_bytes(&bytes) }
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
    pub fn trust(&mut self, bic: Bic, verifying_key: [u8; 32]) -> Result<(), Rejected> {
        let key = VerifyingKey::from_bytes(&verifying_key).map_err(|_| Rejected::BadSignature)?;
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
    use std::io::Read;

    fn entropy() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom").unwrap().read_exact(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn a_sealed_message_opens_and_a_changed_byte_does_not() {
        let (message, _) = sample::order();
        let signer = Signer::from_bytes(message.header.from, entropy());
        let mut keyring = Keyring::default();
        let envelope = signer.seal(message.header.to, &message);
        assert_eq!(keyring.open(&envelope), Err(Rejected::UnknownSender));
        keyring.trust(signer.bic(), signer.verifying_key()).unwrap();
        assert_eq!(open_message(&keyring, &envelope).unwrap(), message);
        let mut mislabeled = message.clone();
        mislabeled.header.to = "CBETECEQ".parse().unwrap();
        let crossed = signer.seal(message.header.to, &mislabeled);
        assert_eq!(open_message(&keyring, &crossed), Err(OpenError::Rejected(Rejected::Unbound)));
        let mut tampered = envelope.clone();
        tampered.body[40] ^= 0x01;
        assert_eq!(keyring.open(&tampered), Err(Rejected::BadSignature));
        let mut redirected = envelope;
        redirected.to = "CBETECEQ".parse().unwrap();
        assert_eq!(keyring.open(&redirected), Err(Rejected::BadSignature));
    }
}
