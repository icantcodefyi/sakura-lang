#!/bin/sh
# Installs `sakura` from a Sakura Lang release (design §17.147, research/13 §4).
#
#   sh install.sh --version 0.1.0-alpha.1 [--prefix DIR] [--target TRIPLE] [--base-url URL-or-DIR]
#
# Downloads SHA256SUMS and this platform's archive, verifies the SHA-256 before extracting
# anything, refuses unsafe archive entries, runs the binary once, then copies it to
# PREFIX/bin/sakura (default ~/.local). It never uses sudo, edits shell profiles, installs Node
# or falls back to another CPU/ABI. --target deliberately overrides CPU detection
# (including Rosetta); it must name a supported triple for the host OS. Read it before running it; docs/release.md shows the same
# steps by hand. The copy attached to a release defaults to that release's version.
set -eu

default_version=""
releases="https://github.com/icantcodefyi/sakuralang/releases/download"

die() {
	echo "sakura install: $*" >&2
	exit 1
}

usage() {
	echo "usage: sh install.sh --version VERSION [--prefix DIR] [--target TRIPLE] [--base-url https://URL-or-DIR]"
	exit "$1"
}

version=$default_version
prefix="${HOME:?HOME is not set}/.local"
base=""
requested_target=""
while [ $# -gt 0 ]; do
	case $1 in
	--version) [ $# -ge 2 ] || die "--version needs a value"; version=$2; shift 2 ;;
	--target) [ $# -ge 2 ] && [ -n "$2" ] || die "--target needs a value"; requested_target=$2; shift 2 ;;
	--prefix) [ $# -ge 2 ] || die "--prefix needs a value"; prefix=$2; shift 2 ;;
	--base-url) [ $# -ge 2 ] || die "--base-url needs a value"; base=$2; shift 2 ;;
	-h | --help) usage 0 ;;
	*) echo "sakura install: unknown argument $1" >&2; usage 2 ;;
	esac
done

version=${version#v}
[ -n "$version" ] || die "no version given: pass --version (for example --version 0.1.0-alpha.1)"
case $version in
*[!0-9A-Za-z.-]* | .* | *..*) die "not a version: $version" ;;
esac
case $prefix in
/*) ;;
*) die "--prefix must be an absolute path" ;;
esac

# Platform detection: exactly the host's own CPU and ABI.
os=$(uname -s)
arch=$(uname -m)
case $os in
Darwin)
	# A shell under Rosetta reports x86_64 on Apple silicon.
	if [ "$arch" = x86_64 ] && [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = 1 ]; then
		arch=arm64
	fi
	case $arch in
	arm64 | aarch64) target=aarch64-apple-darwin ;;
	x86_64) target=x86_64-apple-darwin ;;
	*) die "unsupported macOS CPU: $arch" ;;
	esac
	;;
Linux)
	if ls /lib/ld-musl-* >/dev/null 2>&1 || (ldd --version 2>&1 | grep -qi musl); then
		die "musl Linux (e.g. Alpine) is not supported; releases are built for glibc"
	fi
	case $arch in
	x86_64 | amd64) target=x86_64-unknown-linux-gnu ;;
	aarch64 | arm64) target=aarch64-unknown-linux-gnu ;;
	*) die "unsupported Linux CPU: $arch" ;;
	esac
	;;
*) die "unsupported operating system: $os" ;;
esac

if [ -n "$requested_target" ]; then
	case "$os:$requested_target" in
	Darwin:aarch64-apple-darwin | Darwin:x86_64-apple-darwin | \
	Linux:aarch64-unknown-linux-gnu | Linux:x86_64-unknown-linux-gnu)
		target=$requested_target
		;;
	*) die "unsupported --target for $os: $requested_target" ;;
	esac
fi

[ -n "$base" ] || base="$releases/v$version"
base=${base%/}
archive="sakuralang-$version-$target.tar.gz"
top="sakuralang-$version-$target"

tmp=$(mktemp -d) || die "cannot create a temporary directory"
staged=""
trap 'rm -rf "$tmp"; [ -z "$staged" ] || rm -f "$staged"' EXIT
trap 'exit 130' INT TERM

fetch() {
	case $base in
	https://*)
		if command -v curl >/dev/null 2>&1; then
			curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
				--retry 3 --connect-timeout 15 --max-time 600 --output "$2" "$base/$1" ||
				die "download failed: $base/$1"
		elif command -v wget >/dev/null 2>&1; then
			wget --https-only --tries=3 --timeout=60 --quiet --output-document="$2" "$base/$1" ||
				die "download failed: $base/$1"
		else
			die "neither curl nor wget is installed"
		fi
		;;
	*://*) die "only https:// URLs or a local directory are accepted: $base" ;;
	*) [ -f "$base/$1" ] || die "no $1 in $base"; cp "$base/$1" "$2" ;;
	esac
}

fetch SHA256SUMS "$tmp/SHA256SUMS"
expected=$(awk -v f="$archive" '$2 == f { print $1 }' "$tmp/SHA256SUMS")
[ -n "$expected" ] || die "release $version has no $archive: $target is not a supported platform for it"
case $expected in
*[!0-9a-f]* | *"
"*) die "malformed SHA256SUMS entry for $archive" ;;
esac
[ ${#expected} -eq 64 ] || die "malformed SHA256SUMS entry for $archive"

fetch "$archive" "$tmp/$archive"
if command -v sha256sum >/dev/null 2>&1; then
	actual=$(sha256sum "$tmp/$archive" | cut -d ' ' -f 1)
elif command -v shasum >/dev/null 2>&1; then
	actual=$(shasum -a 256 "$tmp/$archive" | cut -d ' ' -f 1)
else
	die "neither sha256sum nor shasum is installed, so $archive cannot be verified; nothing was installed"
fi
[ "$actual" = "$expected" ] || die "SHA-256 mismatch for $archive (expected $expected, got $actual); nothing was installed"

# Attestation verification must succeed before extracting or executing the archive.
if command -v gh >/dev/null 2>&1; then
	gh attestation verify "$tmp/$archive" --repo icantcodefyi/sakuralang ||
		die "attestation verification failed for $archive; nothing was installed"
else
	echo "integrity verified, not attested"
fi

# Only regular files and directories, all directly under the archive's top directory.
tar -tvzf "$tmp/$archive" >"$tmp/listing" || die "cannot read $archive"
if cut -c 1 "$tmp/listing" | grep -qv '[-d]'; then
	die "$archive contains links or special files; refusing it"
fi
tar -tzf "$tmp/$archive" >"$tmp/names"
while IFS= read -r entry; do
	case $entry in
	"$top" | "$top/") ;;
	*..* | "$top"/*/*) die "unsafe path in $archive: $entry" ;;
	"$top"/*) ;;
	*) die "unexpected path in $archive: $entry" ;;
	esac
done <"$tmp/names"
mkdir "$tmp/unpacked"
tar -xzf "$tmp/$archive" -C "$tmp/unpacked"
binary="$tmp/unpacked/$top/sakura"
[ -f "$binary" ] || die "$archive has no sakura binary"

reported=$("$binary" --version) || die "the downloaded sakura does not run on this host"
case $reported in
"sakura $version" | "sakura $version "*) ;;
*) die "the archive's binary reports '$reported', not version $version" ;;
esac

mkdir -p "$prefix/bin"
staged=$(mktemp "$prefix/bin/.sak.install.XXXXXX") || die "cannot create a staging file"
cp "$binary" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$prefix/bin/sakura"
staged=""

echo "installed $reported to $prefix/bin/sakura"
case ":$PATH:" in
*":$prefix/bin:"*) ;;
*) echo "note: $prefix/bin is not on PATH; add it in your shell profile" ;;
esac
if ! command -v node >/dev/null 2>&1; then
	echo "note: Node 26 is needed for 'sakura run' and 'sakura test' (check, build and mcp serve work without it)"
fi
