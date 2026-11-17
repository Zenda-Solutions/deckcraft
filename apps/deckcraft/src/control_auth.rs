//! Authentication for the control channel, the same handshake as PhotoCraft's (NV-6):
//!
//! - challenge: `auth.challenge {clientNonce}` (64 hex) is answered with `{serverNonce, serverProof}`, then
//!   `auth {proof}` authenticates. Each proof is HMAC-SHA256 keyed by the token over
//!   `"deckcraft-control-v1 server|client" \n clientNonce \n serverNonce`, so the token never crosses the
//!   wire and the client learns the server holds it before revealing anything; or
//! - one frame: `auth {token}`, what Work's client sends today.
//!
//! The token comes from `DECKCRAFT_CONTROL_TOKEN` (64 hexadecimal characters). Without one the stock
//! build keeps upstream's open channel; Zenda's build does not open the channel at all, so it never
//! listens unauthenticated (Theo's chat keys travel over it).

use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

pub const TOKEN_VAR: &str = "DECKCRAFT_CONTROL_TOKEN";
pub const AUTH: &str = "auth";
pub const CHALLENGE: &str = "auth.challenge";

/// Exactly 64 hexadecimal characters.
pub fn is_hex256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The token in the environment, if it has the right shape.
pub fn token_from_env() -> Option<String> {
    std::env::var(TOKEN_VAR).ok().filter(|t| is_hex256(t)).map(|t| t.to_ascii_lowercase())
}

/// Equal without an early exit, hex letters in either case.
pub fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x.to_ascii_lowercase() ^ y.to_ascii_lowercase())) == 0
}

#[derive(Clone, Copy)]
pub enum Role {
    Server,
    Client,
}

pub fn proof(token: &str, role: Role, client_nonce: &str, server_nonce: &str) -> String {
    let label: &[u8] = match role {
        Role::Server => b"deckcraft-control-v1 server",
        Role::Client => b"deckcraft-control-v1 client",
    };
    // HMAC takes a key of any length; an empty proof never matches, so a failure only refuses.
    let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(token.to_ascii_lowercase().as_bytes()) else { return String::new() };
    for part in [label, b"\n", client_nonce.to_ascii_lowercase().as_bytes(), b"\n", server_nonce.to_ascii_lowercase().as_bytes()] {
        mac.update(part);
    }
    mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

fn nonce() -> Option<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).ok()?;
    Some(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Authenticated,
    Challenged,
    /// Send the reply, then close the connection.
    Rejected,
}

/// One per connection, until it authenticates. Never dispatches a control method.
#[derive(Default)]
pub struct Handshake {
    challenge: Option<(String, String)>,
}

impl Handshake {
    pub fn reply(&mut self, id: Value, method: &str, params: &Value, token: &str) -> (Value, Step) {
        let refused = |id: Value| (json!({"id": id, "ok": false, "error": "authentication required"}), Step::Rejected);
        let param = |k: &str| params.get(k).and_then(Value::as_str);
        match (method, self.challenge.take()) {
            (CHALLENGE, None) => {
                let Some(client) = param("clientNonce").filter(|n| is_hex256(n)).map(str::to_ascii_lowercase) else { return refused(id) };
                let Some(server) = nonce() else { return refused(id) };
                let server_proof = proof(token, Role::Server, &client, &server);
                self.challenge = Some((client, server.clone()));
                (json!({"id": id, "ok": true, "result": {"serverNonce": server, "serverProof": server_proof}}), Step::Challenged)
            }
            (AUTH, Some((client, server))) => match param("proof") {
                Some(p) if same(&proof(token, Role::Client, &client, &server), p) => {
                    (json!({"id": id, "ok": true, "result": {"authenticated": true}}), Step::Authenticated)
                }
                _ => refused(id),
            },
            (AUTH, None) if param("token").is_some_and(|t| is_hex256(t) && same(token, t)) => {
                (json!({"id": id, "ok": true, "result": {"authenticated": true}}), Step::Authenticated)
            }
            _ => refused(id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_one_frame_token_authenticates_and_nothing_else_does() {
        let mut h = Handshake::default();
        assert_eq!(h.reply(json!(1), AUTH, &json!({"token": TOKEN.to_uppercase()}), TOKEN).1, Step::Authenticated);
        for (method, params) in [
            ("document.inspect", json!({})),
            (AUTH, json!({"token": "0".repeat(64)})),
            (AUTH, json!({"token": "short"})),
            (AUTH, json!({})),
            ("zenda.chat.key", json!({"llave": "zck_a", "venceEn": 3600})),
        ] {
            assert_eq!(Handshake::default().reply(json!(1), method, &params, TOKEN).1, Step::Rejected, "{method} {params}");
        }
    }

    #[test]
    fn the_challenge_proves_both_sides_without_sending_the_token() {
        let mut h = Handshake::default();
        let client = "a".repeat(64);
        let (reply, step) = h.reply(json!(1), CHALLENGE, &json!({"clientNonce": client}), TOKEN);
        assert_eq!(step, Step::Challenged);
        let server = reply["result"]["serverNonce"].as_str().unwrap().to_string();
        assert_eq!(reply["result"]["serverProof"], proof(TOKEN, Role::Server, &client, &server));
        assert!(!reply.to_string().contains(TOKEN));
        // A server proof does not pass as the client's.
        let wrong = proof(TOKEN, Role::Server, &client, &server);
        assert_eq!(
            Handshake { challenge: Some((client.clone(), server.clone())) }.reply(json!(2), AUTH, &json!({"proof": wrong}), TOKEN).1,
            Step::Rejected
        );
        let right = proof(TOKEN, Role::Client, &client, &server);
        assert_eq!(h.reply(json!(2), AUTH, &json!({"proof": right}), TOKEN).1, Step::Authenticated);
    }

    #[test]
    fn a_bad_nonce_or_a_replayed_challenge_is_refused() {
        assert_eq!(Handshake::default().reply(json!(1), CHALLENGE, &json!({"clientNonce": "xyz"}), TOKEN).1, Step::Rejected);
        let mut h = Handshake::default();
        h.reply(json!(1), CHALLENGE, &json!({"clientNonce": "b".repeat(64)}), TOKEN);
        assert_eq!(h.reply(json!(2), CHALLENGE, &json!({"clientNonce": "b".repeat(64)}), TOKEN).1, Step::Rejected);
    }

    #[test]
    fn only_a_64_hex_token_counts() {
        assert!(is_hex256(TOKEN) && !is_hex256("abc") && !is_hex256(&"g".repeat(64)));
    }
}
