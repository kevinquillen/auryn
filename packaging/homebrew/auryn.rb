# Homebrew formula template for Auryn.
#
# This lives in a tap repository (for example kevinquillen/homebrew-tap) as
# Formula/auryn.rb. The version, URLs, and SHA-256 values are updated for each
# release; cargo-dist can generate and publish this automatically. See
# docs/release.md.

class Auryn < Formula
  desc "Browse, search, preview, and resume AI coding sessions from the terminal"
  homepage "https://github.com/kevinquillen/auryn"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/kevinquillen/auryn/releases/download/v#{version}/auryn-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_APPLE_DARWIN_SHA256"
    end
    on_intel do
      url "https://github.com/kevinquillen/auryn/releases/download/v#{version}/auryn-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X86_64_APPLE_DARWIN_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/kevinquillen/auryn/releases/download/v#{version}/auryn-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_AARCH64_LINUX_SHA256"
    end
    on_intel do
      url "https://github.com/kevinquillen/auryn/releases/download/v#{version}/auryn-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_X86_64_LINUX_SHA256"
    end
  end

  def install
    bin.install "auryn"
  end

  test do
    assert_match "auryn", shell_output("#{bin}/auryn --version")
  end
end
