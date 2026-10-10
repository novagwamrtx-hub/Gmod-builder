//! Addon thumbnails.
//!
//! A preview on the Workshop is whatever its author uploaded: often a picture
//! of several megabytes, or an animation, for something shown 64 pixels wide.
//! Loading those as they are costs a page of search results tens of megabytes
//! to download and as much again in memory. So a thumbnail is asked for at
//! the size it is shown (Steam's image server resizes on request), only the
//! start of an animation is fetched (its first frame), and the result is kept
//! on disk so the next time needs no download at all.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use eframe::egui::load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint};
use eframe::egui::{self, ColorImage};
use tokio::sync::Semaphore;

use crate::files;

const SCHEME: &str = "thumb://";
/// Thumbnails downloaded at the same time.
const PARALLEL: usize = 8;
/// Memory for thumbnails on screen and seen lately. Older ones are dropped,
/// and read from disk again if they come back into view.
const MEMORY_BUDGET: usize = 96 * 1024 * 1024;
/// A thumbnail that could not be loaded is tried again after this long.
const RETRY_AFTER: Duration = Duration::from_secs(30);
/// Nothing sensible is larger than this; a server that sends more is ignored.
const MAX_DOWNLOAD: usize = 8 * 1024 * 1024;
const MAX_SIDE: u32 = 4096;
pub const KEPT_ON_DISK: Duration = Duration::from_secs(30 * 24 * 60 * 60);
pub const DISK_BUDGET: u64 = 200 * 1024 * 1024;

/// The address to give `egui::Image` for the picture at `url`, shown
/// `pixels` wide. Sizes come in a few steps so that a picture shown at
/// nearly the same size in two places is loaded once.
pub fn uri(url: &str, pixels: f32) -> String {
    let side = match pixels {
        p if p <= 144.0 => 128,
        p if p <= 288.0 => 256,
        _ => 512,
    };
    format!("{SCHEME}{side}/{url}")
}

fn parse(uri: &str) -> Option<(u32, &str)> {
    let (side, url) = uri.strip_prefix(SCHEME)?.split_once('/')?;
    Some((side.parse().ok()?, url))
}

/// `url` with Steam's request to scale the picture to fit a square of `side`.
fn sized(url: &str, side: u32) -> String {
    let joiner = if url.contains('?') { '&' } else { '?' };
    format!("{url}{joiner}imw={side}&imh={side}&ima=fit&impolicy=Letterbox")
}

/// How much of an animation to fetch to be sure of its first frame.
fn first_part(side: u32) -> usize {
    match side {
        0..=128 => 64 * 1024,
        129..=256 => 192 * 1024,
        _ => 640 * 1024,
    }
}

fn is_animation(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF8") || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
}

/// Turns a downloaded file into pixels no larger than `side`. Of an
/// animation, only the first frame is read.
fn decode(bytes: &[u8], side: u32) -> Result<ColorImage, String> {
    let failed = |e: image::ImageError| e.to_string();
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut picture = reader.decode().map_err(failed)?;
    if picture.width() > side || picture.height() > side {
        picture = picture.thumbnail(side, side);
    }
    let pixels = picture.to_rgba8();
    Ok(ColorImage::from_rgba_unmultiplied([pixels.width() as usize, pixels.height() as usize], pixels.as_raw()))
}

enum Entry {
    Loading,
    Ready(Arc<ColorImage>),
    Failed { why: String, at: Instant },
}

#[derive(Default)]
struct State {
    entries: HashMap<String, Entry>,
    /// Waiting for a free download. The newest is served first: it is the
    /// one on screen now.
    queue: Vec<String>,
    /// What egui has been given and holds as textures, oldest first, with
    /// the memory each takes.
    shown: VecDeque<(String, usize)>,
    shown_bytes: usize,
}

struct Downloaded {
    bytes: Vec<u8>,
    /// The server sent only the part asked for, and there is more.
    partial: bool,
}

struct Shared {
    http: reqwest::Client,
    dir: Option<PathBuf>,
    slots: Semaphore,
    state: Mutex<State>,
}

impl Shared {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Loads the request made last, once a download slot is free.
    async fn serve_one(self: Arc<Self>, ctx: egui::Context) {
        let Ok(_slot) = self.slots.acquire().await else { return };
        let Some(uri) = self.state().queue.pop() else { return };
        let outcome = match parse(&uri) {
            Some((side, url)) => self.fetch(side, url).await,
            None => Err("not a thumbnail address".to_string()),
        };
        let entry = match outcome {
            Ok(image) => Entry::Ready(Arc::new(image)),
            Err(why) => Entry::Failed { why, at: Instant::now() },
        };
        {
            let mut state = self.state();
            // Unless nobody is waiting for it any more.
            if matches!(state.entries.get(&uri), Some(Entry::Loading)) {
                state.entries.insert(uri, entry);
            }
        }
        // Only now that the state is unlocked: egui may be asking us about
        // it from the other side.
        ctx.request_repaint();
    }

    async fn fetch(&self, side: u32, url: &str) -> Result<ColorImage, String> {
        let file = self.dir.as_ref().map(|dir| dir.join(format!("{:016x}-{side}.img", files::fnv1a(url.as_bytes()))));
        let kept = file.as_ref().and_then(|file| std::fs::read(file).ok());
        if let Some(image) = kept.and_then(|bytes| decode(&bytes, side).ok()) {
            return Ok(image);
        }
        let (image, bytes) = self.download_thumbnail(side, url).await?;
        if let Some(file) = file {
            // Best effort: without it the thumbnail is downloaded again next time.
            let _ = std::fs::write(file, bytes);
        }
        Ok(image)
    }

    /// The thumbnail and the file it was read from, which is what is kept.
    async fn download_thumbnail(&self, side: u32, url: &str) -> Result<(ColorImage, Vec<u8>), String> {
        let address = sized(url, side);
        let mut got = self.download(&address, Some(first_part(side))).await?;
        // The first frame of an animation is at its start. Anything else is
        // needed whole, and after resizing is small enough to have come whole.
        let mut image = match got.partial && !is_animation(&got.bytes) {
            true => Err("the picture is larger than expected".to_string()),
            false => decode(&got.bytes, side),
        };
        if image.is_err() && got.partial {
            got = self.download(&address, None).await?;
            image = decode(&got.bytes, side);
        }
        Ok((image?, got.bytes))
    }

    /// Downloads `address`, or with `first` only that many bytes from its start.
    async fn download(&self, address: &str, first: Option<usize>) -> Result<Downloaded, String> {
        let failed = |e: reqwest::Error| e.without_url().to_string();
        let mut request = self.http.get(address);
        if let Some(first) = first {
            request = request.header("range", format!("bytes=0-{}", first - 1));
        }
        let mut response = request.send().await.map_err(failed)?;
        let status = response.status();
        if !status.is_success() {
            return Err(format!("the server answered {}", status.as_u16()));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(failed)? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > MAX_DOWNLOAD {
                return Err("the picture is too large".to_string());
            }
        }
        let partial = status == reqwest::StatusCode::PARTIAL_CONTENT && first.is_some_and(|first| bytes.len() >= first);
        Ok(Downloaded { bytes, partial })
    }
}

/// Loads thumbnails for egui. Register it with [`Thumbnails::install`], then
/// show a thumbnail with `egui::Image::new(thumbs::uri(..))`.
pub struct Thumbnails {
    runtime: tokio::runtime::Handle,
    shared: Arc<Shared>,
}

impl Thumbnails {
    /// `dir` is where downloaded thumbnails are kept; without one they are
    /// downloaded again every time the app starts.
    pub fn new(runtime: tokio::runtime::Handle, dir: Option<PathBuf>) -> Self {
        let dir = dir.filter(|dir| std::fs::create_dir_all(dir).is_ok());
        let shared = Shared {
            http: gmcb_core::http::client(),
            dir,
            slots: Semaphore::new(PARALLEL),
            state: Mutex::new(State::default()),
        };
        Self { runtime, shared: Arc::new(shared) }
    }

    pub fn install(self: &Arc<Self>, ctx: &egui::Context) {
        ctx.add_image_loader(self.clone());
        // egui keeps the texture; it has no need for our copy of the pixels.
        ctx.options_mut(|options| options.reduce_texture_memory = true);
    }

    /// Drops the thumbnails loaded longest ago once they take too much
    /// memory. One that is still in view is read back from disk. Call once a
    /// frame, outside of drawing.
    pub fn trim(&self, ctx: &egui::Context) {
        let mut dropped = Vec::new();
        {
            let mut state = self.shared.state();
            while state.shown_bytes > MEMORY_BUDGET {
                let Some((uri, bytes)) = state.shown.pop_front() else { break };
                state.shown_bytes -= bytes;
                dropped.push(uri);
            }
        }
        for uri in dropped {
            ctx.forget_image(&uri);
        }
    }
}

impl ImageLoader for Thumbnails {
    fn id(&self) -> &str {
        concat!(module_path!(), "::Thumbnails")
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        if !uri.starts_with(SCHEME) {
            return Err(LoadError::NotSupported);
        }
        let mut state = self.shared.state();
        match state.entries.get(uri) {
            Some(Entry::Loading) => return Ok(ImagePoll::Pending { size: None }),
            Some(Entry::Ready(image)) => {
                let image = image.clone();
                let bytes = image.pixels.len() * size_of::<egui::Color32>();
                state.shown.push_back((uri.to_owned(), bytes));
                state.shown_bytes += bytes;
                return Ok(ImagePoll::Ready { image });
            }
            Some(Entry::Failed { why, at }) if at.elapsed() < RETRY_AFTER => {
                return Err(LoadError::Loading(why.clone()));
            }
            Some(Entry::Failed { .. }) | None => {}
        }
        state.entries.insert(uri.to_owned(), Entry::Loading);
        state.queue.push(uri.to_owned());
        drop(state);
        self.runtime.spawn(self.shared.clone().serve_one(ctx.clone()));
        Ok(ImagePoll::Pending { size: None })
    }

    fn forget(&self, uri: &str) {
        self.shared.state().entries.remove(uri);
    }

    fn forget_all(&self) {
        self.shared.state().entries.clear();
    }

    fn byte_size(&self) -> usize {
        let state = self.shared.state();
        let held = state.entries.values().map(|entry| match entry {
            Entry::Ready(image) => image.pixels.len() * size_of::<egui::Color32>(),
            _ => 0,
        });
        held.sum()
    }

    fn has_pending(&self) -> bool {
        self.shared.state().entries.values().any(|entry| matches!(entry, Entry::Loading))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;
    use crate::files::TestDir;

    /// Noise, so that the files cannot be stored small.
    fn noise(x: u32, y: u32, seed: u32) -> [u8; 3] {
        let mixed = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503) ^ seed.wrapping_mul(97))
            .wrapping_mul(2_246_822_519);
        [mixed as u8, (mixed >> 11) as u8, (mixed >> 22) as u8]
    }

    fn picture(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
        let pixels = image::RgbImage::from_fn(width, height, |x, y| image::Rgb(noise(x, y, 0)));
        let mut bytes = Vec::new();
        pixels.write_to(&mut std::io::Cursor::new(&mut bytes), format).unwrap();
        bytes
    }

    fn animation(side: u32, frames: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 30);
            for frame in 0..frames {
                let pixels = image::RgbaImage::from_fn(side, side, |x, y| {
                    let [r, g, b] = noise(x, y, frame);
                    image::Rgba([r, g, b, 255])
                });
                encoder.encode_frame(image::Frame::new(pixels)).unwrap();
            }
        }
        bytes
    }

    #[test]
    fn addresses_name_the_size_wanted() {
        let url = "https://images.steamusercontent.com/ugc/1/ABC/";
        assert_eq!(uri(url, 64.0), format!("thumb://128/{url}"));
        assert_eq!(uri(url, 144.0), format!("thumb://128/{url}"));
        assert_eq!(uri(url, 200.0), format!("thumb://256/{url}"));
        assert_eq!(uri(url, 450.0), format!("thumb://512/{url}"));
        assert_eq!(parse(&uri(url, 64.0)), Some((128, url)));
        assert_eq!(parse("https://example.com/a.png"), None);
        assert_eq!(parse("thumb://big/https://example.com/"), None);

        assert_eq!(sized(url, 128), format!("{url}?imw=128&imh=128&ima=fit&impolicy=Letterbox"));
        assert!(sized("https://example.com/a?b=1", 256).ends_with("a?b=1&imw=256&imh=256&ima=fit&impolicy=Letterbox"));
    }

    #[test]
    fn pictures_are_decoded_no_larger_than_shown() {
        let small = decode(&picture(100, 60, image::ImageFormat::Png), 128).unwrap();
        assert_eq!(small.size, [100, 60], "small pictures are left as they are");
        let large = decode(&picture(600, 300, image::ImageFormat::Jpeg), 128).unwrap();
        assert_eq!(large.size, [128, 64], "large ones keep their shape");
        assert!(decode(b"<html>not found</html>", 128).is_err());
        assert!(decode(&[], 128).is_err());
    }

    #[test]
    fn the_first_frame_of_an_animation_is_enough() {
        let whole = animation(96, 6);
        assert!(is_animation(&whole) && !is_animation(&picture(8, 8, image::ImageFormat::Png)));
        // Somewhere in the second frame.
        let part = &whole[..whole.len() / 4];
        let frame = decode(part, 128).expect("a cut-off animation still has its first frame");
        assert_eq!(frame.size, [96, 96]);
        assert_eq!(frame.pixels, decode(&whole, 128).unwrap().pixels);
        assert!(decode(&whole[..200], 128).is_err(), "though not when even that is cut off");
    }

    /// A server on a local port that answers each connection with the next
    /// of `replies`, and notes what it was asked.
    fn serve(replies: Vec<Vec<u8>>) -> (String, Arc<Mutex<Vec<String>>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = asked.clone();
        std::thread::spawn(move || {
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0; 4096];
                let length = stream.read(&mut request).unwrap_or(0);
                log.lock().unwrap().push(String::from_utf8_lossy(&request[..length]).to_lowercase());
                let _ = stream.write_all(&reply);
            }
        });
        (address, asked)
    }

    fn reply(status: &str, body: &[u8]) -> Vec<u8> {
        let head = format!("HTTP/1.1 {status}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", body.len());
        [head.as_bytes(), body].concat()
    }

    /// Asks the loader for `uri` until it has an answer.
    fn wait_for(loader: &Thumbnails, ctx: &egui::Context, uri: &str) -> Result<Arc<ColorImage>, String> {
        let started = Instant::now();
        loop {
            match loader.load(ctx, uri, SizeHint::default()) {
                Ok(ImagePoll::Ready { image }) => return Ok(image),
                Ok(ImagePoll::Pending { .. }) => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => return Err(e.to_string()),
            }
            assert!(started.elapsed() < Duration::from_secs(20), "the thumbnail never arrived");
        }
    }

    #[test]
    fn thumbnails_are_downloaded_small_and_then_kept() {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let dir = TestDir::new("thumbs");
        let ctx = egui::Context::default();
        let (server, asked) = serve(vec![reply("200 OK", &picture(120, 90, image::ImageFormat::Png))]);
        let uri = uri(&format!("{server}/preview/"), 64.0);

        let loader = Thumbnails::new(runtime.handle().clone(), Some(dir.0.clone()));
        assert!(matches!(loader.load(&ctx, "https://example.com/a.png", SizeHint::default()), Err(LoadError::NotSupported)));
        assert_eq!(wait_for(&loader, &ctx, &uri).unwrap().size, [120, 90]);
        let asked = asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 1);
        assert!(asked[0].starts_with("get /preview/?imw=128&imh=128&ima=fit&impolicy=letterbox "), "{}", asked[0]);
        assert!(asked[0].contains("range: bytes=0-65535"), "{}", asked[0]);
        assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1, "kept on disk");

        // A new start: the server is gone, the file is not.
        let loader = Thumbnails::new(runtime.handle().clone(), Some(dir.0.clone()));
        assert_eq!(wait_for(&loader, &ctx, &uri).unwrap().size, [120, 90]);

        // What egui has taken is not held twice.
        loader.forget(&uri);
        assert_eq!(loader.byte_size(), 0);
        assert!(!loader.has_pending());
    }

    #[test]
    fn a_cut_off_animation_is_used_and_a_cut_off_picture_is_fetched_whole() {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let ctx = egui::Context::default();
        let loader = Thumbnails::new(runtime.handle().clone(), None);

        let gif = animation(128, 40);
        assert!(gif.len() > first_part(128), "the animation must be longer than what is fetched: {}", gif.len());
        let (server, asked) = serve(vec![reply("206 Partial Content", &gif[..first_part(128)])]);
        let image = wait_for(&loader, &ctx, &uri(&format!("{server}/gif/"), 64.0)).unwrap();
        assert_eq!(image.size, [128, 128]);
        assert_eq!(asked.lock().unwrap().len(), 1, "the first part was enough");

        // A server that ignores the size and sends a large picture in parts.
        let png = picture(700, 700, image::ImageFormat::Png);
        assert!(png.len() > first_part(128), "{}", png.len());
        let (server, asked) = serve(vec![reply("206 Partial Content", &png[..first_part(128)]), reply("200 OK", &png)]);
        let image = wait_for(&loader, &ctx, &uri(&format!("{server}/png/"), 64.0)).unwrap();
        assert_eq!(image.size, [128, 128]);
        let asked = asked.lock().unwrap().clone();
        assert_eq!(asked.len(), 2);
        assert!(!asked[1].contains("range:"), "the second time it asks for everything: {}", asked[1]);
    }

    #[test]
    fn a_missing_picture_fails_without_hammering_the_server() {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let ctx = egui::Context::default();
        let loader = Thumbnails::new(runtime.handle().clone(), None);
        let (server, asked) = serve(vec![reply("404 Not Found", b"")]);
        let uri = uri(&format!("{server}/gone/"), 64.0);
        assert!(wait_for(&loader, &ctx, &uri).unwrap_err().contains("404"));
        assert!(wait_for(&loader, &ctx, &uri).is_err(), "asking again right away does not download again");
        assert_eq!(asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn old_thumbnails_are_dropped_from_memory() {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).enable_all().build().unwrap();
        let ctx = egui::Context::default();
        let loader = Arc::new(Thumbnails::new(runtime.handle().clone(), None));
        {
            let mut state = loader.shared.state();
            state.shown.push_back(("thumb://128/a".into(), MEMORY_BUDGET));
            state.shown.push_back(("thumb://128/b".into(), 1000));
            state.shown_bytes = MEMORY_BUDGET + 1000;
        }
        loader.trim(&ctx);
        let state = loader.shared.state();
        assert_eq!(state.shown.len(), 1, "the oldest went, the newer one stays");
        assert_eq!(state.shown_bytes, 1000);
    }

    /// How the real Workshop behaves: do first parts decode, and how much
    /// is downloaded?
    ///
    ///     cargo test -p gmcb-desktop live_thumbnails -- --ignored --nocapture
    #[test]
    #[ignore = "needs network access and a saved Steam key"]
    fn live_thumbnails() {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        let key = crate::settings::Settings::load().api_key;
        assert!(!key.is_empty(), "save a Steam key in the app first");
        let client = gmcb_core::SteamClient::new(Some(key));
        let loader = Thumbnails::new(runtime.handle().clone(), None);
        for (label, text, sort) in [
            ("trending", "", gmcb_core::Sort::Trending),
            ("most subscribed", "", gmcb_core::Sort::MostSubscribed),
            ("newest", "", gmcb_core::Sort::Newest),
            ("search zombie", "zombie", gmcb_core::Sort::Relevance),
        ] {
            let query = gmcb_core::SearchQuery {
                text: text.to_string(),
                tags: vec!["Addon".to_string()],
                sort,
                per_page: 100,
                ..Default::default()
            };
            let urls: Vec<String> = runtime
                .block_on(client.search(&query))
                .expect("the search failed")
                .addons
                .into_iter()
                .filter_map(|addon| addon.preview_url)
                .collect();
            for side in [128, 256] {
                let started = Instant::now();
                let jobs: Vec<_> = urls
                    .iter()
                    .map(|url| {
                        let (shared, url) = (loader.shared.clone(), url.clone());
                        runtime.spawn(async move {
                            let _slot = shared.slots.acquire().await;
                            shared.download_thumbnail(side, &url).await
                        })
                    })
                    .collect();
                let (mut bytes, mut whole, mut animations, mut failed, mut pixels) = (0, 0, 0, Vec::new(), 0);
                for (url, job) in urls.iter().zip(jobs) {
                    match runtime.block_on(job).unwrap() {
                        Ok((image, file)) => {
                            bytes += file.len();
                            whole += usize::from(file.len() > first_part(side));
                            animations += usize::from(is_animation(&file));
                            pixels += image.pixels.len() * 4;
                        }
                        Err(why) => failed.push(format!("{url}: {why}")),
                    }
                }
                println!(
                    "{label} at {side}: {} thumbnails in {:.1}s, {:.2} MB downloaded, {:.1} MB in memory, \
                     {animations} animations, {whole} needed the whole file, {} failed",
                    urls.len(),
                    started.elapsed().as_secs_f32(),
                    bytes as f64 / 1e6,
                    pixels as f64 / 1e6,
                    failed.len()
                );
                for failure in &failed {
                    println!("    {failure}");
                }
                assert!(failed.len() * 20 <= urls.len(), "more than one in twenty failed");
            }
        }
    }
}
