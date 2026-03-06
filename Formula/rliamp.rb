class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://github.com/0smboy/rliamp/archive/refs/tags/v0.1.13.tar.gz"
  sha256 "2c47a4bba278caf1ccfa75f2c0288d2156e987f9e2561edfbe33166ff99a3066"
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
