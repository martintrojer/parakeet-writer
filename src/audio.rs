use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::fs::File;
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<i16>>>,
    stream: Option<cpal::Stream>,
    sample_rate: u32,
}

impl AudioRecorder {
    pub fn new() -> Result<Self> {
        Ok(Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            stream: None,
            sample_rate: 16000,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("No input device available")?;

        log::debug!("Using input device: {}", device.name()?);

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(self.sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        self.samples.lock().unwrap().clear();
        let samples = Arc::clone(&self.samples);

        let stream = device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mut samples = samples.lock().unwrap();
                for &sample in data {
                    samples.push((sample * i16::MAX as f32) as i16);
                }
            },
            |err| log::error!("Audio stream error: {}", err),
            None,
        )?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn stop(&mut self) -> Result<PathBuf> {
        self.stream = None;
        std::thread::sleep(std::time::Duration::from_millis(100));

        let samples = self.samples.lock().unwrap();
        let temp_file = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()?
            .into_temp_path()
            .keep()?;

        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let file = File::create(&temp_file)?;
        let mut writer = hound::WavWriter::new(BufWriter::new(file), spec)?;
        for &sample in samples.iter() {
            writer.write_sample(sample)?;
        }
        writer.finalize()?;

        log::debug!(
            "Recorded {} samples ({:.2}s)",
            samples.len(),
            samples.len() as f64 / self.sample_rate as f64
        );

        Ok(temp_file)
    }
}
