//go:build linux

package setup

import (
	"fmt"
	"os"
	"os/exec"
	"strings"

	"go.uber.org/zap"
)

func findFFmpeg() (string, error) {
	return exec.LookPath("ffmpeg")
}

func installFFmpeg(log *zap.Logger) (string, error) {
	type pm struct {
		bin  string
		args []string
		hint string
	}
	var selected *pm
	for _, candidate := range []pm{
		{bin: "apt-get", args: []string{"apt-get", "install", "-y", "ffmpeg"}, hint: "sudo apt-get install -y ffmpeg"},
		{bin: "dnf", args: []string{"dnf", "install", "-y", "ffmpeg"}, hint: "sudo dnf install -y ffmpeg"},
		{bin: "pacman", args: []string{"pacman", "-S", "--noconfirm", "ffmpeg"}, hint: "sudo pacman -S --noconfirm ffmpeg"},
		{bin: "zypper", args: []string{"zypper", "--non-interactive", "install", "ffmpeg"}, hint: "sudo zypper --non-interactive install ffmpeg"},
	} {
		if _, err := exec.LookPath(candidate.bin); err == nil {
			c := candidate
			selected = &c
			break
		}
	}
	if selected == nil {
		return "", fmt.Errorf("ffmpeg not found and no supported package manager detected")
	}

	run := func(bin string, args ...string) error {
		out, err := runCommand(bin, args...)
		if err != nil {
			log.Warn("install command failed",
				zap.String("cmd", strings.Join(append([]string{bin}, args...), " ")),
				zap.String("out", strings.TrimSpace(out)),
				zap.Error(err))
		}
		return err
	}

	log.Info("installing FFmpeg via package manager", zap.String("cmd", selected.hint))
	if os.Geteuid() == 0 {
		_ = run(selected.args[0], selected.args[1:]...)
	} else if _, err := exec.LookPath("sudo"); err == nil {
		if run("sudo", append([]string{"-n"}, selected.args...)...) != nil {
			_ = run("sudo", selected.args...)
		}
	} else {
		_ = run(selected.args[0], selected.args[1:]...)
	}

	if path, err := findFFmpeg(); err == nil {
		log.Info("FFmpeg ready", zap.String("path", path))
		return path, nil
	}
	return "", fmt.Errorf("ffmpeg install failed — try manually: %s", selected.hint)
}

func installHint() string {
	for _, pm := range []struct{ bin, cmd string }{
		{"apt-get", "sudo apt-get install -y ffmpeg"},
		{"dnf", "sudo dnf install -y ffmpeg"},
		{"pacman", "sudo pacman -S --noconfirm ffmpeg"},
		{"zypper", "sudo zypper --non-interactive install ffmpeg"},
	} {
		if _, err := exec.LookPath(pm.bin); err == nil {
			return pm.cmd
		}
	}
	return "install ffmpeg via your package manager"
}

func detectMicDevice(ffmpegBin string, log *zap.Logger) string {
	log.Info("detecting audio devices (PulseAudio/PipeWire)")

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
