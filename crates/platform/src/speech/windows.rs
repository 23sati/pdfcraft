//! Safe WinRT synthesis into an in-memory stream; no SSML, shell or network input.
use super::{Backend, Options, Voice};
use std::sync::{Arc, Mutex};
use windows::{
    Foundation::TypedEventHandler,
    Media::{
        Core::MediaSource,
        Playback::{IMediaPlaybackSource, MediaPlayer},
        SpeechSynthesis::{SpeechSynthesisStream, SpeechSynthesizer},
    },
    core::HSTRING,
};
use windows_future::{AsyncStatus, IAsyncOperation};

fn error(e: windows::core::Error) -> String {
    format!("Windows speech: {e}")
}

pub(super) struct WindowsSpeech {
    synthesizer: SpeechSynthesizer,
    player: MediaPlayer,
    synthesis: Option<IAsyncOperation<SpeechSynthesisStream>>,
    stream: Option<SpeechSynthesisStream>,
    ended: Arc<Mutex<bool>>,
    failure: Arc<Mutex<Option<String>>>,
    paused: bool,
}

impl WindowsSpeech {
    pub fn new() -> Result<Self, String> {
        let synthesizer = SpeechSynthesizer::new().map_err(error)?;
        let player = MediaPlayer::new().map_err(error)?;
        player.SetAutoPlay(false).map_err(error)?;
        // Disable global media-key interception; the user operates PdfCraft's controls.
        player.CommandManager().and_then(|m| m.SetIsEnabled(false)).map_err(error)?;
        let ended = Arc::new(Mutex::new(false));
        let end = ended.clone();
        player
            .MediaEnded(&TypedEventHandler::new(move |_, _| {
                if let Ok(mut done) = end.lock() {
                    *done = true;
                }
                Ok(())
            }))
            .map_err(error)?;
        let failure = Arc::new(Mutex::new(None));
        let fail = failure.clone();
        player
            .MediaFailed(&TypedEventHandler::<MediaPlayer, windows::Media::Playback::MediaPlayerFailedEventArgs>::new(move |_, args| {
                if let Ok(mut message) = fail.lock() {
                    *message = Some(
                        args.as_ref()
                            .and_then(|a| a.ErrorMessage().ok())
                            .map_or_else(|| "Windows audio playback failed".into(), |s| s.to_string_lossy()),
                    );
                }
                Ok(())
            }))
            .map_err(error)?;
        Ok(Self { synthesizer, player, synthesis: None, stream: None, ended, failure, paused: false })
    }
}

impl Backend for WindowsSpeech {
    fn voices(&mut self) -> Result<Vec<Voice>, String> {
        let voices = SpeechSynthesizer::AllVoices().map_err(error)?;
        let n = voices.Size().map_err(error)?;
        if n > 256 {
            return Err("Windows reported too many speech voices".into());
        }
        (0..n)
            .map(|i| {
                let v = voices.GetAt(i).map_err(error)?;
                Ok(Voice {
                    id: v.Id().map_err(error)?.to_string_lossy(),
                    name: v.DisplayName().map_err(error)?.to_string_lossy(),
                    language: v.Language().map_err(error)?.to_string_lossy(),
                })
            })
            .collect()
    }

    fn start(&mut self, text: &str, options: &Options) -> Result<(), String> {
        self.stop()?;
        // A fresh player isolates delayed Ended/Failed callbacks from the previous chunk.
        // The old player's callbacks retain their own flags and cannot finish this chunk.
        *self = Self::new()?;
        let voice = if let Some(id) = &options.voice {
            let voices = SpeechSynthesizer::AllVoices().map_err(error)?;
            let n = voices.Size().map_err(error)?.min(256);
            let mut found = None;
            for i in 0..n {
                let v = voices.GetAt(i).map_err(error)?;
                if v.Id().map_err(error)?.to_string_lossy() == *id {
                    found = Some(v);
                    break;
                }
            }
            found.ok_or("the selected voice is not installed")?
        } else {
            SpeechSynthesizer::DefaultVoice().map_err(error)?
        };
        self.synthesizer.SetVoice(&voice).map_err(error)?;
        self.synthesizer.Options().and_then(|o| o.SetSpeakingRate(options.rate)).map_err(error)?;
        self.synthesis = Some(self.synthesizer.SynthesizeTextToStreamAsync(&HSTRING::from(text)).map_err(error)?);
        Ok(())
    }

    fn pause(&mut self) -> Result<(), String> {
        self.player.Pause().map_err(error)?;
        self.paused = true;
        Ok(())
    }
    fn resume(&mut self) -> Result<(), String> {
        self.paused = false;
        if self.stream.is_some() {
            self.player.Play().map_err(error)?;
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<(), String> {
        if let Some(op) = self.synthesis.take() {
            let _ = op.Cancel();
        }
        self.player.Pause().map_err(error)?;
        self.player.SetSource(None::<&IMediaPlaybackSource>).map_err(error)?;
        if let Some(stream) = self.stream.take() {
            let _ = stream.Close();
        }
        if let Ok(mut done) = self.ended.lock() {
            *done = false;
        }
        if let Ok(mut failure) = self.failure.lock() {
            *failure = None;
        }
        self.paused = false;
        Ok(())
    }

    fn finished(&mut self) -> Result<bool, String> {
        if let Some(message) = self.failure.lock().map_err(|_| "Windows audio state is unavailable")?.take() {
            return Err(message);
        }
        if let Some(op) = &self.synthesis {
            match op.Status().map_err(error)? {
                AsyncStatus::Completed => {
                    let stream = op.GetResults().map_err(error)?;
                    let source = MediaSource::CreateFromStream(&stream, &stream.ContentType().map_err(error)?).map_err(error)?;
                    self.player.SetSource(&source).map_err(error)?;
                    self.stream = Some(stream);
                    self.synthesis = None;
                    if !self.paused {
                        self.player.Play().map_err(error)?;
                    }
                }
                AsyncStatus::Error => return Err(op.ErrorCode().map_err(error)?.message()),
                AsyncStatus::Canceled => return Err("speech synthesis was cancelled".into()),
                _ => {}
            }
        }
        self.ended.lock().map(|done| *done).map_err(|_| "Windows audio state is unavailable".into())
    }
}

impl Drop for WindowsSpeech {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = self.player.Close();
        let _ = self.synthesizer.Close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "explicit native Windows audio check; requires installed voices and MediaPlayer"]
    fn windows_speech_pause_preserves_position_and_stop_releases_stream() {
        let mut b = WindowsSpeech::new().unwrap();
        let voices = b.voices().unwrap();
        assert!(!voices.is_empty());
        b.start(&"Questa è una prova sintetica di lettura. ".repeat(20), &Options { voice: Some(voices[0].id.clone()), rate: 0.9 }).unwrap();
        b.player.SetIsMuted(true).unwrap();
        let until = Instant::now() + Duration::from_secs(15);
        while b.player.PlaybackSession().unwrap().Position().unwrap().Duration < 2_000_000 {
            assert!(!b.finished().unwrap());
            assert!(Instant::now() < until, "audio did not start");
            std::thread::sleep(Duration::from_millis(30));
        }
        b.pause().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let paused = b.player.PlaybackSession().unwrap().Position().unwrap().Duration;
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(b.player.PlaybackSession().unwrap().Position().unwrap().Duration, paused, "pause must preserve the audio position");
        b.resume().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(b.player.PlaybackSession().unwrap().Position().unwrap().Duration > paused);
        b.stop().unwrap();
        assert!(b.stream.is_none() && b.synthesis.is_none());
    }
}
