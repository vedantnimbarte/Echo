cask "echo" do
  version "0.3.0"
  sha256 "81730b10a14cb24e0b3bd9eef03c9b9d86f7ca8f473a7e3ae5af19b6fab503d7"

  url "https://github.com/vedantnimbarte/Echo/releases/download/v#{version}/Echo_#{version}_aarch64.dmg",
      verified: "github.com/vedantnimbarte/Echo/"
  name "Echo"
  desc "Privacy-first voice keyboard that types your speech into any app"
  homepage "https://github.com/vedantnimbarte/Echo"

  livecheck do
    url :url
    strategy :github_latest
  end

  # Apple Silicon only: the ONNX Runtime behind Echo's voice-activity detection
  # publishes no Intel macOS binaries, so no x86_64 build exists.
  depends_on arch: :arm64
  depends_on macos: ">= :monterey"

  app "Echo.app"

  # Echo is not notarised yet, so Gatekeeper refuses a quarantined copy and
  # offers only "Move to Trash". Homebrew quarantines downloads by default;
  # tell the user the one flag that gets past it rather than letting them hit a
  # dialog that reads like a malware warning.
  caveats do
    <<~EOS
      Echo is not code-signed or notarised yet. If macOS refuses to open it,
      either reinstall with quarantine disabled:

        brew reinstall --cask --no-quarantine echo

      or right-click Echo in Applications and choose Open, then confirm.
    EOS
  end

  zap trash: [
    "~/Library/Application Support/com.echo.app",
    "~/Library/Caches/com.echo.app",
    "~/Library/Preferences/com.echo.app.plist",
    "~/Library/Saved Application State/com.echo.app.savedState",
  ]
end
