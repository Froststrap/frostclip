//go:build darwin

package clipboard

import (
	"fmt"
	"os/exec"
)

// Write copies text to the system clipboard using pbcopy.
func Write(text string) error {
	cmd := exec.Command("pbcopy")
	pipe, err := cmd.StdinPipe()
	if err != nil {
		return err
	}
	if err := cmd.Start(); err != nil {
		return err
	}
	fmt.Fprint(pipe, text)
	pipe.Close()
	return cmd.Wait()
}
