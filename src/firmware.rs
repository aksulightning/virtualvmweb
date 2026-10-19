//! Typed guest firmware selection and host capability checks for the Q35 profile.

use crate::{
    error::{Error, Result},
    xml,
};
use serde::Deserialize;
use virt::connect::Connect;

/// BIOS stays the default for existing clients; unknown form values are rejected by Serde.
#[derive(Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Firmware {
    #[default]
    Bios,
    Uefi,
}

impl Firmware {
    /// Returns the stable form value carried through the confirmation step.
    pub fn value(self) -> &'static str {
        match self {
            Self::Bios => "bios",
            Self::Uefi => "uefi",
        }
    }

    /// Returns the user-facing firmware name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Bios => "BIOS",
            Self::Uefi => "UEFI",
        }
    }

    /// Lets libvirt choose installed UEFI code and per-guest NVRAM, without Secure Boot.
    pub fn os_open(self) -> &'static str {
        match self {
            Self::Bios => "<os>",
            Self::Uefi => concat!(
                "<os firmware='efi'><firmware>",
                "<feature enabled='no' name='secure-boot'/></firmware>"
            ),
        }
    }
}

/// Checks the firmware enum advertised for this precise architecture/machine profile.
pub fn supports_uefi(capabilities: &str) -> Result<bool> {
    let root = xml::parse(capabilities)?;
    let Some(os) = root.get_child("os") else {
        return Ok(false);
    };
    Ok(xml::attr(os, "supported") == "yes"
        && xml::children(os, "enum").any(|values| {
            xml::attr(values, "name") == "firmware"
                && xml::children(values, "value").any(|v| v.get_text().is_some_and(|s| s == "efi"))
        }))
}

/// Disables UEFI when the host cannot advertise it, without breaking BIOS/test-driver pages.
pub fn available(conn: &Connect) -> bool {
    let result =
        conn.get_domain_capabilities(None, Some("x86_64"), Some("q35"), Some("kvm"), 0);
    match result {
        Ok(text) => supports_uefi(&text).unwrap_or(false),
        Err(error) => {
            tracing::warn!(%error, "UEFI capability check unavailable");
            false
        }
    }
}

/// Rechecks UEFI availability before preview and before allocating any disk.
pub fn validate(conn: &Connect, firmware: Firmware) -> Result<()> {
    if firmware == Firmware::Uefi && !available(conn) {
        return Err(Error::bad("UEFI ei ole käytettävissä. Asenna palvelimen OVMF-firmware ja tarkista libvirtin Q35/KVM-tuki."));
    }
    Ok(())
}

/// Exercises supported, missing and unrelated firmware capability data.
#[cfg(test)]
mod tests {
    use super::*;

    /// Only an enabled OS firmware enum advertising EFI enables the choice.
    #[test]
    fn firmware_capabilities_are_explicit() {
        let supported = "<domainCapabilities><os supported='yes'><enum name='firmware'><value>bios</value><value>efi</value></enum></os></domainCapabilities>";
        assert!(supports_uefi(supported).unwrap());
        assert!(!supports_uefi(&supported.replace("efi", "bios")).unwrap());
        assert!(!supports_uefi(&supported.replace("supported='yes'", "supported='no'")).unwrap());
        assert!(!supports_uefi("<domainCapabilities/>").unwrap());
        assert!(supports_uefi("invalid xml").is_err());
    }
}

