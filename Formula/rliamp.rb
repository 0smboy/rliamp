class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://github.com/0smboy/rliamp/releases/download/v0.1.8/rliamp-v0.1.8-src.tar.gz"
  sha256 "58dbee64b75332a2a0e8c398d813438e1fec50a70dce2ff66fed561e0a837a47"
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
