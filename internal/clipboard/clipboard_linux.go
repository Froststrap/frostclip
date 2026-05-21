//go:build linux

package clipboard

import (
	"fmt"
	"os/exec"
)

// Write copies text to the system clipboard (tries xclip, xsel, wl-copy).
func Write(text string) error {
	tools := [][]string{
		{"wl-copy"},          // Wayland
		{"xclip", "-selection", "clipboard"},
		{"xsel", "--clipboard", "--input"},
	}
	for _, args := range tools {
		cmd := exec.Command(args[0], args[1:]...)
		cmd.Stdin = nil
		// pipe text via stdin
		pipe, err := cmd.StdinPipe()
		if err != nil {
			continue
		}
		if err := cmd.Start(); err != nil {
			continue
		}
		fmt.Fprint(pipe, text)
		pipe.Close()
		if err := cmd.Wait(); err == nil {
			return nil
		}
	}
	return fmt.Errorf("no clipboard tool found (install wl-copy, xclip, or xsel)")
}
