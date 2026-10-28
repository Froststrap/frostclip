#[cfg(all(unix, not(target_os = "macos")))]
use ffmpeg_next as ffmpeg;

#[cfg(all(unix, not(target_os = "macos")))]
use ffmpeg::format::{Pixel, input};
#[cfg(all(unix, not(target_os = "macos")))]
use ffmpeg::media::Type;
#[cfg(all(unix, not(target_os = "macos")))]
use ffmpeg::software::scaling::{context::Context, flag::Flags};
#[cfg(all(unix, not(target_os = "macos")))]
use ffmpeg::util::frame::video::Video;

use notify_rust::Notification;
use tracing::warn;

#[cfg(all(unix, not(target_os = "macos")))]
const THUMB_WIDTH: u32 = 320;

#[cfg(all(unix, not(target_os = "macos")))]
fn thumbnail(path: &str) -> Option<notify_rust::Image> {
    let mut ictx = input(path).ok()?;
    let stream = ictx.streams().best(Type::Video)?;
    let index = stream.index();
    let codec = ffmpeg::codec::context::Context::from_parameters(stream.parameters()).ok()?;
    let mut decoder = codec.decoder().video().ok()?;

    let out_height =
        ((decoder.height() as u64 * THUMB_WIDTH as u64 / decoder.width() as u64) as u32) & !1;

    let mut scaler = Context::get(
        decoder.format(),
        decoder.width(),
        decoder.height(),
        Pixel::RGB24,
        THUMB_WIDTH,
        out_height,
        Flags::BILINEAR,
    )
    .ok()?;

    let mut decoded = Video::empty();

    for (packet_stream, packet) in ictx.packets() {
        if packet_stream.index() != index {
            continue;
        }

        decoder.send_packet(&packet).ok()?;

        if decoder.receive_frame(&mut decoded).is_ok() {
            let mut rgb = Video::empty();
            scaler.run(&decoded, &mut rgb).ok()?;

            let width = rgb.width() as usize;
            let height = rgb.height() as usize;
            let stride = rgb.stride(0);
            let data = rgb.data(0);

            let mut pixels = Vec::with_capacity(width * height * 3);
            for row in 0..height {
                let start = row * stride;
                pixels.extend_from_slice(&data[start..start + width * 3]);
            }

            return notify_rust::Image::from_rgb(width as i32, height as i32, pixels).ok();
        }
    }

    None
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn send(summary: &str, body: &str, video: Option<&str>) {
    let summary = summary.to_string();
    let body = body.to_string();
    let video = video.map(str::to_string);

    tokio::spawn(async move {
        let image = match video {
            Some(path) => tokio::task::spawn_blocking(move || thumbnail(&path))
                .await
                .ok()
                .flatten(),
            None => None,
        };

        let mut notification = Notification::new();
        notification
            .appname("FrostClip")
            .summary(&summary)
            .body(&body);

        if let Some(image) = image {
            notification.image_data(image);
        }

        if let Err(err) = notification.show_async().await {
            warn!("NOTIFY: failed to show notification: {}", err);
        }
    });
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn send(summary: &str, body: &str, _video: Option<&str>) {
    let summary = summary.to_string();
    let body = body.to_string();

    std::thread::spawn(move || {
        if let Err(err) = Notification::new()
            .appname("FrostClip")
            .summary(&summary)
            .body(&body)
            .show()
        {
            warn!("NOTIFY: failed to show notification: {}", err);
        }
    });
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn init() {
    let _ = std::thread::spawn(|| {
        let _ = &*notify_rust::SPEC_VERSION;
    })
    .join();
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
pub fn init() {}
