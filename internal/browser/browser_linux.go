//go:build linux

package browser

import "frostclip/internal/process"

func Open(url string) error {
	cmd := process.Command("xdg-open", url)
	return cmd.Start()
}
