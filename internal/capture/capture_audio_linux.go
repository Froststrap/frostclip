//go:build linux

package capture

import (
	"os/exec"
	"strings"
)

func loopbackInputArgs() []string {
	return []string{
		"-f", "pulse",
		"-i", defaultMonitorSource(),
	}
}

func micInputArgs(device string) []string {
	return []string{
		"-f", "pulse",
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
