//go:build windows

package process

import (
	"os/exec"
	"syscall"
)

// Command creates an exec.Cmd that, on Windows, won't open a console window.
func Command(name string, args ...string) *exec.Cmd {
	cmd := exec.Command(name, args...)
	cmd.SysProcAttr = &syscall.SysProcAttr{
		CreationFlags: 0x08000000, // CREATE_NO_WINDOW
	}
	return cmd
}
