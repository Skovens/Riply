package move

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"unicode"

	"riply/internal/logger"
)

// MovedEpisode is one TV episode file landed in the library.
type MovedEpisode struct {
	Path    string `json:"path"`
	Episode int    `json:"episode"`
}

// MoveResult holds the result of a move operation.
type MoveResult struct {
	Source       string `json:"source"`
	Destination  string `json:"destination"`
	MainFile     string `json:"main_file"`
	Success      bool   `json:"success"`
	FailedExtras int    `json:"failed_extras,omitempty"`
	Error        string `json:"error,omitempty"`
	// Episodes lists every episode file landed by a TV move (N1); empty
	// for movie moves, which are fully described by MainFile.
	Episodes []MovedEpisode `json:"episodes,omitempty"`
}

// Move moves a ripped folder to the library with Jellyfin-compliant naming.
// season > 0 selects the TV layout (see moveTV); season == 0 is the movie
// layout: {Title} ({Year})/{Title} ({Year}).mkv with remaining MKVs under Extras/.
func Move(ripDir, moviesDir, title, year string, season, episode int) (*MoveResult, error) {
	title = SanitizeName(title)
	if season > 0 {
		return moveTV(ripDir, moviesDir, title, year, season, episode)
	}
	logger.Info("Moving to library", "title", title, "year", year, "rip_dir", ripDir)

	// Build destination path: {Title} ({Year})/
	destDir := filepath.Join(moviesDir, folderName(title, year))

	// Handle duplicates
	var finalDest string
	if _, err := os.Stat(destDir); os.IsNotExist(err) {
		finalDest = destDir
	} else {
		// Append (Copy N) suffix
		n := 1
		for {
			candidate := filepath.Join(moviesDir, fmt.Sprintf("%s (Copy %d)", folderName(title, year), n))
			if _, err := os.Stat(candidate); os.IsNotExist(err) {
				finalDest = candidate
				break
			}
			n++
			if n > 100 {
				return nil, fmt.Errorf("too many duplicates for %s", title)
			}
		}
	}

	// Create destination directory
	if err := os.MkdirAll(finalDest, 0755); err != nil {
		return nil, fmt.Errorf("mkdir %s: %w", finalDest, err)
	}

	// Find the biggest MKV file and rename it to {Title}.mkv
	files, err := FindMKVs(ripDir)
	if err != nil {
		return nil, fmt.Errorf("find MKVs: %w", err)
	}

	if len(files) == 0 {
		return nil, fmt.Errorf("no MKV files found in %s", ripDir)
	}

	// Find biggest file
	biggest, err := BiggestFile(files)
	if err != nil {
		return nil, err
	}

	// Move main file: {Title} ({Year}).mkv, or {Title}.mkv when year is unknown.
	mainDest := filepath.Join(finalDest, fmt.Sprintf("%s.mkv", folderName(title, year)))
	if err := safeMove(biggest, mainDest); err != nil {
		return nil, fmt.Errorf("move main file: %w", err)
	}

	// Move extras to Extras/ subfolder
	extrasDir := filepath.Join(finalDest, "Extras")
	if err := os.MkdirAll(extrasDir, 0755); err != nil {
		return nil, fmt.Errorf("create extras dir: %w", err)
	}

	var failed []string
	for _, f := range files {
		if f == biggest {
			continue
		}
		dest := filepath.Join(extrasDir, filepath.Base(f))
		if err := moveFile(f, dest); err != nil {
			logger.Warn("Could not move extra", "file", f, "error", err)
			failed = append(failed, filepath.Base(f))
		}
	}

	// H2: failed extras still live in ripDir — removing it would destroy
	// data while reporting success. Keep the dir and report partial failure.
	if len(failed) > 0 {
		msg := fmt.Sprintf("%d extra(s) failed to move; rip dir kept at %s", len(failed), ripDir)
		logger.Error(msg, "title", title)
		return &MoveResult{
			Source:       ripDir,
			Destination:  finalDest,
			MainFile:     mainDest,
			Success:      false,
			FailedExtras: len(failed),
			Error:        msg,
		}, fmt.Errorf("%s", msg)
	}

	// Clean up empty rip directory
	if err := os.RemoveAll(ripDir); err != nil {
		logger.Warn("Failed to remove rip dir after move", "path", ripDir, "error", err)
	}

	return &MoveResult{
		Source:      ripDir,
		Destination: finalDest,
		MainFile:    mainDest,
		Success:     true,
	}, nil
}

// moveTV lands every ripped MKV as a numbered episode in the TV layout:
//
//	{Series} ({Year})/Season {N}/{Series} - S{NN}E{NN} - {Title}.mkv
//
// Files are ordered largest-first (mirrors the movie main-file rule) so the
// biggest file becomes the first episode. The show folder merges with an
// existing one (unlike movies, a second copy of the same show folder would
// split the series across "(Copy N)" dirs); per-file collisions go through
// uniqueDest. Episode numbers count up from `episode` (<=0 → 1).
// An episode that fails to move keeps the rip dir and reports partial
// failure through the result, mirroring the movie extras contract.
func moveTV(ripDir, moviesDir, title, year string, season, episode int) (*MoveResult, error) {
	logger.Info("Moving to library as TV", "title", title, "year", year, "season", season, "episode", episode, "rip_dir", ripDir)

	if episode <= 0 {
		episode = 1
	}

	showDir := filepath.Join(moviesDir, folderName(title, year))
	seasonDir := filepath.Join(showDir, fmt.Sprintf("Season %d", season))
	if err := os.MkdirAll(seasonDir, 0755); err != nil {
		return nil, fmt.Errorf("mkdir %s: %w", seasonDir, err)
	}

	files, err := FindMKVs(ripDir)
	if err != nil {
		return nil, fmt.Errorf("find MKVs: %w", err)
	}
	if len(files) == 0 {
		return nil, fmt.Errorf("no MKV files found in %s", ripDir)
	}

	// Largest first: same ordering rule as the movie layout's biggest-file
	// pick, so a disc's main feature leads the episode sequence.
	biggest, err := BiggestFile(files)
	if err != nil {
		return nil, err
	}
	ordered := []string{biggest}
	for _, f := range files {
		if f != biggest {
			ordered = append(ordered, f)
		}
	}

	var landed []MovedEpisode
	var failed []string
	epNum := episode
	for _, f := range ordered {
		dest, err := uniqueDest(filepath.Join(seasonDir,
			fmt.Sprintf("%s - S%02dE%02d - %s.mkv", title, season, epNum, strings.TrimSuffix(filepath.Base(f), ".mkv"))))
		if err != nil {
			return nil, err
		}
		if err := moveFile(f, dest); err != nil {
			logger.Warn("Could not move episode", "file", f, "error", err)
			failed = append(failed, filepath.Base(f))
			continue
		}
		landed = append(landed, MovedEpisode{Path: dest, Episode: epNum})
		epNum++
	}

	if len(failed) > 0 {
		msg := fmt.Sprintf("%d episode(s) failed to move; rip dir kept at %s", len(failed), ripDir)
		logger.Error(msg, "title", title)
		return &MoveResult{
			Source:       ripDir,
			Destination:  seasonDir,
			MainFile:     firstPath(landed),
			Success:      false,
			FailedExtras: len(failed),
			Error:        msg,
			Episodes:     landed,
		}, fmt.Errorf("%s", msg)
	}

	// Clean up empty rip directory (all files moved).
	if err := os.RemoveAll(ripDir); err != nil {
		logger.Warn("Failed to remove rip dir after move", "path", ripDir, "error", err)
	}

	logger.Info("TV move complete", "title", title, "episodes", len(landed), "season_dir", seasonDir)
	return &MoveResult{
		Source:      ripDir,
		Destination: seasonDir,
		MainFile:    firstPath(landed),
		Success:     true,
		Episodes:    landed,
	}, nil
}

func firstPath(l []MovedEpisode) string {
	if len(l) > 0 {
		return l[0].Path
	}
	return ""
}

// MoveTitlesResult holds the result of a per-title move operation.
type MoveTitlesResult struct {
	Title   string `json:"title"`
	Source  string `json:"source"`
	Dest    string `json:"dest"`
	Success bool   `json:"success"`
	Error   string `json:"error,omitempty"`
}

// MoveTitleFile moves exactly the named source file from ripDir into the
// library, applying the Jellyfin naming used by Move/moveTV. It never
// re-picks a file by name/size heuristics — the caller already knows which
// file the user selected, and guessing moved the wrong MKV when titles
// shared base names or sizes.
func MoveTitleFile(ripDir, moviesDir, title string, year int, sourceBase string, isMainFile bool, season, episode int) (*MoveTitlesResult, error) {
	title = SanitizeName(title)
	yearStr := ""
	if year > 0 {
		yearStr = fmt.Sprintf("%d", year)
	}
	logger.Info("Moving title to library", "source", sourceBase, "title", title)

	src := filepath.Join(ripDir, sourceBase)
	if info, err := os.Stat(src); err != nil || info.IsDir() {
		return nil, fmt.Errorf("source file %s not found in rip output", sourceBase)
	}

	base := sourceBase
	var dest string
	if season > 0 {
		dest = filepath.Join(moviesDir, folderName(title, yearStr), fmt.Sprintf("Season %d", season),
			fmt.Sprintf("%s - S%02dE%02d - %s.mkv", title, season, episode, base))
	} else if isMainFile {
		dest = filepath.Join(moviesDir, folderName(title, yearStr), fmt.Sprintf("%s.mkv", folderName(title, yearStr)))
	} else {
		dest = filepath.Join(moviesDir, folderName(title, yearStr), "Extras", base)
	}

	// H3: os.Rename silently replaces an existing destination — re-accepting
	// the same title/season/episode, or landing two same-named extras from
	// different discs in one folder, used to overwrite the library file.
	dest, err := uniqueDest(dest)
	if err != nil {
		return nil, err
	}

	if err := os.MkdirAll(filepath.Dir(dest), 0755); err != nil {
		return nil, fmt.Errorf("mkdir %s: %w", filepath.Dir(dest), err)
	}
	if err := safeMove(src, dest); err != nil {
		return nil, fmt.Errorf("move file: %w", err)
	}

	return &MoveTitlesResult{
		Title:   title,
		Source:  src,
		Dest:    dest,
		Success: true,
	}, nil
}

// PreviewTitlesResult represents the proposed destination for a single title.
type PreviewTitlesResult struct {
	SourceFile          string  `json:"source_file"`
	SourcePath          string  `json:"source_path"`
	SizeMB              float64 `json:"size_mb"`
	IsMainFile          bool    `json:"is_main_file"`
	TitleName           string  `json:"title_name"`
	ProposedDestination string  `json:"proposed_destination"`
	TvSeason            int     `json:"tv_season,omitempty"`
	TvEpisode           int     `json:"tv_episode,omitempty"`
}

// PreviewTitles computes proposed Jellyfin-compliant paths without moving files.
// For TV, episodes are numbered sequentially from episode (the first title on
// the disc gets episode, the next episode+1, and so on).
func PreviewTitles(ripDir, moviesDir, title, year string, mediaType string, season, episode int) ([]PreviewTitlesResult, error) {
	title = SanitizeName(title)
	files, err := FindMKVs(ripDir)
	if err != nil {
		return nil, fmt.Errorf("find MKVs: %w", err)
	}

	if len(files) == 0 {
		return nil, fmt.Errorf("no MKV files found in %s", ripDir)
	}

	var biggest string
	if mediaType == "movie" {
		biggest, _ = BiggestFile(files)
	}

	var results []PreviewTitlesResult
	for idx, f := range files {
		info, err := os.Stat(f)
		if err != nil {
			continue
		}
		sizeMB := float64(info.Size()) / 1024 / 1024
		base := filepath.Base(f)
		isMain := mediaType == "movie" && f == biggest

		ep := episode
		if season > 0 {
			ep = episode + idx
		}

		var dest string
		if season > 0 {
			// TV: {Series}/Season {N}/{Series} - S{N}E{E} - {Episode}.mkv
			dest = filepath.Join(moviesDir, folderName(title, year), fmt.Sprintf("Season %d", season),
				fmt.Sprintf("%s - S%02dE%02d - %s.mkv", title, season, ep, base))
		} else if isMain {
			// Movie main: {Title} ({Year})/{Title} ({Year}).mkv
			dest = filepath.Join(moviesDir, folderName(title, year), fmt.Sprintf("%s.mkv", folderName(title, year)))
		} else {
			// Movie extra: {Title} ({Year})/Extras/{base}.mkv
			dest = filepath.Join(moviesDir, folderName(title, year), "Extras", base)
		}

		results = append(results, PreviewTitlesResult{
			SourceFile:          base,
			SourcePath:          f,
			SizeMB:              sizeMB,
			IsMainFile:          isMain,
			TitleName:           base,
			ProposedDestination: dest,
			TvSeason:            season,
			TvEpisode:           ep,
		})
	}

	return results, nil
}

func FindMKVs(dir string) ([]string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}

	var files []string
	for _, e := range entries {
		if !e.IsDir() && strings.HasSuffix(strings.ToLower(e.Name()), ".mkv") {
			files = append(files, filepath.Join(dir, e.Name()))
		}
	}
	logger.Debug("Found MKV files", "count", len(files), "dir", dir)
	return files, nil
}

func BiggestFile(files []string) (string, error) {
	if len(files) == 0 {
		return "", fmt.Errorf("no files")
	}

	var biggest string
	var max int64
	for _, f := range files {
		info, err := os.Stat(f)
		if err != nil {
			continue
		}
		if info.Size() > max {
			max = info.Size()
			biggest = f
		}
	}

	if biggest == "" {
		return files[0], nil
	}
	return biggest, nil
}

// moveFile is the move primitive used by Move's extra loop. Tests replace it
// to inject deterministic per-file failures; production uses safeMove.
var moveFile = safeMove

// uniqueDest returns dest unchanged unless a file already occupies it, in
// which case it appends " (Copy N)" before the extension (N starting at 1)
// until the name is free. Mirrors the "(Copy N)" convention Move uses for
// duplicate folders, so a re-accepted title or a same-named extra from
// another disc never overwrites the library file.
func uniqueDest(dest string) (string, error) {
	if _, err := os.Stat(dest); os.IsNotExist(err) {
		return dest, nil
	} else if err != nil {
		return "", fmt.Errorf("stat %s: %w", dest, err)
	}
	ext := filepath.Ext(dest)
	stem := strings.TrimSuffix(dest, ext)
	for n := 1; n <= 100; n++ {
		candidate := fmt.Sprintf("%s (Copy %d)%s", stem, n, ext)
		if _, err := os.Stat(candidate); os.IsNotExist(err) {
			logger.Info("Destination occupied; using copy suffix", "dest", candidate)
			return candidate, nil
		} else if err != nil {
			return "", fmt.Errorf("stat %s: %w", candidate, err)
		}
	}
	return "", fmt.Errorf("too many duplicates for %s", dest)
}

// renameFile/copyData are seams for tests to force the cross-device fallback
// and to inject mid-copy failures; production uses os.Rename/io.Copy.
var (
	renameFile = os.Rename
	copyData   = io.Copy
)

// safeMove moves a file, falling back to copy+delete for cross-device moves.
// M10: any copy/sync/close failure removes the partial destination before
// returning — a truncated file left behind would silently masquerade as a
// complete rip (and be overwritten by a later accept of the same name).
// src is only removed once the copy has fully succeeded.
// L7: the copy fallback first checks free space on the destination volume
// and refuses to start when the source does not fit (renames never need it —
// they do not consume space).
func safeMove(src, dst string) error {
	if err := renameFile(src, dst); err == nil {
		return nil
	}

	// Fallback: copy + delete (cross-device)
	srcInfo, err := os.Stat(src)
	if err != nil {
		return err
	}
	if free, ferr := FreeSpace(filepath.Dir(dst)); ferr == nil && free < uint64(srcInfo.Size()) {
		return fmt.Errorf("insufficient space to move %s: %d free, need %d", filepath.Base(dst), free, srcInfo.Size())
	}

	srcFile, err := os.Open(src)
	if err != nil {
		return err
	}
	defer srcFile.Close()

	dstFile, err := os.Create(dst)
	if err != nil {
		return err
	}

	if _, err := copyData(dstFile, srcFile); err != nil {
		dstFile.Close()
		os.Remove(dst)
		return err
	}

	if err := dstFile.Sync(); err != nil {
		dstFile.Close()
		os.Remove(dst)
		return err
	}

	if err := dstFile.Close(); err != nil {
		os.Remove(dst)
		return err
	}

	return os.Remove(src)
}

// windowsReserved are the Win32 device names, reserved even with an
// extension (CON, NUL.txt); a library share served to Windows must not
// contain them. Keys are lowercased for case-insensitive matching.
var windowsReserved = map[string]bool{
	"con": true, "prn": true, "aux": true, "nul": true,
	"com1": true, "com2": true, "com3": true, "com4": true, "com5": true,
	"com6": true, "com7": true, "com8": true, "com9": true,
	"lpt1": true, "lpt2": true, "lpt3": true, "lpt4": true, "lpt5": true,
	"lpt6": true, "lpt7": true, "lpt8": true, "lpt9": true,
}

// SanitizeName makes s safe for use as a single path element.
func SanitizeName(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case r == '/' || r == '\\' || r == ':' || r == '*' || r == '?' || r == '"' || r == '<' || r == '>' || r == '|':
			b.WriteByte('-')
		case unicode.IsLetter(r) || unicode.IsDigit(r) || r == ' ' || r == '-' || r == '_' || r == '.':
			b.WriteRune(r)
		}
	}
	out := strings.TrimSpace(b.String())
	for strings.Contains(out, "--") {
		out = strings.ReplaceAll(out, "--", "-")
	}
	if out == "" {
		return "Untitled"
	}
	// L5: "." and ".." survive the character filter but are whole path
	// elements — filepath.Join(ripDir, "..") resolves one level above the
	// library root, so an accepted title of literally ".." escaped moviesDir.
	if out == "." || out == ".." {
		return "Untitled"
	}
	// L5: Windows device names are reserved with or without an extension.
	// Prefix the element so it stays human-recognizable and distinct while
	// being inert.
	stem := out
	if i := strings.IndexByte(out, '.'); i >= 0 {
		stem = out[:i]
	}
	if windowsReserved[strings.ToLower(stem)] {
		return "Untitled-" + out
	}
	return out
}

// folderName renders "{Title} ({Year})", or just the title when year is 0/empty.
func folderName(title, year string) string {
	if year == "" || year == "0" {
		return title
	}
	return fmt.Sprintf("%s (%s)", title, year)
}
