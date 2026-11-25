package makemkvinstall

import (
	"strings"
	"testing"
)

// The embedded installer must be a real, non-empty bash script: an empty
// embed (e.g. a bad //go:embed path) would silently ship a broken install
// offer.
func TestScriptEmbedded(t *testing.T) {
	if !strings.Contains(scriptContent, "#!/usr/bin/env bash") {
		t.Error("embedded script missing shebang")
	}
	for _, want := range []string{"makemkv-bin-", "makemkv-oss-", "sha256", "make install", "ldconfig", "makemkvcon"} {
		if !strings.Contains(scriptContent, want) {
			t.Errorf("embedded script does not mention %q", want)
		}
	}
}

func TestManualInstructionsHaveLinks(t *testing.T) {
	s := ManualInstructions()
	for _, want := range []string{
		"https://www.makemkv.com/download/history.html",
		"makemkv-bin-",
		"makemkv-oss-",
		"makemkv-sha-",
		"sudo apt-get install",
		"--disable-gui --disable-qt5",
		"makemkvcon -r info disc:0",
	} {
		if !strings.Contains(s, want) {
			t.Errorf("manual instructions missing %q", want)
		}
	}
}

func TestInstallCommandPath(t *testing.T) {
	if InstallCommand != "scripts/install-makemkv.sh" {
		t.Errorf("InstallCommand = %q, want the repo-relative installer path", InstallCommand)
	}
}
