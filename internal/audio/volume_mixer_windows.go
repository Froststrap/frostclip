//go:build windows

package audio

import (
	"go.uber.org/zap"
)

type windowsVolumeMixer struct {
	log *zap.Logger
}

func newPlatformMixer(log *zap.Logger) VolumeMixerImpl {
	return &windowsVolumeMixer{log: log}
}

// SetVolume sets volume using Windows Core Audio API
// Windows per-app volume control requires COM/Win32 API access
// This is a placeholder - full implementation would require cgo and Windows SDK
func (wvm *windowsVolumeMixer) SetVolume(appNameOrPID string, volume float64) error {
	wvm.log.Debug("volume mixer not yet implemented on Windows",
		zap.String("app", appNameOrPID),
		zap.Float64("volume", volume))
	// TODO: Implement using Windows Volume Mixer API or third-party tools
	// For now, just log the intention
	return nil
}

// Close does nothing on Windows
func (wvm *windowsVolumeMixer) Close() error {
	return nil
}
