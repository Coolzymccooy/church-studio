//! Engine start: find the devices by name, plan which bus plays on which
//! device pair, and pick one sample rate for everything connected
//! (`docs/specs/2026-11-20-usb-interfaces.md`).
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{Device, Host, SupportedStreamConfig, SupportedStreamConfigRange};

use super::config::{negotiate_stream_configs, widest_config};
use super::device_names::{device_name, device_uid, listed_name_of, named_devices, ListedDevice};
use super::output_plan::{plan_outputs, BusRequest, OutputPlan};
use super::DeviceSelection;
use crate::dsp::mixer::{BUS_MAIN, BUS_MONITOR, BUS_MONITOR2, BUS_MONITOR3, BUS_STREAM};

/// Everything `AudioEngine::start` opens.
pub(super) struct Resolved {
    pub input_name: String,
    pub in_dev: Device,
    pub in_cfg: SupportedStreamConfig,
    pub plan: OutputPlan,
    /// Per plan group: the device and its config when connected now.
    pub outputs: Vec<Option<(Device, SupportedStreamConfig)>>,
}

/// The listed name a selection refers to: the name itself, or the name in a
/// legacy "index:name" id. A selection that matches nothing is missing; it
/// keeps its name (without a legacy index), so it joins when plugged in.
fn listed_name(listed: &[(String, Device)], selection: &str) -> (String, Option<Device>) {
    if let Some((name, device)) = listed.iter().find(|(name, _)| name == selection) {
        return (name.clone(), Some(device.clone()));
    }
    if let Some((_, legacy)) = selection.split_once(':') {
        if let Some((name, device)) = listed.iter().find(|(name, _)| name == legacy) {
            return (name.clone(), Some(device.clone()));
        }
    }
    (missing_name(selection), None)
}

/// The name a missing selection waits for: a legacy "index:name" id loses
/// its index.
pub(super) fn missing_name(selection: &str) -> String {
    match selection.split_once(':') {
        Some((index, name)) if !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) => name.to_string(),
        _ => selection.to_string(),
    }
}

/// The default device's unique listed name ("X (2)" when it is the second
/// of two identical units), so it groups and reconnects as that unit.
fn default_name(listed: &[(String, Device)], device: &Device, fallback: &str) -> String {
    let entries: Vec<ListedDevice> = listed
        .iter()
        .map(|(name, d)| ListedDevice { name: name.clone(), uid: device_uid(d) })
        .collect();
    let raw = device_name(device).unwrap_or_else(|_| fallback.to_string());
    listed_name_of(&entries, device_uid(device).as_deref(), &raw)
}

fn resolve_input(host: &Host, selection: &Option<String>) -> Result<(String, Device), String> {
    match selection {
        Some(selected) => {
            let listed = named_devices(host, true)?;
            match listed_name(&listed, selected) {
                (name, Some(device)) => Ok((name, device)),
                (name, None) => Err(format!("Input '{name}' is not connected")),
            }
        }
        None => {
            let device = host
                .default_input_device()
                .ok_or_else(|| "No default input device".to_string())?;
            let listed = named_devices(host, true)?;
            Ok((default_name(&listed, &device, "Unknown input"), device))
        }
    }
}

/// The bus requests in opening order: monitor, broadcast, main, mon 2,
/// mon 3. Returns the devices found for them alongside.
fn bus_requests(
    host: &Host,
    devices: &DeviceSelection,
) -> Result<(Vec<BusRequest>, Vec<(String, Device)>), String> {
    let listed = named_devices(host, false)?;
    let mut found: Vec<(String, Device)> = Vec::new();
    let mut requests = Vec::new();
    let mut add = |bus: usize, role: &'static str, name: String, device: Option<Device>, exclusive: bool| {
        if let Some(device) = device.as_ref() {
            if !found.iter().any(|(n, _)| *n == name) {
                found.push((name.clone(), device.clone()));
            }
        }
        requests.push(BusRequest {
            bus,
            role,
            present: device.is_some(),
            name,
            pair: devices.output_pairs[bus],
            exclusive,
        });
    };

    let (monitor_name, monitor_dev) = match devices.monitor_output_id.as_ref() {
        Some(selected) => listed_name(&listed, selected),
        None => {
            let device = host
                .default_output_device()
                .ok_or_else(|| "No default monitor output device".to_string())?;
            (default_name(&listed, &device, "Unknown output"), Some(device))
        }
    };
    add(BUS_MONITOR, "monitor output", monitor_name, monitor_dev, false);
    let optional = [
        (BUS_STREAM, "broadcast output", &devices.broadcast_output_id, false),
        (BUS_MAIN, "main output", &devices.main_output_id, false),
        (BUS_MONITOR2, "monitor 2 output", &devices.monitor2_output_id, true),
        (BUS_MONITOR3, "monitor 3 output", &devices.monitor3_output_id, true),
    ];
    for (bus, role, selection, exclusive) in optional {
        if let Some(selected) = selection.as_ref() {
            let (name, device) = listed_name(&listed, selected);
            add(bus, role, name, device, exclusive);
        }
    }
    Ok((requests, found))
}

fn widen_output(device: &Device, cfg: SupportedStreamConfig) -> SupportedStreamConfig {
    match device.supported_output_configs() {
        Ok(ranges) => widest_config(&ranges.collect::<Vec<SupportedStreamConfigRange>>(), cfg),
        Err(_) => cfg,
    }
}

/// Find every device, plan the outputs and negotiate one rate. A missing
/// input is an error; a missing output is planned but not opened.
pub(super) fn resolve(devices: &DeviceSelection) -> Result<Resolved, String> {
    let host = cpal::default_host();
    let (input_name, in_dev) = resolve_input(&host, &devices.input_id)?;
    let (requests, found) = bus_requests(&host, devices)?;
    let plan = plan_outputs(&requests)?;

    let present: Vec<(usize, &Device, &str)> = plan
        .groups
        .iter()
        .enumerate()
        .filter_map(|(index, group)| {
            found
                .iter()
                .find(|(name, _)| *name == group.name)
                .filter(|_| group.present)
                .map(|(name, device)| (index, device, name.as_str()))
        })
        .collect();
    let out_devs: Vec<(&Device, &str)> = present.iter().map(|(_, d, n)| (*d, *n)).collect();
    let (in_cfg, out_cfgs) = negotiate_stream_configs(&in_dev, &input_name, &out_devs)?;
    let in_cfg = match in_dev.supported_input_configs() {
        Ok(ranges) => widest_config(&ranges.collect::<Vec<_>>(), in_cfg),
        Err(_) => in_cfg,
    };

    let mut outputs: Vec<Option<(Device, SupportedStreamConfig)>> = vec![None; plan.groups.len()];
    for ((index, device, _), cfg) in present.iter().zip(out_cfgs) {
        outputs[*index] = Some(((*device).clone(), widen_output(device, cfg)));
    }
    Ok(Resolved {
        input_name,
        in_dev,
        in_cfg,
        plan,
        outputs,
    })
}
