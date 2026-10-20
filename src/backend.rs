//! Serialized libvirt access on blocking workers, with bounded request admission.

use crate::error::{Error, Result};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;
use virt::{connect::Connect, domain::Domain};

/// A shared connection preserves test-driver state and avoids concurrent mutations.
#[derive(Clone)]
pub struct Backend {
    uri: String,
    connection: Arc<Mutex<Option<Connect>>>,
    slots: Arc<Semaphore>,
}

/// Guest facts safe to display in the browser; no raw domain XML is exposed.
#[derive(Serialize)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub state: u32,
    pub active: bool,
    pub persistent: bool,
    pub autostart: bool,
    pub vcpus: u32,
    pub memory_mib: u64,
    pub cpu_time_ns: u64,
}

/// Host summary and current guest inventory returned by one serialized query.
#[derive(Serialize)]
pub struct Overview {
    pub hostname: String,
    pub cpus: u32,
    pub memory_mib: u64,
    pub free_mib: Option<u64>,
    pub machines: Vec<Machine>,
}

impl Backend {
    /// Defers the connection until a request so the UI can report daemon outages.
    pub fn new(uri: String) -> Self {
        Self {
            uri,
            connection: Arc::new(Mutex::new(None)),
            slots: Arc::new(Semaphore::new(8)),
        }
    }

    /// Runs a synchronous operation without blocking Tokio's HTTP worker threads.
    pub async fn run<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Connect) -> Result<T> + Send + 'static,
    {
        let permit =
            self.slots.clone().try_acquire_owned().map_err(|_| {
                Error::conflict("Palvelu on varattu. Yritä hetken kuluttua uudelleen.")
            })?;
        let connection = self.connection.clone();
        let uri = self.uri.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let mut guard = connection.lock().map_err(Error::internal)?;
            if guard
                .as_ref()
                .is_some_and(|conn| !conn.is_alive().unwrap_or(false))
            {
                *guard = None;
            }
            if guard.is_none() {
                *guard = Some(Connect::open(Some(&uri))?);
            }
            operation(
                guard
                    .as_ref()
                    .ok_or_else(|| Error::internal("missing connection"))?,
            )
        })
        .await
        .map_err(Error::internal)?
    }
}

/// Rejects malformed identifiers before passing them to the C library.
pub fn domain(conn: &Connect, id: &str) -> Result<Domain> {
    uuid::Uuid::parse_str(id).map_err(|_| Error::bad("Virheellinen koneen tunniste."))?;
    Ok(Domain::lookup_by_uuid_string(conn, id)?)
}

/// Collects inexpensive domain counters; memory is assigned RAM, not guest usage.
pub fn machine(dom: &Domain) -> Result<Machine> {
    let info = dom.get_info()?;
    let persistent = dom.is_persistent()?;
    Ok(Machine {
        id: dom.get_uuid_string()?,
        name: dom.get_name()?,
        state: info.state,
        active: dom.is_active()?,
        persistent,
        autostart: persistent && dom.get_autostart()?,
        vcpus: info.nr_virt_cpu,
        memory_mib: info.memory / 1024,
        cpu_time_ns: info.cpu_time,
    })
}

/// Reports a coherent host snapshot, tolerating guests disappearing during enumeration.
pub fn overview(conn: &Connect) -> Result<Overview> {
    let node = conn.get_node_info()?;
    let mut machines = Vec::new();
    for dom in conn.list_all_domains(0)? {
        match machine(&dom) {
            Ok(value) => machines.push(value),
            Err(error) => tracing::warn!(?error, "guest changed during inventory"),
        }
    }
    machines.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Overview {
        hostname: conn.get_hostname()?,
        cpus: node.cpus,
        memory_mib: node.memory / 1024,
        free_mib: conn.get_free_memory().ok().map(|bytes| bytes / 1024 / 1024),
        machines,
    })
}

/// Applies only explicitly supported actions and checks the current guest state.
pub fn action(conn: &Connect, id: &str, action: &str, confirmation: &str) -> Result<()> {
    let dom = domain(conn, id)?;
    let info = machine(&dom)?;
    match action {
        "start" if !info.active => {
            dom.create()?;
        }
        "shutdown" if info.active => {
            dom.shutdown()?;
        }
        "reboot" if info.active => {
            dom.reboot(0)?;
        }
        "pause" if info.state == 1 => {
            dom.suspend()?;
        }
        "resume" if info.state == 3 => {
            dom.resume()?;
        }
        "autostart-on" if info.persistent => {
            dom.set_autostart(true)?;
        }
        "autostart-off" if info.persistent => {
            dom.set_autostart(false)?;
        }
        "force-off" if info.active && confirmation == info.name => {
            dom.destroy()?;
        }
        "force-reset" if info.state == 1 && confirmation == info.name => {
            dom.reset()?;
        }
        "undefine" if !info.active && info.persistent && confirmation == info.name => {
            if dom.has_managed_save(0)? || !dom.list_all_snapshots(0)?.is_empty() {
                return Err(Error::conflict(
                    "Poista tallennetut tilat ja snapshotit ensin libvirt-työkaluilla.",
                ));
            }
            let root = crate::xml::parse(&dom.get_xml_desc(virt::sys::VIR_DOMAIN_XML_INACTIVE)?)?;
            if root
                .get_child("os")
                .is_some_and(|os| os.get_child("nvram").is_some())
            {
                // Preserve firmware variables just like guest disks; never remove them implicitly.
                dom.undefine_flags(virt::sys::VIR_DOMAIN_UNDEFINE_KEEP_NVRAM)?;
            } else {
                dom.undefine()?;
            }
        }
        _ => {
            return Err(Error::conflict(
                "Toiminto ei sovi koneen nykyiseen tilaan tai vahvistus puuttuu.",
            ));
        }
    }
    tracing::info!(guest = id, action, "administrator changed guest");
    Ok(())
}
