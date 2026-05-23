//go:build darwin

package audio

import (
	"fmt"

	"go.uber.org/zap"
)

type darwinVolumeMixer struct {
	log *zap.Logger
}

func newPlatformMixer(log *zap.Logger) VolumeMixerImpl {
	return &darwinVolumeMixer{log: log}
}

// SetVolume sets volume using macOS Audio APIs
// macOS per-app volume control requires AVFoundation and Core Audio APIs
// This is a placeholder - full implementation would require cgo and macOS SDK
func (dvm *darwinVolumeMixer) SetVolume(appNameOrPID string, volume float64) error {
	dvm.log.Debug("volume mixer not yet implemented on macOS",
		zap.String("app", appNameOrPID),
		zap.Float64("volume", volume))
	// TODO: Implement using macOS Core Audio or AVFoundation APIs
	// For now, just log the intention
	return nil
}

// Close does nothing on macOS
func (dvm *darwinVolumeMixer) Close() error {
	return nil
}
