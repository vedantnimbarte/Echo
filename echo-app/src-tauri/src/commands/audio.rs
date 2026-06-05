use tauri::State;

use crate::{
    core::audio::AudioDevice,
    error::Result,
    state::AppState,
};

#[tauri::command]
pub fn get_audio_devices(state: State<'_, AppState>) -> Result<Vec<AudioDevice>> {
    state.audio.list_input_devices()
}
