# Homebrew formula. After a release, copy each sha256 from the release's
# SHA256SUMS file, then publish this in a tap repo (qobulovasror/homebrew-tap).
class Mdvw < Formula
  desc "Fast terminal viewer for Markdown, Org, AsciiDoc and reStructuredText"
  homepage "https://github.com/qobulovasror/markdown-viewer"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ME"
    end
    on_intel do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_ME"
    end
  end

  def install
    bin.install "mdvw"
    man1.install "mdvw.1"
    bash_completion.install "completions/mdvw.bash" => "mdvw"
    zsh_completion.install "completions/_mdvw"
    fish_completion.install "completions/mdvw.fish"
  end

  test do
    (testpath/"t.md").write("# Hi\n")
    assert_match "Hi", shell_output("#{bin}/mdvw -p --color never #{testpath}/t.md")
  end
end
