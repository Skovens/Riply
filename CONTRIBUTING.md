# Contributing to Riply

Thanks for taking a look! This is a solo-maintained project, but outside
contributions are welcome. A few conventions keep things tidy.

## Workflow

1. **Open an issue first.** Even for yourself. One paragraph: what's broken
   or what you want, and ideally how to reproduce it. The issue is the
   permanent record of *why* a change was made.
2. **Branch off `main`.** Name it after the issue:
   ```bash
   git checkout -b fix/12-empty-disc-label
   ```
   Conventions: `fix/<issue>-<slug>`, `feat/<issue>-<slug>`,
   `docs/<slug>` (no issue needed for docs).
3. **Commit in small logical steps.** Each commit is one thing, with a
   conventional-commit subject line:
   ```
   fix: handle empty disc label in drive scanner
   feat: add pause to rip progress
   docs: clarify MakeMKV install steps
   ```
   Types used: `feat`, `fix`, `docs`, `test`, `chore`. No `wip` commits on a
   PR — force-push to keep the history clean.
4. **Verify locally before pushing.** See [Verification](#verification).
5. **Open a pull request** from your branch to `main`. Reference the issue
   in the description: `Fixes #12`. CI runs automatically (build, vet,
   race tests). Review your own diff before requesting merge — you are the
   reviewer.
6. **Merge and delete the branch.** `Fixes #12` closes the issue on merge.

Trivial one-line changes (a typo in a log message) can skip the issue, but
still go through branch → PR → merge. Nothing lands on `main` without a
green CI.

## Verification

CI (`.github/workflows/ci.yml`) runs:

```bash
go build ./...
go vet ./...
go test -race ./...
```

That's the floor, not the ceiling. Riply is hardware-dependent:

- **Bug fixes:** reproduce locally first (`RIPLY_DEBUG=1 ./riply`), confirm
  the fix on a real disc if the change touches drive/rip/naming paths.
- **New features:** same — at minimum, exercise the feature through the web
  UI and read the log output to confirm the expected state transitions.
- **If you can't test on hardware** (no optical drive), say so in the PR
  description so the maintainer knows what's unverified.

## Releases

Releases are automated via GoReleaser (`.goreleaser.yaml`,
`.github/workflows/release.yml`). To cut a release:

1. Ensure `main` is green and `CHANGELOG.md` has an entry for the new
   version (format: `## v0.x.y — YYYY-MM-DD` with `### Added` / `### Fixed`
   subsections).
2. Bump the version in `internal/version/version.go` (currently `0.0.1`).
3. Tag and push:
   ```bash
   git tag v0.0.2
   git push origin v0.0.2
   ```
4. The release workflow runs tests, builds linux/amd64 + arm64 binaries,
   generates `checksums.txt`, and publishes a GitHub Release with the
   changelog (commits prefixed `docs:`, `test:`, `chore:` are excluded from
   the generated notes).

Do not run GoReleaser locally; the workflow handles it.

## Coding conventions

See [`AGENTS.md`](AGENTS.md) for the full list. The short version for humans:

- Go 1.26+, stdlib `net/http` only — no new HTTP frameworks
- Dependencies limited to `BurntSushi/toml` and `modernc.org/sqlite`
- Use `riply/internal/logger`, never `fmt.Printf` or stdlib `log`
- TV handling (`IsTV`, season/episode paths) must keep working
- Drive state stays in memory; completed rips persist via `internal/library`
