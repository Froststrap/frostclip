//go:build darwin

package notify

import (
	"fmt"
	"os/exec"
	"strings"
)

func Send(title, message string) {
	script := fmt.Sprintf(
		`display notification %q with title %q`,
		message, title,
	)
	_ = exec.Command("osascript", "-e", script).Run()
}

func SendWithAction(title, message, actionLabel, actionURI string) {
	body := strings.TrimSpace(message + "\n" + actionLabel + ": " + actionURI)
	Send(title, body)
}
