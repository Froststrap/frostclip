//go:build windows

package setup

import (
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"strings"

	"frostclip/internal/notify"
	"frostclip/internal/process"

	"go.uber.org/zap"
)

func findFFmpeg() (string, error) {
	return exec.LookPath("ffmpeg")
}

func installFFmpeg(log *zap.Logger) (string, error) {
	log.Info("installing FFmpeg via winget...")
	var errBuf strings.Builder
	cmd := process.Command("winget", "install",
		"FFmpeg (Essentials Build)",
		"--accept-package-agreements",
		"--accept-source-agreements",
	)
	cmd.Stdout = os.Stdout
	cmd.Stderr = io.MultiWriter(os.Stderr, &errBuf)
	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf("winget install failed: %w\n%s", err, errBuf.String())
	}
	out, err := process.Command("where.exe", "ffmpeg").Output()
	if err == nil {
		path := strings.TrimSpace(strings.Split(string(out), "\n")[0])
		if path != "" {
			log.Info("FFmpeg ready", zap.String("path", path))
			return path, nil
		}
	}
	return "", fmt.Errorf("ffmpeg installed but not found — restart FrostClip")
}

func installHint() string {
	return `run: winget install "FFmpeg (Essentials Build)"`
}

func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices (dshow)")
	out, _ := runCommand(ffmpegBin, "-list_devices", "true", "-f", "dshow", "-i", "dummy")
	devices := parseDshowAudioDevices(out)
	if len(devices) == 0 {
		log.Warn("no audio devices found")
		_ = notify.Send // reference to avoid import error
		return ""
	}
	log.Info("found audio devices", zap.Int("count", len(devices)), zap.Strings("devices", devices))
	priorities := []string{"stereo mix", "virtual", "cable", "vb-audio", "what u hear", "wave out", "loopback"}
	for _, p := range priorities {
		for _, d := range devices {
			if strings.Contains(strings.ToLower(d), p) {
				log.Info("selected audio device", zap.String("device", d))
				return d
			}
		}
	}
	log.Info("selected audio device (fallback)", zap.String("device", devices[0]))
	return devices[0]
}

func parseDshowAudioDevices(output string) []string {
	var devices []string
	re := regexp.MustCompile(`"([^"]+)"\s+\(audio\)`)
	for _, line := range strings.Split(output, "\n") {
		if m := re.FindStringSubmatch(line); len(m) > 1 {
			if name := strings.TrimSpace(m[1]); name != "" {
				devices = append(devices, name)
			}
		}
	}
	return devices
}
