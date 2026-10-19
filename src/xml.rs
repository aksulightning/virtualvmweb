//! Domain XML helpers that preserve unrelated devices and escape inserted values.

use xmltree::{Element, XMLNode};
use crate::error::{Error, Result};

/// Parses XML from libvirt; external entity expansion is not enabled.
pub fn parse(text: &str) -> Result<Element> {
    Element::parse(text.as_bytes()).map_err(Error::internal)
}

/// Serializes a modified tree as UTF-8 without manually concatenating user XML.
pub fn serialize(root: &Element) -> Result<String> {
    let mut out = Vec::new();
    root.write(&mut out).map_err(Error::internal)?;
    String::from_utf8(out).map_err(Error::internal)
}

/// Reads a named attribute without allocating a fallback string.
pub fn attr<'a>(node: &'a Element, name: &str) -> &'a str {
    node.attributes.get(name).map_or("", String::as_str)
}

/// Iterates only element children with the requested name.
pub fn children<'a>(node: &'a Element, name: &'a str) -> impl Iterator<Item = &'a Element> {
    node.children.iter().filter_map(XMLNode::as_element).filter(move |el| el.name == name)
}

/// Replaces one scalar configuration value and makes its unit unambiguous.
fn set_text(root: &mut Element, key: &str, value: String, unit: Option<&str>) {
    if root.get_child(key).is_none() {
        root.children.push(XMLNode::Element(Element::new(key)));
    }
    if let Some(child) = root.get_mut_child(key) {
        child.children = vec![XMLNode::Text(value)];
        if let Some(unit) = unit {
            child.attributes.insert("unit".into(), unit.into());
        }
    }
}

/// Edits simple offline CPU/RAM settings, refusing advanced layouts we cannot preserve safely.
pub fn edit_resources(root: &mut Element, cpus: u32, memory_mib: u64) -> Result<()> {
    if root.get_child("vcpus").is_some() || root.get_child("maxMemory").is_some()
        || root.get_child("cpu").is_some_and(|cpu| cpu.get_child("topology").is_some() || cpu.get_child("numa").is_some()) {
        return Err(Error::conflict("Tämän koneen CPU/NUMA/muistilaajennusasetukset vaativat libvirt-työkalun."));
    }
    set_text(root, "vcpu", cpus.to_string(), None);
    if let Some(vcpu) = root.get_mut_child("vcpu") {
        vcpu.attributes.remove("current");
    }
    set_text(root, "memory", memory_mib.to_string(), Some("MiB"));
    set_text(root, "currentMemory", memory_mib.to_string(), Some("MiB"));
    Ok(())
}

/// Changes the first file-backed CD-ROM without replacing other devices or boot rules.
pub fn edit_cdrom(root: &mut Element, path: Option<&str>) -> Result<()> {
    let devices = root.get_mut_child("devices").ok_or_else(|| Error::bad("Koneelta puuttuvat laitteet."))?;
    let disk = devices.children.iter_mut().filter_map(XMLNode::as_mut_element)
        .find(|el| el.name == "disk" && attr(el, "device") == "cdrom" && attr(el, "type") == "file")
        .ok_or_else(|| Error::conflict("Koneella ei ole muokattavaa tiedostopohjaista CD-ROM-asemaa."))?;
    disk.children.retain(|node| !node.as_element().is_some_and(|el| el.name == "source"));
    if let Some(path) = path {
        let mut source = Element::new("source");
        source.attributes.insert("file".into(), path.into());
        disk.children.push(XMLNode::Element(source));
    }
    Ok(())
}

/// Lists disk and network descriptions without exposing XML or graphical passwords.
pub fn hardware(root: &Element) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if let Some(devices) = root.get_child("devices") {
        for disk in children(devices, "disk") {
            let source = disk.get_child("source").map(|s| ["file", "dev", "volume", "name"].iter().map(|key| attr(s, key)).find(|s| !s.is_empty()).unwrap_or("")).unwrap_or("");
            rows.push((format!("Levy / {}", attr(disk, "device")), source.to_string()));
        }
        for nic in children(devices, "interface") {
            let source = nic.get_child("source").map(|s| ["network", "bridge", "dev"].iter().map(|key| attr(s, key)).find(|s| !s.is_empty()).unwrap_or("")).unwrap_or("");
            let mac = nic.get_child("mac").map_or("", |s| attr(s, "address"));
            rows.push((format!("Verkko / {}", attr(nic, "type")), format!("{source} · {mac}")));
        }
    }
    rows
}

/// Finds the actual graphics-device index for libvirt's authenticated socket handoff.
pub fn vnc_index(root: &Element) -> Option<u32> {
    let devices = root.get_child("devices")?;
    children(devices, "graphics").position(|el| attr(el, "type") == "vnc").map(|i| i as u32)
}
