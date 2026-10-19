//! Validated ISO selection, transactional guest creation and offline configuration.

use crate::{
    backend,
    error::{Error, Result},
    views::escape,
    xml,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use virt::{
    connect::Connect, domain::Domain, network::Network, storage_pool::StoragePool,
    storage_vol::StorageVol,
};

/// Creation fields are revalidated on the final POST, not trusted from the preview.
#[derive(Clone, Deserialize)]
pub struct Create {
    pub csrf: String,
    pub name: String,
    pub cpus: u32,
    pub memory_mib: u64,
    pub disk_gib: u64,
    pub pool: String,
    pub network: String,
    pub iso: String,
    #[serde(default)]
    pub confirm: String,
}

/// Offline edits deliberately exclude arbitrary XML and advanced topology changes.
#[derive(Deserialize)]
pub struct Edit {
    pub csrf: String,
    pub cpus: u32,
    pub memory_mib: u64,
    pub iso: String,
    #[serde(default)]
    pub change_iso: bool,
}

/// Checks conservative bounds before building XML or allocating storage.
pub fn resources(cpus: u32, memory_mib: u64) -> Result<()> {
    if !(1..=256).contains(&cpus) || !(128..=1_048_576).contains(&memory_mib) {
        return Err(Error::bad(
            "CPU-määrän on oltava 1–256 ja muistin 128–1048576 MiB.",
        ));
    }
    Ok(())
}

/// Resolves a single file name beneath a trusted ISO root, rejecting traversal and escaping symlinks.
pub fn iso_path(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || !name.to_ascii_lowercase().ends_with(".iso")
    {
        return Err(Error::bad(
            "Valitse ISO-tiedosto määritetystä hakemistosta.",
        ));
    }
    let root = root.canonicalize().map_err(Error::internal)?;
    let path = root
        .join(name)
        .canonicalize()
        .map_err(|_| Error::bad("ISO-tiedostoa ei löytynyt."))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err(Error::bad(
            "ISO-tiedoston on oltava sallitussa hakemistossa.",
        ));
    }
    Ok(path)
}

/// Lists valid ISO names only; the ISO directory must be controlled by the host administrator.
pub fn isos(root: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root).map_err(Error::internal)? {
        let entry = entry.map_err(Error::internal)?;
        if let Some(name) = entry.file_name().to_str()
            && iso_path(root, name).is_ok()
        {
            names.push(name.to_string());
        }
    }
    names.sort();
    Ok(names)
}

/// Validates the user's requested name and resources before showing a summary.
pub fn validate(input: &Create, iso_dir: &Path) -> Result<()> {
    resources(input.cpus, input.memory_mib)?;
    if input.name.is_empty()
        || input.name.len() > 64
        || !input
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    {
        return Err(Error::bad(
            "Nimessä voi olla 1–64 merkkiä: a–z, A–Z, 0–9, viiva, piste ja alaviiva.",
        ));
    }
    if !(1..=16_384).contains(&input.disk_gib) {
        return Err(Error::bad("Levyn koon on oltava 1–16384 GiB."));
    }
    iso_path(iso_dir, &input.iso)?;
    Ok(())
}

/// Constructs our small x86_64 Q35/BIOS guest profile with a libvirt-only VNC listener.
pub fn domain_xml(input: &Create, id: &str, disk: &str, iso: &str) -> String {
    format!(
        r#"<domain type='kvm'><name>{}</name><uuid>{id}</uuid>
<memory unit='MiB'>{}</memory><currentMemory unit='MiB'>{}</currentMemory><vcpu>{}</vcpu>
<os><type arch='x86_64' machine='q35'>hvm</type><boot dev='hd'/><boot dev='cdrom'/><bootmenu enable='yes'/></os>
<features><acpi/><apic/></features><cpu mode='host-model'/><clock offset='utc'/>
<on_poweroff>destroy</on_poweroff><on_reboot>restart</on_reboot><on_crash>destroy</on_crash>
<devices><disk type='file' device='disk'><driver name='qemu' type='qcow2'/><source file='{}'/><target dev='vda' bus='virtio'/></disk>
<disk type='file' device='cdrom'><driver name='qemu' type='raw'/><source file='{}'/><target dev='sda' bus='sata'/><readonly/></disk>
<interface type='network'><source network='{}'/><model type='virtio'/></interface>
<graphics type='vnc'><listen type='none'/></graphics><video><model type='vga'/></video>
<input type='tablet' bus='usb'/><controller type='usb' model='qemu-xhci'/><memballoon model='virtio'/>
<channel type='unix'><target type='virtio' name='org.qemu.guest_agent.0'/></channel>
</devices></domain>"#,
        escape(&input.name),
        input.memory_mib,
        input.memory_mib,
        input.cpus,
        escape(disk),
        escape(iso),
        escape(&input.network)
    )
}

/// Creates a new disk and persistent guest; rolls the disk back if domain definition fails.
pub fn create(conn: &Connect, input: &Create, iso_dir: &Path) -> Result<String> {
    validate(input, iso_dir)?;
    if input.confirm != "yes" {
        return Err(Error::bad("Vahvista asetukset ennen luontia."));
    }
    if conn
        .list_all_domains(0)?
        .iter()
        .any(|d| d.get_name().is_ok_and(|name| name == input.name))
    {
        return Err(Error::conflict("Samanniminen kone on jo olemassa."));
    }
    let pool = StoragePool::lookup_by_name(conn, &input.pool)?;
    if !pool.is_active()? || xml::attr(&xml::parse(&pool.get_xml_desc(0)?)?, "type") != "dir" {
        return Err(Error::bad("Valitse aktiivinen hakemistopooli."));
    }
    if !Network::lookup_by_name(conn, &input.network)?.is_active()? {
        return Err(Error::conflict("Valitun verkon on oltava aktiivinen."));
    }
    pool.refresh(0)?;
    if pool.get_info()?.available < input.disk_gib * 1024 * 1024 * 1024 {
        return Err(Error::conflict(
            "Poolissa ei ole riittävästi vapaata tilaa levyn enimmäiskoolle.",
        ));
    }
    let iso = iso_path(iso_dir, &input.iso)?;
    let iso = iso
        .to_str()
        .ok_or_else(|| Error::bad("ISO-polun on oltava UTF-8-muotoinen."))?;
    let id = uuid::Uuid::new_v4().to_string();
    let volume_xml = format!(
        "<volume><name>virtualvmweb-{id}.qcow2</name><capacity unit='GiB'>{}</capacity><allocation>0</allocation><target><format type='qcow2'/></target></volume>",
        input.disk_gib
    );
    let volume = StorageVol::create_xml(&pool, &volume_xml, 0)?;
    let result = (|| -> Result<String> {
        let disk = volume.get_path()?;
        Domain::define_xml(conn, &domain_xml(input, &id, &disk, iso))?;
        Ok(id.clone())
    })();
    if result.is_err() {
        if let Err(error) = volume.delete(0) {
            tracing::error!(%error, volume = %format!("virtualvmweb-{id}.qcow2"), "creation rollback failed; manual disk cleanup required");
            return Err(Error::conflict(&format!(
                "Luonti epäonnistui ja levy virtualvmweb-{id}.qcow2 jäi pooliin. Tarkista loki ja Tallennus-sivu."
            )));
        }
    } else {
        tracing::info!(guest = %id, "administrator created guest; left shut off");
    }
    result
}

/// Updates persistent configuration only after checking that no saved runtime state exists.
pub fn edit(conn: &Connect, id: &str, input: &Edit, iso_dir: &Path) -> Result<()> {
    resources(input.cpus, input.memory_mib)?;
    let dom = backend::domain(conn, id)?;
    if dom.is_active()? || !dom.is_persistent()? || dom.has_managed_save(0)? {
        return Err(Error::conflict(
            "Muokkaus vaatii sammutetun pysyvän koneen ilman tallennettua ajotilaa.",
        ));
    }
    let mut root = xml::parse(&dom.get_xml_desc(virt::sys::VIR_DOMAIN_XML_INACTIVE)?)?;
    xml::edit_resources(&mut root, input.cpus, input.memory_mib)?;
    if input.change_iso {
        let path = if input.iso.is_empty() {
            None
        } else {
            Some(iso_path(iso_dir, &input.iso)?)
        };
        let path = path
            .as_ref()
            .map(|p| {
                p.to_str()
                    .ok_or_else(|| Error::bad("ISO-polun on oltava UTF-8-muotoinen."))
            })
            .transpose()?;
        xml::edit_cdrom(&mut root, path)?;
    }
    Domain::define_xml(conn, &xml::serialize(&root)?)?;
    tracing::info!(guest = id, "administrator updated offline configuration");
    Ok(())
}
