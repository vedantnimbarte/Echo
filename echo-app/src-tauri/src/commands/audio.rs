use tauri::State;

use crate::{core::audio::AudioDevice, error::Result, state::AppState};

#[tauri::command]
pub fn get_audio_devices(state: State<'_, AppState>) -> Result<Vec<AudioDevice>> {
    state.audio.list_input_devices()
}

/// Whether the neural voice-activity model loaded at startup.
///
/// Settings offers a choice between it and the energy detector, and the choice
/// is a lie if the model never loaded — `start_recording` falls back to energy
/// whatever the setting says. So the picker asks, and says so when the answer
/// is no, rather than letting someone select a detector that is not running.
#[tauri::command]
pub fn silero_available(state: State<'_, AppState>) -> bool {
    state.silero.is_some()
}
