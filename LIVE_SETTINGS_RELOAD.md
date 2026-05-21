# Live Settings Reload

## Overview

FrostClip now supports live settings reload without requiring an app restart. Simply edit `settings.json` and the changes are automatically detected and applied.

## How It Works

### File Monitoring
- The settings watcher monitors `settings.json` in the same directory as the FrostClip executable
- Changes are detected with **100ms debouncing** to avoid multiple restarts on rapid edits
- Invalid JSON is logged as a warning and ignored — your previous valid settings are retained

### Video Settings Changes (FPS, Resolution, Bitrate)
When any of these settings change:
- The current FFmpeg capture process is killed
- A brief capture gap occurs (< 1 second) while FFmpeg restarts
- FFmpeg automatically restarts with the new video settings
- Audio continues and new segments are captured with the new video configuration

### Audio Settings Changes (Audio Mode)
- Detected and logged
- Applied on the next capture segment
- No interruption to recording

### Upload Settings Changes (AutoUpload)
- Applied immediately to subsequent save operations
- Current saves in progress are not affected

## Supported Settings That Can Be Changed Live

| Setting | Type | Effect |
|---------|------|--------|
| `fps` | number or "refresh_rate" | Triggers capture restart |
| `resolution` | string or "full_screen" | Triggers capture restart |
| `bitrate` | string or "auto" | Triggers capture restart |
| `audio` | "microphone", "system", "both", "off" | Applied on next segment |
| `auto_upload` | boolean | Applied immediately |
| `segment_temp_dir` | string | Applied on next segment |

## Logging

Settings changes are logged to both console and log file:
```
2026-05-21 20:40:15	INFO	settings	capture settings changed	fps=true resolution=false bitrate=true
2026-05-21 20:40:15	INFO	capture	applying new capture settings	fps=144 resolution=2560x1440 bitrate=8000k
```

## Example Usage

1. Edit `settings.json`:
```json
{
  "fps": "refresh_rate",
  "resolution": "full_screen",
  "bitrate": "auto",
  "audio": "system",
  "segment_temp_dir": "",
  "auto_upload": false
}
```

2. Change a setting:
```json
{
  "fps": 144,
  "resolution": "2560x1440",
  "bitrate": "10000k",
  "audio": "system",
  "segment_temp_dir": "",
  "auto_upload": false
}
```

3. Save the file — FrostClip automatically detects the change and applies it within ~1 second

## Implementation Details

### Architecture
- **Watcher**: Monitors settings.json using `fsnotify` (cross-platform file watching)
- **Update Channel**: Settings updates flow through a channel from settings package to capture and save handlers
- **Change Tracking**: Each update includes a map of which fields changed, so handlers only react to relevant changes
- **Thread Safety**: Settings are accessed safely from multiple goroutines

### Files Modified
- `internal/settings/settings.go` — Added `NewWatcher()` and `Watch()` functions, `SettingsUpdate` type
- `main.go` — Start watcher, pass update channel to capture and save handlers
- `internal/capture/capture.go` — Added `UpdateCh` to Config
- `internal/capture/capture_linux.go` — Listen for updates, restart FFmpeg on video setting changes
- `internal/capture/capture_loop_windows.go` — Listen for updates, restart FFmpeg on video setting changes
- `internal/capture/capture_loop_darwin.go` — Listen for updates, restart FFmpeg on video setting changes
- `internal/save/save.go` — Listen for updates, apply AutoUpload changes dynamically

## Limitations

- **Network paths**: If `segment_temp_dir` points to a network location, changes may not apply until next segment
- **FFmpeg process**: Video setting changes require a brief restart (~500ms), during which no frames are captured
- **Validation**: Invalid JSON is silently ignored; use your editor's JSON validation to catch syntax errors

## Testing

To test live settings reload:

1. Start FrostClip
2. Make a clip with current settings
3. Edit `settings.json` and change a video setting (e.g., `fps` or `bitrate`)
4. Save the file
5. Check the log to confirm the setting was detected:
   ```
   INFO	capture settings changed	fps=true
   INFO	applying new capture settings	fps=...
   ```
6. Make another clip — it should use the new settings

