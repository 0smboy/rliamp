class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://github.com/0smboy/rliamp/releases/download/v0.1.10/rliamp-v0.1.10-src.tar.gz"
  sha256 "ac30e96c35502c6f252181c68679389cd9e533e90f1acf9fdd73d984835448c2"
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
