# Video survey: what egui-pop needs, and whether embedded players can play inside

This survey asks what egui-pop's applications need from video, which
native players could serve them, and whether videos hosted online
(YouTube, Vimeo, Dailymotion, PeerTube, Twitch, plain video files on the
web) can be shown inside an application rather than in the browser. It
covers the use cases and what each needs, what other applications do,
the native playback backends for the desktop, every way of showing an
online video inside (the service's own player in a web view, our own
player, the web build), the terms of service of each video host, and
ends with ranked follow-ups.

It feeds the native `Video` widget planned after the viewer's video page
([Links and outside content](links.md#the-video-page)) and decides what
happens to the YouTube trial of
[ADR 0017](adr/0017-links-and-outside-content.md#youtube-inside-a-trial-amended-2026-11-03).
The web side extends the [web survey](web-survey.md)'s rows M1 to M3 and
its follow-up 21.

Read and run on 2026-11-04, against egui-pop's `main` (cc28fc41), egui
0.37, wgpu 30, on Fedora 44 with GStreamer 1.28.8 and WebKitGTK 2.54.2,
an AMD Radeon 680M (Rembrandt) with Mesa's VA-API driver. Windows and
macOS were **not run**; what is said of them is read. Every finding
says where it comes from:

- **[code]**: read in egui-pop's source, or a crate's source.
- **[docs]**: read in a document, linked.
- **[tried]**: run on the machine above, headless (`gst-launch-1.0` and
  `ffmpeg` on generated clips, `vainfo`, `dnf`); no window was opened.

## Summary

- **YouTube can play inside the application, but only in YouTube's own
  player, in a web view.** YouTube's terms allow an application to show
  YouTube content through its embedded player and forbid everything else:
  reaching the streams by other means, keeping them, separating sound from
  picture, hiding ads or branding, or covering the player. So a native
  YouTube player (a stream extracted the way `yt-dlp` does it, played by
  GStreamer or mpv) is **ruled out**, however well it would work. The web
  view trial (wry over WebKitGTK, Linux) already does it the permitted way,
  and it plays [tried in ADR 0017's trial]. Vimeo, Dailymotion and Twitch
  are in the same position; PeerTube and plain video files on the web are
  not, and our own player can play them.
- **A web view inside a panel is a native window on top of egui.** It
  plays well, but egui's menus and popups go under it, and the keyboard
  leaves egui while it has the focus. On Linux it can be inside the panel
  only on X11; on Wayland it opens in its own window. Windows (WebView2)
  and macOS (WKWebView) can host it inside the panel. Drawing a web page
  into a texture instead (CEF, Servo, Ultralight, WPE) would fix the
  overlay, at the price of a second browser engine of 100 MB or more, or a
  licence `deny.toml` refuses: not worth it for a help video.
- **For local files, GStreamer stays the best backend**, and the cheapest
  improvement is in how we use it: on this machine the viewer's pipeline
  plays a 4K VP9 clip at about **22 frames a second**, under real time,
  because the hardware decoder's frames are copied back and converted to
  RGBA on the CPU; converting them on the GPU first (`vapostproc`) brings
  it to about **104 frames a second** [tried]. libmpv is a fine player but
  is GPL as distributions build it, and its usual Rust crates are LGPL;
  FFmpeg's crates are a decoder without a player, under the WTFPL; the
  egui video crates found are old, tied to another egui, or new and
  untried.
- **What egui-pop's applications need is mostly small, quiet video**: a
  preview that loops without sound, a clip in a photo library with a poster
  frame and a scrub bar, a tutorial with full controls. Helios's own
  minimum, like the darktable request it echoes, is to *show that a video
  is there* (a thumbnail and a badge), before playing it at all.
- **Ranked follow-ups** are at the [end](#follow-ups-ranked): the native
  `Video` widget first, the GPU conversion with it, the YouTube trial kept
  as an opt-in, the other hosts' players behind the same opt-in, plain
  web videos through our own player, and the web build's overlays.

## egui-pop today

- **The viewer's video page** (feature `video`) plays a local file in a
  panel: Play and Pause, a position slider, the volume, the time; Space,
  Left, Right, Up and Down on the picture; it opens paused, stops when its
  panel is hidden and comes back at the same place
  ([links.md](links.md#the-video-page)) [code]
  (`crates/egui-pop-views/src/viewer/video.rs`).
- **Behind a trait**: `VideoBackend` opens a `VideoPlayer` (poll, frame,
  play, pause, seek, volume); a host can give its own, a test a pretend
  one. The page knows nothing of GStreamer [code].
- **GStreamer** (`gst_player.rs`) builds a `playbin`, gives it an
  `appsink` that asks for RGBA, keeps only the last frame, and asks egui for
  a repaint; the bus wakes the interface on errors, end of stream and state
  changes [code]. Each frame then goes through `rgba_image` (a copy into a
  new buffer), `ColorImage::from_rgba_unmultiplied` (a pass to premultiply)
  and `TextureHandle::set` (egui-wgpu's upload): with GStreamer's own
  conversion, four passes over every frame on the CPU [code].
- **No widget**: a page or a panel cannot place a video of its own, and
  there is no bare mode (autoplay, loop, muted, no controls) [code].
- **YouTube** (feature `webview`, Linux): a YouTube link plays in a web
  view, wry over WebKitGTK, YouTube's privacy-enhanced player loaded as the
  page with a `Referer` header (without one YouTube answers its error
  153); a child window over the panel on X11, a window of its own on
  Wayland or for a detached panel; every other navigation goes outside
  ([links.md](links.md#the-youtube-page)) [code], and it played in the
  trial ([ADR 0017's analysis](adr/0017-links-and-outside-content-analysis.md#youtube-in-a-web-view-the-trial))
  [tried there].
- **Everything else** online (Vimeo, a remote `.mp4`) opens in the
  browser; the web build has no video and opens video links in a new tab
  ([web survey](web-survey.md#media-and-embeds), M1 to M3) [code].
- **The file picker** previews videos only through the system's
  thumbnailers (feature `platform-thumbnails`): on this machine
  `totem` and `gst-video-thumbnailer` are installed for that
  ([files.md](files.md)) [code], [tried] (`/usr/share/thumbnailers`).

## Needs

### Use cases

| Use case | Who | What it shows | Where |
|---|---|---|---|
| U1. Clips in a photo library | Helios | short clips and timelapses shot with the photos, beside them in the library | the library grid (a thumbnail), the single-image view (playback) |
| U2. Timelapse review | Helios | an image sequence or an exported timelapse, played to check flicker and pacing | a viewer, frame by frame |
| U3. Tutorials and help | every application | a "how to" video linked from a tip, a help page or the about page; often on YouTube | the viewer, a help panel |
| U4. Gallery demos | egui-pop | a widget's behaviour as a short looping clip; the video widget itself | gallery pages |
| U5. File picker preview | every application | the selected video's first frame, maybe playing muted | the picker's preview pane |
| U6. Onboarding and empty states | every application | a short silent loop showing a gesture | a welcome page, an empty panel |

Helios's own context: it is a raw photo editor first; video as a domain
is explicitly not built, only kept possible (Helios's roadmap). The
users it comes from asked darktable for **basic support for video files**
in the lighttable: the request, closed without an answer, wants clips
imported and shown as a thumbnail or a badge so they are not forgotten on
the card, and explicitly not playback
([darktable#11576](https://github.com/darktable-org/darktable/issues/11576))
[docs].

### What each use case needs

| Need | U1 library | U2 timelapse | U3 tutorial | U4 demo | U5 picker | U6 onboarding |
|---|---|---|---|---|---|---|
| Controls | compact (play, scrub, time) | full, plus frame step | full | none | none or compact | none |
| Autoplay | no | no | no (a click first) | yes | on hover or selection | yes |
| Loop | optional | yes | no | yes | yes | yes |
| Muted | starts muted or not at all | no sound | sound | muted | muted | muted |
| Poster frame | yes, also as the library thumbnail | first frame | the host's thumbnail, after a click (ADR 0017's rule) | first frame | yes | first frame |
| Scrub with thumbnails | nice to have | yes | the host's player | no | no | no |
| Frame-accurate stepping | no | **yes** | no | no | no | no |
| Audio | yes | no | yes | no | no | no |
| Subtitles | no | no | yes (the host's player has them) | no | no | no |
| HDR | phones record HLG and Dolby Vision: shown tone-mapped, not wrong | no | no | no | no | no |
| Formats | H.264, HEVC (phones), AV1, ProRes and camera MOV/MP4/MTS | image sequences, MP4, ProRes | the host's | WebM or MP4 we ship | whatever is on disk | WebM or MP4 we ship |
| Online | no | no | **yes** (YouTube first) | no | no | no |

Two things follow. **Most uses want a bare, muted, looping picture**
(U4, U5, U6), which today's viewer page cannot be: that is the case for a
widget with a "no controls" mode. And **only U3 needs online video**, and
there the host's own player, with its subtitles, quality menu and
captions, is what the user expects anyway.

### What other applications do

| Application | Video | What carries over |
|---|---|---|
| **Lightroom Classic** | plays clips in the Library module, trims them, sets a **poster frame** (Frame > Set Poster Frame) and **captures a frame** as a JPEG stacked with the clip; AVI, MP4, MOV, AVCHD ([Adobe](https://helpx.adobe.com/lightroom-classic/help/video-lightroom.html)) [docs, from search excerpts: the page refused the fetch] | a compact player in the single-image view; the poster frame is also the grid thumbnail; frame capture is a photo editor's natural video feature |
| **darktable** | no video: it does not import, show or edit clips ([forum](https://discuss.pixls.us/t/do-we-want-darktable-to-copy-and-import-movies-from-our-sd-cards/38560)); the request for basic support was closed unanswered ([#11576](https://github.com/darktable-org/darktable/issues/11576)) [docs] | the gap Helios can fill cheaply: show clips, then play them |
| **digiKam** | plays videos in its preview and slideshow; it dropped QtAV, Qt 6 builds use QtMultimedia over FFmpeg or the system's backend ([digikam-users, 2024-10](https://mail.kde.org/pipermail/digikam-users/2024-October/036584.html)) [docs] | a system-level player behind a toolkit layer, as `VideoBackend` |
| **VS Code** | its built-in media preview opens images, audio and video files in Electron's `<video>`; MP4 "does not support aac audio tracks" (Electron's codecs) ([extension's README](https://gitea.com/karlorz/vscode-1/src/branch/main/extensions/media-preview), a mirror) [docs] | a preview with the platform's codecs, honest about what it cannot play |
| **Obsidian** | a YouTube link written as an image (`![](url)`) shows YouTube's player as an `<iframe>` inside the note ([help](https://obsidian.md/help/embed-web-pages)); the Web viewer core plugin, desktop only, off by default, opens pages in tabs ([help](https://obsidian.md/help/plugins/web-viewer)) [docs] | the host's player in place, in the content, for online video; local files in the app's own player |

So: the photo applications (Lightroom, digiKam) play local clips with
compact controls and a poster frame; none plays online video; the
note-taking and editor applications built on a browser engine show
YouTube's player in place, because they have the engine anyway.

## Native playback

### What was measured

GStreamer's pipelines were timed headless with `gst-launch-1.0`, every
sink unsynchronised (`sync=false`) so the time is what decoding and
conversion cost, on two clips made with `ffmpeg`'s test source: 10 s of
**4K (3840x2160) VP9 at 30 frames a second** (300 frames) and 10 s of
1080p H.264 (OpenH264) [tried]. The viewer's own shape is `playbin` with
a video sink that only accepts RGBA, as its `appsink` does.

| Pipeline (4K VP9, 300 frames) | Time | Frames a second |
|---|---|---|
| software decode (`vp9dec`), frames left as decoded | 4.35 s | 69 |
| software decode, converted to RGBA (`videoconvert`) | 5.64 s | 53 |
| hardware decode (`vavp9dec`), frames left on the GPU | 2.10 s | 143 |
| hardware decode, converted to RGBA on the GPU (`vapostproc`), then in memory | 2.69 s | 111 |
| hardware decode, copied to memory as NV12, converted on the CPU | 13.77 s | 22 |
| **`playbin`, an RGBA sink: the viewer's shape today** | **13.33 s** | **22** |
| the same with the VA decoder's rank set to none (software decode) | 4.35 s | 69 |
| `playbin`, the sink `videoconvert n-threads=0 ! RGBA` (all 12 threads) | 5.07 s | 59 |
| `playbin`, the sink `vapostproc ! RGBA` | 2.89 s | 104 |
| `playbin`, 1080p H.264, RGBA sink | 1.15 s | 261 |

So **today's backend cannot play this 4K clip in real time on this
machine**: `playbin` picks the hardware decoder (`vavp9dec`'s rank is
primary + 1, above `vp9dec`'s primary), then its own `videoconvert` copies
every frame back and converts it on one CPU thread (its `n-threads`
defaults to 1) [tried, `gst-inspect-1.0`]. Asking the GPU to convert
(`vapostproc`, or GL with `glcolorconvert` and `gldownload`) is five times
faster; letting `videoconvert` use every thread, or turning the hardware
decoder off, is two to three times faster. 1080p is fine either way.
That is before egui's side, which adds two more passes and an upload of
33 MB per 4K frame [code].

What decodes in hardware here [tried, `gst-inspect-1.0`, `vainfo`, `rpm`]:
Fedora's Mesa VA-API driver offers **VP9 and AV1 but not H.264 nor HEVC**
(`vavp9dec`, `vaav1dec` exist, `vah264dec`, `vah265dec` do not), as
Fedora leaves those codecs out of its own Mesa build; H.264 is decoded in
software by `avdec_h264` (from `gstreamer1-plugin-libav`) or
`openh264dec`. Hardware decode therefore depends on the distribution and
the driver, not on us: the backend must work well in software and use
hardware when it is there.

HLS plays: `playbin3` on a public HLS test stream
(`test-streams.mux.dev`) buffered and went to PLAYING within 20 s, with
`hlsdemux2` and `dashdemux2` both installed [tried].

### Getting frames to the screen

Three ways exist for a decoded frame to reach egui [code], [docs]:

1. **RGBA in memory, uploaded by egui** (today): the player converts to
   RGBA, egui uploads it as a managed texture. Simple and portable; at 4K
   about 33 MB a frame crosses the CPU several times.
2. **YUV planes in memory, converted on the GPU**: the player hands NV12
   or I420 (1.5 bytes a pixel instead of 4), egui-pop uploads the planes to
   its own wgpu textures and converts in a shader, registered with
   `egui_wgpu::Renderer::register_native_texture` or drawn through a paint
   callback (`CallbackTrait`), both in egui-wgpu 0.37 [code]. This is
   `iced_video_player`'s design ([repository](https://github.com/jazzfool/iced_video_player))
   [docs]. Safe Rust, one CPU copy, no CPU conversion.
3. **Zero copy**: the hardware decoder's surface imported into wgpu
   (DMA-BUF on Linux, a shared D3D11 handle on Windows, an IOSurface on
   macOS). wgpu 30 can import DMA-BUF on Vulkan through
   `create_texture_from_hal` ([changelog](https://github.com/gfx-rs/wgpu/blob/trunk/CHANGELOG.md))
   [docs], which is `unsafe`, and the workspace forbids `unsafe`; wgpu's
   `Features::EXTERNAL_TEXTURE` (multi-plane YUV sampling) lists only DX12
   and Metal in wgpu-types 30 [code]. Helper crates exist
   ([`wgpu-external-frame`](https://crates.io/crates/wgpu-external-frame)
   0.4.0, MIT OR Apache-2.0) [docs], untried. Not now: way 2 removes most of
   the cost.

### Backends compared

| Backend | Licence (crate; library) | Platforms and what to install | Hardware decode | Frames to the texture | Verdict |
|---|---|---|---|---|---|
| **GStreamer** through [`gstreamer`](https://gitlab.freedesktop.org/gstreamer/gstreamer-rs) 0.25.4 (2026-09-21), `gstreamer-app`, `gstreamer-video` | MIT OR Apache-2.0; GStreamer LGPL-2.1+, a system library | Linux: the distribution's packages; Windows: the official installer (517 MB for x86_64 with development files, a runtime-only install type since 1.28); macOS: a 149 MB runtime package ([downloads](https://gstreamer.freedesktop.org/download/)) [docs] | `va` on Linux ([`gstreamer-vaapi` deprecated in 1.26](https://gstreamer.freedesktop.org/releases/1.26/)), d3d11 and d3d12 on Windows, `vtdec` on macOS, nvcodec, Vulkan Video [docs]; VP9 and AV1 here, not H.264 [tried] | appsink in memory (ways 1 and 2); GL or DMA-BUF memory for way 3; no wgpu sink exists; `gtk4paintablesink` does zero copy for GTK 4 but is MPL-2.0 and GTK only [docs] | **keep ours**: the player is GStreamer's (`playbin`), adopted; the widget, the controls and the texture path are ours. Fix the conversion (follow-up 2) |
| **libmpv** through [`libmpv2`](https://github.com/kohsine/libmpv2-rs) 7.0.0 (2026-10-30) | **LGPL-2.1 crate**, refused by `deny.toml`; [mpv](https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright) is **GPLv2+ unless built with `-Dgpl=false`**; Fedora's `mpv-libs` 0.41.0 is GPL-2.0-or-later AND LGPL-2.1-or-later [tried, `dnf`] | Linux packages (3 MB installed here, plus FFmpeg); Windows and macOS: ship a libmpv build | `hwdec` (VA-API, D3D11, VideoToolbox) [docs] | render API: OpenGL into a caller's framebuffer, or software ("extremely simple (but slow)"); a D3D11 backend landed on mpv's master on 2026-10-15, unreleased; no Vulkan, Metal or wgpu [docs] | **reject** as default: the crate's licence, a GPL library on most systems, and OpenGL where egui-pop draws with wgpu. Permissive bindings exist since 2026-09 ([`rsmpv`](https://crates.io/crates/rsmpv) 0.2.1 and [`mpv-engine`](https://github.com/ophymx/mpv-engine) 0.2.1, MIT OR Apache-2.0, wgpu 30 import of mpv's GL output, zero copy by its README): **watch**, a possible host-supplied `VideoBackend` for a GPL application such as Helios |
| **FFmpeg** through [`ffmpeg-next`](https://github.com/zmwangx/rust-ffmpeg) 9.0.0 (2026-08-05), or [`rsmpeg`](https://crates.io/crates/rsmpeg) 0.18 | `ffmpeg-next` **WTFPL** (not on the allow-list), "maintenance-only mode"; `rsmpeg` MIT, no release for 14 months; [FFmpeg](https://ffmpeg.org/legal.html) LGPL-2.1+, GPL with GPL parts | the FFmpeg libraries, at build and run time | hwaccel (VA-API, d3d11va, VideoToolbox) through the bindings [docs] | frames in memory; zero copy would be ours | **reject**: a decoder, not a player; sound output, the clock, seeking and buffering would all be ours |
| [`video-rs`](https://github.com/oddity-ai/video-rs) 0.12.0 (2026-09-09) | MIT OR Apache-2.0, over `ffmpeg-next` (WTFPL) | FFmpeg's libraries | through FFmpeg | `ndarray` frames in memory | **reject**: no playback, no sound |
| **AVFoundation** through [`objc2-av-foundation`](https://github.com/madsmtm/objc2) 0.3.2 | Zlib OR Apache-2.0 OR MIT; the system's | macOS, nothing to install | VideoToolbox, implicitly | `AVPlayerItemVideoOutput` gives IOSurface-backed pixel buffers: zero copy is possible, through `unsafe` wgpu-hal; a copy otherwise [docs] | **later, wrap**: a `VideoBackend` for macOS when GStreamer's installer is too heavy there; not before a macOS user |
| **Media Foundation** through [`windows`](https://github.com/microsoft/windows-rs) 0.62.2 | MIT OR Apache-2.0; the system's | Windows, nothing to install | DXVA | `IMFMediaEngine::TransferVideoFrame` into a D3D11 texture, then a shared handle or a read-back [docs] | **later, wrap**, as AVFoundation |
| [`alhazen-core`](https://github.com/alhazen-rs/alhazen-core) 0.7.1 (2026-11-02) | Apache-2.0 | pure Rust AV1, VP9, VP8, ProRes; Media Foundation, VideoToolbox, NVDEC; everything else through an `ffmpeg` program the user installs; sound through `cpal` | the systems' decoders | BGRA, YUV, an IOSurface on macOS | **watch**: first released 2026-10-08; on Linux without NVIDIA, H.264 needs a separate `ffmpeg` |
| [`gpu-video`](https://crates.io/crates/gpu-video) 0.4.0 (ex `vk-video`) | MIT | Vulkan Video, Linux and Windows, no macOS | H.264 decode only | frames stay on the GPU; wgpu 29 (a fork for 30) | **reject**: a bitstream decoder, no container, no sound |

Sources [docs]: crates.io's API for every version and date, each linked
repository for its activity.

### The egui video crates today

| Crate | Version, date | Licence | egui | Backend | Verdict |
|---|---|---|---|---|---|
| [`egui-video`](https://github.com/n00kii/egui-video) | 0.9.0, 2024-10-28 | MIT | 0.29 | FFmpeg (`ffmpeg-the-third`, WTFPL), SDL2 for sound | **reject**: unmaintained, three eguis behind |
| [`egui-sharkplayer`](https://codeberg.org/sharkyshark/egui-sharkplayer) | 0.5.1, 2026-07-05 | Unlicense | eframe 0.35 | `libmpv2` (LGPL crate); fast on glow, through memory on wgpu | **reject**: licence, egui version |
| [`door_player`](https://crates.io/crates/door_player) | 0.3.23, 2026-10-25 | Apache-2.0 | 0.37 | `ffmpeg-next` (WTFPL), `cpal` | **reject**: an application more than a widget; FFmpeg and WTFPL |
| [`re_video`](https://crates.io/crates/re_video) (Rerun) | 0.41.0, 2026-10-28 | MIT OR Apache-2.0 | none | AV1 through `re_rav1d`, H.264 through an `ffmpeg` program; no sound | **reject**: a decoder for Rerun |
| `ironav-player` | 0.1.2, 2026-11-03 | **AGPL-3.0** | eframe 0.37 | its own codecs | **reject**: licence |
| [`playa`](https://crates.io/crates/playa) | 0.1.142, 2025-12-25 | MIT | eframe 0.33 (glow) | image sequences, FFmpeg | **reject**: an application; worth reading for timelapse review (U2) |

None is a widget we could adopt: **keep ours**, as ADR 0017's appendix
already concluded for the viewer page
([analysis](adr/0017-links-and-outside-content-analysis.md#playing-a-local-video)).

## Online video: can it play inside?

### What the hosts allow

The question is not only technical. Every big host lets others show its
videos **through its own embedded player** and forbids reaching the
streams any other way. What matters for a desktop application:

| Host | The permitted way | What its terms forbid | Own player possible? |
|---|---|---|---|
| **YouTube** | the embedded player (the IFrame player, `youtube.com/embed/<id>` or `youtube-nocookie.com/embed/<id>`), in a web view; oEmbed for title and thumbnail | downloading, caching or storing the content; separating or modifying sound and picture; modifying or blocking the player or its ads; hiding links, attribution or branding; overlays in front of the player; a player under 200x200; more than one autoplaying player; autoplay before the player is visible; masking the client's identity | **no** |
| **Vimeo** | the embeddable player (`player.vimeo.com/video/<id>`), its Player SDK (`@vimeo/player`, MIT); oEmbed | accessing, downloading or framing beyond what is authorised; automated access; circumventing DRM; removing logos | **no**, except an account's **own** videos, whose file links the API gives "for use with third-party media players" |
| **Dailymotion** | its player; oEmbed (now needs a player id made in Dailymotion Studio) | modifying, building upon or blocking the player; automated access; scraping, extracting, downloading | **no** |
| **Twitch** | the embed (`player.twitch.tv`), with a `parent` parameter naming the embedding domain, over HTTPS | the embed obscured by other elements; under 400x300; scraping and downloading (Twitch's terms of service, as quoted by a third party; its developer agreement could not be read) | **no** |
| **PeerTube** | its embed (`/videos/embed/<id>`, a JavaScript API with `api=1`) | each instance sets its own terms, each video its own licence | **yes**: its videos are plain MP4 files ("web videos") or HLS playlists, given by its REST API |
| **A plain file** (`.mp4`, `.webm`, an HLS `.m3u8` or DASH `.mpd`) | any player | only the content's own rights | **yes** |

Sources [docs]:
YouTube's [API Services Terms of Service](https://developers.google.com/youtube/terms/api-services-terms-of-service)
(section 16.3: no right to make content available "in any manner other
than through the use of the YouTube API Services"), its
[Developer Policies](https://developers.google.com/youtube/terms/developer-policies)
(III.E.1(a): do not "download, import, backup, cache, or store copies of
YouTube audiovisual content"; III.I.5: do not "modify, interfere with,
replace, or block advertisements"; III.I.6: do not "modify, build upon,
or block any portion or functionality of a YouTube player"; III.I.7: do
not "separate, isolate, or modify the audio or video components"; III.D.1.a:
do not "mask or misrepresent your identity or your API Client's identity";
III.B.1: an installed application "must be capable of being remotely
updated"), its
[Required Minimum Functionality](https://developers.google.com/youtube/terms/required-minimum-functionality)
(a viewport of at least 200 by 200 pixels; identification "through the
HTTP Referer request header", which "in WebView integrations such as a
mobile app or desktop app ... is empty by default" and must be set, as a
header or through the page's base URL; no overlays, frames or other
visual elements "in front of any part of a YouTube embedded player"; no
automatic playback "until the player is visible"; non-compliance "might
result in reduced functionality"), its
[player parameters](https://developers.google.com/youtube/player_parameters)
(`origin`, `widget_referrer`), and the
[YouTube Terms of Service](https://www.youtube.com/static?template=terms)
("You may also show YouTube videos through the embeddable YouTube
player"; no access by "automated means", no circumventing what restricts
copying).
[Vimeo's terms](https://vimeo.com/terms) (sections 2 and 5.4), its
[API addendum](https://vimeo.com/legal/service-terms/api), its
[file links for third-party players](https://help.vimeo.com/hc/en-us/articles/12427806914577)
and its [domain-restricted embeds](https://help.vimeo.com/hc/en-us/articles/12426259908881).
[Dailymotion's terms](https://legal.dailymotion.com/en/terms-of-use/)
(sections 2.6, 6.1, 6.6) and its
[oEmbed](https://developers.dailymotion.com/docs/embed-with-oembed).
[Twitch's embed documentation](https://dev.twitch.tv/docs/embed/) and
[its size rule](https://dev.twitch.tv/docs/embed/video-and-clips/).
PeerTube's [embed API](https://docs.joinpeertube.org/api/embed-player)
and its [6.0 release](https://github.com/Chocobozzz/PeerTube/releases/tag/v6.0.0)
("web videos" in place of WebTorrent, HLS recommended).

### In our own player: ruled out for the big hosts

Extracting a stream's address the way `yt-dlp` does and handing it to
GStreamer or mpv would give a native picture with our own controls, no
web engine, and no overlay problem. **It is ruled out for YouTube, Vimeo,
Dailymotion and Twitch**: it is access by means other than their player,
it separates the picture from the player, its ads and its branding, and
for YouTube it breaks section 16.3 of the API terms and III.I.5 to III.I.7
of the Developer Policies quoted above. It would also break whenever the
host changes its pages, which is how `yt-dlp` lives. egui-pop does not
do it, does not document how, and does not take a dependency that does.

What **can** play in our own player: a plain video file on the web, an
HLS or DASH stream (GStreamer's `hlsdemux2` and `dashdemux2`, with
`playbin3`, from the good plugins since 1.22,
[design](https://gstreamer.freedesktop.org/documentation/additional/design/adaptive-demuxer.html))
[docs], [tried: HLS plays]; a PeerTube video through its file or HLS
address; a Vimeo account's own videos through their file links. These
need nothing new in the backend: GStreamer opens `https:` addresses
(`souphttpsrc`, installed) as it opens files. What they need is a policy
change: today a remote video always opens outside
([ADR 0017](adr/0017-links-and-outside-content.md)).

### Inside, through a web view

The host's player is a web page; showing it inside the application
means a web engine. Two ways exist: a native web view placed over the
panel, or a web engine drawing into a texture egui shows.

**Native web views (wry).** [wry](https://github.com/tauri-apps/wry)
0.57.1 (MIT OR Apache-2.0) wraps the system's engine [docs]:

| System | Engine | In the panel? | Media |
|---|---|---|---|
| Linux, X11 (and Xwayland) | WebKitGTK | **yes**: a child window over the panel; wry's `build_as_child` is "X11 only", the trial makes the child with x11rb because the workspace forbids `unsafe` [code] | through GStreamer: the codecs installed decide; YouTube needs MSE and VP9 or AV1 ([WebKit bug 261892](https://bugs.webkit.org/show_bug.cgi?id=261892)) [docs]; played in the trial [tried there] |
| Linux, Wayland | WebKitGTK | **no**: Wayland has no foreign child windows; wry needs a GTK container (`build_gtk`), so the trial opens a GTK window of its own [code], [docs] | same |
| Windows | WebView2 (Edge's Chromium) | **yes**: a child window of the main window [docs] | Chromium's codecs; no native HLS ([WebView2Feedback#5092](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5092)) [docs]; not run |
| macOS | WKWebView | **yes**: an `NSView` subview of the window's content view [docs] | the system's (VideoToolbox), HLS natively in Safari; not run |

wry's [`build_as_child`](https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html)
states the three systems and the X11 limit. On Linux, wry still uses the
GTK 3 bindings (`gtk` 0.18 in our lock file [code]), whose repository
[gtk-rs/gtk3-rs](https://github.com/gtk-rs/gtk3-rs) is archived [docs].

What a native web view costs, everywhere:

- **It is on top.** A web view is a native window or view above egui's
  surface: egui's menus, popups, tooltips and toasts over the panel are
  drawn **under** it (the "airspace" problem; [gpui-wry's README](https://docs.rs/crate/gpui-wry/latest/source/README.md)
  says the same for GPUI) [code], [docs]. For a YouTube player this is,
  oddly, what its terms want: nothing may be drawn in front of the player.
  The fix is to hide or shrink the web view while a popup crosses it, or
  to accept the gap.
- **Focus and keys** go to the web view while it has the focus: egui's
  shortcuts stop until the user clicks back into egui; WebView2 hands
  accelerator keys back (`AcceleratorKeyPressed`), WebKitGTK and
  WKWebView do not by themselves [docs].
- **Screenshots and the debug bridge** see a hole where the web view is:
  egui never had its pixels [code].
- **On Linux**, WebKitGTK (about 96 MB installed on Fedora) and GTK 3
  are required at build and run time, and GTK's loop is pumped from
  egui's thread [tried, `dnf`], [code].

**Offscreen engines, drawn into a texture.** They solve the overlay, the
screenshot and the Wayland problems, at a price:

| Engine | Licence | Size and state | Verdict |
|---|---|---|---|
| CEF through [`cef`](https://crates.io/crates/cef) (tauri-apps/cef-rs) 156.3.0, 2026-11-01 | Apache-2.0 OR MIT; CEF BSD-3-Clause, Chromium BSD and many more | Chromium shipped with the application (on the order of 100 MB or more per platform, not measured), its own processes; shared-texture offscreen rendering (`osr_texture_import`, dmabuf on Linux); the official builds **leave out H.264** ([gstcefsrc](https://github.com/livekit/gstcefsrc)), YouTube still plays through VP9 and AV1 | **reject**: a second browser for a help video |
| [Servo](https://servo.org/blog/2026/04/13/servo-0.1.0-release/) (`servo` 0.7.0, 2026-10-05) | **MPL-2.0**: `deny.toml` refuses it | media through servo-media over GStreamer; whether YouTube plays not verified | **reject** (licence), watch |
| [Ultralight](https://ultralig.ht/pricing/) | proprietary; the free tier "does not include ... media playback" | | **reject** |
| WPE WebKit | LGPL system library, Linux only | no maintained Rust binding found | **reject** |
| [`egui_chromite`](https://crates.io/crates/egui_chromite) 0.0.1, 2026-10-23 | MIT OR Apache-2.0 | "Chromium web views as egui widgets", 12 downloads | **watch** |
| `egui_webview` ([hello_egui](https://github.com/lucasmerlin/hello_egui)) | MIT | wry, unreleased, `unsafe` around Send and Sync | **reject** |

And a texture would let egui draw over the YouTube player, which its
terms forbid: the overlay problem becomes a rule to enforce rather than a
problem solved.

### On the web build

The page is already in a browser, so the host's player is an `<iframe>`
and a plain video is a `<video>` element, both positioned over the canvas
on the panel's rectangle ([web survey](web-survey.md#media-and-embeds),
M1 to M3, follow-up 21) [docs]. The same overlay rule holds: egui's popups
are drawn in the canvas, under the element, so the element hides while
one is open. Drawing the video into egui's own texture instead
(`copyExternalImageToTexture`, WebGPU's `importExternalTexture`) only
works for a video the page may read: a cross-origin video without CORS
throws a security error
([MDN](https://developer.mozilla.org/docs/Web/API/GPUDevice/importExternalTexture)),
and no web API draws an `<iframe>` into a canvas at all [docs]. So the
host's players can only be overlays; our own files, served with the
gallery, could go either way.

### Per host

| Host | Desktop, Linux | Desktop, Windows and macOS | Web build |
|---|---|---|---|
| YouTube | the trial: wry, privacy-enhanced player, `Referer` set; inside on X11, own window on Wayland | same through wry's WebView2 and WKWebView backends (not written, not run) | an `<iframe>` over the canvas |
| Vimeo | the same shape works technically (`player.vimeo.com/video/<id>`); a domain-restricted video plays only if the `Referer` names an allowed domain (inferred from Vimeo's help, not tried) | same | `<iframe>` |
| Dailymotion | the same shape, with a player id from Dailymotion Studio | same | `<iframe>` |
| Twitch | **not worth it**: `parent` must name the page's domain over HTTPS, a `file://` page fails ([forum](https://discuss.dev.twitch.com/t/embedding-twitch-in-an-electron-app/40334)), at least 400x300; live streams are no help-video use case | same | `<iframe>`, with the site's domain as `parent` |
| PeerTube | **our own player** on the video's MP4 or HLS address | same | `<video>` (HLS needs a script outside Safari) |
| Plain file, HLS, DASH | **our own player** | same | `<video>` |

### Is the trial compliant?

Read against the Required Minimum Functionality [docs], [code]:

- **Identity**: the trial sends `https://localhost/` unless the
  application sets `ViewerPlugin::embed_referrer`. The rules ask for a
  `Referer` that identifies the application, a stable HTTPS address whose
  domain is the application's id (reverse-DNS in Google's examples), and
  forbid masking it. `localhost` identifies nobody: the default should be
  derived from the application's id, and an application should be told to
  set its own. Since follow-up 4 (2026-11-04) it is
  `https://<app id>/`, from `AppBuilder::app_id`.
- **Size**: the player must be at least 200 by 200 pixels; a narrow panel
  can be smaller. Below that the page should show the card with "Open
  outside" instead. Since follow-up 4 it does.
- **Autoplay**: the trial loads `autoplay=1`, after the user clicked the
  link, with the web view visible; it closes when its panel is hidden. That
  fits "not until the player is visible" and "one autoplaying player".
- **Nothing in front**: egui cannot draw over a native web view, so this
  holds by construction on X11, Windows and macOS.
- **Links and branding**: the player is loaded unmodified; its links go
  outside through the links service.
- **Remote update**: installed applications "must be capable of being
  remotely updated"; that is the application's packaging, not egui-pop's,
  and worth a line in the documentation.

## Follow-ups, ranked

Sizes: S (a day or less), M (a few days), L (more). "Decided" means the
owner already settled it; everything else is a proposal.

1. **The native `Video` widget** (the plan already proposed for it):
   `Video::new(source)` with `controls(Full | Compact | None)`,
   `autoplay`, `looping`, `muted`, `fit(Contain | Cover)`, a poster
   frame, its own player per widget on the existing `VideoBackend`; the
   viewer's page rebuilt on it; the gallery showing full, compact and bare;
   a placeholder with "Open outside" without the `video` feature. It serves
   U1, U4, U5 and U6. **M.** Not decided: the plan waits on this survey.
   Two additions from it: `VideoBackend::open` takes a URI rather than a
   path (follow-up 5), and the player can pre-roll paused to give a poster
   frame without playing (it already opens paused on its first picture
   [code]).
2. **The frame path: YUV to the GPU.** Ask `appsink` for NV12 or I420,
   upload the planes to wgpu textures, convert in a shader, drop the CPU
   RGBA conversion and the two egui-side copies (way 2 above). Until then,
   a one-line stopgap: `videoconvert n-threads=0` in the sink, which took
   the 4K clip from 22 to 59 frames a second here [tried]. **S** for the
   stopgap, **M** for the planes. Not decided.
3. **Frame stepping and loop without a gap** (U2): GStreamer's step event
   ([design](https://gstreamer.freedesktop.org/documentation/additional/design/framestep.html))
   behind `VideoPlayer::step`, a segment seek for seamless loops; with the
   widget's `Full` controls. **S to M.** Not decided; only if Helios takes
   timelapse review.
4. **Keep the YouTube trial, opt-in, and make it comply**: the default
   `Referer` derived from the application's id (not `localhost`), a
   minimum of 200 by 200 pixels below which the card with "Open outside"
   shows, a line in the documentation on the terms (no extraction, nothing
   over the player, an application that can be updated). **S.** **Done**
   (2026-11-04, the owner chose to keep the trial, opt-in): the `Referer`
   is `https://<app id>/` from `AppBuilder::app_id`, the card shows below
   200 by 200 of the web view's pixels, and
   [links.md](links.md#terms) says what the terms allow.
5. **Plain web videos in our own player**: a remote `.mp4`, `.webm`,
   `.m3u8` or `.mpd`, and PeerTube videos through their file address,
   played by the same widget (GStreamer opens `https:`), after a click as
   any remote content. A change to ADR 0017's policy ("remote video stays
   outside"). **M.** **Not decided**: it is a policy change for the owner.
6. **Vimeo and Dailymotion in the same web view**: the embed URL per
   host in `EmbedBackend`'s request, the same `Referer`, the same rules.
   **S** once follow-up 4 is done. Not decided; only if an application
   links tutorials there.
7. **Windows and macOS web views**: wry's WebView2 and WKWebView backends
   for the same `EmbedBackend`, inside the panel (`build_as_child` works
   there without the X11 dance). **M**, and run on those systems by hand.
   Not decided; after a Windows or macOS user.
8. **The web build's overlays** (web survey follow-up 21): `<video>` for
   files and `<iframe>` for the hosts, on the panel's rectangle, hidden
   while an egui popup is open. **M to L.** Not decided; already ranked in
   the web survey.
9. **Hide a native web view while an egui popup crosses it**, so menus
   stay usable over a playing YouTube panel. **S to M.** Not decided.

### What to drop

- **Extracting streams from YouTube, Vimeo, Dailymotion or Twitch** to
  play them in our own player: against their terms. Dropped, not to be
  revisited.
- **Twitch inside the application**: its `parent` rule, HTTPS origin and
  400 by 300 minimum fit a website, not a desktop panel, and live streams
  are not a use case here. Outside, always.
- **An offscreen browser engine** (CEF, Servo, Ultralight, WPE) to fix
  the overlay: 100 MB or more, or a refused licence, for help videos; and
  YouTube's terms forbid drawing over its player anyway. Dropped unless a
  real need for full web pages inside appears.
- **libmpv, FFmpeg or an egui video crate in place of GStreamer**: none
  is a better player under our licence policy today. Watch `mpv-engine`
  and `alhazen-core`; a GPL application may plug libmpv in through
  `VideoBackend` on its own.
- **Zero-copy import** (way 3) for now: `unsafe`, platform by platform,
  and way 2 gets most of the gain.

### For the owner

- **Decided already**: local video plays inside with the `video` feature
  (2026-11-01); the YouTube page exists as an opt-in trial (2026-11-03);
  `option-ext` is allowed as a narrow MPL exception for it (2026-11-03).
- **Decided since**: the YouTube trial stays, opt-in, made to comply (4,
  2026-11-04, done).
- **To decide**: the `Video` widget's plan (1); whether remote video files and PeerTube may play inside (5);
  whether the WTFPL (`ffmpeg-next`) ever joins the allow-list (only if an
  FFmpeg backend is wanted, which this survey does not recommend).

## Sources

The sources are linked where they are used above. The main ones:
YouTube's [API Services Terms of Service](https://developers.google.com/youtube/terms/api-services-terms-of-service),
[Developer Policies](https://developers.google.com/youtube/terms/developer-policies)
and [Required Minimum Functionality](https://developers.google.com/youtube/terms/required-minimum-functionality);
[Vimeo's terms](https://vimeo.com/terms),
[Dailymotion's terms](https://legal.dailymotion.com/en/terms-of-use/),
[Twitch's embed documentation](https://dev.twitch.tv/docs/embed/),
[PeerTube's embed API](https://docs.joinpeertube.org/api/embed-player);
[wry's `WebViewBuilder`](https://docs.rs/wry/latest/wry/struct.WebViewBuilder.html);
[GStreamer's releases](https://gstreamer.freedesktop.org/releases/1.26/)
and [downloads](https://gstreamer.freedesktop.org/download/);
[mpv's copyright notice](https://raw.githubusercontent.com/mpv-player/mpv/master/Copyright);
[FFmpeg's legal page](https://ffmpeg.org/legal.html);
crates.io for every crate's version, date and licence.
