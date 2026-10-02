//! CUSTOM BRANDING: the machine registers itself in its brand's catalog on the API
//! server (BR Suporte), so the attendants of that catalog connect to it without
//! typing a password. See docs/3_CatalogoMaquinas.md.
//!
//! What goes up is the permanent password's h1 - SHA256 of the password and this
//! machine's salt - the very value a personal address book carries as `hash`. It
//! opens this machine only, and the password itself is never read: the storage
//! already holds h1. A machine with no permanent password first gets a random one
//! nobody ever sees; an existing one is kept. A changed password, or a new name, is
//! registered again within a minute, so the catalog never loses access.
//!
//! The brand comes from the build (crate::brand). It is self-declared - a public
//! installer can hold no secret to prove it - so what the server does with a new
//! registration (accept, hold for approval) is the server's call.

use hbb_common::{
    config::{decode_permanent_password_h1_from_storage, Config},
    log,
    sodiumoxide::base64,
    tokio::time::Instant,
};
use serde_json::json;
use std::time::Duration;

/// How often the password and the machine's name are looked at.
const CHECK_EVERY: Duration = Duration::from_secs(60);
/// After a refused or failed registration of the same data, the first wait before
/// sending it again; it doubles on each further refusal, up to RETRY_MAX. A refusal
/// can be permanent (a reformatted machine whose registration the server holds until
/// someone releases it in the portal), so this must not hammer the server.
const RETRY_FIRST: Duration = Duration::from_secs(300);
const RETRY_MAX: Duration = Duration::from_secs(3600);
/// Even with nothing changed, register again this often: a machine deleted from its
/// catalog in the portal comes back on its own while it still runs the app (Douglas:
/// "se a máquina voltar, ela se cadastra de novo"), and so does one the server lost.
const REFRESH_EVERY: Duration = Duration::from_secs(24 * 3600);
const ROUTE: &str = "/api/maquina-registro";
/// The only reply that counts as registered. post_request hands back the body and
/// drops the status, so a 409 or an HTML error page would otherwise look the same.
const REGISTERED: &str = "MACHINE_REGISTERED";
const GENERATED_PASSWORD_LEN: usize = 24;

#[derive(Default)]
pub struct Catalog {
    last_check: Option<Instant>,
    /// What the server last accepted, and when; nothing is sent while it still holds,
    /// until REFRESH_EVERY has passed.
    registered: String,
    registered_at: Option<Instant>,
    /// What was last sent without success, when, and how long to wait after it.
    attempted: String,
    last_attempt: Option<Instant>,
    retry_after: Option<Duration>,
}

impl Catalog {
    /// `api` is the API server's base URL, without /api/...; `id` this machine's ID.
    pub async fn tick(&mut self, api: &str, id: &str) {
        if api.is_empty() || id.is_empty() {
            return;
        }
        if self.last_check.map(|t| t.elapsed() < CHECK_EVERY).unwrap_or(false) {
            return;
        }
        self.last_check = Some(Instant::now());

        ensure_permanent_password();
        let Some(hash) = permanent_password_h1() else {
            return;
        };
        let info = crate::get_sysinfo();
        let hostname = info["hostname"].as_str().unwrap_or_default().to_owned();
        let brand = crate::brand::BRAND_FOLDER;
        // Carries the hash, so this string is never logged.
        let current = format!("{api}\n{id}\n{hash}\n{hostname}\n{brand}");
        if current == self.registered
            && self.registered_at.map(|t| t.elapsed() < REFRESH_EVERY).unwrap_or(false)
        {
            return;
        }
        if current == self.attempted {
            let wait = self.retry_after.unwrap_or(RETRY_FIRST);
            if self.last_attempt.map(|t| t.elapsed() < wait).unwrap_or(false) {
                return;
            }
            self.retry_after = Some((wait * 2).min(RETRY_MAX));
        } else {
            self.attempted = current.clone();
            self.retry_after = Some(RETRY_FIRST);
        }
        self.last_attempt = Some(Instant::now());

        let body = json!({
            "id": id,
            "uuid": crate::encode64(hbb_common::get_uuid()),
            "hostname": hostname,
            "username": info["username"].as_str().unwrap_or_default(),
            "os": info["os"].as_str().unwrap_or_default(),
            "marca": brand,
            "hash": hash,
        });
        match crate::post_request(format!("{api}{ROUTE}"), body.to_string(), "").await {
            Ok(reply) if reply.trim() == REGISTERED => {
                self.registered = current;
                self.registered_at = Some(Instant::now());
                self.attempted.clear();
                self.retry_after = None;
                log::info!("machine registered in catalog '{}'", brand);
            }
            Ok(reply) => {
                let reply: String = reply.chars().take(120).collect();
                log::info!("machine registration not accepted: {}", reply);
            }
            Err(err) => log::info!("machine registration failed: {}", err),
        }
    }
}

/// A machine without a permanent password gets a random one nobody sees, so it can
/// be registered. One already set - by the client or a technician - is kept, and
/// one that is cleared later gets replaced the same way.
fn ensure_permanent_password() {
    if Config::has_permanent_password() {
        return;
    }
    let password = Config::get_auto_password(GENERATED_PASSWORD_LEN);
    if Config::set_permanent_password(&password) {
        log::info!("generated a permanent password for the machine catalog");
    }
}

/// The permanent password's h1, base64 as an address book `hash` carries it (the
/// controlling side decodes it with Variant::Original); None while there is no
/// local permanent password in the current hashed storage.
fn permanent_password_h1() -> Option<String> {
    let (storage, _salt) = Config::get_local_permanent_password_storage_and_salt();
    let h1 = decode_permanent_password_h1_from_storage(&storage)?;
    Some(base64::encode(h1, base64::Variant::Original))
}
