//go:build linux

package setup

import (
	"os/exec"
	"strings"

	"go.uber.org/zap"
)

func findFFmpeg() (string, error) {
	return exec.LookPath("ffmpeg")
}

// installFFmpeg on Linux we can't auto-install; just return a friendly error.
func installFFmpeg(log *zap.Logger) (string, error) {
	hint := installHint()
	log.Error("FFmpeg not found", zap.String("install", hint))
	return "", nil // non-fatal: log shows the hint, user installs manually
}

func installHint() string {
	// Detect package manager and give the right command
	for _, pm := range []struct{ bin, cmd string }{
		{"apt-get", "sudo apt-get install -y ffmpeg"},
		{"dnf", "sudo dnf install -y ffmpeg"},
		{"pacman", "sudo pacman -S ffmpeg"},
		{"zypper", "sudo zypper install ffmpeg"},
	} {
		if _, err := exec.LookPath(pm.bin); err == nil {
			return pm.cmd
		}
	}
	return "install ffmpeg via your package manager"
}

// detectMicDevice finds a PulseAudio/PipeWire source for microphone input.
func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices (PulseAudio/PipeWire)")

	// pactl list sources short  →  index name driver state
	out, err := runCommand("pactl", "list", "sources", "short")
	if err != nil {
		log.Warn("pactl not available — mic disabled", zap.Error(err))
		return ""
	}

	var devices []string
	for _, line := range strings.Split(out, "\n") {
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		name := fields[1]
		// Skip monitor sources (loopback — those are for SystemLoopback)
		if strings.Contains(name, ".monitor") {
			continue
		}
		devices = append(devices, name)
	}

	if len(devices) == 0 {
		log.Warn("no microphone sources found")
		return ""
	}

	log.Info("found microphone sources",
		zap.Int("count", len(devices)),
		zap.Strings("sources", devices),
	)
	log.Info("selected microphone", zap.String("device", devices[0]))
	return devices[0]
}
