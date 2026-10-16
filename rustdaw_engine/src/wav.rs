use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

/// WAV file header structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavSpec {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub sample_format: SampleFormat,
}

/// Sample format for WAV files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 16-bit integer PCM.
    I16,
    /// 32-bit float PCM.
    F32,
}

impl WavSpec {
    /// Calculate the frame size in bytes.
    pub fn frame_size(&self) -> usize {
        (self.channels as usize) * (self.bits_per_sample as usize / 8)
    }

    /// Calculate the data size for a given number of samples.
    pub fn data_size(&self, sample_count: usize) -> usize {
        sample_count * self.frame_size()
    }
}

/// Read a WAV file and return the samples as f32 in the range [-1.0, 1.0].
pub fn reader<P: AsRef<std::path::Path>>(path: P) -> io::Result<(WavSpec, Vec<f32>)> {
    let mut file = File::open(path)?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header)?;
    if &header[0..4] != b"RIFF" || &header[8..12] != b"WAVE" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Not a valid WAV file",
        ));
    }

    // Find the fmt chunk.
    loop {
        let mut chunk_header = [0u8; 8];
        file.read_exact(&mut chunk_header)?;
        let chunk_size = (&chunk_header[4..8]).read_u32::<LittleEndian>()?;
        let chunk_type = &chunk_header[0..4];
        if chunk_type == b"fmt " {
            // Read the fmt chunk.
            let mut fmt_data = [0u8; 16];
            file.read_exact(&mut fmt_data)?;
            let audio_format = (&fmt_data[0..2]).read_u16::<LittleEndian>()?;
            let channels = (&fmt_data[2..4]).read_u16::<LittleEndian>()?;
            let sample_rate = (&fmt_data[4..8]).read_u32::<LittleEndian>()?;
            let _byte_rate = (&fmt_data[8..12]).read_u32::<LittleEndian>()?;
            let _block_align = (&fmt_data[12..14]).read_u16::<LittleEndian>()?;
            let bits_per_sample = (&fmt_data[14..16]).read_u16::<LittleEndian>()?;

            if audio_format != 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Only PCM format is supported",
                ));
            }

            let sample_format = match bits_per_sample {
                16 => SampleFormat::I16,
                32 => SampleFormat::F32,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("Unsupported bits per sample: {}", bits_per_sample),
                    ));
                }
            };

            let spec = WavSpec {
                channels,
                sample_rate,
                bits_per_sample,
                sample_format,
            };

            // Now find the data chunk.
            loop {
                let mut chunk_header = [0u8; 8];
                file.read_exact(&mut chunk_header)?;
                let chunk_size = (&chunk_header[4..8]).read_u32::<LittleEndian>()?;
                let chunk_type = &chunk_header[0..4];
                if chunk_type == b"data" {
                    // Read the data.
                    let mut data = vec![0u8; chunk_size as usize];
                    file.read_exact(&mut data)?;
                    let samples = match spec.sample_format {
                        SampleFormat::I16 => {
                            let mut samples = Vec::with_capacity(data.len() / 2);
                            for i in (0..data.len()).step_by(2) {
                                let sample = (&data[i..i+2]).read_i16::<LittleEndian>()?;
                                samples.push(sample as f32 / 32768.0);
                            }
                            samples
                        }
                        SampleFormat::F32 => {
                            let mut samples = Vec::with_capacity(data.len() / 4);
                            for i in (0..data.len()).step_by(4) {
                                let sample = (&data[i..i+4]).read_f32::<LittleEndian>()?;
                                samples.push(sample);
                            }
                            samples
                        }
                    };
                    return Ok((spec, samples));
                } else {
                    // Skip this chunk.
                    file.seek(SeekFrom::Current(chunk_size as i64))?;
                }
            }
        } else {
            // Skip this chunk.
            file.seek(SeekFrom::Current(chunk_size as i64))?;
        }
    }
}

/// Write a WAV file from f32 samples in the range [-1.0, 1.0].
pub fn writer<P: AsRef<std::path::Path>>(
    path: P,
    spec: &WavSpec,
    samples: &[f32],
) -> io::Result<()> {
    let mut file = File::create(path)?;
    // Write RIFF header.
    file.write_all(b"RIFF")?;
    // Placeholder for file size.
    file.write_all(&0u32.to_le_bytes())?;
    file.write_all(b"WAVE")?;

    // Write fmt chunk.
    file.write_all(b"fmt ")?;
    let chunk_size = 16u32; // PCM
    file.write_all(&chunk_size.to_le_bytes())?;
    let audio_format = 1u16; // PCM
    file.write_all(&audio_format.to_le_bytes())?;
    file.write_all(&spec.channels.to_le_bytes())?;
    file.write_all(&spec.sample_rate.to_le_bytes())?;
    let byte_rate = spec.sample_rate * spec.channels as u32 * (spec.bits_per_sample as u32 / 8);
    file.write_all(&byte_rate.to_le_bytes())?;
    let block_align = spec.channels * (spec.bits_per_sample / 8);
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&spec.bits_per_sample.to_le_bytes())?;

    // Write data chunk.
    file.write_all(b"data")?;
    let data_size = spec.data_size(samples.len()) as u32;
    file.write_all(&data_size.to_le_bytes())?;
    match spec.sample_format {
        SampleFormat::I16 => {
            for &sample in samples {
                // Clip to [-1.0, 1.0] and convert to i16.
                let sample = sample.clamp(-1.0, 1.0);
                let sample = (sample * 32767.0) as i16;
                file.write_all(&sample.to_le_bytes())?;
            }
        }
        SampleFormat::F32 => {
            for &sample in samples {
                file.write_all(&sample.to_le_bytes())?;
            }
        }
    }

    // Now go back and fix the file size placeholder.
    let file_size = file.metadata()?.len();
    file.seek(SeekFrom::Start(4))?;
    file.write_all(&((file_size - 8) as u32).to_le_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_wav_writer_reader() {
        // Create a simple sine wave.
        let spec = WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: SampleFormat::F32,
        };
        let duration = 0.1; // seconds
        let sample_count = (spec.sample_rate as f32 * duration) as usize;
        let mut samples = Vec::with_capacity(sample_count);
        for i in 0..sample_count {
            let t = i as f32 / spec.sample_rate as f32;
            samples.push((2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }

        // Write to a temporary file.
        let temp_path = "test_output.wav";
        writer(temp_path, &spec, &samples).expect("Failed to write WAV file");

        // Read back.
        let (read_spec, read_samples) = reader(temp_path).expect("Failed to read WAV file");
        assert_eq!(read_spec, spec);
        assert_eq!(read_samples.len(), samples.len());
        for (i, (&a, &b)) in samples.iter().zip(read_samples.iter()).enumerate() {
            // Allow small floating point differences due to rounding.
            assert!((a - b).abs() < 1e-5, "Sample {} mismatch: {} vs {}", i, a, b);
        }

        // Clean up.
        fs::remove_file(temp_path).expect("Failed to remove temporary file");
    }
}
