//go:build linux

package capture

import (
	"bufio"
	"bytes"
	"context"
	"fmt"
	"frostclip/internal/buffer"
	"frostclip/internal/notify"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"

	"github.com/neurlang/wayland/wl"
	"go.uber.org/zap"
)

type gpuVendor int

const (
	vendorUnknown gpuVendor = iota
	vendorNVIDIA
	vendorAMD
	vendorIntel
)

func detectGPUVendor() gpuVendor {
	entries, err := os.ReadDir("/sys/class/drm")
	if err != nil {
		return vendorUnknown
	}
	for _, e := range entries {
		data, err := os.ReadFile(fmt.Sprintf("/sys/class/drm/%s/device/vendor", e.Name()))
		if err != nil {
			continue
		}
		switch strings.TrimSpace(string(data)) {
		case "0x10de":
			return vendorNVIDIA
		case "0x1002":
			return vendorAMD
		case "0x8086":
			return vendorIntel
		}
	}
	return vendorUnknown
}

func detectRenderNode() string {
	nodes, err := filepath.Glob("/dev/dri/renderD*")
	if err == nil && len(nodes) > 0 {
		return nodes[0]
	}
	return "/dev/dri/renderD128"
}

func buildScaleFilter(resolution string) string {
	if resolution == "" {
		return ""
	}
	parts := strings.SplitN(resolution, "x", 2)
	if len(parts) != 2 {
		return ""
	}
	return fmt.Sprintf(",scale=w=%s:h=%s:flags=fast_bilinear", parts[0], parts[1])
}

func buildScaleVaapiFilter(resolution string) string {
	if resolution == "" {
		return ""
	}
	parts := strings.SplitN(resolution, "x", 2)
	if len(parts) != 2 {
		return ""
	}
	return fmt.Sprintf(",scale_vaapi=w=%s:h=%s", parts[0], parts[1])
}

func appendAudio(args *[]string, audioLabel string) {
	if audioLabel != "" {
		*args = append(*args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
	}
}

func hwVideoArgs(cfg Config, segPattern string, v gpuVendor, inputW, inputH uint32, inputPixFmt string) []string {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}
	if inputPixFmt == "" {
		inputPixFmt = "bgra"
	}
	br := bitrate(cfg)

	w, h := int(inputW), int(inputH)
	if w <= 0 || h <= 0 {
		w, h = 1920, 1080
	}

	args := []string{
		"-y",
		"-f", "rawvideo",
		"-pix_fmt", inputPixFmt,
		"-s", fmt.Sprintf("%dx%d", w, h),
		"-fflags", "+genpts",
		"-use_wallclock_as_timestamps", "1",
		"-r", fmt.Sprintf("%d", fps),
		"-i", "pipe:0",
	}

	inputArgs, audioRefs := audioArgs(cfg)
	args = append(args, inputArgs...)

	audioFrag, audioLabel := audioMixFragment(audioRefs)

	switch v {
	case vendorNVIDIA:
		scale := buildScaleFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=yuv420p%s[vout]", scale)
		args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "h264_nvenc", "-preset", "p4",
			"-cq", "23", "-b:v", "0", "-maxrate", br, "-bufsize", br,
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
		)
	case vendorAMD, vendorIntel:
		renderNode := detectRenderNode()
		scale := buildScaleVaapiFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=nv12,hwupload=extra_hw_frames=64%s[vout]", scale)
		args = append(args,
			"-init_hw_device", fmt.Sprintf("vaapi=va:%s", renderNode),
			"-filter_hw_device", "va",
			"-filter_complex", fc+audioFrag,
			"-map", "[vout]",
		)
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "h264_vaapi",
			"-b:v", br, "-maxrate", br, "-bufsize", br,
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
		)
	default:
		scale := buildScaleFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=yuv420p%s[vout]", scale)
		args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "libx264", "-preset", "ultrafast", "-crf", "23",
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
			"-sc_threshold", "0",
		)
	}

	return append(args,
		"-force_key_frames", fmt.Sprintf("expr:gte(t,n_forced*%d)", segmentSeconds),
		"-f", "segment",
		"-segment_time", fmt.Sprintf("%d", segmentSeconds),
		"-segment_format", "mpegts",
		"-reset_timestamps", "1",
		segPattern,
	)
}

func x11GrabArgs(cfg Config, segPattern string, v gpuVendor) []string {
	fps := cfg.Framerate
	if fps == 0 {
		fps = 60
	}
	display := os.Getenv("DISPLAY")
	if display == "" {
		display = ":0"
	}
	br := bitrate(cfg)

	args := []string{
		"-y",
		"-f", "x11grab",
		"-framerate", fmt.Sprintf("%d", fps),
		"-i", display,
	}

	inputArgs, audioRefs := audioArgs(cfg)
	args = append(args, inputArgs...)

	audioFrag, audioLabel := audioMixFragment(audioRefs)

	switch v {
	case vendorNVIDIA:
		scale := buildScaleFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=yuv420p%s[vout]", scale)
		args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "h264_nvenc", "-preset", "p4",
			"-cq", "23", "-b:v", "0", "-maxrate", br, "-bufsize", br,
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
		)
	case vendorAMD, vendorIntel:
		renderNode := detectRenderNode()
		scale := buildScaleVaapiFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=nv12,hwupload=extra_hw_frames=64%s[vout]", scale)
		args = append(args,
			"-init_hw_device", fmt.Sprintf("vaapi=va:%s", renderNode),
			"-filter_hw_device", "va",
			"-filter_complex", fc+audioFrag,
			"-map", "[vout]",
		)
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "h264_vaapi",
			"-b:v", br, "-maxrate", br, "-bufsize", br,
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
		)
	default:
		scale := buildScaleFilter(cfg.Resolution)
		fc := fmt.Sprintf("[0:v]format=yuv420p%s[vout]", scale)
		args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
		appendAudio(&args, audioLabel)
		args = append(args,
			"-vcodec", "libx264", "-preset", "ultrafast", "-crf", "23",
			"-g", fmt.Sprintf("%d", fps*segmentSeconds),
			"-keyint_min", fmt.Sprintf("%d", fps*segmentSeconds),
			"-sc_threshold", "0",
		)
	}

	return append(args,
		"-force_key_frames", fmt.Sprintf("expr:gte(t,n_forced*%d)", segmentSeconds),
		"-f", "segment",
		"-segment_time", fmt.Sprintf("%d", segmentSeconds),
		"-segment_format", "mpegts",
		"-reset_timestamps", "1",
		segPattern,
	)
}

func watchSegments(ctx context.Context, buf *buffer.CircularBuffer, dir string, log *zap.Logger) {
	seen := map[string]bool{}
	for {
		select {
		case <-ctx.Done():
			return
		case <-time.After(200 * time.Millisecond):
		}
		for _, seg := range finalizedSegments(dir, seen) {
			seen[seg.name] = true
			buf.Push(seg.path)
			log.Debug("segment pushed", zap.String("file", seg.name))
		}
	}
}

type readySegment struct {
	name string
	path string
	idx  int
}

func finalizedSegments(dir string, seen map[string]bool) []readySegment {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil
	}
	var all []readySegment
	for _, e := range entries {
		name := e.Name()
		if seen[name] || !strings.HasPrefix(name, "seg") || !strings.HasSuffix(name, ".ts") {
			continue
		}
		n := strings.TrimSuffix(strings.TrimPrefix(name, "seg"), ".ts")
		idx, err := strconv.Atoi(n)
		if err != nil {
			continue
		}
		full := filepath.Join(dir, name)
		info, err := os.Stat(full)
		if err != nil || info.Size() == 0 {
			continue
		}
		all = append(all, readySegment{name: name, path: full, idx: idx})
	}
	if len(all) < 2 {
		return nil
	}
	sort.Slice(all, func(i, j int) bool { return all[i].idx < all[j].idx })
	return all[:len(all)-1]
}

func Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	if os.Getenv("WAYLAND_DISPLAY") != "" {
		log.Info("capture: Wayland — trying ext-image-copy-capture-v1")
		waylandLoop(buf, cfg, log)
	} else {
		log.Info("capture: X11 — using x11grab")
		x11Loop(buf, cfg, log)
	}
}

func x11Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	vendor := detectGPUVendor()
	log.Info("x11grab detected GPU vendor", zap.Int("vendor", int(vendor)))

	// Start settings update monitor
	settingsChan := make(chan Config, 1)
	if cfg.UpdateCh != nil {
		go func() {
			for update := range cfg.UpdateCh {
				newCfg := cfg
				newCfg.Framerate = update.Settings.FPS
				newCfg.Resolution = update.Settings.Resolution
				newCfg.Bitrate = update.Settings.Bitrate

				if update.Changed["fps"] || update.Changed["resolution"] || update.Changed["bitrate"] {
					log.Info("capture settings changed",
						zap.Bool("fps", update.Changed["fps"]),
						zap.Bool("resolution", update.Changed["resolution"]),
						zap.Bool("bitrate", update.Changed["bitrate"]),
					)
					settingsChan <- newCfg
				}
			}
		}()
	}

	for {
		// Check for settings update
		select {
		case newCfg := <-settingsChan:
			cfg = newCfg
			log.Info("applying new capture settings",
				zap.Int("fps", cfg.Framerate),
				zap.String("resolution", cfg.Resolution),
				zap.String("bitrate", cfg.Bitrate),
			)
			// Kill current FFmpeg process to force restart with new settings
			Kill()
			time.Sleep(500 * time.Millisecond)
			continue
		default:
		}

		segPattern := filepath.Join(buf.TempDir(), "seg%06d.ts")
		args := x11GrabArgs(cfg, segPattern, vendor)
		cmd := exec.Command(cfg.FFmpegBin, args...)
		cmd.Stderr = os.Stderr

		log.Debug("x11grab: ffmpeg command", zap.String("cmd", fmt.Sprintf("%s %s", cfg.FFmpegBin, strings.Join(args, " "))))
		if err := cmd.Start(); err != nil {
			log.Warn("x11grab: start failed", zap.Error(err))
			time.Sleep(time.Second)
			continue
		}
		setActiveCmd(cmd.Process)

		ctx, cancel := context.WithCancel(context.Background())
		go watchSegments(ctx, buf, buf.TempDir(), log)

		if err := cmd.Wait(); err != nil {
			log.Warn("x11grab: ffmpeg exited", zap.Error(err))
		}
		cancel()
		clearActiveCmd()
		time.Sleep(500 * time.Millisecond)
	}
}

func waylandLoop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	vendor := detectGPUVendor()
	log.Info("wayland detected GPU vendor", zap.Int("vendor", int(vendor)))
	consecErr := 0

	// Start settings update monitor
	settingsChan := make(chan Config, 1)
	if cfg.UpdateCh != nil {
		go func() {
			for update := range cfg.UpdateCh {
				newCfg := cfg
				newCfg.Framerate = update.Settings.FPS
				newCfg.Resolution = update.Settings.Resolution
				newCfg.Bitrate = update.Settings.Bitrate

				if update.Changed["fps"] || update.Changed["resolution"] || update.Changed["bitrate"] {
					log.Info("capture settings changed",
						zap.Bool("fps", update.Changed["fps"]),
						zap.Bool("resolution", update.Changed["resolution"]),
						zap.Bool("bitrate", update.Changed["bitrate"]),
					)
					settingsChan <- newCfg
				}
			}
		}()
	}

	for {
		// Check for settings update
		select {
		case newCfg := <-settingsChan:
			cfg = newCfg
			log.Info("applying new capture settings",
				zap.Int("fps", cfg.Framerate),
				zap.String("resolution", cfg.Resolution),
				zap.String("bitrate", cfg.Bitrate),
			)
			// Kill current FFmpeg process to force restart with new settings
			Kill()
			time.Sleep(500 * time.Millisecond)
			continue
		default:
		}

		err := runWaylandCapture(buf, cfg, vendor, log)
		if err != nil {
			consecErr++
			log.Warn("wayland capture error — retrying in 1s", zap.Error(err), zap.Int("consecutive", consecErr))

			// If compositor reports invalid buffer or the wayland connection is closed abruptly,
			// attempt wf-recorder fallback immediately as a last resort for broken compositors.
			errStr := err.Error()
			if strings.Contains(errStr, "invalid buffer") || strings.Contains(errStr, "unable to read message header") {
				wfPath, wfErr := exec.LookPath("wf-recorder")
				if wfErr != nil {
					notify.Send("FrostClip: Wayland fallback missing", "wf-recorder not found. Install wf-recorder for automatic Wayland fallback.")
					log.Warn("wf-recorder not found; cannot fallback")
				} else {
					log.Info("attempting wf-recorder fallback due to compositor error", zap.String("wf-recorder", wfPath))
					if fbErr := runWfFallback(buf, cfg, log); fbErr != nil {
						log.Warn("wf-recorder fallback failed", zap.Error(fbErr))
					} else {
						log.Info("wf-recorder fallback finished; resuming main capture")
					}
				}
				consecErr = 0
				time.Sleep(time.Second)
				continue
			}

			// Otherwise, fall back to wf-recorder after repeated failures
			if consecErr >= 3 {
				wfPath, wfErr := exec.LookPath("wf-recorder")
				if wfErr != nil {
					// notify user once
					notify.Send("FrostClip: Wayland fallback missing", "wf-recorder not found. Install wf-recorder for automatic Wayland fallback.")
					log.Warn("wf-recorder not found; cannot fallback")
				} else {
					log.Info("attempting wf-recorder fallback", zap.String("wf-recorder", wfPath))
					if fbErr := runWfFallback(buf, cfg, log); fbErr != nil {
						log.Warn("wf-recorder fallback failed", zap.Error(fbErr))
					} else {
						log.Info("wf-recorder fallback finished; resuming main capture")
					}
				}
				consecErr = 0
			}

			time.Sleep(time.Second)
			continue
		}
		consecErr = 0
	}
}

// runWfFallback uses wf-recorder as a quick Wayland capture fallback when
// wlr-screencopy path repeatedly fails. It records short MP4 segments with
// wf-recorder, remuxes them into MPEG-TS segments and pushes into the circular
// buffer so the rest of pipeline can operate unchanged.
func runWfFallback(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) error {
	wfPath, err := exec.LookPath("wf-recorder")
	if err != nil {
		// notify user once
		notify.Send("FrostClip: Wayland fallback missing", "wf-recorder not found. Install wf-recorder for automatic Wayland fallback.")
		return fmt.Errorf("wf-recorder not found: %w", err)
	}
	ffmpegPath, _ := exec.LookPath("ffmpeg")
	segDir := buf.TempDir()
	idx := 0
	segDur := time.Duration(segmentSeconds) * time.Second
	log.Info("starting wf-recorder fallback", zap.String("wf-recorder", wfPath), zap.Duration("segment", segDur))
	for {
		idx++
		mp4 := filepath.Join(segDir, fmt.Sprintf("wfseg%06d.mp4", idx))
		ts := filepath.Join(segDir, fmt.Sprintf("seg%06d.ts", idx))

		// Ensure previous files removed
		_ = os.Remove(mp4)
		_ = os.Remove(ts)

		ctx, cancel := context.WithTimeout(context.Background(), segDur+5*time.Second)
		cmd := exec.CommandContext(ctx, wfPath, "-f", mp4)
		// add audio flag if configured
		if cfg.hasAudio() {
			cmd.Args = append(cmd.Args, "-a")
		}
		// start wf-recorder
		log.Debug("running wf-recorder", zap.Strings("args", cmd.Args))
		setActiveCmd(cmd.Process)
		if err := cmd.Start(); err != nil {
			cancel()
			clearActiveCmd()
			return fmt.Errorf("start wf-recorder: %w", err)
		}
		// Wait for command or timeout
		err = cmd.Wait()
		cancel()
		clearActiveCmd()
		if ctx.Err() == context.DeadlineExceeded {
			log.Warn("wf-recorder segment timeout, proceeding", zap.Int("idx", idx))
		}

		// Validate mp4 exists
		st, statErr := os.Stat(mp4)
		if statErr != nil || st.Size() == 0 {
			log.Warn("wf-recorder produced empty file, retrying", zap.Int("idx", idx), zap.Error(statErr))
			// If wf-recorder repeatedly fails, abort fallback
			if err != nil {
				return fmt.Errorf("wf-recorder failed: %w", err)
			}
			continue
		}

		// Remux to mpegts so rest of pipeline can consume segments
		if ffmpegPath == "" {
			// try to find ffmpeg in config
			if cfg.FFmpegBin != "" {
				ffmpegPath = cfg.FFmpegBin
			}
		}
		if ffmpegPath == "" {
			log.Warn("ffmpeg not found; cannot remux wf-recorder output")
			return fmt.Errorf("ffmpeg not found")
		}
		remux := exec.Command(ffmpegPath, "-y", "-i", mp4, "-c", "copy", "-f", "mpegts", ts)
		log.Debug("remuxing wf segment", zap.Strings("args", remux.Args))
		if out, e := remux.CombinedOutput(); e != nil {
			log.Warn("ffmpeg remux failed", zap.Error(e), zap.ByteString("out", out))
			// keep mp4 for debugging, continue
			continue
		}
		// Push ts into buffer and cleanup mp4
		buf.Push(ts)
		_ = os.Remove(mp4)
		log.Info("wf-recorder segment pushed", zap.String("segment", ts))
	}
}

type pendingGlobal struct {
	name    uint32
	iface   string
	version uint32
}

type globalCollector struct {
	mu      sync.Mutex
	pending []pendingGlobal
}

func (c *globalCollector) HandleRegistryGlobal(e wl.RegistryGlobalEvent) {
	switch e.Interface {
	case "wl_shm", "ext_image_copy_capture_manager_v1", "ext_output_image_capture_source_manager_v1", "zwlr_screencopy_manager_v1", "wl_output":
		c.mu.Lock()
		c.pending = append(c.pending, pendingGlobal{e.Name, e.Interface, e.Version})
		c.mu.Unlock()
	}
}

type shmFormatCollector struct {
	mu      sync.Mutex
	formats map[uint32]bool
}

type displayErrorLogger struct {
	log *zap.Logger
}

func (d *displayErrorLogger) HandleDisplayError(e wl.DisplayErrorEvent) {
	objID := uint32(0)
	if e.ObjectId != nil {
		objID = uint32(e.ObjectId.Id())
	}
	d.log.Error("wayland compositor protocol error",
		zap.Uint32("object_id", objID),
		zap.Uint32("code", e.Code),
		zap.String("message", e.Message),
	)
}

func (c *shmFormatCollector) HandleShmFormat(e wl.ShmFormatEvent) {
	c.mu.Lock()
	if c.formats == nil {
		c.formats = make(map[uint32]bool)
	}
	c.formats[e.Format] = true
	c.mu.Unlock()
}

func (c *shmFormatCollector) Has(format uint32) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.formats[format]
}

func runWaylandCapture(buf *buffer.CircularBuffer, cfg Config, vendor gpuVendor, log *zap.Logger) error {
	display, err := wl.Connect("")
	if err != nil {
		return fmt.Errorf("wayland connect: %w", err)
	}
	defer display.Context().Close()
	display.AddErrorHandler(&displayErrorLogger{log: log})

	registry, err := display.GetRegistry()
	if err != nil {
		return fmt.Errorf("get registry: %w", err)
	}

	collector := &globalCollector{}
	registry.AddGlobalHandler(collector)

	for i := 0; i < 2; i++ {
		cb, err := display.Sync()
		if err != nil {
			return fmt.Errorf("display sync %d: %w", i, err)
		}
		if err := display.Context().RunTill(cb); err != nil {
			return fmt.Errorf("context run %d: %w", i, err)
		}
	}

	var shm *wl.Shm
	var mgr *ExtImageCopyCaptureManagerV1
	var sourceMgr *ExtOutputImageCaptureSourceManagerV1
	var wlrMgr *WlrScreencopyManagerV1
	var wlrVersion uint32
	var output *wl.Output

	collector.mu.Lock()
	pending := collector.pending
	collector.mu.Unlock()

	for _, g := range pending {
		switch g.iface {
		case "wl_shm":
			if shm == nil {
				shm = wl.NewShm(display.Context())
				if err := registry.Bind(g.name, g.iface, g.version, shm); err != nil {
					return fmt.Errorf("bind wl_shm: %w", err)
				}
			}
		case "ext_image_copy_capture_manager_v1":
			if mgr == nil {
				mgr = NewExtImageCopyCaptureManagerV1(display.Context())
				if err := registry.Bind(g.name, g.iface, g.version, mgr); err != nil {
					return fmt.Errorf("bind ext manager: %w", err)
				}
			}
		case "ext_output_image_capture_source_manager_v1":
			if sourceMgr == nil {
				sourceMgr = NewExtOutputImageCaptureSourceManagerV1(display.Context())
				if err := registry.Bind(g.name, g.iface, g.version, sourceMgr); err != nil {
					return fmt.Errorf("bind ext output source manager: %w", err)
				}
			}
		case "zwlr_screencopy_manager_v1":
			if wlrMgr == nil {
				wlrMgr = NewWlrScreencopyManagerV1(display.Context())
				wlrVersion = g.version
				if wlrVersion > 3 {
					wlrVersion = 3
				}
				if err := registry.Bind(g.name, g.iface, wlrVersion, wlrMgr); err != nil {
					return fmt.Errorf("bind wlr screencopy manager: %w", err)
				}
			}
		case "wl_output":
			if output == nil {
				output = wl.NewOutput(display.Context())
				if err := registry.Bind(g.name, g.iface, g.version, output); err != nil {
					return fmt.Errorf("bind wl_output: %w", err)
				}
			}
		}
	}

	shmFormats := &shmFormatCollector{formats: make(map[uint32]bool)}
	if shm != nil {
		shm.AddFormatHandler(shmFormats)
		for i := 0; i < 2; i++ {
			cb, err := display.Sync()
			if err != nil {
				return fmt.Errorf("display sync shm formats %d: %w", i, err)
			}
			if err := display.Context().RunTill(cb); err != nil {
				return fmt.Errorf("context run shm formats %d: %w", i, err)
			}
		}
	}

	log.Debug("wayland registry globals:",
		zap.Bool("has_shm", shm != nil),
		zap.Bool("has_manager", mgr != nil),
		zap.Bool("has_source_manager", sourceMgr != nil),
		zap.Bool("has_wlr_screencopy", wlrMgr != nil),
		zap.Bool("has_output", output != nil),
	)

	if shm == nil {
		return fmt.Errorf("no wl_shm available")
	}
	if output == nil {
		return fmt.Errorf("no wl_output available")
	}
	if mgr == nil || sourceMgr == nil {
		if wlrMgr == nil {
			return fmt.Errorf("compositor does not support ext_image_copy_capture_manager_v1 or zwlr_screencopy_manager_v1")
		}
		log.Info("wayland ext-image-copy not available — falling back to wlr-screencopy")
		return runWlrScreencopyCapture(display, shm, output, wlrMgr, wlrVersion, cfg, vendor, buf, log)
	}

	// Try ext-image-copy capture; fall back to wlr-screencopy if it fails
	extErr := runExtImageCopyCapture(display, shm, output, mgr, sourceMgr, shmFormats, cfg, vendor, buf, log)
	if extErr != nil && wlrMgr != nil {
		log.Warn("ext-image-copy capture failed, falling back to wlr-screencopy", zap.Error(extErr))
		return runWlrScreencopyCapture(display, shm, output, wlrMgr, wlrVersion, cfg, vendor, buf, log)
	}
	return extErr
}

func runExtImageCopyCapture(display *wl.Display, shm *wl.Shm, output *wl.Output, mgr *ExtImageCopyCaptureManagerV1, sourceMgr *ExtOutputImageCaptureSourceManagerV1, shmFormats *shmFormatCollector, cfg Config, vendor gpuVendor, buf *buffer.CircularBuffer, log *zap.Logger) error {
	shmFormat := uint32(wl.ShmFormatArgb8888)
	inputPixFmt := "bgra"

	source, err := sourceMgr.CreateSource(output)
	if err != nil {
		return fmt.Errorf("create capture source: %w", err)
	}
	defer source.Destroy()

	session, err := mgr.CreateSession(source, 0)
	if err != nil {
		return fmt.Errorf("create capture session: %w", err)
	}
	defer session.Destroy()

	// Wait until compositor sends initial buffer constraints.
	sessionReady := make(chan error, 1)
	session.OnDone = func() {
		select {
		case sessionReady <- nil:
		default:
		}
	}
	session.OnStopped = func() {
		select {
		case sessionReady <- fmt.Errorf("capture session stopped"):
		default:
		}
	}
	if err := dispatchUntilDone(display, sessionReady, 3*time.Second); err != nil {
		return fmt.Errorf("wait capture constraints: %w", err)
	}

	if session.SupportsShmFormat(uint32(wl.ShmFormatXrgb8888)) {
		shmFormat = uint32(wl.ShmFormatXrgb8888)
		inputPixFmt = "bgr0"
	} else if session.SupportsShmFormat(uint32(wl.ShmFormatArgb8888)) {
		shmFormat = uint32(wl.ShmFormatArgb8888)
		inputPixFmt = "bgra"
	} else if shmFormats.Has(uint32(wl.ShmFormatXrgb8888)) {
		shmFormat = uint32(wl.ShmFormatXrgb8888)
		inputPixFmt = "bgr0"
	} else if shmFormats.Has(uint32(wl.ShmFormatArgb8888)) {
		shmFormat = uint32(wl.ShmFormatArgb8888)
		inputPixFmt = "bgra"
	} else {
		log.Warn("compositor did not advertise XRGB8888/ARGB8888; falling back to ARGB8888")
	}
	log.Debug("wayland shm format selected", zap.String("ffmpeg_pix_fmt", inputPixFmt))

	width, height := session.BufferWidth, session.BufferHeight
	if width == 0 || height == 0 {
		return fmt.Errorf("invalid capture buffer size from compositor: %dx%d", width, height)
	}
	log.Info("wayland capture starting", zap.Uint32("width", width), zap.Uint32("height", height))

	frameSize := int(width * height * 4) // BGRA = 4 bytes
	poolSize := frameSize * 2

	shmFd, shmData, cleanup, err := allocShm(poolSize)
	if err != nil {
		return fmt.Errorf("alloc shm: %w", err)
	}
	defer cleanup()

	pool, err := shm.CreatePool(uintptr(shmFd), int32(poolSize))
	if err != nil {
		return fmt.Errorf("create shm pool: %w", err)
	}
	defer pool.Destroy()

	segPattern := filepath.Join(buf.TempDir(), "seg%06d.ts")
	waylandCfg := cfg
	if waylandCfg.Framerate <= 0 || waylandCfg.Framerate > 60 {
		waylandCfg.Framerate = 60
	}
	ffArgs := hwVideoArgs(waylandCfg, segPattern, vendor, width, height, inputPixFmt)
	log.Debug("wayland: ffmpeg command", zap.String("cmd", fmt.Sprintf("%s %s", cfg.FFmpegBin, strings.Join(ffArgs, " "))))
	ffCmd := exec.Command(cfg.FFmpegBin, ffArgs...)
	// monitor stderr for early encoder errors
	stderrPipe, err := ffCmd.StderrPipe()
	if err != nil {
		return fmt.Errorf("ffmpeg stderr pipe: %w", err)
	}
	var stderrBuf bytes.Buffer
	go func() {
		scanner := bufio.NewScanner(stderrPipe)
		for scanner.Scan() {
			line := scanner.Text()
			stderrBuf.WriteString(line + "\n")
			fmt.Fprintln(os.Stderr, line)
			if strings.Contains(line, "A hardware frames reference is required") || strings.Contains(line, "Error while opening encoder") {
				_ = ffCmd.Process.Kill()
			}
		}
	}()
	stdin, err := ffCmd.StdinPipe()
	if err != nil {
		return fmt.Errorf("ffmpeg stdin pipe: %w", err)
	}
	if err := ffCmd.Start(); err != nil {
		return fmt.Errorf("ffmpeg start: %w", err)
	}
	setActiveCmd(ffCmd.Process)
	// short delay to detect immediate encoder init failures (VAAPI)
	time.Sleep(900 * time.Millisecond)
	if err := ffCmd.Process.Signal(syscall.Signal(0)); err != nil {
		out := stderrBuf.String()
		log.Warn("ffmpeg exited immediately, attempting software fallback", zap.String("out", out))
		// fallback to software encoder
		ffArgs = hwVideoArgs(waylandCfg, segPattern, vendorUnknown, width, height, inputPixFmt)
		ffCmd = exec.Command(cfg.FFmpegBin, ffArgs...)
		stderrPipe2, err2 := ffCmd.StderrPipe()
		if err2 != nil {
			return fmt.Errorf("ffmpeg stderr pipe fallback: %w", err2)
		}
		go func() {
			scanner := bufio.NewScanner(stderrPipe2)
			for scanner.Scan() {
				line := scanner.Text()
				fmt.Fprintln(os.Stderr, line)
			}
		}()
		stdin, err = ffCmd.StdinPipe()
		if err != nil {
			return fmt.Errorf("ffmpeg stdin pipe fallback: %w", err)
		}
		if err := ffCmd.Start(); err != nil {
			return fmt.Errorf("ffmpeg start fallback: %w", err)
		}
		setActiveCmd(ffCmd.Process)
	}
	defer func() {
		stdin.Close()
		ffCmd.Wait() //nolint:errcheck
		clearActiveCmd()
	}()

	watchCtx, watchCancel := context.WithCancel(context.Background())
	defer watchCancel()
	go watchSegments(watchCtx, buf, buf.TempDir(), log)

	fps := waylandCfg.Framerate
	frameInterval := time.Second / time.Duration(fps)
	nextFrame := time.Now()

	slot := 0
	for {
		offset := slot * frameSize

		wlBuf, err := pool.CreateBuffer(
			int32(offset), int32(width), int32(height),
			int32(width*4),
			shmFormat,
		)
		if err != nil {
			return fmt.Errorf("create wl_buffer: %w", err)
		}

		frame, err := session.CreateFrame()
		if err != nil {
			wlBuf.Destroy()
			return fmt.Errorf("create frame: %w", err)
		}
		if err := frame.AttachBuffer(wlBuf); err != nil {
			frame.Destroy()
			wlBuf.Destroy()
			return fmt.Errorf("attach frame buffer: %w", err)
		}
		if err := frame.DamageBuffer(0, 0, int32(width), int32(height)); err != nil {
			frame.Destroy()
			wlBuf.Destroy()
			return fmt.Errorf("damage frame buffer: %w", err)
		}
		log.Debug("wayland: capturing frame", zap.Int("offset", offset), zap.Int("frameSize", frameSize))
		if err := frame.Capture(); err != nil {
			frame.Destroy()
			wlBuf.Destroy()
			return fmt.Errorf("capture frame: %w", err)
		}

		done := make(chan error, 1)
		frame.OnReady = func() { done <- nil }
		frame.OnFailed = func(reason uint32) {
			done <- fmt.Errorf("frame capture failed: reason=%d", reason)
		}

		frameErr := dispatchUntilDone(display, done, 3*time.Second)
		frame.Destroy()
		wlBuf.Destroy()

		log.Debug("wayland: frame captured successfully")

		if frameErr != nil {
			return frameErr
		}

		frameSlice := shmData[offset : offset+frameSize]
		log.Debug("wayland: writing frame to ffmpeg", zap.Int("frameSize", frameSize))
		if err := writeAll(stdin, frameSlice); err != nil {
			return fmt.Errorf("write to ffmpeg: %w", err)
		}

		slot = (slot + 1) % 2
		nextFrame = nextFrame.Add(frameInterval)
		if sleep := time.Until(nextFrame); sleep > 0 {
			time.Sleep(sleep)
		} else {
			nextFrame = time.Now()
		}
	}
}

func runWlrScreencopyCapture(display *wl.Display, shm *wl.Shm, output *wl.Output, mgr *WlrScreencopyManagerV1, mgrVersion uint32, cfg Config, vendor gpuVendor, buf *buffer.CircularBuffer, log *zap.Logger) error {
	segPattern := filepath.Join(buf.TempDir(), "seg%06d.ts")
	waylandCfg := cfg
	if waylandCfg.Framerate <= 0 || waylandCfg.Framerate > 60 {
		waylandCfg.Framerate = 60
	}

	var (
		width, height uint32
		stride        uint32
		shmFormat     uint32
		inputPixFmt   string
		frameSize     int
		pool          *wl.ShmPool
		shmFd         int
		shmData       []byte
		cleanupShm    func()
		started       bool
		stdin         io.WriteCloser
		ffCmd         *exec.Cmd
	)

	startFFmpeg := func() error {
		ffArgs := hwVideoArgs(waylandCfg, segPattern, vendor, width, height, inputPixFmt)
		log.Debug("wayland (wlr): ffmpeg command", zap.String("cmd", fmt.Sprintf("%s %s", cfg.FFmpegBin, strings.Join(ffArgs, " "))))
		ffCmd = exec.Command(cfg.FFmpegBin, ffArgs...)
		// monitor stderr for early encoder errors
		stderrPipe, err := ffCmd.StderrPipe()
		if err != nil {
			return fmt.Errorf("ffmpeg stderr pipe: %w", err)
		}
		var stderrBuf bytes.Buffer
		go func() {
			scanner := bufio.NewScanner(stderrPipe)
			for scanner.Scan() {
				line := scanner.Text()
				stderrBuf.WriteString(line + "\n")
				fmt.Fprintln(os.Stderr, line)
				if strings.Contains(line, "A hardware frames reference is required") || strings.Contains(line, "Error while opening encoder") {
					_ = ffCmd.Process.Kill()
				}
			}
		}()
		stdin, err = ffCmd.StdinPipe()
		if err != nil {
			return fmt.Errorf("ffmpeg stdin pipe: %w", err)
		}
		if err := ffCmd.Start(); err != nil {
			return fmt.Errorf("ffmpeg start: %w", err)
		}
		setActiveCmd(ffCmd.Process)
		// give ffmpeg a brief moment to fail fast on encoder init
		time.Sleep(900 * time.Millisecond)
		if err := ffCmd.Process.Signal(syscall.Signal(0)); err != nil {
			out := stderrBuf.String()
			log.Warn("ffmpeg exited immediately, attempting software fallback", zap.String("out", out))
			// attempt software fallback using libx264
			ffArgs = hwVideoArgs(waylandCfg, segPattern, vendorUnknown, width, height, inputPixFmt)
			ffCmd = exec.Command(cfg.FFmpegBin, ffArgs...)
			stderrPipe2, err2 := ffCmd.StderrPipe()
			if err2 != nil {
				return fmt.Errorf("ffmpeg stderr pipe fallback: %w", err2)
			}
			go func() {
				scanner := bufio.NewScanner(stderrPipe2)
				for scanner.Scan() {
					line := scanner.Text()
					fmt.Fprintln(os.Stderr, line)
				}
			}()
			stdin, err = ffCmd.StdinPipe()
			if err != nil {
				return fmt.Errorf("ffmpeg stdin pipe fallback: %w", err)
			}
			if err := ffCmd.Start(); err != nil {
				return fmt.Errorf("ffmpeg start fallback: %w", err)
			}
			setActiveCmd(ffCmd.Process)
		}
		started = true
		return nil
	}

	stopFFmpeg := func() {
		if !started {
			return
		}
		_ = stdin.Close()
		_ = ffCmd.Wait()
		clearActiveCmd()
		started = false
	}

	defer func() {
		stopFFmpeg()
		if pool != nil {
			pool.Destroy()
		}
		if cleanupShm != nil {
			cleanupShm()
		}
	}()

	watchCtx, watchCancel := context.WithCancel(context.Background())
	defer watchCancel()
	go watchSegments(watchCtx, buf, buf.TempDir(), log)

	fps := waylandCfg.Framerate
	frameInterval := time.Second / time.Duration(fps)
	nextFrame := time.Now()
	slot := 0

	// wlBufs holds the two pre-allocated wl_buffer objects (one per slot).
	// They are created once after the first frame metadata is received and
	// reused for every subsequent frame. Destroying and recreating a wl_buffer
	// at the same shm offset each frame causes some compositors (niri, Hyprland)
	// to raise a protocol error (invalid buffer, code 1) and close the connection,
	// because they may hold an internal reference to the buffer object between frames.
	var wlBufs [2]*wl.Buffer

	defer func() {
		for _, b := range wlBufs {
			if b != nil {
				b.Destroy()
			}
		}
	}()

	// Phase 2: capture loop. Initialize shm/ffmpeg on first frame metadata.
	for {
		offset := slot * frameSize

		frame, err := mgr.CaptureOutput(output, 0)
		if err != nil {
			return fmt.Errorf("wlr capture output: %w", err)
		}

		done := make(chan error, 1)
		bufReady := make(chan error, 1)
		var (
			frameFormat uint32
			frameW      uint32
			frameH      uint32
			frameStride uint32
			gotBuffer   bool
		)
		frame.OnBuffer = func(format, w, h, s uint32) {
			frameFormat = format
			frameW = w
			frameH = h
			frameStride = s
			gotBuffer = true
			log.Debug("wlr frame buffer",
				zap.Uint32("format", format),
				zap.Uint32("width", w),
				zap.Uint32("height", h),
				zap.Uint32("stride", s),
			)
			if mgrVersion < 3 {
				select {
				case bufReady <- nil:
				default:
				}
			}
		}
		frame.OnBufferDone = func() {
			if mgrVersion >= 3 {
				select {
				case bufReady <- nil:
				default:
				}
			}
		}
		frame.OnFailed = func() { done <- fmt.Errorf("frame capture failed") }
		if err := dispatchUntilDone(display, bufReady, 3*time.Second); err != nil {
			frame.Destroy()
			return err
		}
		if !gotBuffer {
			frame.Destroy()
			return fmt.Errorf("wlr frame did not provide wl_shm buffer metadata")
		}
		if !started {
			shmFormat = frameFormat
			width = frameW
			height = frameH
			stride = frameStride
			log.Debug("wlr first frame buffer",
				zap.Uint32("mgr_version", mgrVersion),
				zap.Uint32("format", shmFormat),
				zap.Uint32("width", width),
				zap.Uint32("height", height),
				zap.Uint32("stride", stride),
			)
			switch shmFormat {
			case uint32(wl.ShmFormatXrgb8888):
				inputPixFmt = "bgr0"
			case uint32(wl.ShmFormatArgb8888):
				inputPixFmt = "bgra"
			default:
				frame.Destroy()
				return fmt.Errorf("unsupported wl_shm format: %d", shmFormat)
			}
			if width == 0 || height == 0 {
				frame.Destroy()
				return fmt.Errorf("invalid screencopy buffer size: %dx%d", width, height)
			}
			if stride != width*4 {
				frame.Destroy()
				return fmt.Errorf("unsupported screencopy stride: %d (expected %d)", stride, width*4)
			}

			frameSize = int(stride * height)
			poolSize := frameSize * 2
			var allocErr error
			shmFd, shmData, cleanupShm, allocErr = allocShm(poolSize)
			if allocErr != nil {
				frame.Destroy()
				return fmt.Errorf("alloc shm: %w", allocErr)
			}
			pool, allocErr = shm.CreatePool(uintptr(shmFd), int32(poolSize))
			if allocErr != nil {
				frame.Destroy()
				return fmt.Errorf("create shm pool: %w", allocErr)
			}

			// Pre-allocate both slot buffers once. Reusing the same wl_buffer
			// objects across frames avoids the "invalid buffer" protocol error
			// that occurs when compositors retain internal references between frames.
			for i := 0; i < 2; i++ {
				wlBufs[i], allocErr = pool.CreateBuffer(
					int32(i*frameSize), int32(width), int32(height),
					int32(stride), shmFormat,
				)
				if allocErr != nil {
					frame.Destroy()
					return fmt.Errorf("create wl_buffer slot %d: %w", i, allocErr)
				}
			}

			log.Info("wayland screencopy starting", zap.Uint32("width", width), zap.Uint32("height", height))
			if err := startFFmpeg(); err != nil {
				frame.Destroy()
				return err
			}
		}
		if frameFormat != shmFormat || frameW != width || frameH != height || frameStride != stride {
			frame.Destroy()
			return fmt.Errorf(
				"wlr frame metadata changed (got format=%d %dx%d stride=%d, expected format=%d %dx%d stride=%d)",
				frameFormat, frameW, frameH, frameStride, shmFormat, width, height, stride,
			)
		}

		// Reuse the pre-allocated buffer for this slot.
		wlBuf := wlBufs[slot]

		// Per-frame sync channels
		bufferDone := make(chan error, 1)
		frame.OnReady = func() { done <- nil }
		frame.OnFailed = func() { done <- fmt.Errorf("frame capture failed") }
		// override OnBufferDone for this frame so we can wait until compositor finishes with buffer
		frame.OnBufferDone = func() {
			select {
			case bufferDone <- nil:
			default:
			}
		}

		var copyErr error
		if mgrVersion >= 2 {
			copyErr = frame.CopyWithDamage(wlBuf)
		} else {
			copyErr = frame.Copy(wlBuf)
		}
		if copyErr != nil {
			frame.Destroy()
			return fmt.Errorf("copy frame: %w", copyErr)
		}
		frameErr := dispatchUntilDone(display, done, 3*time.Second)
		if frameErr != nil {
			frame.Destroy()
			return frameErr
		}

		// For mgrVersion >=3, compositor will send buffer_done when finished;
		// wait for it before reading the shm data.
		if mgrVersion >= 3 {
			if err := dispatchUntilDone(display, bufferDone, 3*time.Second); err != nil {
				frame.Destroy()
				return fmt.Errorf("wait buffer_done: %w", err)
			}
		}

		// Compositor is done with the buffer; safe to read.
		frameSlice := shmData[offset : offset+frameSize]
		if err := writeAll(stdin, frameSlice); err != nil {
			return fmt.Errorf("write to ffmpeg: %w", err)
		}

		frame.Destroy()
		// wlBuf is NOT destroyed here — it is reused next time this slot is active.

		slot = (slot + 1) % 2
		nextFrame = nextFrame.Add(frameInterval)
		if sleep := time.Until(nextFrame); sleep > 0 {
			time.Sleep(sleep)
		} else {
			nextFrame = time.Now()
		}
	}
}

func dispatchUntilDone(display *wl.Display, done <-chan error, timeout time.Duration) error {
	deadline := time.Now().Add(timeout)
	for {
		select {
		case err := <-done:
			return err
		default:
		}
		if time.Now().After(deadline) {
			return fmt.Errorf("frame dispatch timed out")
		}
		if err := display.Context().Run(); err != nil {
			return fmt.Errorf("wayland dispatch: %w", err)
		}
	}
}

func writeAll(w io.Writer, data []byte) error {
	for len(data) > 0 {
		n, err := w.Write(data)
		data = data[n:]
		if err != nil {
			return err
		}
	}
	return nil
}
