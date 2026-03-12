package setup

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"

	"github.com/go-toast/toast"
	"go.uber.org/zap"
	"frostclip/internal/process"
	"frostclip/internal/settings"
)

const minFFmpegMajor = 8

func EnsureFFmpeg(log *zap.Logger) (string, error) {
	if path, err := exec.LookPath("ffmpeg"); err == nil {
		log.Info("FFmpeg found on PATH", zap.String("path", path))
		if err := checkFFmpegVersion(path, log); err != nil {
			return "", err
		}
		return path, nil
	}

	log.Info("FFmpeg not found, installing via winget...")

	var errBuf bytes.Buffer
	cmd := process.Command("winget", "install",
		"FFmpeg (Essentials Build)",
		"--accept-package-agreements",
		"--accept-source-agreements",
	)
	cmd.Stdout = os.Stdout
	cmd.Stderr = io.MultiWriter(os.Stderr, &errBuf)

	if err := cmd.Run(); err != nil {
		return "", fmt.Errorf(
			"winget install failed: %w\n%s\nMake sure winget is available (Windows 10 1709+ / App Installer from Microsoft Store)",
			err, strings.TrimSpace(errBuf.String()),
		)
	}

	out, err := process.Command("where.exe", "ffmpeg").Output()
	if err == nil {
		path := strings.TrimSpace(strings.Split(string(out), "\n")[0])
		if path != "" {
			if err := checkFFmpegVersion(path, log); err != nil {
				return "", err
			}
			log.Info("FFmpeg ready", zap.String("path", path))
			return path, nil
		}
	}

	return "", fmt.Errorf("ffmpeg installed but could not be located — try restarting FrostClip")
}

func checkFFmpegVersion(ffmpegBin string, log *zap.Logger) error {
	out, err := process.Command(ffmpegBin, "-version").Output()
	if err != nil {
		log.Warn("could not read FFmpeg version", zap.Error(err))
		return nil
	}

	version, major, minor := parseFFmpegVersion(string(out))
	if version == "" {
		log.Warn("could not parse FFmpeg version string")
		return nil
	}

	log.Info("FFmpeg version detected", zap.String("version", version), zap.Int("major", major), zap.Int("minor", minor))

	if major < minFFmpegMajor {
		msg := fmt.Sprintf(
			"FFmpeg %d.x or newer is required, but found %s.\n\nRun: winget install \"FFmpeg (Essentials Build)\"",
			minFFmpegMajor, version,
		)
		log.Error("FFmpeg version too old",
			zap.String("found", version),
			zap.Int("required_major", minFFmpegMajor),
		)
		sendVersionErrorNotification(version)
		return fmt.Errorf(msg)
	}

	return nil
}

func parseFFmpegVersion(output string) (version string, major, minor int) {
	re := regexp.MustCompile(`ffmpeg version (\d+)\.(\d+)`)
	lines := strings.Split(output, "\n")
	for _, line := range lines {
		m := re.FindStringSubmatch(line)
		if len(m) >= 3 {
			major, _ = strconv.Atoi(m[1])
			minor, _ = strconv.Atoi(m[2])
			versionRe := regexp.MustCompile(`ffmpeg version ([\d.]+)`)
			vm := versionRe.FindStringSubmatch(line)
			if len(vm) >= 2 {
				version = vm[1]
			} else {
				version = fmt.Sprintf("%d.%d", major, minor)
			}
			return
		}
	}
	return "", 0, 0
}

func sendVersionErrorNotification(found string) {
	n := toast.Notification{
		AppID: "FrostClip",
		Title: "❌ FFmpeg Too Old",
		Message: fmt.Sprintf(
			"Found FFmpeg %s but version %d.0+ is required.\nRun: winget install \"FFmpeg (Essentials Build)\"",
			found, minFFmpegMajor,
		),
	}
	n.Push()
}

type AudioConfig struct {
	MicDevice string
	SystemLoopback bool
}

func ResolveAudio(ffmpegBin string, mode settings.AudioMode, log *zap.Logger) AudioConfig {
	log.Info("resolving audio", zap.String("mode", string(mode)))

	switch mode {
	case settings.AudioOff:
		log.Info("audio disabled by settings")
		return AudioConfig{}

	case settings.AudioSystem:
		log.Info("system audio only (WASAPI loopback)")
		return AudioConfig{SystemLoopback: true}

	case settings.AudioBoth:
		mic := detectMicDevice(ffmpegBin, log)
		if mic == "" {
			log.Warn("no mic found for 'both' mode — falling back to system only")
			return AudioConfig{SystemLoopback: true}
		}
		log.Info("system audio + microphone", zap.String("mic", mic))
		return AudioConfig{MicDevice: mic, SystemLoopback: true}

	default: // AudioMicrophone
		mic := detectMicDevice(ffmpegBin, log)
		return AudioConfig{MicDevice: mic}
	}
}

func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices")

	out, _ := runCommand(ffmpegBin,
		"-list_devices", "true",
		"-f", "dshow",
		"-i", "dummy",
	)

	devices := parseAudioDevices(out)
	if len(devices) == 0 {
		log.Warn("no audio devices found — audio disabled")
		return ""
	}

	log.Info("found audio devices", zap.Int("count", len(devices)), zap.Strings("devices", devices))

	priorities := []string{
		"stereo mix",
		"virtual",
		"cable",
		"vb-audio",
		"what u hear",
		"wave out",
		"loopback",
	}

	for _, priority := range priorities {
		for _, device := range devices {
			if strings.Contains(strings.ToLower(device), priority) {
				log.Info("selected audio device", zap.String("device", device))
				return device
			}
		}
	}

	log.Info("selected audio device (fallback)", zap.String("device", devices[0]))
	return devices[0]
}

func parseAudioDevices(output string) []string {
	var devices []string
	re := regexp.MustCompile(`"([^"]+)"\s+\(audio\)`)

	for _, line := range strings.Split(output, "\n") {
		if matches := re.FindStringSubmatch(line); len(matches) > 1 {
			name := strings.TrimSpace(matches[1])
			if name != "" {
				devices = append(devices, name)
			}
		}
	}
	return devices
}
