//go:build darwin

package setup

import (
	"fmt"
	"os/exec"
	"path/filepath"
	"regexp"
	"strings"

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

	log.Info("installing FFmpeg from evermeet.cx", zap.String("dir", installDir))
	if err := installEvermeetBinary("ffmpeg", filepath.Join(installDir, "ffmpeg")); err != nil {
		return "", err
	}
	_ = installEvermeetBinary("ffprobe", filepath.Join(installDir, "ffprobe"))
	return filepath.Join(installDir, "ffmpeg"), nil
}

func installHint() string {
	if _, err := exec.LookPath("brew"); err == nil {
		return "brew install ffmpeg"
	}
	return "download evermeet.cx ffmpeg build or use Homebrew"
}

func installEvermeetBinary(name, outPath string) error {
	urls := []string{
		fmt.Sprintf("https://evermeet.cx/ffmpeg/getrelease/%s/zip", name),
		fmt.Sprintf("https://evermeet.cx/ffmpeg/getrelease/%s", name),
	}
	var lastErr error
	for _, u := range urls {
		data, err := downloadBytes(u)
		if err != nil {
			lastErr = err
			continue
		}
		if len(data) >= 4 && string(data[:4]) == "PK\x03\x04" {
			bin, _, err := unzipFind(data, func(entry string) bool {
				return strings.HasSuffix(entry, "/"+name) || entry == name
			})
			if err != nil {
				lastErr = err
				continue
			}
			return writeExecutable(outPath, bin)
		}
		return writeExecutable(outPath, data)
	}
	return fmt.Errorf("download %s from evermeet.cx failed: %w", name, lastErr)
}

func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices (AVFoundation)")
	out, _ := runCommand(ffmpegBin, "-f", "avfoundation", "-list_devices", "true", "-i", "dummy")

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
