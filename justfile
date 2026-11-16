set shell := ["zsh", "-c"]

app := "rune-soundcore"
bundle := "dist/Soundcore.app"

# Build the debug binary.
build:
    cargo build

# Run the app from source.
run:
    cargo run -- $@

# Build the release binary.
release:
    cargo build --release

# Assemble and ad-hoc sign a .app bundle.
bundle: release
    rm -rf "{{bundle}}"
    mkdir -p "{{bundle}}/Contents/MacOS" "{{bundle}}/Contents/Resources"
    cp target/release/{{app}} "{{bundle}}/Contents/MacOS/{{app}}"
    cp bundle/Info.plist "{{bundle}}/Contents/Info.plist"
    cp bundle/AppIcon.icns "{{bundle}}/Contents/Resources/AppIcon.icns"
    codesign --force --sign - --options runtime "{{bundle}}"
    @echo "built {{bundle}}"

# Render the UI headlessly to a PNG (uses a captured snapshot when given one).
snap:
    RUNE_SOUNDCORE_SNAPSHOT="$HOME/Library/Application Support/rune-soundcore/snapshot.json" cargo run -- --snap /tmp/rune-soundcore.png

# Redraw bundle/icon.png and rebuild bundle/AppIcon.icns from it.
icon:
    magick -size 824x824 gradient:'#33479f-#121430' /tmp/icon-bg.png
    magick -size 824x824 xc:black -fill white -draw 'roundrectangle 0,0 823,823 185,185' -transparent black /tmp/icon-mask.png
    magick /tmp/icon-bg.png /tmp/icon-mask.png -compose DstIn -composite /tmp/icon-body.png
    magick /tmp/icon-body.png -fill white -stroke none -draw "circle 400,400 540,400" -draw "roundrectangle 345,430 455,690 55,55" -fill '#1b2168' -draw "circle 400,400 462,400" -fill white -draw "circle 400,400 420,400" -stroke white -fill none -strokewidth 26 -draw "stroke-linecap round arc 230,230 570,570 310,50" -strokewidth 22 -draw "stroke-linecap round arc 160,160 640,640 310,50" -strokewidth 18 -draw "stroke-linecap round arc 90,90 710,710 310,50" -background none -gravity center -extent 1024x1024 -depth 8 PNG32:bundle/icon.png
    rm -f /tmp/icon-bg.png /tmp/icon-mask.png /tmp/icon-body.png
    mkdir -p bundle/AppIcon.iconset
    for s in 16 32 128 256 512; do sips -z $s $s bundle/icon.png --out bundle/AppIcon.iconset/icon_${s}x${s}.png >/dev/null; sips -z $((s*2)) $((s*2)) bundle/icon.png --out bundle/AppIcon.iconset/icon_${s}x${s}@2x.png >/dev/null; done
    iconutil -c icns bundle/AppIcon.iconset -o bundle/AppIcon.icns
    rm -rf bundle/AppIcon.iconset

# Connect to the first Soundcore device, print its settings, and quit.
probe:
    cargo run -- --probe

check:
    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test

# Run the unit tests.
test:
    cargo test
