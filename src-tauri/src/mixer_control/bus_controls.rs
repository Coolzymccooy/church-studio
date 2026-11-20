use super::{bus_f32, bus_index, clamp_key, f32_spec, MixerControl, BUS_F32_KEYS};
use crate::dsp::mixer::fx_params::FX_IDS;
use std::sync::atomic::Ordering::Relaxed;

impl MixerControl {
    pub fn set_bus_param(&self, bus: &str, key: &str, value: f32) -> Result<(), String> {
        if FX_IDS.contains(&bus) && key == "fader_db" {
            return self.set_fx_param(bus, key, value);
        }
        let p = &self.params.buses[bus_index(bus)?];
        match (f32_spec(BUS_F32_KEYS, key), bus_f32(p, key)) {
            (Some(spec), Some(field)) => {
                field.store(clamp_key(spec, value), Relaxed);
                Ok(())
            }
            _ => Err(format!("unknown bus key '{key}'")),
        }
    }

    pub fn set_bus_bool(&self, bus: &str, key: &str, value: bool) -> Result<(), String> {
        if FX_IDS.contains(&bus) && key == "mute" {
            return self.set_fx_bool(bus, key, value);
        }
        let p = &self.params.buses[bus_index(bus)?];
        let field = match key {
            "mute" => &p.mute,
            "leveler_enabled" => &p.leveler_enabled,
            _ => return Err(format!("unknown bus key '{key}'")),
        };
        field.store(value, Relaxed);
        Ok(())
    }
}
