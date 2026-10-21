#!/bin/sh
# Tests install.sh against stub releases (research/13 §4): a good install into a prefix with
# spaces, a repeat install, and every refusal (bad hash, missing entry, empty sums, no
# verifier, links, path traversal, wrong version, http://, failed download). Each refusal must
# leave the prefix without a `sakura`; the native driver lands in lib/sakura/native. Run on an
# inventory host: `sh tools/install/test.sh`.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
installer="$here/install.sh"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
python=$(python3 -c 'import sys; print(sys.executable)')
targets_file="$here/../../packaging/targets.json"
targets=$("$python" -I -c 'import json, sys; print("\n".join(t["target"] for t in json.load(open(sys.argv[1])) if t["os"] != "win32"))' "$targets_file")
version=9.9.9-test.1
failures=0

sha() {
	if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d ' ' -f 1; else shasum -a 256 "$1" | cut -d ' ' -f 1; fi
}

# release DIR [REPORTED-VERSION]: stub archives for every inventory target, plus SHA256SUMS.
release() {
	mkdir -p "$1"
	for target in $targets; do
		top="sakuralang-$version-$target"
		mkdir -p "$work/stage/$top"
		printf '#!/bin/sh\necho "sakura %s (0000000000000000000000000000000000000000) %s"\n' "${2:-$version}" "$target" >"$work/stage/$top/sakura"
		chmod 755 "$work/stage/$top/sakura"
		mkdir -p "$work/stage/$top/lib/sakura/native"
		echo driver >"$work/stage/$top/lib/sakura/native/libsakura_native_driver.a"
		tar -czf "$1/$top.tar.gz" -C "$work/stage" "$top"
		rm -rf "$work/stage"
		echo "$(sha "$1/$top.tar.gz")  $top.tar.gz" >>"$1/SHA256SUMS"
	done
}

# expect NAME ok|fail PREFIX ARGS...: run the installer, check the outcome and the prefix.
expect() {
	name=$1 want=$2 prefix=$3
	shift 3
	if sh "$installer" --version "$version" --prefix "$prefix" "$@" >"$work/out" 2>&1; then got=ok; else got=fail; fi
	if [ "$got" = ok ] && ! "$prefix/bin/sakura" --version >/dev/null 2>&1; then got="ok without a working binary"; fi
	if [ "$got" = fail ] && [ -e "$prefix/bin/sakura" ]; then got="fail but installed"; fi
	if [ "$got" = "$want" ]; then
		echo "ok   $name"
	else
		echo "FAIL $name: wanted $want, got $got"
		sed 's/^/     /' "$work/out"
		failures=$((failures + 1))
	fi
}

good="$work/good release"
release "$good"
# Keep the no-gh path deterministic even on hosts with an authenticated GitHub CLI.
no_gh="$work/no gh"
mkdir "$no_gh"
for tool in sh uname awk cut grep tar gzip mkdir mktemp cp chmod mv rm ls sed cat sysctl ldd dirname sha256sum shasum find wc head curl wget ln; do
	if path=$(command -v "$tool"); then ln -s "$path" "$no_gh/$tool"; fi
done
PATH=$no_gh
export PATH
expect "install into a prefix with spaces" ok "$work/prefix one" --base-url "$good"
if ! grep -q '^integrity verified, not attested$' "$work/out"; then
	echo "FAIL no gh: missing attestation notice"
	failures=$((failures + 1))
fi
if [ "$(cat "$work/prefix one/lib/sakura/native/libsakura_native_driver.a")" != driver ]; then
	echo "FAIL the native driver was not installed beside bin/"
	failures=$((failures + 1))
fi
expect "repeat install" ok "$work/prefix one" --base-url "$good"
if [ "$(find "$work/prefix one/bin" -mindepth 1 | wc -l)" -ne 1 ]; then
	echo "FAIL install left files beside sakura"
	failures=$((failures + 1))
fi

# A stub verifies the exact command and downloaded archive, without network or credentials.
gh_tools="$work/gh tools"
mkdir "$gh_tools"
cat >"$gh_tools/gh" <<'EOF'
#!/bin/sh
set -eu
[ "$#" -eq 5 ] && [ "$1" = attestation ] && [ "$2" = verify ] &&
	[ -f "$3" ] && [ "$4" = --repo ] && [ "$5" = icantcodefyi/sakuralang ] || exit 2
case $3 in
*/sakuralang-9.9.9-test.1-*.tar.gz) ;;
*) exit 2 ;;
esac
echo verified >"$GH_TEST_CALLED"
exit "$GH_TEST_STATUS"
EOF
chmod 755 "$gh_tools/gh"
GH_TEST_CALLED="$work/gh called"
GH_TEST_STATUS=0
export GH_TEST_CALLED GH_TEST_STATUS
PATH="$gh_tools:$no_gh"
expect "gh attestation verified" ok "$work/attested" --base-url "$good"
if [ ! -f "$GH_TEST_CALLED" ] || grep -q 'not attested' "$work/out"; then
	echo "FAIL gh verification was skipped or reported as unattested"
	failures=$((failures + 1))
fi
rm -f "$GH_TEST_CALLED"
GH_TEST_STATUS=1
expect "gh attestation failure" fail "$work/unattested" --base-url "$good"
if [ ! -f "$GH_TEST_CALLED" ] || ! grep -q 'attestation verification failed' "$work/out"; then
	echo "FAIL gh failure: wrong refusal"
	failures=$((failures + 1))
fi
PATH=$no_gh
expect "relative prefix refused" fail "relative-prefix" --base-url "$good"
expect "dash-leading prefix refused" fail "-prefix" --base-url "$good"
expect "empty prefix refused" fail "" --base-url "$good"

bad="$work/bad hash"
release "$bad"
zeros=0000000000000000000000000000000000000000000000000000000000000000
awk -v z="$zeros" '{ print z "  " $2 }' "$bad/SHA256SUMS" >"$bad/sums" && mv "$bad/sums" "$bad/SHA256SUMS"
expect "SHA-256 mismatch" fail "$work/p2" --base-url "$bad"

missing="$work/missing entry"
release "$missing"
: >"$missing/SHA256SUMS"
expect "platform absent from SHA256SUMS" fail "$work/p3" --base-url "$missing"

nosums="$work/no sums"
release "$nosums"
rm "$nosums/SHA256SUMS"
expect "no SHA256SUMS" fail "$work/p4" --base-url "$nosums"

truncated="$work/truncated"
release "$truncated"
for f in "$truncated"/*.tar.gz; do head -c 100 "$f" >"$f.part" && mv "$f.part" "$f"; done
expect "truncated archive" fail "$work/p5" --base-url "$truncated"

# No verifier: a PATH with every tool the installer needs except sha256sum and shasum.
tools="$work/tools"
mkdir "$tools"
for tool in sh uname awk cut grep tar gzip mkdir mktemp cp chmod mv rm ls sed cat sysctl ldd dirname; do
	if path=$(command -v "$tool"); then ln -s "$path" "$tools/$tool"; fi
done
if PATH="$tools" sh "$installer" --version "$version" --prefix "$work/p6" --base-url "$good" >"$work/out" 2>&1; then
	echo "FAIL no verifier: installed anyway"
	failures=$((failures + 1))
elif grep -q "cannot be verified" "$work/out" && [ ! -e "$work/p6/bin/sakura" ]; then
	echo "ok   no verifier"
else
	echo "FAIL no verifier: wrong refusal"
	sed 's/^/     /' "$work/out"
	failures=$((failures + 1))
fi

# Archives with a symlink, and with a path outside the top directory.
links="$work/links"
mkdir -p "$links"
for target in $targets; do
	top="sakuralang-$version-$target"
	mkdir -p "$work/stage/$top"
	ln -s /bin/sh "$work/stage/$top/sakura"
	tar -czf "$links/$top.tar.gz" -C "$work/stage" "$top"
	rm -rf "$work/stage"
	echo "$(sha "$links/$top.tar.gz")  $top.tar.gz" >>"$links/SHA256SUMS"
done
expect "symlink entry" fail "$work/p7" --base-url "$links"

# unsafe DIR ENTRY: archives whose second entry is ENTRY under the top directory.
unsafe() {
"$python" -I - "$1" "$version" "$targets_file" "$2" <<'EOF'
import hashlib, io, json, os, sys, tarfile
out, version, extra = sys.argv[1], sys.argv[2], sys.argv[4]
os.makedirs(out)
with open(os.path.join(out, "SHA256SUMS"), "w") as sums:
    for entry in json.load(open(sys.argv[3])):
        if entry["os"] == "win32":
            continue
        target = entry["target"]
        top = f"sakuralang-{version}-{target}"
        path = os.path.join(out, f"{top}.tar.gz")
        with tarfile.open(path, "w:gz") as tar:
            for name in (f"{top}/sakura", f"{top}/{extra}"):
                data = b"#!/bin/sh\necho sakura " + version.encode() + b"\n"
                info = tarfile.TarInfo(name)
                info.size, info.mode = len(data), 0o755
                tar.addfile(info, io.BytesIO(data))
        sums.write(f"{hashlib.sha256(open(path, 'rb').read()).hexdigest()}  {top}.tar.gz\n")
EOF
}
unsafe "$work/traversal" ../escaped
expect "path traversal entry" fail "$work/p8" --base-url "$work/traversal"
unsafe "$work/nested" lib/sakura/native/deeper/file
expect "nested entry under lib/sakura/native" fail "$work/p8b" --base-url "$work/nested"
[ ! -e "$work/escaped" ] || { echo "FAIL traversal wrote outside"; failures=$((failures + 1)); }

wrong="$work/wrong version"
release "$wrong" 1.0.0
expect "binary reports another version" fail "$work/p9" --base-url "$wrong"

expect "http:// refused" fail "$work/p10" --base-url "http://example.invalid/releases"
expect "download failure" fail "$work/p11" --base-url "https://127.0.0.1:9/releases"
if sh "$installer" --prefix "$work/p12" --base-url "$good" >"$work/out" 2>&1; then
	echo "FAIL no version: installed anyway"
	failures=$((failures + 1))
else
	echo "ok   no version given"
fi

expect "empty explicit target" fail "$work/empty target" --base-url "$good" --target ""
expect "unknown explicit target" fail "$work/unknown target" --base-url "$good" --target unknown-triple
expect "cross-OS explicit target" fail "$work/cross os" --base-url "$good" --target x86_64-pc-windows-msvc
fake_host="$work/fake host"
mkdir "$fake_host"
cat > "$fake_host/uname" <<'EOF'
#!/bin/sh
case $1 in -s) echo Linux;; -m) echo x86_64;; esac
EOF
chmod +x "$fake_host/uname"
PATH="$fake_host:$no_gh" expect "explicit target overrides CPU detection" ok "$work/explicit target" --base-url "$good" --target aarch64-unknown-linux-gnu
if ! "$work/explicit target/bin/sakura" --version | grep -q aarch64-unknown-linux-gnu; then
	echo "FAIL explicit target selected another archive"
	failures=$((failures + 1))
fi

[ "$failures" -eq 0 ] || { echo "$failures installer test(s) failed"; exit 1; }
echo "all installer tests passed"
