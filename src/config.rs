use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::args::QualityArg;

#[derive(Deserialize, Serialize)]
pub struct FileConfig {
    pub output_path: PathBuf,
    pub audio_quality: QualityArg,
    pub max_parallel: usize,
    pub force_download: bool,
    pub no_stream_check: bool,
    pub skip_tag: bool,
    pub skip_transcode: bool,
    pub lyrics: bool,
}

impl Default for FileConfig {
    fn default() -> Self {
        let config_dir = dirs::home_dir().expect("Failed to get user's config directory");
        let default_output_path = config_dir.join("Music").join("yadal");

        FileConfig {
            output_path: default_output_path,
            audio_quality: QualityArg::Lossless,
            max_parallel: 5,
            force_download: false,
            no_stream_check: false,
            skip_tag: false,
            skip_transcode: false,
            lyrics: false,
        }
    }
}

impl FileConfig {
    pub fn try_new() -> Result<Self, Box<dyn std::error::Error>> {
        let config = FileConfig::default();
        config.get_config()
    }

    fn get_config(&self) -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = self.get_default_path();

        // check if config file exists
        if config_path.exists() {
            return self.load_from_file(&config_path);
        } else {
            let default_config = FileConfig::default();
            default_config.save_to_file(&config_path)?;
            return Ok(default_config);
        }
    }

    fn get_default_path(&self) -> PathBuf {
        let config_dir = dirs::config_dir();

        if let Some(config_dir) = config_dir {
            return config_dir.join("yadal").join("config.toml");
        } else {
            panic!("Failed to get user's config directory, something is wrong here.");
        }
    }
    //
    fn load_from_file(&self, path: &PathBuf) -> Result<FileConfig, Box<dyn std::error::Error>> {
        let config_str = std::fs::read_to_string(path)?;
        let config: FileConfig = self.from_toml(&config_str)?;
        Ok(config)
    }

    fn save_to_file(&self, path: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
        let toml_str = toml::to_string(self)?;
        std::fs::write(path, toml_str)?;
        Ok(())
    }

    pub fn from_toml(&self, toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }
}

#[test]
fn test_config_deserialization() {
    let toml_str = r#"
        output_path = "/path/to/output"
        audio_quality = "high"
        max_parallel = 4
        force_download = true
        no_stream_check = false
        skip_tag = false
        skip_transcode = true
        lyrics = true
    "#;

    let config: FileConfig = toml::from_str(toml_str).expect("Failed to deserialize config");

    assert_eq!(config.output_path, PathBuf::from("/path/to/output"));
    assert_eq!(config.audio_quality, QualityArg::High);
    assert_eq!(config.max_parallel, 4);
    assert!(config.force_download);
    assert!(!config.no_stream_check);
    assert!(!config.skip_tag);
    assert!(config.skip_transcode);
    assert!(config.lyrics);
}
