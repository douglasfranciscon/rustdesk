//! CUSTOM BRANDING: the machine's status (disks, RAM, last boot) every 3 hours and its
//! inventory (hardware, Windows, antivirus, network, installed programs) once a day,
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
