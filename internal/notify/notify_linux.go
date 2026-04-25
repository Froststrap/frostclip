//go:build linux

package notify

import (
	"os/exec"
)

func Send(title, message string) {
	path, err := exec.LookPath("notify-send")
	if err != nil {
		return
	}
	_ = exec.Command(path,
		"--app-name=FrostClip",
		"--icon=video-x-generic",
		"--expire-time=5000",
		title,
		message,
	).Run()
}

func SendWithAction(title, message, actionLabel, actionURI string) {
	Send(title, message+"\n"+actionLabel+": "+actionURI)
}
