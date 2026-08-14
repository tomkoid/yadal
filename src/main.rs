use std::process::exit;

use anyhow::{Context, Result};
use clap::Parser;

mod args;
mod auth;
mod downloader;
mod options;
mod output;
mod parser;
mod types;

use auth::{authenticate, load_or_authenticate};
use downloader::Downloader;
use types::MediaType;

use crate::{
    args::{Cli, MediaTypeArg},
    options::DownloaderOptions,
    output::prepare_output_directory,
    parser::parse_tidal_input,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // authenticate
    let mut client = if cli.reauth {
        println!("forcing re-authentication...\n");
        authenticate(&cli.session_file, cli.oauth2).await?
    } else {
        load_or_authenticate(&cli.session_file, cli.oauth2).await?
    };

    // refresh user info, thus validating the session and ensuring we have the latest user info
    client.refresh_user_info().await?;
    let user_info = client.user_info.as_ref().unwrap();
    println!(
        "logged in as: {} ({})\n",
        user_info.user_id, user_info.username
    );

    if cli.id.contains("upload") {
        eprintln!(
            "error: uploads are not supported yet. please provide a valid track, album, or playlist ID."
        );
        exit(1);
    }

    // parse ID and determine media type
    let (media_id, detected_type) = parse_tidal_input(&cli.id);

    let media_type = match cli.media_type {
        MediaTypeArg::Track => MediaType::Track,
        MediaTypeArg::Album => MediaType::Album,
        MediaTypeArg::Playlist => MediaType::Playlist,
        MediaTypeArg::Auto => detected_type,
    };

    let output_path = match cli.output {
        Some(path) => path,
        None => prepare_output_directory().context("Failed to prepare output directory")?,
    };

    // this is not necessarilly needed right now but will be used if a config file is added
    let options = DownloaderOptions {
        media_type,
        output_path,
        audio_quality: cli.quality.into(),
        force_download: cli.force,
        max_parallel: cli.parallel,
    };

    println!("audio quality: {:?}", options.audio_quality);
    println!("media type: {:?}", options.media_type);
    println!("output directory: {}\n", options.output_path.display());

    // create downloader
    let downloader = Downloader::new(client, options);

    // download based on type
    let summary = match media_type {
        MediaType::Track => {
            println!("downloading track {}...", media_id);
            downloader.download_track(&media_id).await?
        }
        MediaType::Album => {
            println!("downloading album {}...", media_id);
            downloader
                .download_media(&media_id, MediaType::Album)
                .await?
        }
        MediaType::Playlist => {
            println!("downloading playlist {}...", media_id);
            downloader
                .download_media(&media_id, MediaType::Playlist)
                .await?
        }
    };

    summary.print();
    exit(summary.get_exit_code());
}
