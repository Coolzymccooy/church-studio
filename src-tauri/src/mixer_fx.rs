use crate::dsp::mixer::fx_params::{
    bound, FxScene, DECAY, FEEDBACK, FX_COUNT, FX_IDS, OUTPUT_SEND_KEYS, RETURN_SEND_DEFAULTS,
    TEMPO,
};
use crate::dsp::mixer::params::{FADER_MAX_DB, MIN_DB};
use crate::mixer_control::MixerControl;
use serde::Serialize;
use std::sync::atomic::Ordering::Relaxed;
use tauri::State;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FxReturnState {
    id: &'static str,
    fader_db: f32,
    send_main_db: f32,
    send_stream_db: f32,
    send_monitor_db: f32,
    send_monitor2_db: f32,
    send_monitor3_db: f32,
    mute: bool,
    solo: bool,
    monitor_post_fader: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    decay_s: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tempo_bpm: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback_pct: Option<f32>,
}

fn index(id: &str) -> Result<usize, String> {
    FX_IDS
        .iter()
        .position(|name| *name == id)
        .ok_or_else(|| "Unknown FX return".into())
}

impl MixerControl {
    pub fn fx_state(&self) -> [FxReturnState; FX_COUNT] {
        let scene = FxScene::capture(&self.params.fx);
        std::array::from_fn(|i| {
            let p = &scene.returns[i];
            let [main, stream, mon, mon2, mon3] = p.sends;
            FxReturnState {
                id: FX_IDS[i],
                fader_db: p.fader,
                send_main_db: main,
                send_stream_db: stream,
                send_monitor_db: mon,
                send_monitor2_db: mon2,
                send_monitor3_db: mon3,
                mute: p.mute,
                solo: p.solo,
                monitor_post_fader: p.monitor_post,
                decay_s: (i == 0).then_some(scene.decay),
                tempo_bpm: (i == 1).then_some(scene.tempo),
                feedback_pct: (i == 1).then_some(scene.feedback),
            }
        })
    }

    pub fn set_fx_param(&self, id: &str, key: &str, value: f32) -> Result<(), String> {
        let i = index(id)?;
        let fx = &self.params.fx;
        let ret = &fx.returns[i];
        let (field, range) = match (i, key) {
            (0, "decay_s") => (&fx.decay, DECAY),
            (1, "tempo_bpm") => (&fx.tempo, TEMPO),
            (1, "feedback_pct") => (&fx.feedback, FEEDBACK),
            (_, "fader_db") => (&ret.fader, (MIN_DB, FADER_MAX_DB, 0.0)),
            _ => {
                let bus = OUTPUT_SEND_KEYS
                    .iter()
                    .position(|name| *name == key)
                    .ok_or_else(|| "Unknown FX parameter".to_string())?;
                (
                    &ret.sends[bus],
                    (MIN_DB, FADER_MAX_DB, RETURN_SEND_DEFAULTS[bus]),
                )
            }
        };
        field.store(bound(value, range), Relaxed);
        Ok(())
    }

    pub fn set_fx_bool(&self, id: &str, key: &str, value: bool) -> Result<(), String> {
        let ret = &self.params.fx.returns[index(id)?];
        let flag = match key {
            "mute" => &ret.mute,
            "solo" => &ret.solo,
            "monitor_post_fader" => &ret.monitor_post,
            _ => return Err("Unknown FX switch".into()),
        };
        flag.store(value, Relaxed);
        Ok(())
    }
}

#[tauri::command]
pub fn mixer_set_fx_param(
    mixer: State<'_, MixerControl>,
    id: String,
    key: String,
    value: f32,
) -> Result<(), String> {
    mixer.set_fx_param(&id, &key, value)
}

#[tauri::command]
pub fn mixer_set_fx_bool(
    mixer: State<'_, MixerControl>,
    id: String,
    key: String,
    value: bool,
) -> Result<(), String> {
    mixer.set_fx_bool(&id, &key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fx_controls_survive_scene_recall_and_legacy_scenes_reset_them() {
        let mixer = MixerControl::new();
        mixer.set_fx_param("fx_b", "tempo_bpm", 999.0).unwrap();
        mixer.set_bus_bool("fx_a", "mute", true).unwrap();
        mixer.set_bus_param("fx_a", "fader_db", -8.0).unwrap();
        mixer.set_strip_param(0, "send_fx_a_db", -6.0).unwrap();
        assert!(mixer.set_fx_param("fx_a", "tempo_bpm", 100.0).is_err());
        assert!(mixer.set_fx_param("fx_b", "send_fx_a_db", 0.0).is_err());
        assert!(mixer.set_fx_bool("main", "mute", true).is_err());

        let mut scene = serde_json::to_value(mixer.capture_scene("FX")).unwrap();
        mixer.set_fx_param("fx_b", "tempo_bpm", 40.0).unwrap();
        mixer.apply_scene(&serde_json::from_value(scene.clone()).unwrap());
        assert_eq!(mixer.fx_state()[1].tempo_bpm, Some(240.0));
        assert!(mixer.fx_state()[0].mute);
        assert_eq!(mixer.fx_state()[0].fader_db, -8.0);
        assert_eq!(mixer.params.strips[0].fx_send_db[0].load(Relaxed), -6.0);

        scene["mixer"].as_object_mut().unwrap().remove("fx");
        for strip in scene["mixer"]["strips"].as_array_mut().unwrap() {
            strip.as_object_mut().unwrap().remove("fx_send_db");
        }
        mixer.apply_scene(&serde_json::from_value(scene).unwrap());
        assert_eq!(mixer.fx_state()[1].tempo_bpm, Some(120.0));
        assert!(!mixer.fx_state()[0].mute);
        assert_eq!(mixer.params.strips[0].fx_send_db[0].load(Relaxed), MIN_DB);
    }
}
