cask "ralfred" do
  version "0.1.0"
  sha256 "6e002678da43393ad40450e493fdb65fbeee0062d3ea3dbd91dc7d5ecd871132"

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
