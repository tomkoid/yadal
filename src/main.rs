use std::path::PathBuf;
use std::{fs::create_dir_all, process::exit};

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

    if cli.init_config_file {
        let path = FileConfig::init_default_config()?;

        println!(
            "generated fully defaulted config file at: {}",
            path.display()
        );
        return Ok(());
    }

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
    let targets = if let Some(targets) = parse_id_input(&cli.id) {
        targets
    } else {
        bail!(
            "invalid/malformed TIDAL URL/ID: nearly all formats in a TIDAL URL, as well as a raw ID/UUID are supported"
        );
    };

    // get config from file
    let config = FileConfig::try_new()?;

    let d = &config.download;
    let t = &config.tags;

    let download_path = if let Some(path) = expand_home_symbol(&d.output_path) {
        path
    } else {
        bail!("failed to expand output_path home symbol");
    };

    if !PathBuf::from(&download_path).try_exists()? {
        create_dir_all(&download_path)?;
    }

    let options = DownloaderConfig {
        download: DownloaderConfigDownload {
            audio_quality: cli.quality.unwrap_or(d.audio_quality).into(),
            output_path: cli.output.unwrap_or(download_path.clone()),
            output_template: cli.template.unwrap_or(d.output_template.clone()),
            force_download: cli.force.unwrap_or(d.force_download),
            no_stream_check: cli.no_stream_check.unwrap_or(d.no_stream_check),
            max_parallel: cli.parallel.unwrap_or(d.max_parallel),
            range: cli.range,
            skip_transcode: cli.skip_transcode.unwrap_or(d.skip_transcode),
        },
        tags: DownloaderConfigTags {
            enable: !(cli.skip_tag.unwrap_or(!t.enable)),
            lyrics: cli.lyrics.unwrap_or(t.lyrics.clone().into()).into(),
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
