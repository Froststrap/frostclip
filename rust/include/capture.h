#ifndef RUST_CAPTURE_H
#define RUST_CAPTURE_H

#include <stdint.h>
#include <stddef.h>

typedef struct CaptureContext CaptureContext;

CaptureContext* capture_init(
    uint32_t framerate,
    uint32_t width,
    uint32_t height,
    uint32_t bitrate_kbps,
    uint32_t replay_buffer_seconds,
    uint32_t default_clip_seconds,
    const char* output_dir,
    const char* audio_mode
);

int capture_start(CaptureContext* ctx);
int capture_stop(CaptureContext* ctx);
const char* capture_save_clip(CaptureContext* ctx, uint32_t seconds);
const char* capture_save_clip_metadata(
    CaptureContext* ctx,
    uint32_t seconds,
    const char* game_name,
    const char* event_type
);
int capture_is_running(CaptureContext* ctx);
void capture_close(CaptureContext* ctx);
void capture_free_string(const char* ptr);
const char* capture_last_error(void);

#endif
