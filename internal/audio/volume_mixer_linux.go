//go:build linux

package audio

import (
	"fmt"
	"os/exec"
	"strconv"
	"strings"

	"go.uber.org/zap"
)

type linuxVolumeMixer struct {
	log *zap.Logger
}

func newPlatformMixer(log *zap.Logger) VolumeMixerImpl {
	return &linuxVolumeMixer{log: log}
}

// SetVolume sets volume using PulseAudio pactl or PipeWire pw-cli
func (lvm *linuxVolumeMixer) SetVolume(appNameOrPID string, volume float64) error {
	// Try PulseAudio first (more widely available)
	if err := lvm.setPulseAudioVolume(appNameOrPID, volume); err == nil {
		return nil
	}

	// Fall back to PipeWire
	return lvm.setPipeWireVolume(appNameOrPID, volume)
}

// setPulseAudioVolume sets volume on PulseAudio sinks/applications
func (lvm *linuxVolumeMixer) setPulseAudioVolume(appNameOrPID string, volume float64) error {
	// Try to find PID from process name
	pid := appNameOrPID
	if !isNumeric(appNameOrPID) {
		// Search for process by name
		foundPID, err := lvm.findProcessPID(appNameOrPID)
		if err != nil {
			return fmt.Errorf("could not find process %s: %w", appNameOrPID, err)
		}
		pid = foundPID
	}

	// Find sink input for this PID
	sinkInput, err := lvm.findPulseAudioSinkInput(pid)
	if err != nil {
		return err
	}

	// Set volume (0-100 -> 0-65536 scale for pactl)
	volumeValue := int(volume * 655.36)
	cmd := exec.Command("pactl", "set-sink-input-volume", sinkInput, fmt.Sprintf("%d", volumeValue))
	return cmd.Run()
}

// setPipeWireVolume sets volume on PipeWire nodes
func (lvm *linuxVolumeMixer) setPipeWireVolume(appNameOrPID string, volume float64) error {
	// Try to find PID from process name
	pid := appNameOrPID
	if !isNumeric(appNameOrPID) {
		foundPID, err := lvm.findProcessPID(appNameOrPID)
		if err != nil {
			return fmt.Errorf("could not find process %s: %w", appNameOrPID, err)
		}
		pid = foundPID
	}

	// Find node for this PID
	nodeID, err := lvm.findPipeWireNode(pid)
	if err != nil {
		return err
	}

	// Set volume (0-100 -> 0.0-1.0 scale for pw-cli)
	volumeValue := volume / 100.0
	cmd := exec.Command("pw-cli", "set-param", nodeID, "Props", fmt.Sprintf("volume:%f", volumeValue))
	return cmd.Run()
}

// findProcessPID finds a process ID by name
func (lvm *linuxVolumeMixer) findProcessPID(processName string) (string, error) {
	cmd := exec.Command("pgrep", "-f", processName)
	output, err := cmd.Output()
	if err != nil {
		return "", err
	}

	pids := strings.Fields(string(output))
	if len(pids) == 0 {
		return "", fmt.Errorf("no process found for %s", processName)
	}

	// Return first matching PID
	return pids[0], nil
}

// findPulseAudioSinkInput finds the sink input ID for a given PID
func (lvm *linuxVolumeMixer) findPulseAudioSinkInput(pid string) (string, error) {
	cmd := exec.Command("pactl", "list", "sink-inputs")
	output, err := cmd.Output()
	if err != nil {
		return "", err
	}

	lines := strings.Split(string(output), "\n")
	var currentSinkInput string
	for _, line := range lines {
		if strings.Contains(line, "Sink Input #") {
			parts := strings.Fields(line)
			if len(parts) >= 3 {
				currentSinkInput = strings.TrimSuffix(parts[2], "")
			}
		}
		if strings.Contains(line, fmt.Sprintf("application.process.id = \"%s\"", pid)) {
			return currentSinkInput, nil
		}
	}

	return "", fmt.Errorf("sink input not found for PID %s", pid)
}

// findPipeWireNode finds the node ID for a given PID
func (lvm *linuxVolumeMixer) findPipeWireNode(pid string) (string, error) {
	cmd := exec.Command("pw-cli", "list-objects")
	output, err := cmd.Output()
	if err != nil {
		return "", err
	}

	lines := strings.Split(string(output), "\n")
	for _, line := range lines {
		if strings.Contains(line, pid) && strings.Contains(line, "node") {
			// Extract node ID (usually first number after "id" or at line start)
			parts := strings.Fields(line)
			if len(parts) > 0 && isNumeric(parts[0]) {
				return parts[0], nil
			}
		}
	}

	return "", fmt.Errorf("pipewire node not found for PID %s", pid)
}

// Close does nothing on Linux
func (lvm *linuxVolumeMixer) Close() error {
	return nil
}

func isNumeric(s string) bool {
	_, err := strconv.Atoi(s)
	return err == nil
}
