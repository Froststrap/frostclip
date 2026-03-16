package capture

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"frostclip/internal/buffer"
	"frostclip/internal/process"

	"github.com/go-toast/toast"
	"github.com/yusufpapurcu/wmi"
	"go.uber.org/zap"
)

type gpuVendor int

const (
	vendorUnknown gpuVendor = iota
	vendorNVIDIA
	vendorAMD
	vendorIntel
)

var (
	activeCmd   *os.Process
	activeCmdMu sync.Mutex
)

func Kill() {
	activeCmdMu.Lock()
	defer activeCmdMu.Unlock()
	if activeCmd != nil {
		activeCmd.Kill()
		activeCmd = nil
	}
}

func setActiveCmd(p *os.Process) {
	activeCmdMu.Lock()
	defer activeCmdMu.Unlock()
	activeCmd = p
}

func clearActiveCmd() {
	activeCmdMu.Lock()
	defer activeCmdMu.Unlock()
	activeCmd = nil
}

type wmiVideoController struct {
	Name string
}

func detectGPUVendor() gpuVendor {
	var controllers []wmiVideoController
	if err := wmi.Query("SELECT Name FROM Win32_VideoController", &controllers); err != nil {
		return vendorUnknown
	}
	result := vendorUnknown
	for _, c := range controllers {
		name := strings.ToLower(c.Name)
		switch {
		case strings.Contains(name, "nvidia"):
			return vendorNVIDIA
		case strings.Contains(name, "amd"), strings.Contains(name, "advanced micro"), strings.Contains(name, "radeon"):
			return vendorAMD
		case strings.Contains(name, "intel") && result == vendorUnknown:
			result = vendorIntel
		}
	}
	return result
}

func vendorName(v gpuVendor) string {
	switch v {
	case vendorNVIDIA:
		return "NVIDIA"
	case vendorAMD:
		return "AMD"
	case vendorIntel:
		return "Intel"
	default:
		return "Unknown"
	}
}

type Config struct {
	FFmpegBin      string
	MicDevice      string
	SystemLoopback bool // Im changing this to use Stereo Mix because WASAPI doesn't exist on ffmpeg or something
	Framerate      int
	Resolution     string
	Bitrate        string
}

func (c Config) hasAudio() bool {
	return c.MicDevice != "" || c.SystemLoopback
}

func audioInputArgs(cfg Config) (inputArgs []string, audioRefs []string) {
	idx := 0

	if cfg.SystemLoopback {
		inputArgs = append(inputArgs,
			"-f", "wasapi",
			"-loopback", "1",
			"-i", "",
		)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", idx))
		idx++
	}

	if cfg.MicDevice != "" {
		inputArgs = append(inputArgs,
			"-f", "dshow",
			"-rtbufsize", "100M",
			"-i", fmt.Sprintf("audio=%s", cfg.MicDevice),
		)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", idx))
		idx++
	}

	return inputArgs, audioRefs
}

func audioMixFragment(audioRefs []string) (fragment, mapLabel string) {
	switch len(audioRefs) {
	case 0:
		return "", ""
	case 1:
		return "", audioRefs[0]
	default:
		var sb strings.Builder
		for _, r := range audioRefs {
			fmt.Fprintf(&sb, "[%s]", r)
		}
		frag := fmt.Sprintf(";%samerge=inputs=%d,pan=stereo|c0<c0+c2|c1<c1+c3[aout]", sb.String(), len(audioRefs))
		return frag, "[aout]"
	}
}

type encoderPipeline struct {
	encoder   string
	vendor    string
	hw        bool
	buildArgs func(cfg Config, segPattern string) []string
	probeArgs func() []string
}

func hwPipelines() []encoderPipeline {
	return []encoderPipeline{
		{
			encoder: "h264_nvenc",
			vendor:  "NVIDIA",
			hw:      true,
			buildArgs: func(cfg Config, segPattern string) []string {
				fps := cfg.Framerate
				if fps == 0 {
					fps = 60
				}
				fc := fmt.Sprintf("ddagrab=framerate=%d,hwdownload,format=bgra", fps)
				if cfg.Resolution != "" {
					parts := strings.SplitN(cfg.Resolution, "x", 2)
					if len(parts) == 2 {
						fc += fmt.Sprintf(",scale=w=%s:h=%s:flags=fast_bilinear", parts[0], parts[1])
					}
				}
				fc += ",format=yuv420p[vout]"

				args := []string{"-y"}
				inputArgs, audioRefs := audioInputArgs(cfg)
				args = append(args, inputArgs...)
				br := bitrate(cfg)
				audioFrag, audioLabel := audioMixFragment(audioRefs)
				args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
				if audioLabel != "" {
					args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
				}
				args = append(args, "-vcodec", "h264_nvenc", "-preset", "p4", "-cq", "23", "-b:v", "0", "-maxrate", br, "-bufsize", br)
				return append(args, segmentArgs(segPattern)...)
			},
			probeArgs: func() []string {
				return []string{
					"-filter_complex", "ddagrab=framerate=1,hwdownload,format=bgra,format=yuv420p[vout]",
					"-map", "[vout]",
					"-vcodec", "h264_nvenc", "-preset", "p1", "-cq", "28",
					"-frames:v", "1", "-f", "null", "-",
				}
			},
		},
		{
			encoder: "h264_qsv",
			vendor:  "Intel",
			hw:      true,
			buildArgs: func(cfg Config, segPattern string) []string {
				fps := cfg.Framerate
				if fps == 0 {
					fps = 60
				}
				fc := fmt.Sprintf("ddagrab=framerate=%d,hwmap=derive_device=qsv:extra_hw_frames=10,format=qsv", fps)
				if cfg.Resolution != "" {
					parts := strings.SplitN(cfg.Resolution, "x", 2)
					if len(parts) == 2 {
						fc += fmt.Sprintf(",scale_qsv=w=%s:h=%s:format=nv12", parts[0], parts[1])
					} else {
						fc += ",scale_qsv=format=nv12"
					}
				} else {
					fc += ",scale_qsv=format=nv12"
				}
				fc += "[vout]"

				args := []string{
					"-y",
					"-init_hw_device", "d3d11va=d3d",
					"-init_hw_device", "qsv=qsv@d3d",
					"-filter_hw_device", "d3d",
				}
				inputArgs, audioRefs := audioInputArgs(cfg)
				args = append(args, inputArgs...)
				br := bitrate(cfg)
				audioFrag, audioLabel := audioMixFragment(audioRefs)
				args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
				if audioLabel != "" {
					args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
				}
				args = append(args, "-vcodec", "h264_qsv", "-global_quality", "23", "-look_ahead", "0", "-maxrate", br, "-bufsize", br)
				return append(args, segmentArgs(segPattern)...)
			},
			probeArgs: func() []string {
				return []string{
					"-init_hw_device", "d3d11va=d3d",
					"-init_hw_device", "qsv=qsv@d3d",
					"-filter_hw_device", "d3d",
					"-filter_complex", "ddagrab=framerate=1,hwmap=derive_device=qsv,format=qsv[vout]",
					"-map", "[vout]",
					"-vcodec", "h264_qsv", "-global_quality", "26", "-look_ahead", "0",
					"-frames:v", "1", "-f", "null", "-",
				}
			},
		},
		{
			encoder: "h264_amf",
			vendor:  "AMD",
			hw:      true,
			buildArgs: func(cfg Config, segPattern string) []string {
				fps := cfg.Framerate
				if fps == 0 {
					fps = 60
				}
				fc := fmt.Sprintf("ddagrab=framerate=%d,hwdownload,format=bgra", fps)
				if cfg.Resolution != "" {
					parts := strings.SplitN(cfg.Resolution, "x", 2)
					if len(parts) == 2 {
						fc += fmt.Sprintf(",scale=w=%s:h=%s:flags=fast_bilinear", parts[0], parts[1])
					}
				}
				fc += ",format=yuv420p[vout]"
				args := []string{"-y"}
				inputArgs, audioRefs := audioInputArgs(cfg)
				args = append(args, inputArgs...)
				br := bitrate(cfg)
				audioFrag, audioLabel := audioMixFragment(audioRefs)
				args = append(args, "-filter_complex", fc+audioFrag, "-map", "[vout]")
				if audioLabel != "" {
					args = append(args, "-map", audioLabel, "-acodec", "aac", "-b:a", "128k")
				}
				args = append(args, "-vcodec", "h264_amf", "-quality", "balanced", "-rc", "cbr", "-b:v", br, "-maxrate", br, "-bufsize", br)
				return append(args, segmentArgs(segPattern)...)
			},
			probeArgs: func() []string {
				return []string{
					"-filter_complex", "ddagrab=framerate=1,hwdownload,format=bgra,format=yuv420p[vout]",
					"-map", "[vout]",
					"-vcodec", "h264_amf", "-quality", "speed", "-rc", "cbr", "-b:v", "4000k",
					"-frames:v", "1", "-f", "null", "-",
				}
			},
		},
	}
}

var currentVendor gpuVendor

func bitrate(cfg Config) string {
	if cfg.Bitrate != "" {
		return cfg.Bitrate
	}
	return "6000k"
}

func segmentArgs(segPattern string) []string {
	return []string{
		"-f", "segment",
		"-segment_time", "1",
		"-segment_format", "mpegts",
		"-reset_timestamps", "1",
		segPattern,
	}
}

type encoderSet struct {
	hw []encoderPipeline
}

func Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	log = log.Named("capture")
	log.Info("starting capture loop")

	set := probeEncoders(cfg.FFmpegBin, log)

	if !cfg.hasAudio() {
		log.Info("no audio — recording video only")
	} else {
		if cfg.SystemLoopback && cfg.MicDevice != "" {
			log.Info("audio: system loopback + microphone", zap.String("mic", cfg.MicDevice))
		} else if cfg.SystemLoopback {
			log.Info("audio: system loopback (WASAPI)")
		} else {
			log.Info("audio: microphone", zap.String("device", cfg.MicDevice))
		}
	}

	if len(set.hw) == 0 {
		log.Error("no hardware encoders available — cannot record")
		sendNoHWNotification()
		return
	}

	log.Info("hardware encoding active",
		zap.Strings("hw_encoders", encoderNames(set.hw)),
	)

	hwIdx := 0

	consecutiveCrashes := 0
	const maxFastCrashes = 3

	for {
		p := set.hw[hwIdx]

		log.Info("starting capture",
			zap.String("encoder", p.encoder),
			zap.String("vendor", p.vendor),
			zap.Bool("hw", p.hw),
		)

		if err := buf.ResetTempDir(); err != nil {
			log.Error("failed to reset temp dir", zap.Error(err))
			time.Sleep(1 * time.Second)
			continue
		}

		hadSegs, err := runCaptureProcess(buf, p, cfg, log)
		if err == nil {
			return
		}

		if hadSegs {
			consecutiveCrashes = 0
		} else {
			consecutiveCrashes++
		}

		log.Error("capture process crashed",
			zap.String("encoder", p.encoder),
			zap.Error(err),
			zap.Int("consecutive_crashes", consecutiveCrashes),
		)

		if consecutiveCrashes >= maxFastCrashes {
			backoff := min(time.Duration(consecutiveCrashes)*5*time.Second, 60*time.Second)
			log.Warn("encoder crashing repeatedly — backing off",
				zap.String("encoder", p.encoder),
				zap.Duration("backoff", backoff),
			)
			time.Sleep(backoff)
		}

		log.Info("re-probing hardware encoders after crash...")
		newSet := probeEncoders(cfg.FFmpegBin, log)
		if len(newSet.hw) > 0 {
			set = newSet
			hwIdx = 0
			consecutiveCrashes = 0
			sendHWCrashNotification(p.encoder, true)
		} else if hwIdx < len(set.hw)-1 {
			hwIdx++
			consecutiveCrashes = 0
			sendHWCrashNotification(p.encoder, false)
		} else {
			sendHWFailedNotification(p.encoder)
			log.Error("all hardware encoders failed — waiting 5s before re-probing")
			time.Sleep(5 * time.Second)
			newSet2 := probeEncoders(cfg.FFmpegBin, log)
			if len(newSet2.hw) > 0 {
				set = newSet2
				hwIdx = 0
				consecutiveCrashes = 0
			}
		}
	}
}

func encoderNames(chain []encoderPipeline) []string {
	names := make([]string, len(chain))
	for i, p := range chain {
		names[i] = p.encoder
	}
	return names
}

func runCaptureProcess(buf *buffer.CircularBuffer, p encoderPipeline, cfg Config, log *zap.Logger) (hadSegments bool, err error) {
	tmpDir := buf.TempDir()
	segPattern := filepath.Join(tmpDir, "seg_%06d.ts")
	args := p.buildArgs(cfg, segPattern)

	res := cfg.Resolution
	if res == "" {
		res = "native"
	}
	log.Info("FFmpeg starting",
		zap.String("encoder", p.encoder),
		zap.Int("fps", cfg.Framerate),
		zap.String("resolution", res),
		zap.String("output", segPattern),
	)

	cmd := process.Command(cfg.FFmpegBin, args...)
	var errBuf bytes.Buffer
	cmd.Stderr = &errBuf

	if err := cmd.Start(); err != nil {
		return false, fmt.Errorf("failed to start FFmpeg: %w", err)
	}
	setActiveCmd(cmd.Process)

	done := make(chan struct{})
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		watchSegments(buf, tmpDir, done, log)
	}()

	err = cmd.Wait()
	clearActiveCmd()
	close(done)
	wg.Wait()

	hadSegments = len(buf.GetAll()) > 0

	if err != nil {
		stderr := lastMeaningfulLine(errBuf.String())
		log.Error("FFmpeg exited unexpectedly",
			zap.String("encoder", p.encoder),
			zap.String("reason", stderr),
			zap.Error(err),
		)
	} else {
		log.Info("FFmpeg exited cleanly", zap.String("encoder", p.encoder))
	}
	return hadSegments, err
}

func watchSegments(buf *buffer.CircularBuffer, tmpDir string, done <-chan struct{}, log *zap.Logger) {
	seen := map[string]bool{}
	var lastSeg string
	notified := false

	for {
		select {
		case <-done:
			if lastSeg != "" && !seen[lastSeg] {
				seen[lastSeg] = true
				buf.Push(lastSeg)
			}
			return
		default:
		}

		entries, err := os.ReadDir(tmpDir)
		if err != nil {
			time.Sleep(100 * time.Millisecond)
			continue
		}

		var segs []string
		for _, e := range entries {
			if !e.IsDir() && strings.HasSuffix(e.Name(), ".ts") {
				segs = append(segs, filepath.Join(tmpDir, e.Name()))
			}
		}
		sort.Strings(segs)

		for i, seg := range segs {
			if seen[seg] {
				continue
			}
			if i == len(segs)-1 && seg != lastSeg {
				lastSeg = seg
				continue
			}
			seen[seg] = true
			buf.Push(seg)
			if !notified {
				notified = true
				log.Info("capture pipeline confirmed — segments are recording")
				sendCaptureNotification()
			}
		}
		time.Sleep(200 * time.Millisecond)
	}
}

func probeEncoders(ffmpegBin string, log *zap.Logger) encoderSet {
	cmd := process.Command(ffmpegBin, "-encoders")
	out, err := cmd.Output()
	if err != nil {
		log.Warn("could not query FFmpeg encoders")
		return encoderSet{}
	}

	vendor := detectGPUVendor()
	currentVendor = vendor
	log.Info("detected GPU vendor", zap.String("vendor", vendorName(vendor)))

	orderedHW := orderHWByVendor(hwPipelines(), vendor)

	type result struct {
		pipeline encoderPipeline
		ok       bool
		priority int
	}

	results := make([]result, len(orderedHW))
	var wg sync.WaitGroup

	for i, p := range orderedHW {
		if !bytes.Contains(out, []byte(p.encoder)) {
			continue
		}

		if !vendorMatches(p.encoder, vendor) {
			continue
		}
		wg.Add(1)
		i, p := i, p
		go func() {
			defer wg.Done()
			ok := probeEncoder(ffmpegBin, p, log)
			results[i] = result{pipeline: p, ok: ok, priority: i}
		}()
	}
	wg.Wait()

	sort.Slice(results, func(i, j int) bool {
		return results[i].priority < results[j].priority
	})

	var hwChain []encoderPipeline
	for _, r := range results {
		if r.ok {
			hwChain = append(hwChain, r.pipeline)
		}
	}

	log.Info("encoder probe complete",
		zap.Strings("hw", encoderNames(hwChain)),
	)
	return encoderSet{hw: hwChain}
}

func vendorMatches(encoder string, vendor gpuVendor) bool {
	switch encoder {
	case "h264_nvenc":
		return vendor == vendorNVIDIA || vendor == vendorUnknown
	case "h264_amf":
		return vendor == vendorAMD || vendor == vendorUnknown
	case "h264_qsv":
		return vendor == vendorIntel || vendor == vendorUnknown
	}
	return true
}

func orderHWByVendor(all []encoderPipeline, vendor gpuVendor) []encoderPipeline {
	preferred := map[gpuVendor]string{
		vendorNVIDIA: "h264_nvenc",
		vendorAMD:    "h264_amf",
		vendorIntel:  "h264_qsv",
	}
	want, ok := preferred[vendor]
	if !ok {
		return all
	}
	var out, rest []encoderPipeline
	for _, p := range all {
		if p.encoder == want {
			out = append(out, p)
		} else {
			rest = append(rest, p)
		}
	}
	return append(out, rest...)
}

func probeEncoder(ffmpegBin string, p encoderPipeline, log *zap.Logger) bool {
	args := p.probeArgs()
	log.Info("probing encoder", zap.String("encoder", p.encoder), zap.String("vendor", p.vendor))

	cmd := process.Command(ffmpegBin, args...)
	var errBuf bytes.Buffer
	cmd.Stderr = &errBuf

	if err := cmd.Run(); err != nil {
		reason := lastMeaningfulLine(errBuf.String())
		log.Warn("encoder probe failed",
			zap.String("encoder", p.encoder),
			zap.String("vendor", p.vendor),
			zap.String("reason", reason),
		)
		return false
	}
	log.Info("encoder probe passed", zap.String("encoder", p.encoder))
	return true
}

func lastMeaningfulLine(s string) string {
	lines := strings.Split(strings.ReplaceAll(s, "\r\n", "\n"), "\n")
	last := ""
	meaningful := ""
	for _, l := range lines {
		l = strings.TrimSpace(l)
		if l == "" {
			continue
		}
		last = l
		lo := strings.ToLower(l)
		if strings.HasPrefix(l, "[") || strings.Contains(lo, "error") || strings.Contains(lo, "failed") || strings.Contains(lo, "invalid") || strings.Contains(lo, "conversion") {
			meaningful = l
		}
	}
	if meaningful != "" {
		return meaningful
	}
	return last
}

func sendCaptureNotification() {
	n := toast.Notification{
		AppID:   "FrostClip",
		Title:   "FrostClip is Recording",
		Message: "Capture started — press F6/F7/F8/F9 to save a clip.",
	}
	n.Push()
}

func sendHWCrashNotification(encoder string, recovered bool) {
	msg := fmt.Sprintf("%s stopped unexpectedly.", encoder)
	if recovered {
		msg += " Hardware encoder recovered and restarted."
	} else {
		msg += " Trying next available hardware encoder."
	}
	n := toast.Notification{
		AppID:   "FrostClip",
		Title:   "⚠️ Hardware Encoder Crashed",
		Message: msg,
	}
	n.Push()
}

func sendHWFailedNotification(encoder string) {
	n := toast.Notification{
		AppID:   "FrostClip",
		Title:   "❌ Hardware Encoding Failed",
		Message: fmt.Sprintf("%s failed and no other hardware encoder is available. FrostClip is paused — will retry shortly.", encoder),
	}
	n.Push()
}

func sendNoHWNotification() {
	n := toast.Notification{
		AppID:   "FrostClip",
		Title:   "⚠️ No Hardware Encoder Found",
		Message: "No GPU encoder is available. Check your GPU drivers and try restarting FrostClip.",
	}
	n.Push()
}
