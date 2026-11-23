//! CUSTOM BRANDING: the machine's status (disks, RAM, last boot) every 3 hours and its
//! inventory (hardware, Windows, antivirus, network, installed programs, ICP-Brasil
//! certificates) once a day,
//! posted to <api>/api/maquina-inventario so the portal can show them to the
//! attendants of the machine's catalog. Contract fixed with the apiGDe and BR GDe
//! tabs on 22/11/2026 (Docs/contrato-api.md in the apiGDe):
//!
//! - every post carries "status"; "inventario" goes along only when it changed since
//!   the last accepted one, or 7 days after it, so a server that lost it gets it back;
//! - a value the system did not give is left out - never null, never "";
//! - the reply INVENTORY_SAVED is the only success. INVENTORY_REJECTED (no active
//!   catalog row for this id and uuid) waits for the next 3-hour cycle; anything else -
//!   a network error, an older server, an HTML page - is tried again in 15 minutes.
//!
//! The first post comes at a random moment 2 to 30 minutes after start, so the
//! machines of one client that boot together, behind one NAT, do not post together:
//! the server limits requests per IP.
//!
//! The status comes from sysinfo on every desktop platform. The inventory is Windows
//! only, from the registry and WMI, on a blocking thread: a WMI query can take seconds.

use hbb_common::{
    chrono, config, log,
    rand::{self, Rng},
    tokio::{self, time::Instant},
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const ROUTE: &str = "/api/maquina-inventario";
const SAVED: &str = "INVENTORY_SAVED";
const REJECTED: &str = "INVENTORY_REJECTED";
const SEND_EVERY: Duration = Duration::from_secs(3 * 3600);
const RETRY_AFTER: Duration = Duration::from_secs(15 * 60);
const FIRST_SEND_MIN_SECS: u64 = 2 * 60;
const FIRST_SEND_MAX_SECS: u64 = 30 * 60;
#[cfg_attr(not(windows), allow(dead_code))]
const COLLECT_INVENTORY_EVERY: Duration = Duration::from_secs(24 * 3600);
const RESEND_UNCHANGED_SECS: i64 = 7 * 24 * 3600;
/// Status store keys: what the server last took ("<api> <hash>") and when.
const KEY_SENT: &str = "inventario_enviado";
const KEY_SENT_AT: &str = "inventario_enviado_em";
const MAX_DISKS: usize = 32;

/// Lives in the sync loop. The collection and the post run in a task of their own, so
/// a slow WMI query or a 2 MB upload never holds up the heartbeat.
#[derive(Default)]
pub struct Inventory {
    /// None while the task has it.
    state: Option<State>,
    running: Option<tokio::task::JoinHandle<State>>,
}

impl Inventory {
    /// `api` is the API server's base URL, without /api/...; `id` this machine's ID.
    /// Called on every tick of the sync loop; starts the task when a post is due.
    pub async fn tick(&mut self, api: &str, id: &str) {
        if cfg!(any(target_os = "android", target_os = "ios")) || api.is_empty() || id.is_empty() {
            return;
        }
        if let Some(running) = &self.running {
            if !running.is_finished() {
                return;
            }
        }
        if let Some(running) = self.running.take() {
            match running.await {
                Ok(state) => self.state = Some(state),
                Err(err) => log::error!("machine status task failed: {}", err),
            }
        }
        let state = self.state.get_or_insert_with(State::new);
        if Instant::now() < state.next_send {
            return;
        }
        let Some(mut state) = self.state.take() else {
            return;
        };
        let (api, id) = (api.to_owned(), id.to_owned());
        self.running = Some(tokio::spawn(async move {
            state.run(&api, &id).await;
            state
        }));
    }
}

struct State {
    next_send: Instant,
    #[cfg_attr(not(windows), allow(dead_code))]
    collected_at: Option<Instant>,
    /// The last inventory collected and its hash, kept until the next collection.
    inventory: Option<(Value, String)>,
}

impl State {
    fn new() -> Self {
        let first = rand::thread_rng().gen_range(FIRST_SEND_MIN_SECS..=FIRST_SEND_MAX_SECS);
        Self {
            next_send: Instant::now() + Duration::from_secs(first),
            collected_at: None,
            inventory: None,
        }
    }

    async fn run(&mut self, api: &str, id: &str) {
        // Replaced below when the server answers SAVED or REJECTED.
        self.next_send = Instant::now() + RETRY_AFTER;

        let status = match tokio::task::spawn_blocking(collect_status).await {
            Ok(status) => status,
            Err(err) => {
                log::error!("machine status collection failed: {}", err);
                return;
            }
        };
        self.refresh_inventory().await;

        let mut body = json!({
            "id": id,
            "uuid": crate::encode64(hbb_common::get_uuid()),
            "status": status,
        });
        let mut sending = None;
        if let Some((inventory, hash)) = &self.inventory {
            let mark = format!("{api} {hash}");
            if inventory_due(&mark) {
                body["inventario"] = inventory.clone();
                sending = Some(mark);
            }
        }
        match crate::post_request(format!("{api}{ROUTE}"), body.to_string(), "").await {
            Ok(reply) if reply.trim() == SAVED => {
                self.next_send = Instant::now() + SEND_EVERY;
                match sending {
                    Some(mark) => {
                        config::Status::set(KEY_SENT, mark);
                        config::Status::set(KEY_SENT_AT, now_unix().to_string());
                        log::info!("machine status and inventory saved");
                    }
                    None => log::info!("machine status saved"),
                }
            }
            Ok(reply) if reply.trim() == REJECTED => {
                self.next_send = Instant::now() + SEND_EVERY;
                log::info!("machine status rejected: no active catalog row for this machine");
            }
            Ok(reply) => {
                let reply: String = reply.chars().take(120).collect();
                log::info!("machine status not accepted: {}", reply);
            }
            Err(err) => log::info!("machine status failed: {}", err),
        }
    }

    /// Collects the inventory again once a day; posting is decided by its hash.
    #[cfg(windows)]
    async fn refresh_inventory(&mut self) {
        if self
            .collected_at
            .map(|t| t.elapsed() < COLLECT_INVENTORY_EVERY)
            .unwrap_or(false)
        {
            return;
        }
        self.collected_at = Some(Instant::now());
        match tokio::task::spawn_blocking(win::collect_inventory).await {
            Ok(inventory) => {
                let hash = hash_of(&inventory);
                self.inventory = Some((inventory, hash));
            }
            Err(err) => log::error!("machine inventory collection failed: {}", err),
        }
    }

    #[cfg(not(windows))]
    async fn refresh_inventory(&mut self) {}
}

/// True when the server has not taken this inventory (`mark` = "<api> <hash>"), or
/// took it 7 days ago or more.
fn inventory_due(mark: &str) -> bool {
    if config::Status::get(KEY_SENT) != mark {
        return true;
    }
    let sent_at = config::Status::get(KEY_SENT_AT).parse::<i64>().unwrap_or(0);
    now_unix() - sent_at >= RESEND_UNCHANGED_SECS
}

/// The inventory's hash, leaving out when it was collected.
#[cfg_attr(not(windows), allow(dead_code))]
fn hash_of(inventory: &Value) -> String {
    let mut inventory = inventory.clone();
    if let Some(map) = inventory.as_object_mut() {
        map.remove("coletado_em");
    }
    let mut hasher = Sha256::new();
    hasher.update(inventory.to_string().as_bytes());
    hbb_common::base64::encode(&hasher.finalize()[..])
}

fn collect_status() -> Value {
    use hbb_common::sysinfo::{DiskKind, Disks, System};

    let mut system = System::new();
    system.refresh_memory();
    let mut status = Map::new();
    status.insert("coletado_em".to_owned(), json!(now_iso()));
    if let Some(boot) = iso_of_unix(system.boot_time() as i64) {
        status.insert("boot_em".to_owned(), json!(boot));
    }
    status.insert("ram_total_gb".to_owned(), json!(gb(system.total_memory())));
    status.insert("ram_usada_gb".to_owned(), json!(gb(system.used_memory())));

    // Local fixed disks only: sysinfo already leaves network drives out.
    let mut disks: Vec<(String, Value)> = Vec::new();
    for disk in Disks::new_with_refreshed_list().list() {
        if disk.is_removable() || disk.total_space() == 0 {
            continue;
        }
        let fs = disk.file_system().to_string_lossy().trim().to_owned();
        let unidade = drive_name(&disk.mount_point().to_string_lossy());
        if unidade.is_empty() || skip_mount(&unidade, &fs) || disks.iter().any(|(u, _)| *u == unidade) {
            continue;
        }
        let mut d = Map::new();
        d.insert("unidade".to_owned(), json!(unidade));
        d.insert("total_gb".to_owned(), json!(gb(disk.total_space())));
        d.insert("livre_gb".to_owned(), json!(gb(disk.available_space())));
        if !fs.is_empty() {
            d.insert("fs".to_owned(), json!(fs));
        }
        match disk.kind() {
            DiskKind::SSD => {
                d.insert("tipo".to_owned(), json!("SSD"));
            }
            DiskKind::HDD => {
                d.insert("tipo".to_owned(), json!("HDD"));
            }
            _ => {}
        }
        disks.push((unidade, Value::Object(d)));
        if disks.len() >= MAX_DISKS {
            break;
        }
    }
    disks.sort_by(|a, b| a.0.cmp(&b.0));
    status.insert(
        "discos".to_owned(),
        Value::Array(disks.into_iter().map(|(_, d)| d).collect()),
    );
    if let Some(list) = physical_disks_health() {
        status.insert("discos_fisicos".to_owned(), Value::Array(list));
    }
    Value::Object(status)
}

/// The physical disks' health, in the status because temperature and power-on hours
/// change all the time and would make the daily inventory go up every day.
#[cfg(windows)]
fn physical_disks_health() -> Option<Vec<Value>> {
    win::disk_health()
}

#[cfg(not(windows))]
fn physical_disks_health() -> Option<Vec<Value>> {
    None
}

/// "C:\" -> "C:" on Windows; elsewhere the mount point as it is.
fn drive_name(mount: &str) -> String {
    if cfg!(windows) {
        mount.trim_end_matches('\\').to_owned()
    } else {
        mount.to_owned()
    }
}

/// Mounts that are not a disk anyone fills: snaps, RAM file systems, and macOS's
/// system volumes, which share the free space of "/".
fn skip_mount(mount: &str, fs: &str) -> bool {
    if cfg!(windows) {
        return false;
    }
    matches!(
        fs,
        "squashfs" | "tmpfs" | "devtmpfs" | "overlay" | "ramfs" | "autofs"
    ) || mount.starts_with("/snap/")
        || mount.starts_with("/System/Volumes/")
        || mount.starts_with("/private/var/vm")
}

/// Bytes to GB (1024³), two decimals.
fn gb(bytes: u64) -> f64 {
    (bytes as f64 / 1_073_741_824.0 * 100.0).round() / 100.0
}

fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// "2026-11-22T14:00:00-03:00": local time with its offset, in seconds.
fn now_iso() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

fn iso_of_unix(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|t| {
        t.with_timezone(&chrono::Local)
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
    })
}

/// "AAAA-MM-DD" in local time.
#[cfg_attr(not(windows), allow(dead_code))]
fn date_of_unix(secs: i64) -> Option<String> {
    if secs <= 0 {
        return None;
    }
    chrono::DateTime::from_timestamp(secs, 0).map(|t| {
        t.with_timezone(&chrono::Local)
            .format("%Y-%m-%d")
            .to_string()
    })
}

#[cfg(windows)]
mod win {
    use super::{date_of_unix, gb, now_iso};
    use serde_json::{json, Map, Value};
    use std::collections::BTreeMap;
    use winreg::{enums::*, RegKey};

    const CIMV2: &str = "ROOT\\CIMV2";
    const STORAGE: &str = "ROOT\\Microsoft\\Windows\\Storage";
    const SECURITY_CENTER: &str = "ROOT\\SecurityCenter2";
    /// Windows' own application id in the licensing tables.
    const WINDOWS_APP_ID: &str = "55c92734-d682-4d71-983e-d6ec3f16059f";
    const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    const MAX_PROGRAMS: usize = 2000;
    /// The server takes texts of up to 200 characters.
    const MAX_TEXT: usize = 200;
    const MAX_CERTIFICATES: usize = 50;
    const PROFILE_LIST: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList";
    /// The personal ("My") certificate stores: the machine's and a user's in the
    /// registry, and a user's files in the profile.
    const MACHINE_STORE: &str = "SOFTWARE\\Microsoft\\SystemCertificates\\MY\\Certificates";
    const USER_STORE: &str = "Software\\Microsoft\\SystemCertificates\\My\\Certificates";
    const USER_STORE_FOLDER: &str = "AppData\\Roaming\\Microsoft\\SystemCertificates\\My\\Certificates";
    /// Store entry properties: the certificate (DER) and its SHA-1.
    const CERT_PROP_DER: u32 = 32;
    const CERT_PROP_SHA1: u32 = 3;
    /// DER encodings (after the 06 tag and length) of 2.16.76.1.2 (ICP-Brasil
    /// certificate policies), 2.16.76.1.3.3 (the company's CNPJ, in an e-CNPJ) and
    /// 2.16.76.1.3.1 (the holder's birth date and CPF, in an e-CPF).
    const ICP_POLICY: &[u8] = &[0x60, 0x4c, 0x01, 0x02];
    const ICP_CNPJ: &[u8] = &[0x60, 0x4c, 0x01, 0x03, 0x03];
    const ICP_PERSON: &[u8] = &[0x60, 0x4c, 0x01, 0x03, 0x01];
    /// What manufacturers leave in the firmware fields they did not fill in.
    const PLACEHOLDERS: &[&str] = &[
        "to be filled by o.e.m.",
        "default string",
        "system serial number",
        "system product name",
        "system manufacturer",
        "system version",
        "base board serial number",
        "not specified",
        "not applicable",
        "none",
        "n/a",
        "unknown",
        "oem",
        "invalid",
    ];

    pub fn collect_inventory() -> Value {
        let _com = wbem::Com::init();
        let wmi = wbem::Wbem::new();
        let wmi = wmi.as_ref();
        let mut inventory = Map::new();
        inventory.insert("coletado_em".to_owned(), json!(now_iso()));
        put_map(&mut inventory, "hardware", hardware(wmi));
        put_map(&mut inventory, "windows", windows_info(wmi));
        if let Some(list) = antivirus(wmi) {
            inventory.insert("antivirus".to_owned(), Value::Array(list));
        }
        if let Some(list) = network(wmi) {
            inventory.insert("rede".to_owned(), Value::Array(list));
        }
        if let Some(list) = programs() {
            inventory.insert("programas".to_owned(), Value::Array(list));
        }
        let (certificates, truncated) = certificates();
        inventory.insert("certificados".to_owned(), Value::Array(certificates));
        if truncated {
            inventory.insert("certificados_truncado".to_owned(), json!(true));
        }
        Value::Object(inventory)
    }

    fn rows(
        wmi: Option<&wbem::Wbem>,
        namespace: &str,
        wql: &str,
        props: &[&str],
    ) -> Option<Vec<wbem::Row>> {
        wmi?.query(namespace, wql, props)
    }

    fn hardware(wmi: Option<&wbem::Wbem>) -> Map<String, Value> {
        let mut hw = Map::new();
        if let Some(list) = rows(
            wmi,
            CIMV2,
            "SELECT Manufacturer, Model FROM Win32_ComputerSystem",
            &["Manufacturer", "Model"],
        ) {
            if let Some(r) = list.first() {
                put(&mut hw, "fabricante", r.oem("Manufacturer"));
                put(&mut hw, "modelo", r.oem("Model"));
            }
        }
        if let Some(list) = rows(
            wmi,
            CIMV2,
            "SELECT SerialNumber, Manufacturer, SMBIOSBIOSVersion, ReleaseDate FROM Win32_BIOS",
            &["SerialNumber", "Manufacturer", "SMBIOSBIOSVersion", "ReleaseDate"],
        ) {
            if let Some(r) = list.first() {
                put(&mut hw, "serie", r.oem("SerialNumber"));
                let mut bios = Map::new();
                put(&mut bios, "fabricante", r.oem("Manufacturer"));
                put(&mut bios, "versao", r.text("SMBIOSBIOSVersion"));
                put(&mut bios, "data", r.text("ReleaseDate").and_then(|d| yyyymmdd(&d)));
                put_map(&mut hw, "bios", bios);
            }
        }
        if let Some(list) = rows(
            wmi,
            CIMV2,
            "SELECT Manufacturer, Product, SerialNumber FROM Win32_BaseBoard",
            &["Manufacturer", "Product", "SerialNumber"],
        ) {
            if let Some(r) = list.first() {
                let mut board = Map::new();
                put(&mut board, "fabricante", r.oem("Manufacturer"));
                put(&mut board, "modelo", r.oem("Product"));
                put(&mut board, "serie", r.oem("SerialNumber"));
                put_map(&mut hw, "placa_mae", board);
            }
        }
        if let Some(list) = rows(
            wmi,
            CIMV2,
            "SELECT Name, NumberOfCores, NumberOfLogicalProcessors FROM Win32_Processor",
            &["Name", "NumberOfCores", "NumberOfLogicalProcessors"],
        ) {
            if let Some(r) = list.first() {
                let mut cpu = Map::new();
                put(&mut cpu, "nome", r.text("Name"));
                // Summed over the sockets of a multi-processor server.
                let cores: u64 = list.iter().filter_map(|r| r.number("NumberOfCores")).sum();
                let threads: u64 = list
                    .iter()
                    .filter_map(|r| r.number("NumberOfLogicalProcessors"))
                    .sum();
                if cores > 0 {
                    cpu.insert("nucleos".to_owned(), json!(cores));
                }
                if threads > 0 {
                    cpu.insert("threads".to_owned(), json!(threads));
                }
                put_map(&mut hw, "cpu", cpu);
            }
        }
        if let Some(list) = rows(
            wmi,
            CIMV2,
            "SELECT DeviceLocator, Capacity, Speed, Manufacturer, PartNumber FROM Win32_PhysicalMemory",
            &["DeviceLocator", "Capacity", "Speed", "Manufacturer", "PartNumber"],
        ) {
            let memories = list
                .iter()
                .map(|r| {
                    let mut m = Map::new();
                    put(&mut m, "slot", r.text("DeviceLocator"));
                    if let Some(bytes) = r.number("Capacity").filter(|b| *b > 0) {
                        m.insert("capacidade_gb".to_owned(), json!(gb(bytes)));
                    }
                    if let Some(mhz) = r.number("Speed").filter(|s| *s > 0) {
                        m.insert("velocidade_mhz".to_owned(), json!(mhz));
                    }
                    put(&mut m, "fabricante", r.oem("Manufacturer"));
                    put(&mut m, "modelo", r.oem("PartNumber"));
                    Value::Object(m)
                })
                .collect();
            hw.insert("memorias".to_owned(), Value::Array(memories));
        }
        if let Some(list) = physical_disks(wmi) {
            hw.insert("discos".to_owned(), Value::Array(list));
        }
        hw
    }

    /// Health of each physical disk, for the status (22/11/2026, Douglas's call):
    /// Windows' own verdict (MSFT_PhysicalDisk.HealthStatus) and, where the disk reports
    /// them, its reliability counters - temperature, wear, power-on hours, errors. The
    /// counters need an administrator, which the service is, and have to be asked disk by
    /// disk: a plain SELECT on MSFT_StorageReliabilityCounter returns nothing, even as
    /// administrator (measured 22/11/2026). Card readers are left out (Douglas,
    /// 22/11/2026): with a card in, Win32_DiskDrive calls them "Removable Media"; with
    /// none, they show up as a disk of size 0.
    pub fn disk_health() -> Option<Vec<Value>> {
        let _com = wbem::Com::init();
        let wmi = wbem::Wbem::new();
        let wmi = wmi.as_ref();
        let disks = rows(
            wmi,
            STORAGE,
            "SELECT __RELPATH, DeviceId, FriendlyName, HealthStatus, Size FROM MSFT_PhysicalDisk",
            &["__RELPATH", "DeviceId", "FriendlyName", "HealthStatus", "Size"],
        )?;
        // Win32_DiskDrive.Index is the same number as MSFT_PhysicalDisk.DeviceId.
        let removable: Vec<String> = rows(
            wmi,
            CIMV2,
            "SELECT Index, MediaType FROM Win32_DiskDrive",
            &["Index", "MediaType"],
        )
        .unwrap_or_default()
        .iter()
        .filter(|d| {
            d.text("MediaType")
                .map_or(false, |m| m.to_lowercase().contains("removable"))
        })
        .filter_map(|d| d.text("Index"))
        .collect();
        Some(
            disks
                .iter()
                .filter(|d| d.number("Size").unwrap_or(0) > 0)
                .filter(|d| {
                    d.text("DeviceId")
                        .map_or(true, |id| !removable.contains(&id))
                })
                .map(|d| {
                    let mut m = Map::new();
                    put(&mut m, "modelo", d.text("FriendlyName"));
                    // HealthStatus: 0 healthy, 1 warning, 2 unhealthy, 5 unknown.
                    let saude = match d.number("HealthStatus") {
                        Some(0) => Some("saudavel"),
                        Some(1) => Some("atencao"),
                        Some(2) => Some("falha"),
                        _ => None,
                    };
                    put(&mut m, "saude", saude.map(str::to_owned));
                    let counters = d.text("__RELPATH").and_then(|path| {
                        rows(
                            wmi,
                            STORAGE,
                            &format!(
                                "ASSOCIATORS OF {{{}}} \
                                 WHERE ResultClass = MSFT_StorageReliabilityCounter",
                                path
                            ),
                            &[
                                "Temperature",
                                "Wear",
                                "PowerOnHours",
                                "ReadErrorsTotal",
                                "WriteErrorsTotal",
                            ],
                        )
                    });
                    if let Some(c) = counters.as_ref().and_then(|list| list.first()) {
                        if let Some(t) = c.number("Temperature").filter(|t| *t > 0 && *t < 150) {
                            m.insert("temperatura_c".to_owned(), json!(t));
                        }
                        if let Some(w) = c.number("Wear").filter(|w| *w <= 100) {
                            m.insert("desgaste_pct".to_owned(), json!(w));
                        }
                        for (prop, key) in [
                            ("PowerOnHours", "horas_ligado"),
                            ("ReadErrorsTotal", "erros_leitura"),
                            ("WriteErrorsTotal", "erros_escrita"),
                        ] {
                            if let Some(v) = c.number(prop) {
                                m.insert(key.to_owned(), json!(v));
                            }
                        }
                    }
                    Value::Object(m)
                })
                .collect(),
        )
    }

    fn physical_disks(wmi: Option<&wbem::Wbem>) -> Option<Vec<Value>> {
        if let Some(list) = rows(
            wmi,
            STORAGE,
            "SELECT FriendlyName, MediaType, BusType, Size, SerialNumber FROM MSFT_PhysicalDisk",
            &["FriendlyName", "MediaType", "BusType", "Size", "SerialNumber"],
        ) {
            return Some(
                list.iter()
                    // A card reader with no card is a disk of size 0.
                    .filter(|r| r.number("Size").unwrap_or(0) > 0)
                    .map(|r| {
                        let mut d = Map::new();
                        put(&mut d, "modelo", r.text("FriendlyName"));
                        match r.number("MediaType") {
                            Some(3) => {
                                d.insert("tipo".to_owned(), json!("HDD"));
                            }
                            Some(4) => {
                                d.insert("tipo".to_owned(), json!("SSD"));
                            }
                            _ => {}
                        }
                        if let Some(bytes) = r.number("Size").filter(|b| *b > 0) {
                            d.insert("tamanho_gb".to_owned(), json!(gb(bytes)));
                        }
                        put(
                            &mut d,
                            "interface",
                            r.number("BusType").and_then(bus_name).map(str::to_owned),
                        );
                        put(&mut d, "serie", r.oem("SerialNumber"));
                        Value::Object(d)
                    })
                    .collect(),
            );
        }
        // Windows 7 has no storage namespace: model, size and serial only.
        let list = rows(
            wmi,
            CIMV2,
            "SELECT Model, Size, SerialNumber FROM Win32_DiskDrive",
            &["Model", "Size", "SerialNumber"],
        )?;
        Some(
            list.iter()
                .map(|r| {
                    let mut d = Map::new();
                    put(&mut d, "modelo", r.text("Model"));
                    if let Some(bytes) = r.number("Size").filter(|b| *b > 0) {
                        d.insert("tamanho_gb".to_owned(), json!(gb(bytes)));
                    }
                    put(&mut d, "serie", r.oem("SerialNumber"));
                    Value::Object(d)
                })
                .collect(),
        )
    }

    /// MSFT_PhysicalDisk.BusType to the names in the contract.
    fn bus_name(bus: u64) -> Option<&'static str> {
        Some(match bus {
            1 => "SCSI",
            3 => "ATA",
            7 => "USB",
            8 => "RAID",
            10 => "SAS",
            11 => "SATA",
            12 => "SD",
            17 => "NVMe",
            _ => return None,
        })
    }

    fn windows_info(wmi: Option<&wbem::Wbem>) -> Map<String, Value> {
        let mut w = Map::new();
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = hklm.open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        ) {
            let text = |name: &str| reg_text(&key, name);
            let build = text("CurrentBuild").or_else(|| text("CurrentBuildNumber"));
            let build_number = build
                .as_deref()
                .and_then(|b| b.parse::<u32>().ok())
                .unwrap_or(0);
            // ProductName still says "Windows 10" on Windows 11.
            let name = text("ProductName").map(|n| {
                if build_number >= 22000 {
                    n.replacen("Windows 10", "Windows 11", 1)
                } else {
                    n
                }
            });
            put(&mut w, "nome", name);
            put(&mut w, "edicao", text("EditionID"));
            put(
                &mut w,
                "versao",
                text("DisplayVersion").or_else(|| text("ReleaseId")),
            );
            let ubr = key.get_value::<u32, _>("UBR").ok();
            put(
                &mut w,
                "build",
                build.map(|b| match ubr {
                    Some(ubr) => format!("{b}.{ubr}"),
                    None => b,
                }),
            );
            put(
                &mut w,
                "instalado_em",
                key.get_value::<u32, _>("InstallDate")
                    .ok()
                    .and_then(|secs| date_of_unix(secs as i64)),
            );
        }
        if let Ok(key) = hklm.open_subkey_with_flags(
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            KEY_READ,
        ) {
            let arch = reg_text(&key, "PROCESSOR_ARCHITECTURE").and_then(|a| {
                match a.to_uppercase().as_str() {
                    "AMD64" => Some("x64"),
                    "X86" => Some("x86"),
                    "ARM64" => Some("arm64"),
                    _ => None,
                }
            });
            put(&mut w, "arquitetura", arch.map(str::to_owned));
        }
        let wql = format!(
            "SELECT LicenseStatus FROM SoftwareLicensingProduct \
             WHERE ApplicationID = '{WINDOWS_APP_ID}' AND PartialProductKey IS NOT NULL"
        );
        if let Some(list) = rows(wmi, CIMV2, &wql, &["LicenseStatus"]).filter(|l| !l.is_empty()) {
            // LicenseStatus 1 = licensed.
            let activated = list.iter().any(|r| r.number("LicenseStatus") == Some(1));
            w.insert("ativado".to_owned(), json!(activated));
        }
        w
    }

    /// None where there is no Security Center (Windows Server).
    fn antivirus(wmi: Option<&wbem::Wbem>) -> Option<Vec<Value>> {
        let list = rows(
            wmi,
            SECURITY_CENTER,
            "SELECT displayName, productState FROM AntiVirusProduct",
            &["displayName", "productState"],
        )?;
        Some(
            list.iter()
                .map(|r| {
                    let mut a = Map::new();
                    put(&mut a, "nome", r.text("displayName"));
                    if let Some(state) = r.number("productState") {
                        // productState is 0xPPSSDD: SS 0x10 or 0x11 = scanning on,
                        // DD 0x00 = signatures up to date.
                        a.insert("ativo".to_owned(), json!(((state >> 12) & 0xF) == 1));
                        a.insert("atualizado".to_owned(), json!((state & 0xF0) == 0));
                    }
                    Value::Object(a)
                })
                .collect(),
        )
    }

    fn network(wmi: Option<&wbem::Wbem>) -> Option<Vec<Value>> {
        let list = rows(
            wmi,
            CIMV2,
            "SELECT Description, MACAddress, IPAddress, DefaultIPGateway \
             FROM Win32_NetworkAdapterConfiguration WHERE IPEnabled = TRUE",
            &["Description", "MACAddress", "IPAddress", "DefaultIPGateway"],
        )?;
        Some(
            list.iter()
                .map(|r| {
                    let mut n = Map::new();
                    put(&mut n, "nome", r.text("Description"));
                    put(&mut n, "mac", r.text("MACAddress"));
                    let ips = r.texts("IPAddress");
                    if !ips.is_empty() {
                        n.insert("ips".to_owned(), json!(ips));
                    }
                    let gateway = r.texts("DefaultIPGateway");
                    if !gateway.is_empty() {
                        n.insert("gateway".to_owned(), json!(gateway));
                    }
                    Value::Object(n)
                })
                .collect(),
        )
    }

    /// The programs installed for the whole machine (64- and 32-bit views), sorted by
    /// name, without system components and updates. None when neither view opens.
    fn programs() -> Option<Vec<Value>> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let mut found: BTreeMap<(String, String), Value> = BTreeMap::new();
        let mut read_any = false;
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let Ok(root) = hklm.open_subkey_with_flags(UNINSTALL, KEY_READ | view) else {
                continue;
            };
            read_any = true;
            for name in root.enum_keys().flatten() {
                let Ok(app) = root.open_subkey_with_flags(&name, KEY_READ | view) else {
                    continue;
                };
                let text = |value: &str| reg_text(&app, value);
                let Some(nome) = text("DisplayName") else {
                    continue;
                };
                let system = app
                    .get_value::<u32, _>("SystemComponent")
                    .map(|v| v == 1)
                    .unwrap_or(false);
                let update = text("ParentKeyName").is_some()
                    || text("ReleaseType")
                        .map(|t| {
                            let t = t.to_lowercase();
                            t.contains("update") || t.contains("hotfix")
                        })
                        .unwrap_or(false);
                if system || update {
                    continue;
                }
                let versao = text("DisplayVersion");
                let key = (nome.to_lowercase(), versao.clone().unwrap_or_default());
                if found.contains_key(&key) {
                    continue;
                }
                let mut p = Map::new();
                p.insert("nome".to_owned(), json!(nome));
                put(&mut p, "versao", versao);
                put(&mut p, "fabricante", text("Publisher"));
                put(
                    &mut p,
                    "instalado_em",
                    text("InstallDate").and_then(|d| yyyymmdd(&d)),
                );
                found.insert(key, Value::Object(p));
            }
        }
        read_any.then(|| found.into_values().take(MAX_PROGRAMS).collect())
    }

    /// The ICP-Brasil certificates (e-CNPJ, e-CPF) in the machine's personal store and
    /// in each Windows user's, sorted by expiry, at most MAX_CERTIFICATES; the bool
    /// says the list was cut (Douglas, 23/11/2026, for the portal's expiry list).
    ///
    /// Read straight from where Windows keeps those stores, so the service needs no
    /// user's session: the registry for the machine and for a user whose hive is
    /// loaded, and the files in each profile. Every entry there is the same list of
    /// properties, the certificate itself (DER) being property 32 and its SHA-1
    /// property 3 (measured 23/11/2026). ICP-Brasil = a certificate policy under
    /// 2.16.76.1.2 (2.16.76.1.2.1.x is an A1, 2.16.76.1.2.3.x an A3).
    fn certificates() -> (Vec<Value>, bool) {
        let mut found: BTreeMap<(String, String), (i64, Value)> = BTreeMap::new();
        let add = |found: &mut BTreeMap<(String, String), (i64, Value)>,
                       blob: &[u8],
                       name: &str,
                       user: Option<&str>| {
            if let Some((expiry, mut cert)) = icp_certificate(blob, name) {
                let thumbprint = cert["thumbprint"].as_str().unwrap_or_default().to_owned();
                match user {
                    Some(user) => {
                        cert["repositorio"] = json!("usuario");
                        cert["usuario"] = json!(user);
                    }
                    None => cert["repositorio"] = json!("maquina"),
                }
                found
                    .entry((thumbprint, user.unwrap_or_default().to_owned()))
                    .or_insert((expiry, cert));
            }
        };

        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for (blob, name) in registry_store(&hklm, MACHINE_STORE) {
            add(&mut found, blob.as_slice(), name.as_str(), None);
        }
        let hku = RegKey::predef(HKEY_USERS);
        if let Ok(profiles) = hklm.open_subkey(PROFILE_LIST) {
            for sid in profiles.enum_keys().flatten() {
                // Local and domain users (S-1-5-21-…) and Microsoft Entra ones
                // (S-1-12-1-…); not SYSTEM, LocalService, NetworkService.
                if !sid.starts_with("S-1-5-21-") && !sid.starts_with("S-1-12-1-") {
                    continue;
                }
                let Some(path) = profiles
                    .open_subkey(&sid)
                    .ok()
                    .and_then(|k| k.get_value::<String, _>("ProfileImagePath").ok())
                    .map(|p| expand_env(&p))
                else {
                    continue;
                };
                let profile = std::path::Path::new(&path);
                let Some(user) = profile
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .filter(|n| !n.is_empty())
                else {
                    continue;
                };
                if let Ok(files) = std::fs::read_dir(profile.join(USER_STORE_FOLDER)) {
                    for file in files.flatten() {
                        if let Ok(blob) = std::fs::read(file.path()) {
                            let name = file.file_name().to_string_lossy().into_owned();
                            add(&mut found, blob.as_slice(), name.as_str(), Some(user.as_str()));
                        }
                    }
                }
                if let Ok(hive) = hku.open_subkey(&sid) {
                    for (blob, name) in registry_store(&hive, USER_STORE) {
                        add(&mut found, blob.as_slice(), name.as_str(), Some(user.as_str()));
                    }
                }
            }
        }

        // When there are too many, the ones left out are those that expired longest
        // ago (an accountant's machine piles up years of clients' certificates): keep
        // the MAX_CERTIFICATES that expire last, ties by thumbprint and user so the
        // cut is the same every day, then send them soonest expiry first.
        let mut list: Vec<((String, String), (i64, Value))> = found.into_iter().collect();
        list.sort_by(|(ka, (ea, _)), (kb, (eb, _))| eb.cmp(ea).then_with(|| ka.cmp(kb)));
        let truncated = list.len() > MAX_CERTIFICATES;
        list.truncate(MAX_CERTIFICATES);
        list.sort_by(|(ka, (ea, _)), (kb, (eb, _))| ea.cmp(eb).then_with(|| ka.cmp(kb)));
        (list.into_iter().map(|(_, (_, cert))| cert).collect(), truncated)
    }

    /// The entries of a registry certificate store: each subkey's "Blob", with the
    /// subkey's name (the thumbprint).
    fn registry_store(root: &RegKey, path: &str) -> Vec<(Vec<u8>, String)> {
        let Ok(store) = root.open_subkey(path) else {
            return Vec::new();
        };
        store
            .enum_keys()
            .flatten()
            .filter_map(|name| {
                let value = store.open_subkey(&name).ok()?.get_raw_value("Blob").ok()?;
                Some((value.bytes, name))
            })
            .collect()
    }

    /// "%SystemDrive%\Users\x" -> "C:\Users\x"; a variable that is not set stays.
    fn expand_env(text: &str) -> String {
        let mut out = String::new();
        let mut rest = text;
        while let Some(start) = rest.find('%') {
            let Some(len) = rest[start + 1..].find('%') else {
                break;
            };
            let name = &rest[start + 1..start + 1 + len];
            out.push_str(&rest[..start]);
            match std::env::var(name) {
                Ok(value) if !name.is_empty() => out.push_str(&value),
                _ => out.push_str(&rest[start..start + len + 2]),
            }
            rest = &rest[start + len + 2..];
        }
        out.push_str(rest);
        out
    }

    /// One store entry as the contract's object, with its expiry (unix seconds) to
    /// sort by; None when it is not an ICP-Brasil certificate or does not parse.
    /// `name` is the entry's name in the store, the thumbprint when property 3 is
    /// missing.
    fn icp_certificate(blob: &[u8], name: &str) -> Option<(i64, Value)> {
        let encoded = store_property(blob, CERT_PROP_DER)?;
        let cert = der::parse(encoded)?;
        let (oid_len, at) = der::find_oid(cert.extensions, ICP_POLICY)?;
        let level = (oid_len > ICP_POLICY.len())
            .then(|| cert.extensions.get(at + ICP_POLICY.len()).copied())
            .flatten();

        let mut c = Map::new();
        let cn = cert.subject_cn.as_deref().unwrap_or_default();
        // The holder's CN ends in ":<CNPJ>" or ":<CPF>"; the number goes in its own
        // field (the CPF masked) and never in the name.
        let (holder, number) = match cn.rsplit_once(':') {
            Some((holder, digits))
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
            {
                (holder.trim(), Some(digits))
            }
            _ => (cn.trim(), None),
        };
        put(&mut c, "titular", clip(holder));
        if der::find_oid(cert.extensions, ICP_CNPJ).is_some() {
            c.insert("tipo".to_owned(), json!("e-CNPJ"));
            let cnpj = der::other_name(cert.extensions, ICP_CNPJ)
                .map(|v| digits_of(&v))
                .filter(|d| d.len() == 14)
                .or_else(|| number.map(str::to_owned).filter(|d| d.len() == 14));
            put(&mut c, "cnpj", cnpj);
        } else if der::find_oid(cert.extensions, ICP_PERSON).is_some() {
            c.insert("tipo".to_owned(), json!("e-CPF"));
            // Birth date (8), then the CPF (11).
            let cpf = der::other_name(cert.extensions, ICP_PERSON)
                .and_then(|v| v.get(8..19).map(str::to_owned))
                .filter(|d| d.bytes().all(|b| b.is_ascii_digit()) && d.bytes().any(|b| b != b'0'))
                .or_else(|| number.map(str::to_owned).filter(|d| d.len() == 11));
            put(
                &mut c,
                "cpf",
                cpf.map(|d| format!("{}.***.***-{}", &d[..3], &d[9..])),
            );
        }
        if let Some(level @ 1..=4) = level {
            c.insert("modelo".to_owned(), json!(format!("A{}", level)));
        }
        put(&mut c, "emissor", cert.issuer_cn.as_deref().and_then(clip));
        put(&mut c, "valido_desde", super::iso_of_unix(cert.not_before));
        put(&mut c, "valido_ate", super::iso_of_unix(cert.not_after));
        let thumbprint = store_property(blob, CERT_PROP_SHA1)
            .filter(|h| h.len() == 20)
            .map(|h| h.iter().map(|b| format!("{:02X}", b)).collect::<String>())
            .or_else(|| {
                Some(name.to_uppercase())
                    .filter(|n| n.len() == 40 && n.bytes().all(|b| b.is_ascii_hexdigit()))
            })?;
        c.insert("thumbprint".to_owned(), json!(thumbprint));
        Some((cert.not_after, Value::Object(c)))
    }

    /// A property of a store entry: a run of (id u32, reserved u32, length u32, data).
    fn store_property(blob: &[u8], id: u32) -> Option<&[u8]> {
        let mut rest = blob;
        while rest.len() >= 12 {
            let field = |i: usize| u32::from_le_bytes([rest[i], rest[i + 1], rest[i + 2], rest[i + 3]]);
            let (prop, len) = (field(0), field(8) as usize);
            let data = rest.get(12..12 + len)?;
            if prop == id {
                return Some(data);
            }
            rest = &rest[12 + len..];
        }
        None
    }

    fn digits_of(text: &str) -> String {
        text.chars().filter(|c| c.is_ascii_digit()).collect()
    }

    /// Just enough DER to read an X.509 certificate's names, validity and extensions.
    mod der {
        pub struct Certificate<'a> {
            pub subject_cn: Option<String>,
            pub issuer_cn: Option<String>,
            pub not_before: i64,
            pub not_after: i64,
            /// The [3] extensions' content; empty when there are none.
            pub extensions: &'a [u8],
        }

        /// (tag, content, what follows).
        fn tlv(data: &[u8]) -> Option<(u8, &[u8], &[u8])> {
            let (&tag, rest) = data.split_first()?;
            let (&first, rest) = rest.split_first()?;
            let (len, rest) = if first < 0x80 {
                (first as usize, rest)
            } else {
                let n = (first & 0x7f) as usize;
                if n == 0 || n > 4 || rest.len() < n {
                    return None;
                }
                let len = rest[..n].iter().fold(0usize, |len, b| (len << 8) | *b as usize);
                (len, &rest[n..])
            };
            if rest.len() < len {
                return None;
            }
            Some((tag, &rest[..len], &rest[len..]))
        }

        pub fn parse(der: &[u8]) -> Option<Certificate<'_>> {
            let (_, cert, _) = tlv(der).filter(|(tag, _, _)| *tag == 0x30)?;
            let (_, tbs, _) = tlv(cert).filter(|(tag, _, _)| *tag == 0x30)?;
            // The version, or the serial number when the version is left out.
            let (tag, _, mut rest) = tlv(tbs)?;
            if tag == 0xa0 {
                rest = tlv(rest)?.2; // serial number
            }
            let (_, _, rest) = tlv(rest)?; // signature algorithm
            let (_, issuer, rest) = tlv(rest)?;
            let (_, validity, rest) = tlv(rest)?;
            let (_, subject, mut rest) = tlv(rest)?;
            let (nb_tag, nb, after) = tlv(validity)?;
            let (na_tag, na, _) = tlv(after)?;
            let mut extensions: &[u8] = &[];
            while let Some((tag, content, next)) = tlv(rest) {
                if tag == 0xa3 {
                    extensions = content;
                }
                rest = next;
            }
            Some(Certificate {
                subject_cn: common_name(subject),
                issuer_cn: common_name(issuer),
                not_before: time(nb_tag, nb)?,
                not_after: time(na_tag, na)?,
                extensions,
            })
        }

        /// The first CN (2.5.4.3) of a Name.
        fn common_name(name: &[u8]) -> Option<String> {
            let mut sets = name;
            while let Some((_, set, next)) = tlv(sets) {
                let mut atvs = set;
                while let Some((_, atv, after)) = tlv(atvs) {
                    if let Some((0x06, oid, value)) = tlv(atv) {
                        if oid == &[0x55u8, 0x04, 0x03][..] {
                            let (tag, text, _) = tlv(value)?;
                            return Some(text_of(tag, text));
                        }
                    }
                    atvs = after;
                }
                sets = next;
            }
            None
        }

        fn text_of(tag: u8, bytes: &[u8]) -> String {
            match tag {
                // BMPString
                0x1e => String::from_utf16_lossy(
                    &bytes
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]]))
                        .collect::<Vec<u16>>(),
                ),
                // UTF8String
                0x0c => String::from_utf8_lossy(bytes).into_owned(),
                // PrintableString, IA5String, T61String: one byte a character.
                _ => bytes.iter().map(|b| *b as char).collect(),
            }
        }

        /// UTCTime "YYMMDDHHMMSSZ" or GeneralizedTime "YYYYMMDDHHMMSSZ", as unix seconds.
        fn time(tag: u8, bytes: &[u8]) -> Option<i64> {
            let text = std::str::from_utf8(bytes).ok()?;
            let (year, rest) = match tag {
                0x17 => {
                    let yy: i32 = text.get(..2)?.parse().ok()?;
                    (if yy >= 50 { 1900 + yy } else { 2000 + yy }, text.get(2..)?)
                }
                0x18 => (text.get(..4)?.parse::<i32>().ok()?, text.get(4..)?),
                _ => return None,
            };
            let num = |i: usize| -> Option<u32> { rest.get(i..i + 2)?.parse().ok() };
            let at = hbb_common::chrono::NaiveDate::from_ymd_opt(year, num(0)?, num(2)?)?
                .and_hms_opt(num(4)?, num(6)?, num(8).unwrap_or(0))?;
            Some(at.and_utc().timestamp())
        }

        /// Where the OID whose encoding starts with `prefix` sits in `data`: (the OID's
        /// length, the index of its first byte).
        pub fn find_oid(data: &[u8], prefix: &[u8]) -> Option<(usize, usize)> {
            (0..data.len().saturating_sub(prefix.len() + 1)).find_map(|i| {
                let len = data[i + 1] as usize;
                (data[i] == 0x06
                    && len >= prefix.len()
                    && data.get(i + 2..i + 2 + prefix.len()) == Some(prefix))
                .then_some((len, i + 2))
            })
        }

        /// The text of an otherName in the subject alternative name: the OID, then
        /// [0] { a string }.
        pub fn other_name(data: &[u8], oid: &[u8]) -> Option<String> {
            let (len, at) = find_oid(data, oid)?;
            if len != oid.len() {
                return None;
            }
            let (tag, wrapped, _) = tlv(&data[at + len..])?;
            if tag != 0xa0 {
                return None;
            }
            let (tag, text, _) = tlv(wrapped)?;
            Some(text_of(tag, text))
        }
    }

    fn reg_text(key: &RegKey, name: &str) -> Option<String> {
        key.get_value::<String, _>(name)
            .ok()
            .and_then(|v| clip(&v))
    }

    /// "20230515..." (registry dates, WMI CIM_DATETIME) -> "2023-05-15".
    fn yyyymmdd(text: &str) -> Option<String> {
        let digits = text.get(..8)?;
        if !digits.bytes().all(|b| b.is_ascii_digit()) || digits.starts_with("0000") {
            return None;
        }
        Some(format!("{}-{}-{}", &digits[..4], &digits[4..6], &digits[6..8]))
    }

    /// Trimmed, without trailing NULs, at most MAX_TEXT characters; None when empty.
    fn clip(text: &str) -> Option<String> {
        let text = text.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        if text.is_empty() {
            return None;
        }
        Some(text.chars().take(MAX_TEXT).collect())
    }

    fn is_placeholder(text: &str) -> bool {
        let text = text.to_lowercase();
        PLACEHOLDERS.contains(&text.as_str())
    }

    fn put(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
        if let Some(value) = value {
            map.insert(key.to_owned(), json!(value));
        }
    }

    fn put_map(map: &mut Map<String, Value>, key: &str, value: Map<String, Value>) {
        if !value.is_empty() {
            map.insert(key.to_owned(), Value::Object(value));
        }
    }

    /// Just enough WMI: run a WQL query and read the named properties as text.
    mod wbem {
        use super::{clip, is_placeholder};
        use hbb_common::log;
        use std::collections::HashMap;
        use windows::core::{BSTR, PCWSTR, PWSTR};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoSetProxyBlanket,
            CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE,
            RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE,
        };
        use windows::Win32::System::Variant::{
            VariantClear, VariantToStringAlloc, VariantToStringArrayAlloc, VARIANT, VT_ARRAY,
            VT_EMPTY, VT_NULL,
        };
        use windows::Win32::System::Wmi::{
            IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY,
            WBEM_FLAG_RETURN_IMMEDIATELY,
        };

        /// RPC_C_AUTHN_WINNT and RPC_C_AUTHZ_NONE (Win32_System_Rpc, not enabled).
        const AUTHN_WINNT: u32 = 10;
        const AUTHZ_NONE: u32 = 0;
        /// How long one row may take; the licensing provider is slow.
        const NEXT_TIMEOUT_MS: i32 = 60_000;
        const MAX_ROWS: usize = 500;

        /// COM on this (blocking-pool) thread, released when dropped.
        pub struct Com {
            initialized: bool,
        }

        impl Com {
            pub fn init() -> Self {
                let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                if initialized {
                    // Process-wide and settable once; nothing else in the app sets it,
                    // and a refusal (RPC_E_TOO_LATE) is fine: the proxy blankets below
                    // are what WMI looks at.
                    let _ = unsafe {
                        CoInitializeSecurity(
                            None,
                            -1,
                            None,
                            None,
                            RPC_C_AUTHN_LEVEL_DEFAULT,
                            RPC_C_IMP_LEVEL_IMPERSONATE,
                            None,
                            EOAC_NONE,
                            None,
                        )
                    };
                }
                Self { initialized }
            }
        }

        impl Drop for Com {
            fn drop(&mut self) {
                if self.initialized {
                    unsafe { CoUninitialize() };
                }
            }
        }

        /// A row: each property as its texts (one for a value, several for a list).
        #[derive(Default)]
        pub struct Row(HashMap<String, Vec<String>>);

        impl Row {
            pub fn text(&self, prop: &str) -> Option<String> {
                self.0.get(prop)?.first().cloned()
            }

            pub fn texts(&self, prop: &str) -> Vec<String> {
                self.0.get(prop).cloned().unwrap_or_default()
            }

            pub fn number(&self, prop: &str) -> Option<u64> {
                self.text(prop)?.parse().ok()
            }

            /// A firmware text, without the manufacturers' placeholders.
            pub fn oem(&self, prop: &str) -> Option<String> {
                self.text(prop).filter(|t| !is_placeholder(t))
            }
        }

        pub struct Wbem {
            locator: IWbemLocator,
        }

        impl Wbem {
            pub fn new() -> Option<Self> {
                let locator: windows::core::Result<IWbemLocator> =
                    unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) };
                match locator {
                    Ok(locator) => Some(Self { locator }),
                    Err(err) => {
                        log::info!("WMI unavailable: {}", err);
                        None
                    }
                }
            }

            /// None when the query fails (a missing namespace or class included).
            pub fn query(&self, namespace: &str, wql: &str, props: &[&str]) -> Option<Vec<Row>> {
                match unsafe { self.try_query(namespace, wql, props) } {
                    Ok(rows) => Some(rows),
                    Err(err) => {
                        log::info!("WMI query failed ({}): {}", wql, err);
                        None
                    }
                }
            }

            unsafe fn try_query(
                &self,
                namespace: &str,
                wql: &str,
                props: &[&str],
            ) -> windows::core::Result<Vec<Row>> {
                let services: IWbemServices = self.locator.ConnectServer(
                    &BSTR::from(namespace),
                    &BSTR::new(),
                    &BSTR::new(),
                    &BSTR::new(),
                    0,
                    &BSTR::new(),
                    None,
                )?;
                CoSetProxyBlanket(
                    &services,
                    AUTHN_WINNT,
                    AUTHZ_NONE,
                    PCWSTR::null(),
                    RPC_C_AUTHN_LEVEL_CALL,
                    RPC_C_IMP_LEVEL_IMPERSONATE,
                    None,
                    EOAC_NONE,
                )?;
                let found = services.ExecQuery(
                    &BSTR::from("WQL"),
                    &BSTR::from(wql),
                    WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                    None,
                )?;
                let _ = CoSetProxyBlanket(
                    &found,
                    AUTHN_WINNT,
                    AUTHZ_NONE,
                    PCWSTR::null(),
                    RPC_C_AUTHN_LEVEL_CALL,
                    RPC_C_IMP_LEVEL_IMPERSONATE,
                    None,
                    EOAC_NONE,
                );
                let names: Vec<Vec<u16>> = props
                    .iter()
                    .map(|p| p.encode_utf16().chain(Some(0)).collect())
                    .collect();
                let mut rows = Vec::new();
                while rows.len() < MAX_ROWS {
                    let mut objects: [Option<IWbemClassObject>; 1] = Default::default();
                    let mut returned = 0u32;
                    found.Next(NEXT_TIMEOUT_MS, &mut objects, &mut returned).ok()?;
                    let Some(object) = objects[0].take() else {
                        break;
                    };
                    let mut row = Row::default();
                    for (prop, name) in props.iter().zip(&names) {
                        let mut value = VARIANT::default();
                        if object
                            .Get(PCWSTR(name.as_ptr()), 0, &mut value, None, None)
                            .is_ok()
                        {
                            let texts = texts_of(&value, prop.starts_with("__"));
                            if !texts.is_empty() {
                                row.0.insert((*prop).to_owned(), texts);
                            }
                        }
                        let _ = VariantClear(&mut value);
                    }
                    rows.push(row);
                }
                Ok(rows)
            }
        }

        /// A property's value as texts: none for empty or null, several for a list.
        /// `whole` keeps a long text uncut - an object path (__RELPATH) runs past
        /// MAX_TEXT and is no use clipped.
        unsafe fn texts_of(value: &VARIANT, whole: bool) -> Vec<String> {
            let vt = value.Anonymous.Anonymous.vt;
            let mut out = Vec::new();
            if vt.0 == VT_EMPTY.0 || vt.0 == VT_NULL.0 {
                return out;
            }
            if (vt.0 & VT_ARRAY.0) != 0 {
                let mut items: *mut PWSTR = std::ptr::null_mut();
                let mut count = 0u32;
                if VariantToStringArrayAlloc(value, &mut items, &mut count).is_ok()
                    && !items.is_null()
                {
                    for i in 0..count as usize {
                        let item = *items.add(i);
                        if let Some(text) = item.to_string().ok().as_deref().and_then(clip) {
                            out.push(text);
                        }
                        CoTaskMemFree(Some(item.0 as *const _));
                    }
                    CoTaskMemFree(Some(items as *const _));
                }
            } else if let Ok(raw) = VariantToStringAlloc(value) {
                let text = raw.to_string().ok();
                let text = if whole {
                    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
                } else {
                    text.as_deref().and_then(clip)
                };
                if let Some(text) = text {
                    out.push(text);
                }
                CoTaskMemFree(Some(raw.0 as *const _));
            }
            out
        }
    }
}
