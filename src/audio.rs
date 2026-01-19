use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, SupportedStreamConfig};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const DEFAULT_INPUT_SAMPLE_RATE: u32 = 48000;
const TARGET_OUTPUT_SAMPLE_RATE: u32 = 16000;

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
    input_sample_rate: u32,
    output_sample_rate: u32,
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            input_sample_rate: DEFAULT_INPUT_SAMPLE_RATE,
            output_sample_rate: TARGET_OUTPUT_SAMPLE_RATE,
        }
    }
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No input device available")?;

        log::debug!("Using input device: {}", device.name()?);

        let default_config = device
            .default_input_config()
            .context("No default input config")?;

        self.input_sample_rate = default_config.sample_rate().0;
        log::debug!(
            "Using sample rate: {} Hz, {} channels, format: {:?}",
            self.input_sample_rate,
            default_config.channels(),
            default_config.sample_format()
        );

        self.samples.lock().unwrap().clear();
        let samples = Arc::clone(&self.samples);

        let stream = self.build_stream(&device, &default_config, samples)?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    fn build_stream(
        &self,
        device: &cpal::Device,
        config: &SupportedStreamConfig,
        samples: Arc<Mutex<Vec<f32>>>,
    ) -> Result<cpal::Stream> {
        let channels = config.channels() as usize;
        let stream_config = config.config();

        let err_fn = |err| log::error!("Audio stream error: {}", err);

        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| Self::write_samples(&samples, data, channels),
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let float_data: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    Self::write_samples(&samples, &float_data, channels);
                },
                err_fn,
                None,
            )?,
            SampleFormat::I32 => device.build_input_stream(
                &stream_config,
                move |data: &[i32], _| {
                    let float_data: Vec<f32> =
                        data.iter().map(|&s| s as f32 / i32::MAX as f32).collect();
                    Self::write_samples(&samples, &float_data, channels);
                },
                err_fn,
                None,
            )?,
            format => anyhow::bail!("Unsupported sample format: {:?}", format),
        };

        Ok(stream)
    }

    fn write_samples(samples: &Arc<Mutex<Vec<f32>>>, data: &[f32], channels: usize) {
        let mut samples = samples.lock().unwrap();
        if channels == 1 {
            samples.extend_from_slice(data);
        } else {
            for chunk in data.chunks(channels) {
                let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                samples.push(mono);
            }
        }
    }

    pub fn stop(&mut self) -> Result<PathBuf> {
        self.stream = None;
        // Brief delay to ensure the audio stream callback has finished
        // processing any remaining samples before we read the buffer
        std::thread::sleep(std::time::Duration::from_millis(100));

        let samples = self.samples.lock().unwrap();

        // Resample to output rate if needed
        let resampled = if self.input_sample_rate != self.output_sample_rate {
            resample(&samples, self.input_sample_rate, self.output_sample_rate)
        } else {
            samples.clone()
        };

        let temp_file = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()?
            .into_temp_path()
            .keep()?;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.output_sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let file = File::create(&temp_file)?;
        let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)?;
        for &sample in resampled.iter() {
            let i16_sample =
                (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer.write_sample(i16_sample)?;
        }
        writer.finalize()?;

        log::debug!(
            "Recorded {} samples @ {}Hz -> {} samples @ {}Hz ({:.2}s)",
            samples.len(),
            self.input_sample_rate,
            resampled.len(),
            self.output_sample_rate,
            resampled.len() as f64 / self.output_sample_rate as f64
        );

        Ok(temp_file)
    }
}

fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] * (1.0 - frac as f32) + samples[idx + 1] * frac as f32
        } else if idx < samples.len() {
            samples[idx]
        } else {
            0.0
        };
        output.push(sample);
    }

    output
}
