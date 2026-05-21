//go:build windows

package clipboard

import (
	"fmt"
	"os/exec"
)

// Write copies text to the system clipboard.
func Write(text string) error {
	cmd := exec.Command("cmd", "/c", fmt.Sprintf("echo %s | clip", text))
	return cmd.Run()
}
