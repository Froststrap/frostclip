package capture

import (
	"fmt"
	"os"
	"strings"
	"sync"

	"frostclip/internal/settings"
)

type Config struct {
	FFmpegBin      string
	MicDevice      string
	SystemLoopback bool
	SystemDevice   string
	FallbackAudio  string
	Framerate      int
	Resolution     string
	Bitrate        string
	AudioMode      settings.AudioMode
	UpdateCh       <-chan settings.SettingsUpdate
}

func (c Config) hasAudio() bool {
	return c.MicDevice != "" || c.SystemLoopback
}

var (
	activeCmd   *os.Process
	activeCmdMu sync.Mutex
)

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

func Kill() {
	activeCmdMu.Lock()
	defer activeCmdMu.Unlock()
	if activeCmd != nil {
		_ = activeCmd.Kill()
		activeCmd = nil
	}
}

type encoderPipeline struct {
	encoder   string
	vendor    string
	hw        bool
	buildArgs func(cfg Config, output string) []string
	probeArgs func() []string
}

const SegmentDuration = 1

const segmentSeconds = SegmentDuration

func bitrate(cfg Config) string {
	if cfg.Bitrate != "" {
		return cfg.Bitrate
	}
	return "8000k"
}

func audioArgs(cfg Config) (inputArgs []string, audioRefs []string) {
	ref := 1
	if cfg.SystemLoopback {
		if args := loopbackInputArgs(cfg.SystemDevice); len(args) > 0 {
			inputArgs = append(inputArgs, args...)
			audioRefs = append(audioRefs, fmt.Sprintf("%d:a", ref))
			ref++
		}
	}
	if cfg.MicDevice != "" {
		inputArgs = append(inputArgs, micInputArgs(cfg.MicDevice)...)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", ref))
	}
	if len(audioRefs) == 0 && cfg.FallbackAudio != "" {
		inputArgs = append(inputArgs, "-stream_loop", "-1", "-i", cfg.FallbackAudio)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", ref))
	}
	return
}

func audioMixFragment(audioRefs []string) (fragment, label string) {
	switch len(audioRefs) {
	case 0:
		return "", ""
	case 1:
		// aresample=async=1000 allows up to 1000ms of clock drift correction
		// apad pads end of audio to match segment boundaries (avoid truncation in segment muxer)
		return fmt.Sprintf(";[%s]aresample=async=1000,apad=whole_len=48000[aout]", audioRefs[0]), "[aout]"
	default:
		var sb strings.Builder
		for _, r := range audioRefs {
			fmt.Fprintf(&sb, "[%s]", r)
		}
		fmt.Fprintf(&sb, "amix=inputs=%d:duration=first[amixed]", len(audioRefs))
		// After mix, apply async resampling + padding for segment alignment
		fmt.Fprintf(&sb, ";[amixed]aresample=async=1000,apad=whole_len=48000[aout]")
		return ";" + sb.String(), "[aout]"
	}
}

func segmentArgs(segPattern string) []string {
	return []string{
		"-f", "segment",
		"-segment_time", fmt.Sprintf("%d", segmentSeconds),
		"-segment_format", "mpegts",
		"-reset_timestamps", "1",
		segPattern,
	}
}
