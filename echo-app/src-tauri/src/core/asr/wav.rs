use std::io::Cursor;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::error::{EchoError, Result};

/// Encode mono f32 PCM samples into an in-memory 16-bit WAV file. Used to upload
/// captured audio to cloud ASR APIs that expect a standard container.
pub fn pcm_f32_to_wav(samples: &[f32], sample_rate: u32) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut cursor = Cursor::new(Vec::<u8>::new());
    {
        let mut writer = WavWriter::new(&mut cursor, spec)
            .map_err(|e| EchoError::AsrProvider(format!("WAV init failed: {e}")))?;
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer
                .write_sample(v)
                .map_err(|e| EchoError::AsrProvider(format!("WAV write failed: {e}")))?;
        }
        writer
            .finalize()
            .map_err(|e| EchoError::AsrProvider(format!("WAV finalize failed: {e}")))?;
    }
    Ok(cursor.into_inner())
}
