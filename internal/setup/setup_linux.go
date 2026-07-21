//go:build linux

package setup

import (
	"fmt"
	"os"
	"os/exec"
	"strings"

	"frostclip/internal/settings"

	"go.uber.org/zap"
)

// AudioConfig is used by main.go (we return an empty one on Linux)
type AudioConfig struct {
	MicDevice      string
	SystemLoopback bool
	SystemDevice   string
}

// EnsureRecorder installs gpu-screen-recorder if missing
func EnsureRecorder(log *zap.Logger) error {
	if _, err := exec.LookPath("gpu-screen-recorder"); err == nil {
		log.Info("gpu-screen-recorder found")
		return nil
	}
	log.Info("gpu-screen-recorder not found, attempting installation")
	return installGPUScreenRecorder(log)
}

func installGPUScreenRecorder(log *zap.Logger) error {
	type pm struct {
		bin  string
		args []string
		hint string
	}
	var selected *pm

	candidates := []pm{
		{bin: "apt-get", args: []string{"apt-get", "install", "-y", "gpu-screen-recorder"}, hint: "sudo apt-get install -y gpu-screen-recorder"},
		{bin: "dnf", args: []string{"dnf", "install", "-y", "gpu-screen-recorder"}, hint: "sudo dnf install -y gpu-screen-recorder"},
		{bin: "paru", args: []string{"paru", "-S", "--noconfirm", "gpu-screen-recorder"}, hint: "paru -S --noconfirm gpu-screen-recorder"},
		{bin: "yay", args: []string{"yay", "-S", "--noconfirm", "gpu-screen-recorder"}, hint: "yay -S --noconfirm gpu-screen-recorder"},
		{bin: "zypper", args: []string{"zypper", "--non-interactive", "install", "gpu-screen-recorder"}, hint: "sudo zypper --non-interactive install gpu-screen-recorder"},
	}

	for _, candidate := range candidates {
		if _, err := exec.LookPath(candidate.bin); err == nil {
			selected = &candidate
			break
		}
	}
	if selected == nil {
		return fmt.Errorf("no supported package manager found; please install gpu-screen-recorder manually from https://github.com/dec05eba/gpu-screen-recorder")
	}

	run := func(bin string, args ...string) error {
		out, err := exec.Command(bin, args...).CombinedOutput()
		if err != nil {
			log.Warn("install command failed",
				zap.String("cmd", strings.Join(append([]string{bin}, args...), " ")),
				zap.String("out", strings.TrimSpace(string(out))),
				zap.Error(err))
		}
		return err
	}

	log.Info("installing gpu-screen-recorder via package manager", zap.String("cmd", selected.hint))
	if os.Geteuid() == 0 {
		_ = run(selected.args[0], selected.args[1:]...)
	} else if _, err := exec.LookPath("sudo"); err == nil {
		if run("sudo", append([]string{"-n"}, selected.args...)...) != nil {
			_ = run("sudo", selected.args...)
		}
	} else {
		_ = run(selected.args[0], selected.args[1:]...)
	}

	if _, err := exec.LookPath("gpu-screen-recorder"); err == nil {
		log.Info("gpu-screen-recorder installed successfully")
		return nil
	}
	return fmt.Errorf("installation failed; try manually: %s", selected.hint)
}

// EnsureFFmpeg is a stub on Linux – we don't need it.
func EnsureFFmpeg(log *zap.Logger) (string, error) {
	return "", nil
}

// ResolveAudio is a stub on Linux – capture_linux.go handles audio detection.
func ResolveAudio(ffmpegBin string, mode settings.AudioMode, log *zap.Logger) AudioConfig {
	return AudioConfig{}
}
