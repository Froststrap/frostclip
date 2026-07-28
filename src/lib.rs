use config::Config;
use std::ffi::CString;
use std::os::raw::c_char;
use std::sync::Mutex;

pub mod audio;
pub mod buffer;
pub mod capture;
pub mod config;
pub mod encoder;
pub mod engine;
pub mod frame;
pub mod muxer;

use engine::CaptureEngine;

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn set_last_error(err: anyhow::Error) {
    let mut guard = LAST_ERROR.lock().unwrap();
    *guard = Some(err.to_string());
}

fn clear_last_error() {
    let mut guard = LAST_ERROR.lock().unwrap();
    *guard = None;
}

pub extern "C" fn capture_init(
    framerate: u32,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    replay_buffer_seconds: u32,
    default_clip_seconds: u32,
    output_dir: *const c_char,
    _audio_mode: *const c_char,
) -> *mut CaptureEngine {
    clear_last_error();

    let output_dir = if output_dir.is_null() {
        "./clips"
    } else {
        unsafe {
            std::ffi::CStr::from_ptr(output_dir)
                .to_str()
                .unwrap_or("./clips")
        }
    };

    let config = match Config::new(
        width,
        height,
        framerate,
        bitrate_kbps,
        replay_buffer_seconds,
        default_clip_seconds,
        output_dir,
    ) {
        Ok(config) => config,

        Err(e) => {
            set_last_error(e);
            return std::ptr::null_mut();
        }
    };

    match CaptureEngine::new(config) {
        Ok(engine) => Box::into_raw(Box::new(engine)),

        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

pub fn capture_start(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return -1;
    }

    clear_last_error();

    let engine = unsafe { &mut *ctx };

    match engine.start() {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(e);
            -1
        }
    }
}

pub fn capture_stop(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return -1;
    }

    let engine = unsafe { &mut *ctx };

    match engine.stop() {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(e);
            -1
        }
    }
}

pub fn capture_is_running(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
        return 0;
    }

    let engine = unsafe { &*ctx };

    if engine.is_running() { 1 } else { 0 }
}

pub fn capture_save_clip(ctx: *mut CaptureEngine, _seconds: u32) -> *const c_char {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return std::ptr::null();
    }

    /*
        Replay saving is not implemented yet.
        This keeps the C ABI stable while
        the capture/encoder pipeline is being built.
    */

    set_last_error(anyhow::anyhow!("save_clip not implemented"));

    std::ptr::null()
}

pub fn capture_save_clip_metadata(
    ctx: *mut CaptureEngine,
    _seconds: u32,
    _game_name: *const c_char,
    _event_type: *const c_char,
) -> *const c_char {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return std::ptr::null();
    }

    /*
        Metadata replay saving is not implemented yet.
    */

    set_last_error(anyhow::anyhow!("save_clip_with_metadata not implemented"));

    std::ptr::null()
}

pub fn capture_close(ctx: *mut CaptureEngine) {
    if !ctx.is_null() {
        let mut engine = unsafe { Box::from_raw(ctx) };

        let _ = engine.stop();
    }
}

pub fn capture_free_string(ptr: *const c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr as *mut c_char);
        }
    }
}

pub fn capture_last_error() -> *const c_char {
    let guard = LAST_ERROR.lock().unwrap();

    match guard.as_ref() {
        Some(err) => {
            let cstring = CString::new(err.as_str()).unwrap();

            cstring.into_raw()
        }

        None => std::ptr::null(),
    }
}
