class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://github.com/0smboy/rliamp/archive/refs/tags/v0.1.12.tar.gz"
  sha256 "cb3d98e47ccf1f0a9d584ecb9f226ede0a31797a82e6fe277b1283169d42b584"
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
