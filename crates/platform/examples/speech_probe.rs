//! Explicit manual Windows smoke check. Does not speak unless invoked with --speak.
use pdfcraft_platform::speech::{Chunk, Options, Player, State};
use std::time::{Duration, Instant};

fn main() -> Result<(), String> {
    let mut player = Player::new();
    player.refresh_voices()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let status = player.status();
        if let Some(e) = status.error {
            return Err(e);
        }
        if !status.voices.is_empty() {
            for voice in status.voices {
                println!("{}: {} ({})", voice.id, voice.name, voice.language);
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err("no installed voice was returned within 10 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    if !std::env::args().any(|a| a == "--speak") {
        return Ok(());
    }
    player.start(
        vec![Chunk {
            page: 1,
            text: "This is a synthetic PdfCraft read aloud test. The player can pause, resume, and stop without changing the document.".into(),
        }],
        Options::default(),
    )?;
    let started = Instant::now();
    let mut paused = false;
    let mut resumed = false;
    loop {
        let status = player.status();
        if let Some(e) = status.error {
            return Err(e);
        }
        if !paused && started.elapsed() > Duration::from_secs(2) {
            player.pause()?;
            paused = true;
            println!("pause requested");
        }
        if paused && !resumed && started.elapsed() > Duration::from_secs(4) {
            player.resume()?;
            resumed = true;
            println!("resume requested");
        }
        if status.state == State::Completed {
            println!("completed");
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            player.stop()?;
            return Err("speech did not finish within 30 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    Ok(())
}
