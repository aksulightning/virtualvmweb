//! Conservative playback-device setup for new and stopped x86 guests.

use crate::{error::{Error, Result}, xml};
use xmltree::{Element, XMLNode};

/// Output-only Intel HDA and a backend that never opens host speakers or microphone.
pub const DEVICES: &str = "<sound model='ich9'><codec type='output'/><audio id='1'/></sound><audio id='1' type='none'/>";

/// Adds playback to an unconfigured guest, preserving existing custom audio configurations.
pub fn enable(root: &mut Element) -> Result<()> {
    let devices = root.get_mut_child("devices")
        .ok_or_else(|| Error::bad("Koneelta puuttuvat laitteet."))?;
    let sounds: Vec<_> = xml::children(devices, "sound").collect();
    let backends: Vec<_> = xml::children(devices, "audio").collect();
    // libvirt may have inserted its implicit none backend even without a sound card.
    let backend_ok = backends.is_empty() || (backends.len() == 1
        && xml::attr(backends[0], "id") == "1"
        && xml::attr(backends[0], "type") == "none");
    let sound_ok = sounds.is_empty() || (sounds.len() == 1
        && xml::attr(sounds[0], "model") == "ich9"
        && sounds[0].get_child("codec").is_some_and(|c| xml::attr(c, "type") == "output")
        && sounds[0].get_child("audio").is_some_and(|a| xml::attr(a, "id") == "1"));
    if !backend_ok || !sound_ok {
        return Err(Error::conflict("Koneella on oma ääniasetus. Sovita sen VNC-äänitausta libvirt-työkalulla."));
    }
    let add_sound = sounds.is_empty();
    let add_backend = backends.is_empty();
    let graphics = devices.children.iter_mut().filter_map(XMLNode::as_mut_element)
        .find(|e| e.name == "graphics" && xml::attr(e, "type") == "vnc")
        .ok_or_else(|| Error::bad("Selaimen ääni vaatii VNC-näytön."))?;
    if let Some(audio) = graphics.get_child("audio") {
        if xml::attr(audio, "id") != "1" {
            return Err(Error::conflict("VNC käyttää omaa äänitaustaa."));
        }
    } else {
        graphics.children.push(XMLNode::Element(xml::parse("<audio id='1'/>")?));
    }
    let template = xml::parse(&format!("<devices>{DEVICES}</devices>"))?;
    for node in template.children {
        if let XMLNode::Element(ref element) = node
            && ((element.name == "sound" && add_sound) || (element.name == "audio" && add_backend))
        {
            devices.children.push(node);
        }
    }
    Ok(())
}

/// Device insertion and preservation regressions.
#[cfg(test)]
mod tests {
    use super::*;

    /// Repeated enablement keeps one device and preserves unrelated disk configuration.
    #[test]
    fn enable_is_idempotent() {
        let mut root = xml::parse("<domain><devices><disk/><graphics type='vnc'/><audio id='1' type='none'/></devices></domain>").unwrap();
        enable(&mut root).unwrap();
        enable(&mut root).unwrap();
        let devices = root.get_child("devices").unwrap();
        assert_eq!(xml::children(devices, "sound").count(), 1);
        assert_eq!(xml::children(devices, "audio").count(), 1);
        assert!(devices.get_child("disk").is_some());
    }

    /// Existing host audio and non-VNC guests are refused instead of silently replaced.
    #[test]
    fn custom_devices_are_preserved() {
        for inner in ["<graphics type='spice'/>", "<graphics type='vnc'/><audio id='1' type='pulseaudio'/>", "<graphics type='vnc'/><sound model='ac97'/>"] {
            let mut root = xml::parse(&format!("<domain><devices>{inner}</devices></domain>")).unwrap();
            let before = root.clone();
            assert!(enable(&mut root).is_err());
            assert_eq!(root, before);
        }
    }
}
