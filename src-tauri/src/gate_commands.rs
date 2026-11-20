//! Tauri command for the Hyper gate's "learn mic". The learning itself runs
//! in the voice chain on the audio thread (`dsp::hypergate`); this side only
//! raises the request and waits for the result off the async runtime.
use crate::audio::EngineState;
use crate::dsp::hypergate::LearnWait;
use crate::SharedParams;
use std::time::Duration;
use tauri::State;

/// The chain listens for 5 s; allow for a slow start and the hand-back.
const LEARN_TIMEOUT: Duration = Duration::from_secs(8);
const LEARN_POLL: Duration = Duration::from_millis(20);

/// Learn the voice strip's mic fingerprint from the next five seconds of
/// speech. Resolves when it is learned, with a short reason when not.
#[tauri::command]
pub async fn learn_mic_fingerprint(
    params: State<'_, SharedParams>,
    state: State<'_, EngineState>,
) -> Result<(), String> {
    if state.inner().lock().map_err(|_| "engine state unavailable")?.is_none() {
        return Err("Go live first so the engine can hear the mic.".to_string());
    }
    let params = params.0.clone();
    let request = params.hyper.request_learn();
    let wait = tauri::async_runtime::spawn_blocking(move || {
        params.hyper.wait_for_learn(request, LEARN_TIMEOUT, LEARN_POLL)
    })
    .await
    .map_err(|e| format!("learn mic failed: {e}"))?;
    match wait {
        LearnWait::Learned => Ok(()),
        LearnWait::NotEnoughVoice => Err(
            "Not enough speech heard. Talk into the mic for the whole five seconds, then try again."
                .to_string(),
        ),
        LearnWait::GateOff => Err(
            "Turn Reduction on and take the chain out of bypass, then learn the mic.".to_string(),
        ),
        LearnWait::TimedOut => Err(
            "The gate did not hear the mic. Check that the voice strip has a live input.".to_string(),
        ),
    }
}
