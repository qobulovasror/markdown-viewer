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
      sha256 "f0af7f76e9e34bdfa2d37dd8a7107bc0b248cc5836cf3ee6ab89e80b9823e6aa"
    end
    on_intel do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "65467a0fc04618440451094ea5b15e345b9cc889fb8ec9b907d5d502e474e144"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "e9749144d9f0a819eb34d1fe0e6df8630c029e72be46cad2ae81f9d7ae4a2b5d"
    end
    on_intel do
      url "https://github.com/qobulovasror/markdown-viewer/releases/download/v#{version}/mdvw-v#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "5ad1fd8ee0677275ce0d1f28b0b7c7a517df6a1913111c536ab0b12d307839e8"
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
