//! Proving that a guest got the link, and the code both sides compare.
//!
//! - The **join proof** is `HMAC-SHA256(joinSecret, hostId ‖ guestId ‖ sessionId)`. The guest
//!   id is the guest's endpoint key, authenticated by the transport, so a proof cannot be
//!   replayed from another endpoint. The host drops connections with a wrong proof before
//!   anyone is asked.
//! - The **verification code** is `SHA-256(hostId ‖ guestId ‖ sessionId)` as six digits
//!   (`482 913`). It binds both authenticated keys: someone who stole the link and joins in
//!   parallel shows a different code than the real guest.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::link::{HOST_ID_LEN, JOIN_SECRET_LEN, SESSION_ID_LEN};

/// An endpoint key (host or guest).
pub type EndpointKey = [u8; HOST_ID_LEN];

fn mac(secret: &[u8; JOIN_SECRET_LEN]) -> Hmac<Sha256> {
    Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length")
}

/// The proof a guest sends in its knock.
pub fn join_proof(
    secret: &[u8; JOIN_SECRET_LEN],
    host: &EndpointKey,
    guest: &EndpointKey,
    session: &[u8; SESSION_ID_LEN],
) -> [u8; 32] {
    let mut m = mac(secret);
    m.update(host);
    m.update(guest);
    m.update(session);
    m.finalize().into_bytes().into()
}

/// Check a guest's proof in constant time.
pub fn check_proof(
    secret: &[u8; JOIN_SECRET_LEN],
    host: &EndpointKey,
    guest: &EndpointKey,
    session: &[u8; SESSION_ID_LEN],
    proof: &[u8],
) -> bool {
    let mut m = mac(secret);
    m.update(host);
    m.update(guest);
    m.update(session);
    m.verify_slice(proof).is_ok()
}

/// The six-digit verification code, grouped as `482 913`.
pub fn verification_code(
    host: &EndpointKey,
    guest: &EndpointKey,
    session: &[u8; SESSION_ID_LEN],
) -> String {
    let digest = Sha256::new()
        .chain_update(host)
        .chain_update(guest)
        .chain_update(session)
        .finalize();
    let n = u64::from_be_bytes(digest[..8].try_into().expect("8 bytes")) % 1_000_000;
    format!("{:03} {:03}", n / 1000, n % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: [u8; 32] = [3; 32];
    const HOST: EndpointKey = [1; 32];
    const GUEST: EndpointKey = [2; 32];
    const SESSION: [u8; 16] = [4; 16];

    #[test]
    fn proof_checks_every_input() {
        let proof = join_proof(&SECRET, &HOST, &GUEST, &SESSION);
        assert!(check_proof(&SECRET, &HOST, &GUEST, &SESSION, &proof));
        assert!(!check_proof(&[0; 32], &HOST, &GUEST, &SESSION, &proof));
        assert!(!check_proof(&SECRET, &GUEST, &HOST, &SESSION, &proof));
        assert!(!check_proof(&SECRET, &HOST, &[5; 32], &SESSION, &proof));
        assert!(!check_proof(&SECRET, &HOST, &GUEST, &[0; 16], &proof));
        assert!(!check_proof(&SECRET, &HOST, &GUEST, &SESSION, &proof[..31]));
        assert!(!check_proof(&SECRET, &HOST, &GUEST, &SESSION, &[]));
    }

    #[test]
    fn code_is_six_grouped_digits_bound_to_both_keys() {
        let code = verification_code(&HOST, &GUEST, &SESSION);
        assert_eq!(code.len(), 7);
        assert!(code.chars().enumerate().all(|(i, c)| if i == 3 {
            c == ' '
        } else {
            c.is_ascii_digit()
        }));
        assert_eq!(code, verification_code(&HOST, &GUEST, &SESSION));
        assert_ne!(code, verification_code(&HOST, &[5; 32], &SESSION));
    }
}
