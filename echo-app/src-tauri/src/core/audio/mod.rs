use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::error::{EchoError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDevice {
    pub name: String,
    pub is_default: bool,
}

pub struct AudioService {
    host: cpal::Host,
    active_stream: Mutex<Option<Stream>>,
    sample_tx: Mutex<Option<mpsc::Sender<Vec<f32>>>>,
}

// Stream is not Send but we manage it behind a Mutex and never share the reference.
unsafe impl Send for AudioService {}
unsafe impl Sync for AudioService {}

impl AudioService {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        Ok(Self {
            host,
            active_stream: Mutex::new(None),
            sample_tx: Mutex::new(None),
        })
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDevice>> {
        let default_name: Option<String> = self
            .host
            .default_input_device()
            .and_then(|d| d.name().ok());

        let devices: Vec<AudioDevice> = self
            .host
            .input_devices()
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?
            .filter_map(|d| d.name().ok())
            .map(|name| {
                let is_default = default_name.as_deref() == Some(name.as_str());
                AudioDevice { name, is_default }
            })
            .collect();

        Ok(devices)
    }

    /// Start capturing from the named device (or default if None).
    /// Returns a receiver of PCM chunks (f32, mono, 16 kHz).
    pub fn start_capture(&self, device_name: Option<&str>) -> Result<mpsc::Receiver<Vec<f32>>> {
        self.stop_capture();

        let device = self.select_device(device_name)?;
        let config = self.build_config(&device)?;

        let (tx, rx) = mpsc::channel::<Vec<f32>>(256);
        let tx_err = tx.clone();

        let channels = config.channels;
        let source_rate = config.sample_rate.0;
        let stream = device
            .build_input_stream(
                &config,
                move |data: &[f32], _| {
                    let chunk = resample_to_16k(data, source_rate, channels);
                    let _ = tx.try_send(chunk);
                },
                move |err| {
                    error!("Audio stream error: {err}");
                    let _ = tx_err.try_send(vec![]); // signal downstream
                },
                None,
            )
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        stream.play().map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        *self.active_stream.lock().unwrap() = Some(stream);
        info!("Audio capture started");

        Ok(rx)
    }

    pub fn stop_capture(&self) {
        if let Ok(mut guard) = self.active_stream.lock() {
            if let Some(stream) = guard.take() {
                drop(stream);
                info!("Audio capture stopped");
            }
        }
    }

    fn select_device(&self, name: Option<&str>) -> Result<Device> {
        match name {
            None => self
                .host
                .default_input_device()
                .ok_or_else(|| EchoError::AudioDevice("No default input device".into())),
            Some(n) => self
                .host
                .input_devices()
                .map_err(|e| EchoError::AudioDevice(e.to_string()))?
                .find(|d| d.name().ok().as_deref() == Some(n))
                .ok_or_else(|| EchoError::AudioDevice(format!("Device '{n}' not found"))),
        }
    }

    fn build_config(&self, device: &Device) -> Result<StreamConfig> {
        let supported = device
            .default_input_config()
            .map_err(|e| EchoError::AudioDevice(e.to_string()))?;

        // We always capture f32 and handle format conversion ourselves.
        let config = StreamConfig {
            channels: supported.channels(),
            sample_rate: supported.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };
        Ok(config)
    }
}

/// Naive linear resampler: down-mix interleaved frames to mono and resample to 16000 Hz.
/// `channels` is the source channel count so mono (1ch) input is not corrupted.
fn resample_to_16k(data: &[f32], source_rate: u32, channels: u16) -> Vec<f32> {
    // Down-mix interleaved frames to mono by averaging each frame's channels.
    let mono: Vec<f32> = if channels <= 1 {
        data.to_vec()
    } else {
        let ch = channels as usize;
        data.chunks(ch)
            .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
            .collect()
    };

    if source_rate == 16000 {
        return mono;
    }

    let ratio = source_rate as f64 / 16000.0;
    let out_len = (mono.len() as f64 / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);

    for i in 0..out_len {
        let src_idx = (i as f64 * ratio) as usize;
        out.push(*mono.get(src_idx).unwrap_or(&0.0));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::resample_to_16k;

    #[test]
    fn resample_passthrough_16k() {
        // Mono input already at 16 kHz should pass through unchanged.
        let input = vec![0.1, -0.2, 0.3, -0.4];
        let out = resample_to_16k(&input, 16000, 1);
        assert_eq!(out, input);
    }

    #[test]
    fn resample_48k_to_16k_length() {
        // 48 kHz mono → 16 kHz should be ~1/3 the length.
        let input: Vec<f32> = (0..480).map(|i| (i as f32) * 0.001).collect();
        let out = resample_to_16k(&input, 48000, 1);
        // ratio = 3.0, ceil(480 / 3) = 160
        assert_eq!(out.len(), 160);
    }

    #[test]
    fn mono_downmix() {
        // Interleaved stereo [L, R, L, R] is averaged into mono.
        let stereo = vec![1.0, 3.0, 2.0, 4.0];
        let out = resample_to_16k(&stereo, 16000, 2);
        assert_eq!(out, vec![2.0, 3.0]); // (1+3)/2, (2+4)/2
    }
}
