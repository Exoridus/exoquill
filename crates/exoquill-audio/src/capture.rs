//! Microphone capture via cpal.
//!
//! Opens an input stream on the chosen (or default) device, downmixes whatever
//! interleaved format the device delivers to mono `f32`, and forwards buffers
//! over a channel for the dictation worker to segment and transcribe. The cpal
//! callback does no heavy work — it converts to `f32`, runs the channel-aware
//! [`Downmixer`] and (for microphones) [`AutoGain`], and sends — so it never
//! blocks the audio thread.

use std::sync::mpsc::{channel, Receiver, Sender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, Sample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::gain::{AutoGain, Downmixer};

/// A running capture: the live `stream` (kept alive for the session), the
/// device sample rate, and the channel of mono `f32` buffers.
pub struct Capture {
    /// Held to keep the stream running; dropping it stops capture.
    pub stream: Stream,
    pub sample_rate: u32,
    pub frames: Receiver<Vec<f32>>,
}

/// Start capturing from `device_name` (matched by name) or the default device
/// when `None`. With `loopback`, the source is a *render* (output) device and
/// cpal captures the system audio playing on it (WASAPI loopback); otherwise it
/// is a microphone. `gain` selects level handling: `None` runs the adaptive
/// [`AutoGain`] (AGC), `Some(g)` applies a fixed multiplier (`Some(1.0)` = off).
/// The returned [`Capture`] must be kept alive and is owned by the worker thread
/// that created it.
pub fn start_capture(
    device_name: Option<&str>,
    loopback: bool,
    gain: Option<f32>,
) -> Result<Capture, String> {
    let host = cpal::default_host();
    let (device, config) = if loopback {
        // Building an input stream on an output device transparently enables
        // WASAPI loopback (system-audio capture); its config is the render mix.
        let device = match device_name {
            Some(name) => host
                .output_devices()
                .map_err(|e| format!("enumerate output devices: {e}"))?
                .find(|d| device_label(d).as_deref() == Some(name))
                .ok_or_else(|| format!("output device not found: {name}"))?,
            None => host
                .default_output_device()
                .ok_or_else(|| "no default output device".to_string())?,
        };
        let config = device
            .default_output_config()
            .map_err(|e| format!("default output config: {e}"))?;
        (device, config)
    } else {
        let device = match device_name {
            Some(name) => host
                .input_devices()
                .map_err(|e| format!("enumerate input devices: {e}"))?
                .find(|d| device_label(d).as_deref() == Some(name))
                .ok_or_else(|| format!("input device not found: {name}"))?,
            None => host
                .default_input_device()
                .ok_or_else(|| "no default input device".to_string())?,
        };
        let config = device
            .default_input_config()
            .map_err(|e| format!("default input config: {e}"))?;
        (device, config)
    };

    let sample_rate = config.sample_rate();
    let channels = config.channels().max(1);
    let sample_format = config.sample_format();
    let stream_config: StreamConfig = config.into();

    let (tx, rx) = channel::<Vec<f32>>();
    let stream = match sample_format {
        SampleFormat::F32 => build::<f32>(&device, &stream_config, channels, gain, tx),
        SampleFormat::I16 => build::<i16>(&device, &stream_config, channels, gain, tx),
        SampleFormat::I32 => build::<i32>(&device, &stream_config, channels, gain, tx),
        SampleFormat::I8 => build::<i8>(&device, &stream_config, channels, gain, tx),
        SampleFormat::U8 => build::<u8>(&device, &stream_config, channels, gain, tx),
        SampleFormat::U16 => build::<u16>(&device, &stream_config, channels, gain, tx),
        other => return Err(format!("unsupported input sample format: {other}")),
    }
    .map_err(|e| format!("build input stream: {e}"))?;

    stream.play().map_err(|e| format!("start stream: {e}"))?;
    Ok(Capture {
        stream,
        sample_rate,
        frames: rx,
    })
}

fn device_label(device: &Device) -> Option<String> {
    device
        .description()
        .ok()
        .map(|description| description.name().to_owned())
}

/// Build a typed input stream that converts interleaved `T` frames to `f32`,
/// downmixes them to mono with the channel-aware [`Downmixer`], applies gain
/// (`None` = adaptive [`AutoGain`], `Some(g)` = fixed multiplier), and forwards
/// each buffer over `tx`.
fn build<T>(
    device: &Device,
    config: &StreamConfig,
    channels: u16,
    gain: Option<f32>,
    tx: Sender<Vec<f32>>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = channels as usize;
    let mut downmixer = Downmixer::new(channels);
    let mut agc = AutoGain::new();
    device.build_input_stream::<T, _, _>(
        *config,
        move |data: &[T], _| {
            let interleaved: Vec<f32> = data.iter().copied().map(|s| f32::from_sample(s)).collect();
            let mut mono = downmixer.mix(&interleaved);
            match gain {
                None => agc.process(&mut mono),
                Some(g) if g != 1.0 => {
                    for s in mono.iter_mut() {
                        *s = (*s * g).clamp(-1.0, 1.0);
                    }
                }
                Some(_) => {} // unity gain: leave the signal untouched
            }
            // The receiver is gone only once the worker has stopped; ignore.
            let _ = tx.send(mono);
        },
        |err| eprintln!("dictation capture stream error: {err}"),
        None,
    )
}
