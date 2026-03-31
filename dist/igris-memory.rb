class IgrisMemory < Formula
  desc "Persistent memory server for AI coding agents (MCP protocol)"
  homepage "https://github.com/getigris/igris-memory"
  version "${VERSION}"
  license "Elastic-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/getigris/igris-memory/releases/download/v${VERSION}/igris-memory-aarch64-apple-darwin.tar.gz"
      sha256 "${SHA256_MACOS_ARM}"
    end
  end

  on_linux do
    if Hardware::CPU.intel?
      url "https://github.com/getigris/igris-memory/releases/download/v${VERSION}/igris-memory-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA256_LINUX_X64}"
    elsif Hardware::CPU.arm?
      url "https://github.com/getigris/igris-memory/releases/download/v${VERSION}/igris-memory-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "${SHA256_LINUX_ARM}"
    end
  end

  def install
    bin.install "igmem"
  end

  test do
    assert_match "igmem", shell_output("#{bin}/igmem --version")
  end
end
