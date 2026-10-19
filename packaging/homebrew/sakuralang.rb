# Formula for the tap icantcodefyi/homebrew-sakuralang (design §17.147). The version and
# SHA-256 placeholders are filled from the release's archives by `tools/release/release.py
# assemble`; the filled file is a release asset, reviewed before it goes to the tap as
# `Formula/sakuralang.rb`.
class Sakuralang < Formula
  desc "Agent-first programming language: the sakura compiler and toolchain"
  homepage "https://www.sakuralang.com"
  version "@VERSION@"
  license any_of: ["MIT", "Apache-2.0"]

  # `sakura run` and `sakura test` execute programs on Node; the release is qualified on Node 26.
  depends_on "node"

  on_macos do
    depends_on arch: :arm64

    on_arm do
      url "https://github.com/icantcodefyi/sakuralang/releases/download/v@VERSION@/sakuralang-@VERSION@-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_aarch64-apple-darwin@"
    end
  end

  on_linux do
    depends_on arch: :x86_64

    on_arm do
      odie "Homebrew on Linux ARM64 is not supported. Use the release installer (install.sh) or npm install -g sakuralang."
    end

    on_intel do
      url "https://github.com/icantcodefyi/sakuralang/releases/download/v@VERSION@/sakuralang-@VERSION@-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "@SHA256_x86_64-unknown-linux-gnu@"
    end
  end

  def install
    bin.install "sakura"
    doc.install "README.md", "THIRD-PARTY-NOTICES.md"
  end

  test do
    assert_match "sakura #{version} (", shell_output("#{bin}/sakura --version")
    system bin/"sakura", "new", "cli", "demo"
    cd "demo" do
      system bin/"sakura", "check"
      system bin/"sakura", "test"
      system bin/"sakura", "build"
    end
  end
end
