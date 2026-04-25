//go:build !windows

package process

import "os/exec"

// Command creates a plain exec.Cmd on non-Windows platforms.
func Command(name string, args ...string) *exec.Cmd {
	return exec.Command(name, args...)
}
