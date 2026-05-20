//go:build linux

package capture

import (
	"os/exec"
	"strings"
)

func loopbackInputArgs(device string) []string {
	if device == "" {
		device = defaultMonitorSource()
	}
	return []string{
		"-f", "pulse",
		"-async", "10",
		"-i", device,
	}
}

func micInputArgs(device string) []string {
	return []string{
		"-f", "pulse",
		"-async", "10",
		"-i", device,
	}
}

func defaultMonitorSource() string {
	out, err := exec.Command("pactl", "info").Output()
	if err != nil {
		return "default.monitor"
	}
	for _, line := range strings.Split(string(out), "\n") {
		if strings.HasPrefix(line, "Default Sink:") {
			sink := strings.TrimSpace(strings.TrimPrefix(line, "Default Sink:"))
			if sink != "" {
				return sink + ".monitor"
			}
		}
	}
	return "default.monitor"
}
