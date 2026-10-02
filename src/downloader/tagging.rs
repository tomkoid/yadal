use std::fs::File;
use std::io::{self, Cursor};
use std::path::Path;

use lofty::config::{ParseOptions, WriteOptions};
use lofty::error::{FileEncodingError, FileParseError};
use lofty::file::{FileType, TaggedFileExt};
use lofty::flac::FlacFile;
use lofty::ogg::tag::VorbisComments;
use lofty::picture::error::PictureParseError;
use lofty::picture::{Picture, PictureType};
use lofty::prelude::*;
use lofty::probe::Probe;
use lofty::tag::{ItemValue, Tag, TagItem, TagType};

use crate::downloader::Downloader;
use crate::downloader::context::{ReplayGainValues, TagLyrics, TagReplayGain, TrackTagMetadata};

#[derive(Debug, thiserror::Error)]
pub enum TaggingError {
    #[error("I/O Error: {0}")]
    IO(#[from] io::Error),
    #[error("Failed to guess file type")]
    FileTypeGuessFailed,
    #[error("lofty file encoding error: {0}")]
    LoftyFileEncoding(#[from] FileEncodingError),
    #[error("lofty file parse error: {0}")]
    LoftyFileParse(#[from] FileParseError),
    #[error("lofty picture parse error: {0}")]
    LoftyPictureParse(#[from] PictureParseError),
}

impl Downloader {
    pub async fn tag_downloaded_file(
        &self,
        output_path: &Path,
        metadata: &TrackTagMetadata,
        log: &(dyn Fn(String) + Sync),
    ) -> Result<(), TaggingError> {
        let full_title = if let Some(ref version) = metadata.track_version {
            metadata.title.clone() + &format!(" ({version})")
        } else {
            metadata.title.clone()
        };

        let warn = |msg: &str| log(format!("{full_title}: {msg}"));

        // it might be better to strip the existing tags without saving it to disk and reopening
        strip_tags(output_path)?;

        // tag URL here as lofty's ItemKeys for URLs don't map to anything for Vorbis Comments
        // other tagging types like ID3v2 don't even support URL directly
        let file_type = if let Some(file_type) =
            Probe::open(output_path)?.guess_file_type()?.file_type()
        {
            match file_type {
                FileType::Flac => {
                    if let Some(ref url) = metadata.url {
                        let mut flac = FlacFile::read_from(
                            &mut File::open(output_path)?,
                            ParseOptions::new(),
                        )?;
                        let mut tag = VorbisComments::default();

                        tag.push("URL".to_string(), url.to_owned());

                        flac.set_vorbis_comments(tag);
                        flac.save_to_path(output_path, WriteOptions::default())?;
                    }
                }
                _ => warn("unable to write URL tag, file does not support Vorbis Comments (not a FLAC file)")
            }

            file_type
        } else {
            return Err(TaggingError::FileTypeGuessFailed);
        };

        let mut tagged_file = Probe::open(output_path)?.read()?;
        // the primary tag is the native tagging type of the container
        // e.g. FLAC: Vorbis Comments, MP3 and AAC: ID3v2
        let tag = match tagged_file.primary_tag_mut() {
            Some(tag) => tag,
            None => {
                let tag_type = tagged_file.primary_tag_type();
                tagged_file.insert_tag(Tag::new(tag_type));
                tagged_file.primary_tag_mut().unwrap()
            }
        };

        // lofty::tag::Tag.insert_text() returns a bool for whether the tag was successfully written or not
        // it's a general method that exposes many different tag types and not all of them are supported by every tagging format

        if let Some(ref album_artists) = metadata.album_artists {
            for (i, success) in single_tag_multiple_data(tag, ItemKey::AlbumArtist, album_artists)
                .iter()
                .enumerate()
            {
                if !success {
                    warn(&format!(
                        "failed to write album artist {}: \"{}\"",
                        i, album_artists[i]
                    ));
                }
            }
        }

        if let Some(ref album_title) = metadata.album_title {
            tag.set_album(album_title.to_owned());
        }

        if let Some(ref artists) = metadata.artists {
            for (i, success) in single_tag_multiple_data(tag, ItemKey::TrackArtist, artists)
                .iter()
                .enumerate()
            {
                if !success {
                    warn(&format!(
                        "failed to write artist {}: \"{}\"",
                        i, artists[i]
                    ));
                }
            }
        }

        if let Some(bpm) = metadata.bpm
            && !tag.insert_text(ItemKey::Bpm, (bpm.round() as u16).to_string())
        {
            warn("failed to write BPM");
        }

        if let Some(ref copyright) = metadata.copyright
            && !tag.insert_text(ItemKey::CopyrightMessage, copyright.to_owned())
        {
            warn("failed to write copyright");
        }

        if let Some(ref cover) = metadata.cover {
            let mut pic = Picture::from_reader(&mut Cursor::new(cover.data.clone()))?;
            pic.set_pic_type(PictureType::CoverFront);

            tag.push_picture(pic);
        }

        if let Some(disc_number) = metadata.disc_number {
            // lol why is it spelled with a k
            tag.set_disk(disc_number);
        }

        if let Some(ref isrc) = metadata.isrc
            && !tag.insert_text(ItemKey::Isrc, isrc.to_owned())
        {
            warn("failed to write ISRC");
        }

        if let Some(key) = metadata.key.as_deref()
            && let Some(key_scale) = metadata.key_scale.as_deref()
        {
            let key_scale = match key_scale.to_lowercase().as_str() {
                "major" => "", // no need to append anything for major keys
                "minor" => "m",
                _ => {
                    warn(&format!(
                        "unrecognized key scale '{}'. using original value",
                        key_scale,
                    ));
                    key_scale
                }
            };

            if !tag.insert_text(
                ItemKey::InitialKey,
                format!("{}{}", key, key_scale.to_lowercase()),
            ) {
                eprintln!("{}: failed to write initial key and key scale", full_title);
            }
        }

        let write_unsynced_lyrics = |tag: &mut Tag, lyrics: &str| {
            if !tag.insert_text(ItemKey::UnsyncLyrics, lyrics.to_owned()) {
                eprintln!("{}: failed to write unsynced lyrics", full_title);
            }
        };
        let write_synced_lyrics = |tag: &mut Tag, lyrics: &str| {
            if !tag.insert_text(ItemKey::Lyrics, lyrics.to_owned()) {
                eprintln!("{}: failed to write synced lyrics", full_title);
            }
        };
        match &metadata.lyrics {
            TagLyrics::UnsyncedOnly(unsynced) => write_unsynced_lyrics(tag, unsynced),
            TagLyrics::SyncedOnly(synced) => write_synced_lyrics(tag, synced),
            TagLyrics::UnsyncedAndSynced(unsynced, synced) => {
                // MP4 doesn't support unsynced lyrics as a distinct tag, don't write it if that's the file type
                match file_type {
                    FileType::Flac => {
                        write_unsynced_lyrics(tag, unsynced);
                        write_synced_lyrics(tag, synced);
                    }
                    _ => write_synced_lyrics(tag, synced),
                }
            }
            TagLyrics::None => {}
        }

        if let Some(ref release_date) = metadata.release_date
            && !tag.insert_text(ItemKey::RecordingDate, release_date.to_owned())
        {
            eprintln!("{}: failed to write release date", full_title);
        }

        if let Some(total_discs) = metadata.total_discs {
            tag.set_disk_total(total_discs);
        }

        if let Some(total_tracks) = metadata.total_tracks {
            tag.set_track_total(total_tracks);
        }

        if let Some(track_number) = metadata.track_number {
            tag.set_track(track_number);
        }

        if metadata.tag_title {
            tag.set_title(full_title.clone());
        }

        let write_replaygain_track = |tag: &mut Tag, values: &ReplayGainValues| {
            if !tag.insert_text(ItemKey::ReplayGainTrackGain, values.gain.clone()) {
                eprintln!("{}: failed to write track ReplayGain gain", full_title);
            }
            if !tag.insert_text(ItemKey::ReplayGainTrackPeak, values.peak.clone()) {
                eprintln!("{}: failed to write track ReplayGain peak", full_title);
            }
        };
        let write_replaygain_album = |tag: &mut Tag, values: &ReplayGainValues| {
            if !tag.insert_text(ItemKey::ReplayGainAlbumGain, values.gain.clone()) {
                eprintln!("{}: failed to write album ReplayGain gain", full_title);
            }
            if !tag.insert_text(ItemKey::ReplayGainAlbumPeak, values.peak.clone()) {
                eprintln!("{}: failed to write album ReplayGain peak", full_title);
            }
        };
        match &metadata.replaygain {
            TagReplayGain::TrackOnly(values) => write_replaygain_track(tag, values),
            TagReplayGain::AlbumOnly(values) => write_replaygain_album(tag, values),
            TagReplayGain::TrackAndAlbum(track_values, album_values) => {
                write_replaygain_track(tag, track_values);
                write_replaygain_album(tag, album_values);
            }
            TagReplayGain::None => {}
        }

        tag.save_to_path(output_path, WriteOptions::default())?;

        Ok(())
    }
}

fn strip_tags(path: &Path) -> Result<(), TaggingError> {
    let tag_types: Vec<TagType> = Probe::open(path)?
        .read()?
        .tags()
        .iter()
        .map(|t| t.tag_type())
        .collect();

    for tag_type in tag_types {
        tag_type.remove_from_path(path, WriteOptions::default())?;
    }

    Ok(())
}

fn single_tag_multiple_data<S: AsRef<str>>(tag: &mut Tag, key: ItemKey, values: &[S]) -> Vec<bool> {
    let mut successes = Vec::<bool>::with_capacity(values.len());

    for v in values {
        successes.push(tag.push(TagItem::new(key, ItemValue::Text(v.as_ref().to_string()))));
    }

    successes
}
