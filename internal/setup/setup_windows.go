//go:build windows

package setup

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

	"frostclip/internal/notify"

	"go.uber.org/zap"
)

func findFFmpeg() (string, error) {
	return exec.LookPath("ffmpeg")
}

func installFFmpeg(log *zap.Logger) (string, error) {
	baseDir, err := appDir()
	if err != nil {
		return "", fmt.Errorf("resolve app dir: %w", err)
	}
	installDir := filepath.Join(baseDir, "ffmpeg")
	if err := ensureDir(installDir); err != nil {
		return "", fmt.Errorf("create ffmpeg dir: %w", err)
	}

	log.Info("installing FFmpeg from gyan.dev", zap.String("dir", installDir))
	urls := []string{
		"https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
		"https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-full.zip",
	}
	var lastErr error
	for _, u := range urls {
		data, err := downloadBytes(u)
		if err != nil {
			lastErr = err
			continue
		}
		ffmpegData, _, err := unzipFind(data, func(name string) bool {
			return strings.HasSuffix(name, "/bin/ffmpeg.exe")
		})
		if err != nil {
			lastErr = err
			continue
		}
		if err := writeExecutable(filepath.Join(installDir, "ffmpeg.exe"), ffmpegData); err != nil {
			return "", err
		}
		if ffprobeData, _, err := unzipFind(data, func(name string) bool {
			return strings.HasSuffix(name, "/bin/ffprobe.exe")
		}); err == nil {
			_ = writeExecutable(filepath.Join(installDir, "ffprobe.exe"), ffprobeData)
		}
		if ffplayData, _, err := unzipFind(data, func(name string) bool {
			return strings.HasSuffix(name, "/bin/ffplay.exe")
		}); err == nil {
			_ = writeExecutable(filepath.Join(installDir, "ffplay.exe"), ffplayData)
		}
		path := filepath.Join(installDir, "ffmpeg.exe")
		log.Info("FFmpeg ready", zap.String("path", path))
		return path, nil
	}
	return "", fmt.Errorf("could not install ffmpeg from gyan.dev: %w", lastErr)
}

func installHint() string {
	return `download gyan.dev FFmpeg build and place ffmpeg.exe in ./ffmpeg`
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

func detectLoopbackDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("using default system output (WASAPI)")
	return "default"
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
