//go:build linux

package clipboard

import (
	"fmt"
	"os/exec"
	"strings"
)

// Write copies text to the system clipboard (tries xclip, xsel, wl-copy).
func Write(text string) error {
	tools := []struct {
		name string
		args []string
	}{
		{name: "wl-copy", args: []string{"wl-copy"}}, // Wayland
		{name: "xclip", args: []string{"xclip", "-selection", "clipboard"}},
		{name: "xsel", args: []string{"xsel", "--clipboard", "--input"}},
	}
	var lastErr error
	for _, tool := range tools {
		cmd := exec.Command(tool.args[0], tool.args[1:]...)
		cmd.Stdin = strings.NewReader(text)
		if err := cmd.Run(); err == nil {
			return nil
		} else {
			lastErr = fmt.Errorf("%s: %w", tool.name, err)
		}
	}
	if lastErr != nil {
		return fmt.Errorf("failed to copy to clipboard: %w", lastErr)
	}
	return fmt.Errorf("no clipboard tool found (install wl-copy, xclip, or xsel)")
}
