# GoClip 🎬
A lightweight desktop clip recorder written in Go.

Records your screen continuously and saves the last **30 seconds** when you press **F9**.
No installation required for end users — GoClip downloads and manages FFmpeg automatically.

---

## For End Users (Zero Setup)

1. Download `frostclip.exe`
2. Run it
3. That's it

On first launch GoClip will download FFmpeg (~90MB, one-time only) and auto-detect
your audio device. After that it starts recording immediately.

**Press F9** at any time to save a clip to the `clips/` folder next to `goclip.exe`.

---

## For Developers — Building from Source

### Requirements
- **Go 1.21+** → https://go.dev/dl/
- Windows (GoClip uses WinAPI for hotkeys and gdigrab for screen capture)
- Internet connection (first run only, to download FFmpeg)

### Build steps

```
git clone <your-repo>
cd goclip
go mod tidy
go build -o goclip.exe .
```

### Build without console window (for distribution)
```
go build -ldflags="-H windowsgui" -o goclip.exe .
```

---

## How autonomous setup works

On first launch GoClip:

1. Checks if `ffmpeg/ffmpeg.exe` exists next to the exe
2. If not, downloads the latest FFmpeg essentials build from gyan.dev (~90MB)
3. Extracts only `ffmpeg.exe` and `ffprobe.exe` from the zip
4. Writes a `.ffmpeg_ready` marker so it never downloads again
5. Runs `ffmpeg -list_devices` to enumerate all DirectShow audio devices
6. Picks the best one automatically using this priority:
   - Stereo Mix (captures speaker output)
   - VB-Audio / Virtual Cable
   - Wave Out / Loopback devices
   - First available device
   - No audio (video only) if nothing found

---

## Configuration (developers)

All settings are in `main.go` and `internal/capture/capture.go`:

| Setting | Location | Default |
|---|---|---|
| Clip length | `buffer.New(30)` in `main.go` | 30 seconds |
| Framerate | `Framerate: 30` in `main.go` | 30 fps |
| QSV quality | `-global_quality 25` in capture.go | 25 |
| CPU quality | `-crf 28` in capture.go | 28 |
| Audio bitrate | `-b:a 128k` in capture.go | 128k |
| Save hotkey | `hotkey.go` VK_F9 | F9 |
| Output folder | `clipsDir` in `main.go` | `clips/` next to exe |

Rebuild after any changes: `go build -o goclip.exe .`

---

## Project Structure

```
goclip/
├── main.go                    # Entry point
├── go.mod / go.sum
├── ffmpeg/                    # Created automatically on first run
│   ├── ffmpeg.exe
│   └── ffprobe.exe
├── .ffmpeg_ready              # Marker file — skip re-download
├── clips/                     # Your saved clips
└── internal/
    ├── setup/
    │   ├── setup.go           # FFmpeg download + audio detection
    │   └── exec.go            # Command runner helper
    ├── buffer/
    │   └── buffer.go          # Thread-safe circular buffer
    ├── capture/
    │   └── capture.go         # FFmpeg screen capture loop
    ├── hotkey/
    │   └── hotkey.go          # Global F9 hotkey (WinAPI)
    └── save/
        └── save.go            # Concat segments → final clip
```

---

## How It Works

```
First launch:
  goclip.exe
    └─ setup: download ffmpeg (~90MB, once)
    └─ setup: auto-detect audio device
    └─ start recording

Every second:
  FFmpeg (goroutine) ──► 1-sec .ts segment ──► Circular Buffer (last 30)

On F9:
  Hotkey (goroutine) ──► saveChan ──► Save Handler
                                         └─ ffmpeg -c copy
                                         └─ clips/clip_2024-xx-xx.mp4
```

Saving uses `-c copy` (no re-encoding), so clips are saved in under a second
regardless of length.

---

## Troubleshooting

**Stuck on "Downloading FFmpeg"**
→ Check your internet connection. The download is ~90MB from gyan.dev.

**No audio in clips**
→ GoClip logs which audio device it picked. If none was found, enable
  "Stereo Mix" in Windows Sound settings (right-click speaker icon →
  Sounds → Recording tab → right-click empty area → Show Disabled Devices).

**F9 not working in a game**
→ Try running `goclip.exe` as Administrator (right-click → Run as administrator).

**Clips folder is empty after F9**
→ GoClip needs ~5 seconds on startup to fill the buffer. Wait a moment after launch.

**QSV not detected**
→ Make sure Intel Graphics drivers are up to date. GoClip falls back to
  software encoding (libx264) automatically if QSV isn't available.
