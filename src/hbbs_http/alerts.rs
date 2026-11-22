//! CUSTOM BRANDING: alerts sent from the portal ("SEFAZ fora do ar", a system error),
//! shown on the screen of every user logged on to the machine, with the app closed
//! too. Contract fixed with the apiGDe and BR GDe tabs on 22/11/2026
//! (Docs/contrato-api.md in the apiGDe):
//!
//! - the heartbeat reply carries, at its root, "alertas": [{"id", "nivel",
//!   "titulo", "texto", "expira"}] - up to 5, plain text, "expira" in UTC - on every
//!   heartbeat until each one expires or is closed in the portal. Older builds ignore
//!   the key;
//! - each alert shows once per Windows user: the ids and users already shown are kept
//!   in the status store, so a restart does not show them again;
//! - POST <api>/api/alerta-visto reports "exibido" when the box is put up and "ok"
//!   when the user clicks OK. ALERT_ACK and ALERT_IGNORED are final; anything else is
//!   kept and tried again a minute later, for up to a day.
//!
//! Windows only for now: the box is WTSSendMessageW, which a service can put up in
//! any session. It closes by itself when the alert expires.

use hbb_common::{
    chrono, config, log,
    tokio::{sync::mpsc, time::Instant},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, time::Duration};

const ROUTE: &str = "/api/alerta-visto";
const ACK: &str = "ALERT_ACK";
const IGNORED: &str = "ALERT_IGNORED";
const KEY_SHOWN: &str = "alertas_exibidos";
const MAX_ALERTS: usize = 5;
const MAX_TITLE: usize = 80;
const MAX_TEXT: usize = 500;
const MAX_PENDING: usize = 200;
const ACK_RETRY: Duration = Duration::from_secs(60);
const ACK_GIVE_UP: Duration = Duration::from_secs(24 * 3600);

#[cfg_attr(not(windows), allow(dead_code))]
struct Alert {
    id: String,
    nivel: String,
    titulo: String,
    texto: String,
    /// Unix seconds.
    expira: i64,
}

impl Alert {
    fn parse(item: &Value) -> Option<Self> {
        let id = match item.get("id")? {
            Value::String(id) => id.trim().to_owned(),
            Value::Number(id) => id.to_string(),
            _ => return None,
        };
        let expira = chrono::DateTime::parse_from_rfc3339(item.get("expira")?.as_str()?)
            .ok()?
            .timestamp();
        let text = |key: &str, max: usize| -> String {
            item.get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .chars()
                .take(max)
                .collect()
        };
        let titulo = text("titulo", MAX_TITLE);
        let texto = text("texto", MAX_TEXT);
        if id.is_empty() || (titulo.is_empty() && texto.is_empty()) {
            return None;
        }
        Some(Self {
            id,
            nivel: text("nivel", 16),
            titulo,
            texto,
            expira,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Shown {
    expira: i64,
    usuarios: Vec<String>,
}

struct Ack {
    alerta: String,
    evento: &'static str,
    usuario: String,
    em: String,
    since: Instant,
}

impl Ack {
    fn new(alerta: &str, evento: &'static str, usuario: &str) -> Self {
        Self {
            alerta: alerta.to_owned(),
            evento,
            usuario: usuario.to_owned(),
            em: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false),
            since: Instant::now(),
        }
    }
}

pub struct Alerts {
    /// Alert id -> when it expires and the Windows users it was shown to.
    shown: HashMap<String, Shown>,
    pending: Vec<Ack>,
    /// The "ok" of a box comes from its blocking thread.
    tx: mpsc::UnboundedSender<Ack>,
    rx: mpsc::UnboundedReceiver<Ack>,
    next_flush: Instant,
}

impl Default for Alerts {
    fn default() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            shown: serde_json::from_str(&config::Status::get(KEY_SHOWN)).unwrap_or_default(),
            pending: Vec::new(),
            tx,
            rx,
            next_flush: Instant::now(),
        }
    }
}

impl Alerts {
    /// The "alertas" of a heartbeat reply: each one still in force goes up for every
    /// logged-on user who has not seen it yet.
    pub fn handle(&mut self, alertas: Value) {
        let Value::Array(list) = alertas else {
            return;
        };
        let now = chrono::Utc::now().timestamp();
        let before = self.shown.len();
        self.shown.retain(|_, s| s.expira > now);
        let mut changed = self.shown.len() != before;
        let alerts: Vec<Alert> = list
            .iter()
            .take(MAX_ALERTS)
            .filter_map(Alert::parse)
            .filter(|a| a.expira > now)
            .collect();
        if !alerts.is_empty() {
            let sessions = active_sessions();
            for alert in &alerts {
                for (session, user) in &sessions {
                    let shown = self.shown.entry(alert.id.clone()).or_insert_with(|| Shown {
                        expira: alert.expira,
                        usuarios: Vec::new(),
                    });
                    if shown.usuarios.contains(user) {
                        continue;
                    }
                    shown.usuarios.push(user.clone());
                    changed = true;
                    self.pending.push(Ack::new(&alert.id, "exibido", user));
                    show(*session, alert, user, now, self.tx.clone());
                }
            }
        }
        if changed {
            match serde_json::to_string(&self.shown) {
                Ok(shown) => config::Status::set(KEY_SHOWN, shown),
                Err(err) => log::error!("alerts shown not saved: {}", err),
            }
        }
    }

    /// Reports to the server what was shown and clicked. Called on every tick of the
    /// sync loop; posts only when something is waiting.
    pub async fn flush(&mut self, api: &str, id: &str) {
        while let Ok(ack) = self.rx.try_recv() {
            self.pending.push(ack);
        }
        if self.pending.is_empty() || api.is_empty() || id.is_empty() || Instant::now() < self.next_flush {
            return;
        }
        let uuid = crate::encode64(hbb_common::get_uuid());
        let mut queue = std::mem::take(&mut self.pending).into_iter();
        while let Some(ack) = queue.next() {
            let body = json!({
                "id": id,
                "uuid": uuid,
                "alerta": ack.alerta,
                "evento": ack.evento,
                "usuario": ack.usuario,
                "em": ack.em,
            });
            match crate::post_request(format!("{api}{ROUTE}"), body.to_string(), "").await {
                Ok(reply) if matches!(reply.trim(), ACK | IGNORED) => continue,
                Ok(reply) => {
                    let reply: String = reply.chars().take(120).collect();
                    log::info!("alert ack not accepted: {}", reply);
                }
                Err(err) => log::info!("alert ack failed: {}", err),
            }
            // The server is not taking acks now: keep this one and the rest for later.
            self.next_flush = Instant::now() + ACK_RETRY;
            self.pending.push(ack);
            self.pending.extend(queue);
            break;
        }
        self.pending.retain(|ack| ack.since.elapsed() < ACK_GIVE_UP);
        if self.pending.len() > MAX_PENDING {
            let excess = self.pending.len() - MAX_PENDING;
            self.pending.drain(..excess);
        }
    }
}

/// The sessions with a user logged on and active, with the user's name.
#[cfg(windows)]
fn active_sessions() -> Vec<(u32, String)> {
    win::active_sessions()
}

#[cfg(not(windows))]
fn active_sessions() -> Vec<(u32, String)> {
    Vec::new()
}

/// Puts the box up in `session` on a blocking thread, which waits for the click.
#[cfg(windows)]
fn show(session: u32, alert: &Alert, user: &str, now: i64, tx: mpsc::UnboundedSender<Ack>) {
    let app = crate::get_app_name();
    let title = if alert.titulo.is_empty() {
        app
    } else {
        format!("{} - {}", app, alert.titulo)
    };
    let text = if alert.texto.is_empty() {
        alert.titulo.clone()
    } else {
        alert.texto.clone()
    };
    let nivel = alert.nivel.clone();
    let id = alert.id.clone();
    let user = user.to_owned();
    // Closes by itself when the alert expires.
    let timeout = (alert.expira - now).clamp(60, u32::MAX as i64) as u32;
    hbb_common::tokio::task::spawn_blocking(move || {
        if win::message_box(session, &title, &text, &nivel, timeout) {
            let _ = tx.send(Ack::new(&id, "ok", &user));
        }
    });
}

#[cfg(not(windows))]
fn show(_session: u32, _alert: &Alert, _user: &str, _now: i64, _tx: mpsc::UnboundedSender<Ack>) {}

#[cfg(windows)]
mod win {
    use hbb_common::log;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::System::RemoteDesktop::{
        WTSActive, WTSEnumerateSessionsW, WTSFreeMemory, WTSQuerySessionInformationW,
        WTSSendMessageW, WTSUserName, WTS_SESSION_INFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        IDOK, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
        MB_TOPMOST, MESSAGEBOX_RESULT,
    };

    pub fn active_sessions() -> Vec<(u32, String)> {
        let mut out = Vec::new();
        unsafe {
            let mut info: *mut WTS_SESSION_INFOW = std::ptr::null_mut();
            let mut count = 0u32;
            if WTSEnumerateSessionsW(None, 0, 1, &mut info, &mut count).is_err() || info.is_null() {
                return out;
            }
            for session in std::slice::from_raw_parts(info, count as usize) {
                if session.State == WTSActive {
                    if let Some(user) = session_user(session.SessionId) {
                        out.push((session.SessionId, user));
                    }
                }
            }
            WTSFreeMemory(info as *mut _);
        }
        out
    }

    unsafe fn session_user(session: u32) -> Option<String> {
        let mut buffer = PWSTR::null();
        let mut bytes = 0u32;
        WTSQuerySessionInformationW(None, session, WTSUserName, &mut buffer, &mut bytes).ok()?;
        if buffer.is_null() {
            return None;
        }
        let user = buffer.to_string().unwrap_or_default();
        WTSFreeMemory(buffer.0 as *mut _);
        let user = user.trim().to_owned();
        (!user.is_empty()).then_some(user)
    }

    /// Shows the box in `session` and waits until it is closed or `timeout` seconds
    /// pass; true when the user clicked OK.
    pub fn message_box(session: u32, title: &str, text: &str, nivel: &str, timeout: u32) -> bool {
        let icon = match nivel {
            "erro" => MB_ICONERROR,
            "aviso" => MB_ICONWARNING,
            _ => MB_ICONINFORMATION,
        };
        // Lengths in bytes; no terminating NUL needed.
        let title: Vec<u16> = title.encode_utf16().collect();
        let text: Vec<u16> = text.encode_utf16().collect();
        let mut response = MESSAGEBOX_RESULT(0);
        let result = unsafe {
            WTSSendMessageW(
                None,
                session,
                PCWSTR(title.as_ptr()),
                (title.len() * 2) as u32,
                PCWSTR(text.as_ptr()),
                (text.len() * 2) as u32,
                MB_OK | icon | MB_SETFOREGROUND | MB_TOPMOST,
                timeout,
                &mut response,
                true,
            )
        };
        match result {
            Ok(()) => response == IDOK,
            Err(err) => {
                log::info!("alert not shown in session {}: {}", session, err);
                false
            }
        }
    }
}
