// Package capture manages the recording loop.
package capture

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"time"

	"frostclip/internal/buffer"

	"go.uber.org/zap"
)

type Config struct {
	FFmpegBin      string
	MicDevice      string
	SystemLoopback bool
	Framerate      int
	Resolution     string
	Bitrate        string
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
	ref := 1 // video occupies input 0
	if cfg.SystemLoopback {
		inputArgs = append(inputArgs, loopbackInputArgs()...)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", ref))
		ref++
	}
	if cfg.MicDevice != "" {
		inputArgs = append(inputArgs, micInputArgs(cfg.MicDevice)...)
		audioRefs = append(audioRefs, fmt.Sprintf("%d:a", ref))
	}
	return
}

func audioMixFragment(audioRefs []string) (fragment, label string) {
	switch len(audioRefs) {
	case 0:
		return "", ""
	case 1:
		return fmt.Sprintf(";[%s]anull[aout]", audioRefs[0]), "[aout]"
	default:
		var sb strings.Builder
		for _, r := range audioRefs {
			fmt.Fprintf(&sb, "[%s]", r)
		}
		fmt.Fprintf(&sb, "amix=inputs=%d:duration=first[aout]", len(audioRefs))
		return ";" + sb.String(), "[aout]"
	}
}
func Loop(buf *buffer.CircularBuffer, cfg Config, log *zap.Logger) {
	log.Info("capture loop starting")
	seq := 0
	for {
		segPath := filepath.Join(buf.TempDir(), fmt.Sprintf("seg%06d.ts", seq))
		seq++

		cmd := BuildCaptureCommand(cfg, segPath)
		if err := cmd.Start(); err != nil {
			log.Warn("capture: start failed", zap.Error(err))
			time.Sleep(time.Second)
			continue
		}
		setActiveCmd(cmd.Process)

		err := cmd.Wait()
		clearActiveCmd()

		if err != nil && !strings.Contains(err.Error(), "124") {
			log.Debug("recorder exited", zap.Error(err))
		}

		for i := 0; i < 5; i++ {
			if info, err := os.Stat(segPath); err == nil && info.Size() > 0 {
				buf.Push(segPath)
				break
			}
			time.Sleep(60 * time.Millisecond)
		}
	}
}
