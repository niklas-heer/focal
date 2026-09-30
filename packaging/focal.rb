# Template for Casks/focal.rb in niklas-heer/homebrew-tap. After each release,
# set version to "<version>,<build>" and sha256 to the ZIP's checksum
# (build/release/Focal-<version>-<build>.zip.sha256), then open a pull request.
cask "focal" do
  version "0.1.0,1"
  sha256 "REPLACE_WITH_THE_ZIP_SHA256"

  url "https://github.com/niklas-heer/focal/releases/download/v#{version.csv.first}/Focal-#{version.csv.first}-#{version.csv.second}.zip"
  name "Focal"
  desc "Focused Markdown editor with live rendering, opened from the terminal"
  homepage "https://github.com/niklas-heer/focal"

  livecheck do
    url "https://github.com/niklas-heer/focal/releases/latest/download/appcast.xml"
    strategy :sparkle
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: :sonoma

  app "Focal.app"
  binary "#{appdir}/Focal.app/Contents/MacOS/focal"

  zap trash: [
    "~/Library/Application Support/Focal",
    "~/Library/Caches/com.niklasheer.focal",
    "~/Library/HTTPStorages/com.niklasheer.focal",
    "~/Library/Preferences/com.niklasheer.focal.plist",
  ]
end
