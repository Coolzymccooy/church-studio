//! Device names the operator sees and the engine opens by
//! (`docs/specs/2026-11-20-usb-interfaces.md` §1).
//!
//! Ids are names, so a saved choice is found again after a re-plug or a
//! reboot. Two identical interfaces get " (2)", " (3)" by enumeration order;
//! the same rule opens a device, so a listed id always opens that device.
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, InterfaceType, SupportedStreamConfigRange};

use crate::dsp::AudioDeviceInfo;

/// `names` made unique: the first keeps its name, later copies get
/// " (n)", skipping any name already taken.
pub fn unique_names(names: &[String]) -> Vec<String> {
    let mut taken: Vec<String> = Vec::with_capacity(names.len());
    for name in names {
        let unique = if taken.contains(name) {
            next_free(name, names, &taken)
        } else {
            name.clone()
        };
        taken.push(unique);
    }
    taken
}

/// The first "`name` (n)" (n ≥ 2) that is neither taken nor a real name.
fn next_free(name: &str, names: &[String], taken: &[String]) -> String {
    (2..)
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| !taken.contains(candidate) && !names.contains(candidate))
        .unwrap_or_else(|| name.to_string())
}

/// The short transport word for the device list.
pub fn transport_word(interface: InterfaceType) -> &'static str {
    match interface {
        InterfaceType::Usb => "usb",
        InterfaceType::BuiltIn => "builtin",
        InterfaceType::Bluetooth => "bluetooth",
        InterfaceType::Virtual => "virtual",
        InterfaceType::Aggregate => "aggregate",
        InterfaceType::Hdmi | InterfaceType::DisplayPort => "hdmi",
        InterfaceType::Thunderbolt => "thunderbolt",
        InterfaceType::Pci => "pci",
        InterfaceType::Network => "network",
        _ => "other",
    }
}

/// A device's display name (cpal 0.18 keeps it in `description()`).
pub fn device_name(device: &Device) -> Result<String, String> {
    device
        .description()
        .map(|description| description.name().to_string())
        .map_err(|e| e.to_string())
}

/// Every input (or output) device with its unique name, in host order.
/// Devices whose name cannot be read are left out.
pub fn named_devices(host: &Host, input: bool) -> Result<Vec<(String, Device)>, String> {
    let devices: Vec<Device> = if input {
        host.input_devices().map_err(|e| e.to_string())?.collect()
    } else {
        host.output_devices().map_err(|e| e.to_string())?.collect()
    };
    let named: Vec<(String, Device)> = devices
        .into_iter()
        .filter_map(|device| device_name(&device).ok().map(|name| (name, device)))
        .collect();
    let raw: Vec<String> = named.iter().map(|(name, _)| name.clone()).collect();
    Ok(unique_names(&raw)
        .into_iter()
        .zip(named.into_iter().map(|(_, device)| device))
        .collect())
}

/// One listed device: its unique name and its platform id (Core Audio UID,
/// WASAPI endpoint id), which tells identical interfaces apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedDevice {
    pub name: String,
    pub uid: Option<String>,
}

/// A device's platform id, when the host gives one.
pub fn device_uid(device: &Device) -> Option<String> {
    device.id().ok().map(|id| id.id().to_string())
}

/// The listed devices, or `None` when the host could not list them (a
/// failed listing is not an unplug).
pub fn device_entries(host: &Host, input: bool) -> Option<Vec<ListedDevice>> {
    named_devices(host, input).ok().map(|list| {
        list.into_iter()
            .map(|(name, device)| ListedDevice { uid: device_uid(&device), name })
            .collect()
    })
}

/// Just the unique names.
#[cfg(test)]
pub fn device_names(host: &Host, input: bool) -> Vec<String> {
    device_entries(host, input)
        .map(|list| list.into_iter().map(|d| d.name).collect())
        .unwrap_or_default()
}

/// The listed device a stream belongs on. The device it was opened on (by
/// id) wins; otherwise one with its name, unless that one is already in use
/// by another of the engine's streams (`taken` ids), so a bus never moves
/// onto an identical interface another bus plays on.
pub fn match_listed<'a>(
    name: &str,
    bound: Option<&str>,
    listed: &'a [ListedDevice],
    taken: &[String],
) -> Option<&'a ListedDevice> {
    if let Some(uid) = bound {
        if let Some(device) = listed.iter().find(|d| d.uid.as_deref() == Some(uid)) {
            return Some(device);
        }
    }
    listed.iter().find(|d| {
        d.name == name && d.uid.as_ref().is_none_or(|uid| !taken.contains(uid))
    })
}

/// Find a device by its unique name (or a legacy "index:name" id).
#[cfg(test)]
pub fn find_device(host: &Host, input: bool, selection: &str) -> Result<Option<(String, Device)>, String> {
    let legacy = selection.split_once(':').map(|(_, name)| name);
    Ok(named_devices(host, input)?
        .into_iter()
        .find(|(name, _)| name == selection || Some(name.as_str()) == legacy))
}

/// Find the device the watcher chose: by id when it has one, else by name.
pub fn find_target(host: &Host, input: bool, target: &ListedDevice) -> Result<Option<Device>, String> {
    let listed = named_devices(host, input)?;
    let by_uid = target.uid.as_ref().and_then(|uid| {
        listed
            .iter()
            .find(|(_, device)| device_uid(device).as_ref() == Some(uid))
            .map(|(_, device)| device.clone())
    });
    Ok(by_uid.or_else(|| {
        listed
            .into_iter()
            .find(|(name, _)| *name == target.name)
            .map(|(_, device)| device)
    }))
}

fn most_channels(ranges: impl Iterator<Item = SupportedStreamConfigRange>) -> u16 {
    ranges.map(|range| range.channels()).max().unwrap_or(0)
}

/// The device list for `list_input_devices` / `list_output_devices` and
/// the `audio-devices-changed` event.
pub fn list_devices(input: bool) -> Vec<AudioDeviceInfo> {
    let host = cpal::default_host();
    let default_name = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
    .and_then(|d| device_name(&d).ok())
    .unwrap_or_default();
    let Ok(named) = named_devices(&host, input) else {
        return Vec::new();
    };
    let mut default_seen = false;
    named
        .into_iter()
        .map(|(name, device)| {
            let channels = if input {
                device.supported_input_configs().map(most_channels).unwrap_or(0)
            } else {
                device.supported_output_configs().map(most_channels).unwrap_or(0)
            };
            let transport = device
                .description()
                .map(|d| transport_word(d.interface_type()))
                .unwrap_or("other");
            // Only the first device with the default's name is the default.
            let is_default = !default_seen && name == default_name;
            default_seen |= is_default;
            AudioDeviceInfo {
                id: name.clone(),
                name,
                is_default,
                channels,
                transport: transport.to_string(),
                present: true,
            }
        })
        .collect()
}
