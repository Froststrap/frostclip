//go:build darwin

package browser

import "frostclip/internal/process"

func Open(url string) error {
	cmd := process.Command("open", url)
	return cmd.Start()
}
