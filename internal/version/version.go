// Package version holds the Riply version string.
package version

// Version is the Riply version string. Release builds override it via
// -ldflags "-X riply/internal/version.Version={{.Version}}" (see
// .goreleaser.yaml); a plain `go build` reports this dev default.
//
// Tags follow Semantic Versioning (semver.org): MAJOR.MINOR.PATCH —
// MAJOR for breaking changes, MINOR for features, PATCH for fixes;
// 0.x makes no stability promise; suffix -rc.N for release candidates.
// Tags are release events: merging to main does not bump anything.
var Version = "0.0.1-dev"
