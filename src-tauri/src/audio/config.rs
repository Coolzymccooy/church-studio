//! Stream configs: one common sample rate for every open device, the widest
//! channel layout at that rate, the input channel range, and a clear error
//! when the devices share no rate (`docs/specs/2026-11-20-usb-interfaces.md`
//! §4, §5).
use cpal::traits::DeviceTrait;
use cpal::{Device, SupportedStreamConfig, SupportedStreamConfigRange};
use std::cmp::{max, min};

/// A device's name and its supported rate ranges, for the error message.
#[derive(Debug, Clone)]
pub struct RateSpan {
    pub name: String,
    pub ranges: Vec<(u32, u32)>,
}

impl RateSpan {
    pub fn new(name: &str, ranges: &[(u32, u32)]) -> Self {
        RateSpan {
            name: name.to_string(),
            ranges: ranges.to_vec(),
        }
    }

    fn of(name: &str, ranges: &[SupportedStreamConfigRange]) -> Self {
        let spans: Vec<(u32, u32)> = ranges
            .iter()
            .map(|r| (r.min_sample_rate(), r.max_sample_rate()))
            .collect();
        RateSpan::new(name, &spans)
    }
}

fn khz(rate: u32) -> String {
    let value = rate as f64 / 1000.0;
    if (value - value.round()).abs() < 1e-9 {
        format!("{}", value.round() as u64)
    } else {
        format!("{value:.1}")
    }
}

fn describe_rates(ranges: &[(u32, u32)]) -> String {
    let mut spans: Vec<(u32, u32)> = ranges.to_vec();
    spans.sort_unstable();
    spans.dedup();
    let parts: Vec<String> = spans
        .iter()
        .map(|&(lo, hi)| if lo == hi { khz(lo) } else { format!("{}-{}", khz(lo), khz(hi)) })
        .collect();
    if parts.is_empty() {
        "no rates".to_string()
    } else {
        format!("{} kHz", parts.join(", "))
    }
}

/// "No common sample rate: A (44.1-192 kHz), B (24 kHz)".
pub fn no_common_rate_error(devices: &[RateSpan]) -> String {
    let list: Vec<String> = devices
        .iter()
        .map(|d| format!("{} ({})", d.name, describe_rates(&d.ranges)))
        .collect();
    format!("No common sample rate: {}", list.join(", "))
}

/// The input channel range as (first, count), 0-based. `None` = every
/// channel. A count past the end is cut; a first channel past the end is
/// refused naming the device.
pub fn resolve_input_range(
    requested: Option<(usize, Option<usize>)>,
    channels: usize,
    device: &str,
) -> Result<(usize, usize), String> {
    let channels = channels.max(1);
    let (first, count) = requested.unwrap_or((0, None));
    if first >= channels {
        return Err(format!(
            "Input '{device}' has {channels} channels; channel {} is not on it. Choose a lower first channel.",
            first + 1
        ));
    }
    let room = channels - first;
    let count = count.map(|c| c.clamp(1, room)).unwrap_or(room);
    Ok((first, count))
}

/// One common rate for the input and every output, preferring the input's
/// default, then 48 kHz, then 44.1 kHz.
pub fn negotiate_stream_configs(
    in_dev: &Device,
    in_name: &str,
    out_devs: &[(&Device, &str)],
) -> Result<(SupportedStreamConfig, Vec<SupportedStreamConfig>), String> {
    let in_default = in_dev.default_input_config().map_err(|e| e.to_string())?;
    let out_defaults: Vec<SupportedStreamConfig> = out_devs
        .iter()
        .map(|(device, _)| device.default_output_config().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;

    if out_defaults
        .iter()
        .all(|cfg| cfg.sample_rate() == in_default.sample_rate())
    {
        return Ok((in_default, out_defaults));
    }

    let in_ranges: Vec<SupportedStreamConfigRange> = in_dev
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let out_ranges_sets: Vec<Vec<SupportedStreamConfigRange>> = out_devs
        .iter()
        .map(|(device, _)| {
            device
                .supported_output_configs()
                .map_err(|e| e.to_string())
                .map(|iter| iter.collect::<Vec<_>>())
        })
        .collect::<Result<_, _>>()?;

    if in_ranges.is_empty() || out_ranges_sets.iter().any(|ranges| ranges.is_empty()) {
        return Err("Selected input and output devices do not expose any supported audio stream configuration".to_string());
    }

    let mut preferred_rates = vec![in_default.sample_rate(), 48_000, 44_100];
    preferred_rates.extend(out_defaults.iter().map(|cfg| cfg.sample_rate()));
    preferred_rates.sort_unstable();
    preferred_rates.dedup();

    for rate in preferred_rates {
        let Some(input_cfg) = find_config_for_rate(&in_ranges, rate) else {
            continue;
        };
        let outputs: Option<Vec<_>> = out_ranges_sets
            .iter()
            .map(|ranges| find_config_for_rate(ranges, rate))
            .collect();
        if let Some(output_cfgs) = outputs {
            return Ok((input_cfg, output_cfgs));
        }
    }

    for input_cfg in &in_ranges {
        let min_rate = out_ranges_sets.iter().fold(input_cfg.min_sample_rate(), |current, ranges| {
            max(current, ranges.iter().map(|cfg| cfg.min_sample_rate()).min().unwrap_or(current))
        });
        let max_rate = out_ranges_sets.iter().fold(input_cfg.max_sample_rate(), |current, ranges| {
            min(current, ranges.iter().map(|cfg| cfg.max_sample_rate()).max().unwrap_or(current))
        });
        if min_rate <= max_rate {
            let target_rate = preferred_rate_in_range(min_rate, max_rate);
            let outputs: Option<Vec<_>> = out_ranges_sets
                .iter()
                .map(|ranges| find_config_for_rate(ranges, target_rate))
                .collect();
            if let Some(output_cfgs) = outputs {
                return Ok((input_cfg.with_sample_rate(target_rate), output_cfgs));
            }
        }
    }

    let mut spans = vec![RateSpan::of(in_name, &in_ranges)];
    spans.extend(
        out_devs
            .iter()
            .zip(out_ranges_sets.iter())
            .map(|((_, name), ranges)| RateSpan::of(name, ranges)),
    );
    Err(no_common_rate_error(&spans))
}

/// `chosen`, widened to the most channels the device offers at the same
/// rate and sample format. Some devices (a macOS aggregate device of two USB
/// mics, a multi-output interface) report a 1- or 2-channel default config
/// while supporting every channel, and each channel must be reachable.
pub fn widest_config(
    ranges: &[SupportedStreamConfigRange],
    chosen: SupportedStreamConfig,
) -> SupportedStreamConfig {
    let rate = chosen.sample_rate();
    ranges
        .iter()
        .filter(|r| {
            r.sample_format() == chosen.sample_format()
                && rate >= r.min_sample_rate()
                && rate <= r.max_sample_rate()
                && r.channels() > chosen.channels()
        })
        .max_by_key(|r| r.channels())
        .map(|r| r.with_sample_rate(rate))
        .unwrap_or(chosen)
}

pub fn find_config_for_rate(
    ranges: &[SupportedStreamConfigRange],
    rate: u32,
) -> Option<SupportedStreamConfig> {
    ranges
        .iter()
        .find(|cfg| rate >= cfg.min_sample_rate() && rate <= cfg.max_sample_rate())
        .map(|cfg| cfg.with_sample_rate(rate))
}

fn preferred_rate_in_range(min_rate: u32, max_rate: u32) -> u32 {
    if (min_rate..=max_rate).contains(&48_000) {
        48_000
    } else if (min_rate..=max_rate).contains(&44_100) {
        44_100
    } else {
        max_rate
    }
}

/// The widest config `device` offers for output at `rate` (reopen path).
pub fn output_config_at(device: &Device, rate: u32) -> Result<SupportedStreamConfig, String> {
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let cfg = find_config_for_rate(&ranges, rate)
        .ok_or_else(|| format!("does not run at {} kHz", khz(rate)))?;
    Ok(widest_config(&ranges, cfg))
}

/// The widest config `device` offers for input at `rate` (reopen path).
pub fn input_config_at(device: &Device, rate: u32) -> Result<SupportedStreamConfig, String> {
    let ranges: Vec<SupportedStreamConfigRange> = device
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .collect();
    let cfg = find_config_for_rate(&ranges, rate)
        .ok_or_else(|| format!("does not run at {} kHz", khz(rate)))?;
    Ok(widest_config(&ranges, cfg))
}
