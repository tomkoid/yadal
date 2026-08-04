use std::path::PathBuf;

use anyhow::Context;

pub fn prepare_output_directory() -> anyhow::Result<PathBuf> {
    let audio_dir = dirs::audio_dir().context("Failed to get user's music directory")?;
    let yadal_audio_dir = audio_dir.join("yadal");

    match yadal_audio_dir.exists() {
        true => Ok(yadal_audio_dir),
        false => {
            std::fs::create_dir_all(&yadal_audio_dir)
                .context("Failed to create yadal directory in music directory")?;
            Ok(yadal_audio_dir)
        }
    }
}
