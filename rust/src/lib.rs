use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use std::sync::Mutex;

pub mod capture;
pub mod encoder;
pub mod buffer;
pub mod audio;

use capture::CaptureEngine;

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn set_last_error(err: anyhow::Error) {
    let mut guard = LAST_ERROR.lock().unwrap();
    *guard = Some(format!("{}", err));
}

fn clear_last_error() {
    let mut guard = LAST_ERROR.lock().unwrap();
    *guard = None;
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_init(
    framerate: u32,
    width: u32,
    height: u32,
    bitrate_kbps: u32,
    output_dir: *const c_char,
    audio_mode: *const c_char,
) -> *mut CaptureEngine {
    clear_last_error();

    let output_dir_str = unsafe {
        CStr::from_ptr(output_dir).to_str().unwrap_or("")
    };

    let audio_mode_str = if audio_mode.is_null() {
        "off"
    } else {
        unsafe { CStr::from_ptr(audio_mode).to_str().unwrap_or("off") }
    };

    match CaptureEngine::new(
        framerate,
        width,
        height,
        bitrate_kbps,
        output_dir_str,
        audio_mode_str,
    ) {
        Ok(engine) => Box::into_raw(Box::new(engine)),
        Err(e) => {
            set_last_error(e);
            std::ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_start(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return -1;
    }
    let engine = unsafe { &mut *ctx };
    clear_last_error();
    match engine.start() {
        Ok(_) => 0,
        Err(e) => {
            set_last_error(e);
            -1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_stop(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
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

#[unsafe(no_mangle)]
pub extern "C" fn capture_is_running(ctx: *mut CaptureEngine) -> i32 {
    if ctx.is_null() {
        return 0;
    }
    let engine = unsafe { &*ctx };
    if engine.is_running() { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_save_clip(
    ctx: *mut CaptureEngine,
    seconds: u32,
) -> *const c_char {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return std::ptr::null();
    }
    let engine = unsafe { &mut *ctx };
    clear_last_error();
    match engine.save_clip(seconds) {
        Ok(path) => {
            let cstring = CString::new(path).unwrap();
            cstring.into_raw()
        }
        Err(e) => {
            set_last_error(e);
            std::ptr::null()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_save_clip_metadata(
    ctx: *mut CaptureEngine,
    seconds: u32,
    game_name: *const c_char,
    event_type: *const c_char,
) -> *const c_char {
    if ctx.is_null() {
        set_last_error(anyhow::anyhow!("null context"));
        return std::ptr::null();
    }
    let game_name_str = if game_name.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(game_name).to_str().unwrap_or("") }
    };
    let event_type_str = if event_type.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(event_type).to_str().unwrap_or("") }
    };
    let engine = unsafe { &mut *ctx };
    clear_last_error();
    match engine.save_clip_with_metadata(seconds, game_name_str, event_type_str) {
        Ok(path) => {
            let cstring = CString::new(path).unwrap();
            cstring.into_raw()
        }
        Err(e) => {
            set_last_error(e);
            std::ptr::null()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_close(ctx: *mut CaptureEngine) {
    if !ctx.is_null() {
        let mut engine = unsafe { Box::from_raw(ctx) };
        let _ = engine.stop();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_free_string(ptr: *const c_char) {
    if !ptr.is_null() {
        unsafe { let _ = CString::from_raw(ptr as *mut c_char); }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn capture_last_error() -> *const c_char {
    let guard = LAST_ERROR.lock().unwrap();
    match guard.as_ref() {
        Some(err) => {
            let cstring = CString::new(err.as_str()).unwrap();
            cstring.into_raw()
        }
        Option::None => std::ptr::null()
    }
}
