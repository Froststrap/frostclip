// Package setup handles FFmpeg discovery and audio device resolution.
package setup

import (
	"bytes"
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"frostclip/internal/process"
	"frostclip/internal/settings"

	"go.uber.org/zap"
)

const minFFmpegMajor = 6 // Lowered from 8: v6+ has x11grab + VAAPI; v7+ has pipewire

// AudioConfig holds the resolved audio inputs for the capture pipeline.
type AudioConfig struct {
	MicDevice      string
	SystemLoopback bool
}

// EnsureFFmpeg verifies FFmpeg is on PATH (and meets the minimum version),
// or attempts to install it via the platform installer.
func EnsureFFmpeg(log *zap.Logger) (string, error) {
	path, err := findFFmpeg()
	if err == nil {
		log.Info("FFmpeg found on PATH", zap.String("path", path))
		return path, checkFFmpegVersion(path, log)
	}
	log.Info("FFmpeg not found — attempting installation")
	return installFFmpeg(log)
}

// ResolveAudio returns the audio device config for the requested mode.
func ResolveAudio(ffmpegBin string, mode settings.AudioMode, log *zap.Logger) AudioConfig {
	log.Info("resolving audio", zap.String("mode", string(mode)))
	switch mode {
	case settings.AudioOff:
		log.Info("audio disabled")
		return AudioConfig{}
	case settings.AudioSystem:
		log.Info("system audio only")
		return AudioConfig{SystemLoopback: true}
	case settings.AudioBoth:
		mic := detectMicDevice(ffmpegBin, log)
		if mic == "" {
			log.Warn("no mic found — falling back to system only")
			return AudioConfig{SystemLoopback: true}
		}
		log.Info("system + mic", zap.String("mic", mic))
		return AudioConfig{MicDevice: mic, SystemLoopback: true}
	default: // AudioMicrophone
		mic := detectMicDevice(ffmpegBin, log)
		return AudioConfig{MicDevice: mic}
	}
}

// -----------------------------------------------------------------------
// Internal
// -----------------------------------------------------------------------

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
	log.Info("FFmpeg version", zap.String("version", version), zap.Int("major", major), zap.Int("minor", minor))
	if major < minFFmpegMajor {
		return fmt.Errorf("FFmpeg %d.x or newer required, found %s — %s",
			minFFmpegMajor, version, installHint())
	}
	return nil
}

func parseFFmpegVersion(output string) (version string, major, minor int) {
	re := regexp.MustCompile(`ffmpeg version (\d+)\.(\d+)`)
	for _, line := range strings.Split(output, "\n") {
		m := re.FindStringSubmatch(line)
		if len(m) >= 3 {
			major, _ = strconv.Atoi(m[1])
			minor, _ = strconv.Atoi(m[2])
			vm := regexp.MustCompile(`ffmpeg version ([\d.]+)`).FindStringSubmatch(line)
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

func runCommand(bin string, args ...string) (string, error) {
	cmd := process.Command(bin, args...)
	var buf bytes.Buffer
	cmd.Stdout = &buf
	cmd.Stderr = &buf
	return buf.String(), cmd.Run()
}
