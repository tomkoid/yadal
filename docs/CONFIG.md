# Config

The configuration file is at `yadal/config.toml` within your platform's configuration directory, those being:
- Windows: `~\AppData\Roaming`
- macOS: `~/Library/Application Support`
- Linux: `$XDG_CONFIG_HOME`, falling back to `~/.config` if not present

## Values

There are two tables in the configuration file with their possible values listed below.  
If you are unsure of how to format them in the configuration file, run `--init-config-file` in yadal to generate one with every possible field written out with its default value.

### download

These options are for downloading tracks and the downloader itself.

| Field | Explanation | Type | Default | Possible Values |
| ----- | ----------- | ---- | ------- | --------------- |
| `output_path`     | Downloaded tracks location | string | `~/Music/yadal` | any |
| `output_template` | Downloaded track location template (see [templating](#templating)) | string | `{{ album.artists[0] }}/{{ album.title }}/{{ \\"%02d\\"|format(track.number) }} {{ track.title }}{% if track.version %} ({{ track.version }}){% endif %}{% if track.explicit %} (Explicit){% endif %}.{{ track.extension }}` | any |
| `audio_quality`   | Track audio quality | enum | hi-res | `low`, `high`, `lossless`, `hi-res` |
| `max_parallel`    | Maximum number of parallel track downloads | u32 | 5 | any |
| `force_download`  | Whether to overwrite existing files | bool | false | any |
| `no_stream_check` | Whether to skip checking if the stream URL exists before downloading | bool | false | any |
| `skip_transcode`  | Whether to skip transcoding hi-res `.m4a` tracks to `.flac` | bool | false | any |

### tags

These options are for deciding whether specific tags are to be written to the output track files.

| Field | Explanation | Type | Default | Possible Values |
| ----- | ----------- | ---- | ------- | --------------- |
| enable | Whether tagging is enabled at all | bool | true | any |
| album | Album name | bool | true | any |
| album_artist | Album artist(s) | bool | true | any |
| artist | Track artists(s) | bool | true | any |
| bpm | BPM | bool | true | any |
| copyright | Copyright text | bool | true | any |
| cover | Album cover image | bool | true | any |
| date | Album release date | bool | true | any |
| disc_number | Disc/volume number | bool | true | any |
| isrc | [Wikipedia](https://en.wikipedia.org/wiki/International_Standard_Recording_Code) | bool | true | any |
| initial_key_and_key_scale | Track initial key and key scale | bool | true | any |
| lyrics | Unsynced and or synced lyrics | enum | `unsynced-and-synced` | `none`, `unsynced-only`, `synced-only`, `unsynced-and-synced` |
| replaygain | [Wikipedia](https://en.wikipedia.org/wiki/ReplayGain) | enum | `track-and-album` | `none`, `track-only`, `album-only`, `track-and-album` |
| title | Track title | bool | true | any |
| total_discs | Total amount of album discs/volumes | bool | true | any |
| total_tracks | Total amount of album tracks | bool | true | any |
| track_number | Track number in album | bool | true | any |
| track_version | Different variant of the same song, e.g. a remaster or remix | bool | true | any |
| url | TIDAL track URL | bool | true | any |

## Templating

The templating language is MiniJinja.  
You can learn its syntax [here](https://docs.rs/minijinja/latest/minijinja/syntax/index.html).

If you're not bothered to read that documentation, here's a basic overview:
- The template is a normal string of text.
- You can include a template variable by wrapping it in double curly braces and a space (inside) on each side of the variable name, e.g. `albums/{{ album.title }}`.

## Template variables

Every value in a specific section is found underneath that section's name.  
e.g. `artist` from the `track` section can be used with `track.artist` in the template.

The template variable types in the tables listed below are MiniJinja types.  
Note that not all variables will exist for a certain track, album, or playlist. It is good practice to check if it exists first like how the default template does.

### Track

| Variable | Explanation | Type |
| -------- | ----------- | ---- |
| artists | Track artists(s) | list of strings |
| bpm | BPM | int |
| copyright | Copyright text | string |
| disc_number | Disc/volume number | int |
| explicit | Explicitness of the track| bool |
| extension | The file extension of the track | string |
| id | TIDAL track ID | string |
| isrc | [Wikipedia](https://en.wikipedia.org/wiki/International_Standard_Recording_Code) | string |
| initial_key_and_key_scale | Track initial key and key scale | string |
| number | Track number in album | int |
| title | Track title | string |
| version | Different variant of the same song, e.g. a remaster or remix | string |
| url | TIDAL track URL | string |

### Album

| Variable | Explanation | Type |
| -------- | ----------- | ---- |
| artists | Album artist(s) | list of strings |
| explicit | Explicitness of the album | bool |
| release.day | Album release date day | int |
| release.month | Album release date month | int |
| release.year | Album release date year | int |
| id | TIDAL album ID | string |
| title | Album name | string |
| total_discs | Total amount of album discs/volumes | int |
| total_tracks | Total amount of album tracks | int |
| url | TIDAL album URL | string |

### Playlist

| Variable | Explanation | Type |
| -------- | ----------- | ---- |
| created.day | What day the playlist was created | int |
| created.hour | What hour the TIDAL playlist was created | int |
| created.minute | What minute the TIDAL playlist was created | int |
| created.month | What month the playlist was created | int |
| created.second | What second the TIDAL playlist was created | int |
| created.year | What year the playlist was created | int |
| index | Track number in the playlist | int |
| title | Playlist name | string |
| updated.day | What day the playlist was updated | int |
| updated.hour | What hour the TIDAL playlist was updated | int |
| updated.minute | What minute the TIDAL playlist was updated | int |
| updated.month | What month the playlist was updated | int |
| updated.second | What second the TIDAL playlist was updated | int |
| updated.year | What year the playlist was updated | int |
| url | Playlist TIDAL URL | string |
| uuid | Playlist TIDAL UUID | string |
