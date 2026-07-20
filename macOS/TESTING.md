### QuickShare auf macOS ausprobieren

#### 1. Rust Core bauen (einmalig)

```bash
cd /Users/atrium/Documents/\[2\]\ Freizeit/\[6\]\ Dev/QuickShare
cargo build --release --target aarch64-apple-darwin -p quickshare-core
cp target/aarch64-apple-darwin/release/libquickshare_core.a macOS/Bridge/
```

#### 2. Xcode Projekt generieren & öffnen

```bash
xcodegen generate   # erzeugt QuickShare.xcodeproj
open QuickShare.xcodeproj
```

Oder direkt über die generierte `.xcodeproj` öffnen.

#### 3. In Xcode: Schema > QuickShare auswählen, Build & Run (⌘R)

- **Menüleiste**: QuickShare Icon erscheint rechts in der Menüleiste
- **Empfang**: Automatische BLE/mDNS-Erkennung
- **Teilen-Menü**: Nach Installation erscheint QuickShare im systemweiten Teilen-Menü

#### 4. Oder nur den Daemon testen (ohne UI)

```bash
cd macOS
swift build -c release
cp com.quickshare.daemon.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.quickshare.daemon.plist
tail -f /tmp/quickshare-daemon.log
# → "QuickShare Daemon v0.1.0 starting..."
# → "Discovery: active"
# → "Advertising: active"
```
