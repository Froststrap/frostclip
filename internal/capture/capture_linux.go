//go:build linux

package capture

import (
	"context"
	"fmt"
	"frostclip/internal/buffer"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
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
	for {
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
	for {
		err := runWaylandCapture(buf, cfg, vendor, log)
		if err != nil {
			log.Warn("wayland capture error — retrying in 1s", zap.Error(err))
			time.Sleep(time.Second)
		}
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
	case "wl_shm", "ext_image_copy_capture_manager_v1", "ext_output_image_capture_source_manager_v1", "wl_output":
		c.mu.Lock()
		c.pending = append(c.pending, pendingGlobal{e.Name, e.Interface, e.Version})
		c.mu.Unlock()
	}
}

type shmFormatCollector struct {
	mu      sync.Mutex
	formats map[uint32]bool
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
		zap.Bool("has_output", output != nil),
	)

	if shm == nil {
		return fmt.Errorf("no wl_shm available")
	}
	if output == nil {
		return fmt.Errorf("no wl_output available")
	}
	if mgr == nil {
		return fmt.Errorf("compositor does not support ext_image_copy_capture_manager_v1")
	}
	if sourceMgr == nil {
		return fmt.Errorf("compositor does not support ext_output_image_capture_source_manager_v1")
	}

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
	if waylandCfg.Framerate <= 0 || waylandCfg.Framerate > 10 {
		waylandCfg.Framerate = 10
	}
	ffArgs := hwVideoArgs(waylandCfg, segPattern, vendor, width, height, inputPixFmt)
	log.Debug("wayland: ffmpeg command", zap.String("cmd", fmt.Sprintf("%s %s", cfg.FFmpegBin, strings.Join(ffArgs, " "))))
	ffCmd := exec.Command(cfg.FFmpegBin, ffArgs...)
	ffCmd.Stderr = os.Stderr
	stdin, err := ffCmd.StdinPipe()
	if err != nil {
		return fmt.Errorf("ffmpeg stdin pipe: %w", err)
	}
	if err := ffCmd.Start(); err != nil {
		return fmt.Errorf("ffmpeg start: %w", err)
	}
	setActiveCmd(ffCmd.Process)
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
		time.Sleep(frameInterval)
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
