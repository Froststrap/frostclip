//go:build windows

package notify

import "github.com/go-toast/toast"

const appID = "FrostClip"

func Send(title, message string) {
	n := toast.Notification{
		AppID:   appID,
		Title:   title,
		Message: message,
	}
	_ = n.Push()
}

func SendWithAction(title, message, actionLabel, actionURI string) {
	n := toast.Notification{
		AppID:   appID,
		Title:   title,
		Message: message,
		Actions: []toast.Action{
			{Type: "protocol", Label: actionLabel, Arguments: actionURI},
		},
	}
	_ = n.Push()
}
