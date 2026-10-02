# Homebrew formula template. Fill in the URLs/sha256 from a GitHub release,
# then publish it in a tap repository (e.g. <user>/homebrew-tap).
class Mdv < Formula
  desc "Fast terminal viewer for Markdown, Org, AsciiDoc and reStructuredText"
  homepage "https://github.com/OWNER/mdv"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/OWNER/mdv/releases/download/v#{version}/mdv-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/OWNER/mdv/releases/download/v#{version}/mdv-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/OWNER/mdv/releases/download/v#{version}/mdv-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/OWNER/mdv/releases/download/v#{version}/mdv-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  def install
    bin.install "mdv"
  end

  test do
    (testpath/"t.md").write("# Hi\n")
    assert_match "Hi", shell_output("#{bin}/mdv -p --color never #{testpath}/t.md")
  end
end
