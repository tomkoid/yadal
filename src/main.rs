use std::fs::create_dir_all;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

mod args;
mod auth;
mod config;
mod downloader;
mod parser;
mod tracing;
mod types;

use auth::{authenticate, load_or_authenticate};
use downloader::Downloader;
use types::MediaType;

use crate::args::Commands;
use crate::config::expand_home_symbol;
use crate::{
    args::{Cli, MediaTypeArg},
    config::FileConfig,
    downloader::{
        config::DownloaderConfig, config::DownloaderConfigDownload, config::DownloaderConfigTags,
        ui::summary::DownloadSummary,
    },
    parser::parse_id_input,
};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.trace {
        tracing::configure();
    }

    match cli.command {
        Commands::InitConfigFile => cmd_init_config_file(),
        command @ Commands::Download { .. } => cmd_download(command).await,
    }
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

fn cmd_init_config_file() -> Result<()> {
    let path = FileConfig::init_default_config()?;

    println!(
        "generated fully defaulted config file at: {}",
        path.display()
    );

    Ok(())
}

async fn cmd_download(command: Commands) -> Result<()> {
    let Commands::Download {
        id,
        media_type,
        quality,
        output,
        template,
        range,
        parallel,
        reauth,
        oauth2,
        force,
        no_stream_check,
        skip_tag,
        lyrics,
        skip_transcode,
        session_file,
    } = command
    else {
        unreachable!();
    };

    if id.iter().any(|s| s.contains("upload")) {
        bail!(
            "uploads are not supported yet. please provide a valid track, album, or playlist ID."
        );
    }

    // parse IDs and determine media type
    let targets = if let Some(targets) = parse_id_input(&id) {
        targets
    } else {
        bail!("invalid or malformed TIDAL URL/ID.");
    };

    // authenticate
    let mut client = if reauth {
        println!("forcing re-authentication...\n");
        authenticate(&session_file, oauth2).await?
    } else {
        load_or_authenticate(&session_file, oauth2).await?
    };

    // refresh user info, thus validating the session and ensuring we have the latest user info
    client.refresh_user_info().await?;
    let user_info = client.user_info.as_ref().unwrap();
    println!(
        "logged in as: {} ({})\n",
        user_info.user_id, user_info.username
    );

    // get config from file
    let config = FileConfig::try_new()?;

    let d = &config.download;
    let t = &config.tags;

    let download_path = if let Some(path) = expand_home_symbol(&d.output_path) {
        path
    } else {
        bail!("failed to expand output_path home symbol.");
    };

    if !PathBuf::from(&download_path).try_exists()? {
        create_dir_all(&download_path)?;
    }

    let options = DownloaderConfig {
        download: DownloaderConfigDownload {
            audio_quality: quality.unwrap_or(d.audio_quality).into(),
            output_path: output.unwrap_or(download_path.clone()),
            output_template: template.unwrap_or(d.output_template.clone()),
            force_download: force.unwrap_or(d.force_download),
            no_stream_check: no_stream_check.unwrap_or(d.no_stream_check),
            max_parallel: parallel.unwrap_or(d.max_parallel),
            range,
            skip_transcode: skip_transcode.unwrap_or(d.skip_transcode),
        },
        tags: DownloaderConfigTags {
            enable: !(skip_tag.unwrap_or(!t.enable)),
            lyrics: lyrics.unwrap_or(t.lyrics.clone().into()).into(),
            album: t.album,
            album_artist: t.album_artist,
            artist: t.artist,
            bpm: t.bpm,
            copyright: t.copyright,
            cover: t.cover,
            date: t.date,
            disc_number: t.disc_number,
            isrc: t.isrc,
            initial_key_and_key_scale: t.initial_key_and_key_scale,
            replaygain: t.replaygain.clone(),
            title: t.title,
            total_discs: t.total_discs,
            total_tracks: t.total_tracks,
            track_number: t.track_number,
            track_version: t.track_version,
            url: t.url,
        },
    };

    // create downloader
    let mut downloader = Downloader::new(client, options.clone());

    println!("audio quality: {:?}", options.download.audio_quality);
    println!(
        "output directory: {}",
        options.download.output_path.display()
    );

    print_full_line();

    let mut summaries: Vec<DownloadSummary> = Vec::new();
    for target in targets {
        downloader.reset_state();

        let media_type = match media_type {
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

    if !total_summary.did_fail() {
        Ok(())
    } else {
        bail!("download(s) failed.");
    }
}
