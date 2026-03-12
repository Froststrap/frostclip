package setup

import (
	"bytes"

	"frostclip/internal/process"
)

func runCommand(bin string, args ...string) (string, error) {
	cmd := process.Command(bin, args...)
	var buf bytes.Buffer
	cmd.Stdout = &buf
	cmd.Stderr = &buf
	err := cmd.Run()
	return buf.String(), err
}