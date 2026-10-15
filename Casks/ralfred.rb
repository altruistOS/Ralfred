cask "ralfred" do
  version "0.1.0"
  sha256 "0a292071d087bb5563396f98e2e61475e0325410842c2b45950d087a25bbacfb"

  url "https://github.com/altruistOS/Ralfred/releases/download/v#{version}/Ralfred-#{version}.dmg"
  name "Ralfred"
  desc "Alfred-style launcher for macOS — clean-room Rust + React reimplementation"
  homepage "https://github.com/altruistOS/Ralfred"

  livecheck do
    url :url
    strategy :github_latest
  end

  app "Ralfred.app"

  zap trash: "~/Library/Application Support/ralfred"
end
