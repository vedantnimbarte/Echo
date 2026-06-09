# Homebrew Cask (template — fill in version, url, and sha256 per release).
cask "echo" do
  version "0.1.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/vedantnimbarte/Echo/releases/download/v#{version}/Echo_#{version}_universal.dmg"
  name "Echo"
  desc "Privacy-first universal voice keyboard"
  homepage "https://github.com/vedantnimbarte/Echo"

  depends_on macos: ">= :monterey"

  app "Echo.app"

  zap trash: [
    "~/Library/Application Support/com.echo.app",
    "~/Library/Caches/com.echo.app",
  ]
end
