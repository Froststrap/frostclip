// Package userdata manages persistent user data (Discord auth, profile info)
// stored in data.json next to the executable. This file is gitignored.
package userdata

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// Data holds all persisted user information.
type Data struct {
	DiscordID       string `json:"discord_id,omitempty"`
	DiscordUsername string `json:"discord_username,omitempty"`
	DiscordEmail    string `json:"discord_email,omitempty"`
	AccessToken     string `json:"access_token,omitempty"`
	RefreshToken    string `json:"refresh_token,omitempty"`
	MaxClips        int    `json:"max_clips,omitempty"`
	MaxDuration     int    `json:"max_duration_seconds,omitempty"`
	Resolution      string `json:"resolution,omitempty"`
}

func dataFilePath() (string, error) {
	exe, err := os.Executable()
	if err != nil {
		return "", err
	}
	return filepath.Join(filepath.Dir(exe), "data.json"), nil
}

// Load reads data.json. Returns empty Data if the file doesn't exist yet.
func Load() (*Data, error) {
	path, err := dataFilePath()
	if err != nil {
		return nil, err
	}
	f, err := os.Open(path)
	if os.IsNotExist(err) {
		return &Data{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("failed to open data.json: %w", err)
	}
	defer f.Close()

	var d Data
	if err := json.NewDecoder(f).Decode(&d); err != nil {
		return nil, fmt.Errorf("failed to parse data.json: %w", err)
	}
	return &d, nil
}

// Save writes d to data.json atomically.
func Save(d *Data) error {
	path, err := dataFilePath()
	if err != nil {
		return err
	}
	tmp := path + ".tmp"
	f, err := os.OpenFile(tmp, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, 0600)
	if err != nil {
		return fmt.Errorf("failed to write data.json: %w", err)
	}
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	if err := enc.Encode(d); err != nil {
		f.Close()
		os.Remove(tmp)
		return err
	}
	f.Close()
	return os.Rename(tmp, path)
}

// Clear wipes all stored user data (logout).
func Clear() error {
	return Save(&Data{})
}

// LoggedIn returns true if there's a stored Discord access token.
func (d *Data) LoggedIn() bool {
	return d.AccessToken != "" && d.DiscordID != ""
}
