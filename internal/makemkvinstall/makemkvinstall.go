// Package makemkvinstall offers to install MakeMKV when a fresh machine
// lacks it. Riply probes MakeMKV at startup; when it is missing and stdin
// is an interactive terminal, Riply asks "install now?" — yes runs the
// bundled installer (which handles the whole download/build/install),
// anything else prints the manual steps with links so the user can do it
// by hand. When stdin is not a terminal (a service launch, a CI run, a
// non-interactive shell) Riply never prompts: it only prints the manual
// steps, so an unattended server is never blocked on a blocked TTY read.
package makemkvinstall

import (
	"bufio"
	_ "embed"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

//go:embed install-makemkv.sh
var scriptContent string

// DownloadBase is MakeMKV's download root; used to build the manual links.
const DownloadBase = "https://www.makemkv.com/download"

// InstallCommand is the one-liner to run the bundled installer directly.
const InstallCommand = "scripts/install-makemkv.sh"

// IsInteractive reports whether stdin is a terminal. Riply only prompts
// when true; otherwise it prints the manual instructions and moves on.
func IsInteractive() bool {
	fi, err := os.Stdin.Stat()
	if err != nil {
		return false
	}
	return (fi.Mode() & os.ModeCharDevice) != 0
}

// Prompt asks whether to install MakeMKV now. It returns true only on an
// explicit "yes". EOF, non-interactive stdin, and any other answer are
// treated as "no", so a piped/closed stdin can never wedge the server.
func Prompt() bool {
	if !IsInteractive() {
		return false
	}
	fmt.Print("\nMakeMKV is not installed — Riply cannot rip without it.\n" +
		"  Install MakeMKV now? It will ask for your sudo password, download the\n" +
		"  latest release, verify its hash, compile the libraries, and verify the\n" +
		"  install. (Answering no will show the steps to do it yourself.)\n" +
		"  [Y/n] ")
	reader := bufio.NewReader(os.Stdin)
	line, _ := reader.ReadString('\n')
	answer := strings.ToLower(strings.TrimSpace(line))
	return answer == "y" || answer == "yes"
}

// ManualInstructions is the "no" path: how to install MakeMKV by hand, with
// the direct download links. Kept in one place so the terminal prompt, the
// embedded script's --help, and this all stay in agreement.
func ManualInstructions() string {
	return `MakeMKV is required for Riply to rip, but it is not installed.

Riply ships an installer that does the whole job (finds the latest
version, downloads + verifies the release, installs build dependencies,
compiles the libraries, installs, and verifies):

    scripts/install-makemkv.sh

To run it from the Riply repository directory, re-run:

    ./riply

...and answer "yes" when prompted, or run the installer directly:

    bash scripts/install-makemkv.sh

To install by hand instead (the same steps, with links):

  1. MakeMKV Linux is released as two tarballs (no .deb/.rpm). Find the
     latest version here:
       https://www.makemkv.com/download/history.html
     For version X.Y.Z download:
       https://www.makemkv.com/download/makemkv-bin-X.Y.Z.tar.gz   (precompiled makemkvcon)
       https://www.makemkv.com/download/makemkv-oss-X.Y.Z.tar.gz   (libraries source)
     Verify both against:
       https://www.makemkv.com/download/makemkv-sha-X.Y.Z.txt

  2. Install build dependencies (Debian/Ubuntu):
       sudo apt-get install -y build-essential pkg-config \
           zlib1g-dev libexpat1-dev libssl-dev libavcodec-dev libavutil-dev

  3. Build and install the libraries from the oss tarball (skip the GUI,
     which is what needs Qt):
       tar xzf makemkv-oss-X.Y.Z.tar.gz
       cd makemkv-oss-X.Y.Z
       ./configure --disable-gui --disable-qt5
       make -j
       sudo make install

  4. Install the precompiled binary + data from the bin tarball
     (accept the EULA prompt):
       tar xzf makemkv-bin-X.Y.Z.tar.gz
       cd makemkv-bin-X.Y.Z
       sudo make install

  5. Verify:
       makemkvcon -r info disc:0

Riply detects MakeMKV automatically — no restart is needed once it is
on your PATH.`
}

// RunInstall writes the bundled installer to a temp file, marks it
// executable, and runs it with stdio inherited so the user sees the
// progress and the sudo password prompt. It blocks until the installer
// finishes. The returned error is nil on success.
func RunInstall() error {
	path, err := writeTempScript(scriptContent)
	if err != nil {
		return fmt.Errorf("write installer: %w", err)
	}
	defer os.Remove(path)

	cmd := exec.Command("bash", path)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		return fmt.Errorf("MakeMKV install failed: %w", err)
	}
	return nil
}

func writeTempScript(content string) (string, error) {
	f, err := os.CreateTemp("", "riply-install-makemkv-*.sh")
	if err != nil {
		return "", err
	}
	if _, err := f.WriteString(content); err != nil {
		f.Close()
		os.Remove(f.Name())
		return "", err
	}
	if err := f.Close(); err != nil {
		os.Remove(f.Name())
		return "", err
	}
	if err := os.Chmod(f.Name(), 0o755); err != nil {
		os.Remove(f.Name())
		return "", err
	}
	return f.Name(), nil
}
