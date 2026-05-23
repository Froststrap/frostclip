package audio

import (
	"frostclip/internal/settings"
	"go.uber.org/zap"
)

// VolumeMixer manages per-application audio volume levels
type VolumeMixer struct {
	mixer  VolumeMixerImpl
	config settings.VolumeMixer
	log    *zap.Logger
}

// VolumeMixerImpl is the platform-specific implementation
type VolumeMixerImpl interface {
	// SetVolume sets the volume for an app by process name or PID (0-100)
	// Returns error if app not found or volume control not available
	SetVolume(appNameOrPID string, volume float64) error
	// Close releases any resources
	Close() error
}

// NewVolumeMixer creates a volume mixer instance
func NewVolumeMixer(config settings.VolumeMixer, log *zap.Logger) *VolumeMixer {
	log = log.Named("volume_mixer")
	return &VolumeMixer{
		mixer:  newPlatformMixer(log),
		config: config,
		log:    log,
	}
}

// ApplyMix applies all configured volume levels
func (vm *VolumeMixer) ApplyMix() error {
	if vm.mixer == nil {
		return nil
	}

	for appNameOrPID, volume := range vm.config {
		if volume < 0 {
			volume = 0
		}
		if volume > 100 {
			volume = 100
		}

		if err := vm.mixer.SetVolume(appNameOrPID, volume); err != nil {
			vm.log.Debug("could not set volume", zap.String("app", appNameOrPID), zap.Float64("volume", volume), zap.Error(err))
			// Don't fail on individual app volume changes
		} else {
			vm.log.Debug("volume set", zap.String("app", appNameOrPID), zap.Float64("volume", volume))
		}
	}
	return nil
}

// UpdateConfig updates the volume mixer configuration
func (vm *VolumeMixer) UpdateConfig(config settings.VolumeMixer) error {
	vm.config = config
	return vm.ApplyMix()
}

// Close releases resources
func (vm *VolumeMixer) Close() error {
	if vm.mixer != nil {
		return vm.mixer.Close()
	}
	return nil
}
