//! Voice commands: hold ⌥V, say a command, release.
//!
//! Audio is recorded locally only while the key is held (max 10 s), then sent
//! to OpenAI's transcription API with your key. The transcript is matched
//! against a fixed list of commands, so nothing you say is ever executed as-is.
//!
//! Commands: "reply to this", "debug this repo", "shorter", "friendlier",
//! "more professional", "explain more", "regenerate", "new conversation",
//! "add to case", "clear case", "save example".

use crate::flow::{self, Followup, Trigger};
use crate::macos::hud;
use crate::rewrite::RewriteAction;
use crate::state::AppState;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};

const MAX_SECONDS: u64 = 10;
const TARGET_RATE: u32 = 16_000;

struct Recording {
    stop: mpsc::Sender<()>,
    done: mpsc::Receiver<Result<(Vec<f32>, u32), String>>,
}

static ACTIVE: Mutex<Option<Recording>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    Reply,
    Debug,
    Rewrite(RewriteAction),
    NewConversation,
    AddToCase,
    ClearCase,
    SaveExample,
}

/// Maps a transcript to a command. Order matters: specific phrases first.
pub fn parse(transcript: &str) -> Option<Command> {
    let t: String = transcript.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' }).collect();
    let has = |w: &str| t.split_whitespace().collect::<Vec<_>>().join(" ").contains(w);
    if has("clear case") || has("clear the case") {
        Some(Command::ClearCase)
    } else if has("add") && has("case") {
        Some(Command::AddToCase)
    } else if has("new conversation") || has("new case") || has("new student") || has("start over") {
        Some(Command::NewConversation)
    } else if has("save") && (has("example") || has("this reply")) {
        Some(Command::SaveExample)
    } else if has("debug") || has("check the repo") || has("look at the repo") || has("check this repo") {
        Some(Command::Debug)
    } else if has("shorter") || has("shorten") {
        Some(Command::Rewrite(RewriteAction::Shorter))
    } else if has("friendlier") || has("friendly") || has("warmer") || has("nicer") {
        Some(Command::Rewrite(RewriteAction::Friendlier))
    } else if has("professional") || has("formal") {
        Some(Command::Rewrite(RewriteAction::Professional))
    } else if has("explain more") || has("more detail") || has("explain it") || has("elaborate") {
        Some(Command::Rewrite(RewriteAction::ExplainMore))
    } else if has("regenerate") || has("try again") || has("another version") || has("rewrite") {
        Some(Command::Rewrite(RewriteAction::Regenerate))
    } else if has("reply") || has("respond") || has("answer") || has("generate") {
        Some(Command::Reply)
    } else {
        None
    }
}

/// Key down: start recording.
pub fn on_press(app: &AppHandle) {
    if !app.state::<AppState>().settings().voice_enabled {
        return;
    }
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    if active.is_some() {
        return;
    }
    let (stop_tx, stop_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done_tx.send(record(stop_rx));
    });
    *active = Some(Recording { stop: stop_tx, done: done_rx });
    hud::show(app, "Listening… release ⌥V when done", None);
}

/// Key up: stop, transcribe, run the command.
pub fn on_release(app: &AppHandle) {
    let Some(rec) = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).take() else { return };
    let _ = rec.stop.send(());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = tauri::async_runtime::spawn_blocking(move || {
            rec.done.recv_timeout(Duration::from_secs(3)).unwrap_or_else(|_| Err("Recording didn't stop.".into()))
        })
        .await
        .unwrap_or_else(|_| Err("Recording failed.".into()));
        let (samples, rate) = match result {
            Ok(v) => v,
            Err(e) => {
                hud::show(&app, &e, Some(Duration::from_secs(4)));
                return;
            }
        };
        if samples.len() < (rate as usize) / 3 {
            hud::show(&app, "Hold ⌥V while you speak.", Some(Duration::from_secs(2)));
            return;
        }
        let state = app.state::<AppState>();
        let Some(key) = state.api_key() else {
            hud::show(&app, "Add your OpenAI API key in Settings.", Some(Duration::from_secs(3)));
            return;
        };
        hud::show(&app, "Understanding…", None);
        let wav = encode_wav(&resample(&samples, rate, TARGET_RATE), TARGET_RATE);
        let transcript = match state.openai.transcribe(&key, wav).await {
            Ok(t) => t,
            Err(e) => {
                hud::show(&app, &e, Some(Duration::from_secs(4)));
                return;
            }
        };
        match parse(&transcript) {
            Some(cmd) => {
                crate::analytics::log(&state.db(), "voice", None, Some(&format!("{cmd:?}")), None);
                hud::hide(&app);
                run(&app, cmd);
            }
            None => hud::show(&app, &format!("Didn't catch a command: “{}”", transcript.trim()), Some(Duration::from_secs(3))),
        }
    });
}

fn run(app: &AppHandle, cmd: Command) {
    match cmd {
        Command::Reply => flow::trigger(app.clone(), Trigger::Hotkey),
        Command::Debug => crate::debug::open(app.clone(), false),
        Command::Rewrite(a) => flow::trigger_followup(app.clone(), Followup::Rewrite(a)),
        Command::NewConversation => flow::new_conversation(app),
        Command::AddToCase => crate::case::add_selection(app.clone(), false),
        Command::ClearCase => crate::case::clear(app),
        Command::SaveExample => flow::save_last_as_example(app),
    }
}

/// Records mono f32 samples from the default microphone until told to stop.
fn record(stop: mpsc::Receiver<()>) -> Result<(Vec<f32>, u32), String> {
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("No microphone found.")?;
    let config = device.default_input_config().map_err(|_| "Couldn't open the microphone. Check System Settings → Privacy & Security → Microphone.".to_string())?;
    let rate = config.sample_rate();
    let channels = config.channels().max(1) as usize;
    let buffer: Arc<Mutex<Vec<f32>>> = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 4)));
    let max = rate as usize * MAX_SECONDS as usize;

    macro_rules! stream_for {
        ($t:ty, $to_f32:expr) => {{
            let buf = buffer.clone();
            device.build_input_stream::<$t, _, _>(
                config.config(),
                move |data: &[$t], _| {
                    let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                    if b.len() < max {
                        for frame in data.chunks(channels) {
                            let sum: f32 = frame.iter().map(|s| $to_f32(*s)).sum();
                            b.push(sum / channels as f32);
                        }
                    }
                },
                |_| {},
                None,
            )
        }};
    }
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => stream_for!(f32, |s: f32| s),
        cpal::SampleFormat::I16 => stream_for!(i16, |s: i16| s as f32 / i16::MAX as f32),
        cpal::SampleFormat::I32 => stream_for!(i32, |s: i32| s as f32 / i32::MAX as f32),
        cpal::SampleFormat::U16 => stream_for!(u16, |s: u16| (s as f32 - 32768.0) / 32768.0),
        _ => return Err("Unsupported microphone format.".into()),
    }
    .map_err(|_| "Couldn't start the microphone. Allow PastePilot in System Settings → Privacy & Security → Microphone.".to_string())?;
    stream.play().map_err(|_| "Couldn't start the microphone.".to_string())?;
    let _ = stop.recv_timeout(Duration::from_secs(MAX_SECONDS));
    drop(stream);
    let samples = std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
    Ok((samples, rate))
}

/// Linear-interpolation resampling (speech only needs 16 kHz).
fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let len = (input.len() as f64 / ratio) as usize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * ratio;
            let idx = pos as usize;
            let frac = (pos - idx as f64) as f32;
            let a = input[idx.min(input.len() - 1)];
            let b = input[(idx + 1).min(input.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

/// 16-bit PCM mono WAV.
fn encode_wav(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_commands() {
        assert_eq!(parse("Reply to this."), Some(Command::Reply));
        assert_eq!(parse("debug this repo"), Some(Command::Debug));
        assert_eq!(parse("Make it shorter please"), Some(Command::Rewrite(RewriteAction::Shorter)));
        assert_eq!(parse("more professional"), Some(Command::Rewrite(RewriteAction::Professional)));
        assert_eq!(parse("Add this to the case"), Some(Command::AddToCase));
        assert_eq!(parse("new conversation"), Some(Command::NewConversation));
        assert_eq!(parse("what's the weather"), None);
    }

    #[test]
    fn wav_and_resampling() {
        let wav = encode_wav(&[0.0, 0.5, -0.5], 16_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(resample(&vec![0.0; 48_000], 48_000, 16_000).len(), 16_000);
    }
}
