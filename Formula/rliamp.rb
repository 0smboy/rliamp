class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://codeload.github.com/0smboy/rliamp/tar.gz/refs/tags/v0.1.14"
  sha256 "2665818aeb7c89f5162507249ef4395569cf44f2f74dd2b0f763af4ad9624af3"
  license :cannot_represent

  depends_on "rust" => :build

  on_linux do
    depends_on "alsa-lib"
  end

  def install
    system "cargo", "install", "--locked", "--path", ".", "--root", prefix
  end

  test do
    output = shell_output("#{bin}/rliamp 2>&1", 1)
    assert_match "usage: rliamp", output
  end
end
