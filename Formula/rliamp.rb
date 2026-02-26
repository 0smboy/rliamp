class Rliamp < Formula
  desc "Retro terminal music player with visualizer and EQ"
  homepage "https://github.com/0smboy/rliamp"
  url "https://github.com/0smboy/rliamp/releases/download/v0.1.7/rliamp-v0.1.7-src.tar.gz"
  sha256 "11cffdcf37f6a4a5e5c677a0f5dce9294476f3185f10a1ba27d782c05c6b6b58"
  license :cannot_represent

  depends_on "rust" => :build

  on_linux do
    depends_on "alsa-lib"
  end

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    output = shell_output("#{bin}/rliamp 2>&1", 1)
    assert_match "usage: rliamp", output
  end
end
