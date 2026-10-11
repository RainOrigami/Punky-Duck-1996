//! cpal output. The device callback runs on cpal's thread and only drains a
//! queue; the main loop keeps the queue topped up from `Runner::mix_audio`,
//! so the engine itself never crosses threads.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SizedSample, StreamConfig};
use pd_engine::runner::Runner;
use pd_engine::{CHANNELS, SAMPLE_RATE};

/// Sample frames the queue is refilled to on every tick (~70 ms). This is the
/// added latency, and how long the main loop may stall before an underrun.
const TARGET_FRAMES: usize = SAMPLE_RATE as usize * 70 / 1000;

/// Device period requested from the audio server (~23 ms).
const DEVICE_BUFFER_FRAMES: u32 = 1024;

struct Shared {
    queue: Mutex<VecDeque<i16>>,
    /// Samples the device asked for while the queue was empty.
    underrun_samples: AtomicU64,
    /// Set by the first `pump`; silence requested before that is not an underrun.
    started: AtomicBool,
}

pub struct AudioOutput {
    _stream: cpal::Stream,
    shared: Arc<Shared>,
    scratch: Vec<i16>,
}

fn build<T>(device: &cpal::Device, config: StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<i16>,
{
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut queue = shared.queue.lock().unwrap();
            let mut missing = 0u64;
            for out in data.iter_mut() {
                let s = queue.pop_front().unwrap_or_else(|| {
                    missing += 1;
                    0
                });
                *out = T::from_sample_(s);
            }
            if missing > 0 && shared.started.load(Ordering::Relaxed) {
                shared.underrun_samples.fetch_add(missing, Ordering::Relaxed);
            }
        },
        |err| eprintln!("pd-native: audio stream error: {err}"),
        None,
    )
}

impl AudioOutput {
    /// Open the default output device at 44.1 kHz stereo (i16, falling back to f32).
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("no audio output device")?;
        // A fixed, small device buffer: with the default size PulseAudio /
        // pipewire-pulse buffers about two seconds, i.e. two seconds of lag.
        let fixed = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE,
            buffer_size: BufferSize::Fixed(DEVICE_BUFFER_FRAMES),
        };
        let default = StreamConfig { buffer_size: BufferSize::Default, ..fixed };
        let shared = Arc::new(Shared {
            queue: Mutex::new(VecDeque::new()),
            underrun_samples: AtomicU64::new(0),
            started: AtomicBool::new(false),
        });
        let stream = build::<i16>(&device, fixed, shared.clone())
            .or_else(|e| {
                eprintln!("pd-native: i16 output with a fixed buffer refused ({e}), trying f32");
                build::<f32>(&device, fixed, shared.clone())
            })
            .or_else(|e| {
                eprintln!("pd-native: fixed buffer size refused ({e}), using the device default");
                build::<i16>(&device, default, shared.clone())
            })
            .or_else(|_| build::<f32>(&device, default, shared.clone()))
            .map_err(|e| format!("cannot open a 44.1 kHz stereo output stream: {e}"))?;
        stream.play().map_err(|e| format!("cannot start audio stream: {e}"))?;
        Ok(Self { _stream: stream, shared, scratch: Vec::new() })
    }

    /// Mix as much audio as the device consumed since the last call.
    pub fn pump(&mut self, runner: &mut Runner) {
        let queued = self.shared.queue.lock().unwrap().len() / CHANNELS;
        if queued >= TARGET_FRAMES {
            return;
        }
        self.scratch.resize((TARGET_FRAMES - queued) * CHANNELS, 0);
        runner.mix_audio(&mut self.scratch);
        self.shared.queue.lock().unwrap().extend(self.scratch.iter().copied());
        self.shared.started.store(true, Ordering::Relaxed);
    }

    pub fn underrun_samples(&self) -> u64 {
        self.shared.underrun_samples.load(Ordering::Relaxed)
    }
}
