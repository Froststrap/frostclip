//go:build darwin

package setup

import (
	"os/exec"
	"regexp"
	"strings"

	"go.uber.org/zap"
)

func findFFmpeg() (string, error) {
	return exec.LookPath("ffmpeg")
}

func installFFmpeg(log *zap.Logger) (string, error) {
	hint := installHint()
	log.Error("FFmpeg not found", zap.String("install", hint))
	return "", nil
}

func installHint() string {
	if _, err := exec.LookPath("brew"); err == nil {
		return "brew install ffmpeg"
	}
	return "install ffmpeg from https://ffmpeg.org/download.html or via Homebrew"
}

// detectMicDevice returns the AVFoundation audio device index for the first
// real microphone (non-virtual, non-loopback).
func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices (AVFoundation)")
	out, _ := runCommand(ffmpegBin, "-f", "avfoundation", "-list_devices", "true", "-i", "dummy")

	// AVFoundation lists audio devices after "[AVFoundation input device @ ...] AVFoundation audio devices:"
	inAudio := false
	re := regexp.MustCompile(`\[(\d+)\] (.+)`)
	for _, line := range strings.Split(out, "\n") {
		if strings.Contains(line, "AVFoundation audio devices") {
			inAudio = true
			continue
		}
		if !inAudio {
			continue
		}
		m := re.FindStringSubmatch(line)
		if m == nil {
			continue
		}
		idx, name := m[1], strings.TrimSpace(m[2])
		lower := strings.ToLower(name)
		// Skip virtual/loopback devices — those are for SystemLoopback
		if strings.Contains(lower, "blackhole") ||
			strings.Contains(lower, "loopback") ||
			strings.Contains(lower, "soundflower") {
			continue
		}
		log.Info("selected microphone", zap.String("device", name), zap.String("index", idx))
		return idx
	}
	log.Warn("no microphone found")
	return ""
}
