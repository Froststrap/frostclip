//go:build windows

package browser

import "frostclip/internal/process"

func Open(url string) error {
	cmd := process.Command("rundll32", "url.dll,FileProtocolHandler", url)
	return cmd.Start()
}
