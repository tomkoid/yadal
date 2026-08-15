use std::process::exit;

use anyhow::{Context, Result};
use clap::Parser;

mod args;
mod auth;
mod downloader;
mod output;
mod parser;
mod types;

use auth::{authenticate, load_or_authenticate};
use downloader::Downloader;
use types::MediaType;

use crate::{
    args::{Cli, MediaTypeArg},
    downloader::{config::DownloaderConfig, ui::summary::DownloadSummary},
    output::prepare_output_directory,
    parser::parse_id_input,
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

    // parse IDs and determine media type
    let targets = parse_id_input(&cli.id);

    let output_path = match cli.output {
        Some(path) => path,
        None => prepare_output_directory().context("Failed to prepare output directory")?,
    };

    // check if ffmpeg is available for transcoding
    let can_transcode = which::which("ffmpeg").is_ok();
    let skip_transcode = cli.skip_transcode || !can_transcode;

    if !can_transcode && !cli.skip_transcode {
        eprintln!(
            "warning: ffmpeg not found, skipping transcoding. Install ffmpeg to enable transcoding.\n"
        );
    }

    // this is not necessarilly needed right now but will be used if a config file is added
    let options = DownloaderConfig {
        output_path,
        audio_quality: cli.quality.into(),
        force_download: cli.force,
        max_parallel: cli.parallel,
        lyrics: cli.lyrics,
        range: cli.range,
        skip_tag: cli.skip_tag,
        skip_transcode,
    };

    // create downloader
    let mut downloader = Downloader::new(client, options.clone());

    println!("audio quality: {:?}", options.audio_quality);
    println!("output directory: {}", options.output_path.display());

    print_full_line();

    let mut summaries: Vec<DownloadSummary> = Vec::new();
    for target in targets {
        downloader.reset_state();

        let media_type = match cli.media_type {
            MediaTypeArg::Track => MediaType::Track,
            MediaTypeArg::Album => MediaType::Album,
            MediaTypeArg::Playlist => MediaType::Playlist,
            MediaTypeArg::Auto => target.media_type,
        };

        // download based on type
        let summary = match media_type {
            MediaType::Track => {
                println!("downloading track {}...", target.id);
                downloader.download_track(&target.id).await?
            }
            MediaType::Album => {
                println!("downloading album {}...", target.id);
                downloader
                    .download_media(&target.id, MediaType::Album)
                    .await?
            }
            MediaType::Playlist => {
                println!("downloading playlist {}...", target.id);
                downloader
                    .download_media(&target.id, MediaType::Playlist)
                    .await?
            }
        };

        println!(
            "summary for {}: {} downloaded, {} skipped, {} failed",
            target.id,
            summary.downloaded,
            summary.skipped,
            summary.failed.len()
        );

        summaries.push(summary);

        print_full_line();
    }

    let mut total_summary = DownloadSummary::new();
    for summary in summaries {
        total_summary.downloaded += summary.downloaded;
        total_summary.skipped += summary.skipped;
        total_summary.failed.extend(summary.failed);
    }

    total_summary.print();
    exit(total_summary.get_exit_code());
}

fn print_full_line() {
    match crossterm::terminal::size() {
        Ok((width, _)) => {
            println!("{}", "=".repeat(width as usize));
        }
        Err(_) => {
            println!("{}", "=".repeat(15));
        }
    }
}
