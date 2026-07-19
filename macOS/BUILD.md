# macOS QuickShare – Build & Integration Guide

## Prerequisites

- macOS 14+ (Sonoma)
- Xcode 15+
- Rust toolchain with `aarch64-apple-darwin` target

## Project Structure

```
macOS/
├── Bridge/                          # Rust ↔ Swift Bridge
│   ├── quickshare.h                 # C-Header für FFI
│   ├── module.modulemap             # Swift Module Map
│   ├── libquickshare_core.a         # Rust static library (gebaut)
│   └── build.sh                     # Build-Skript
├── QuickShareApp/                   # SwiftUI Haupt-App
│   ├── QuickShareApp.swift          # App Entry Point (MenuBarExtra)
│   ├── QuickShareModel.swift        # @Observable ViewModel
│   ├── MenuBarView.swift            # Menu Bar Dropdown
│   ├── ContentView.swift            # Main Window (Devices, Transfers, Settings)
│   └── Info.plist
├── ShareExtension/                  # Teilen-Menü Extension
│   ├── ShareViewController.swift    # ViewController für Dateiempfang
│   └── ShareExtension-Info.plist
├── ControlCenterWidget/             # Control Center / MenuBar Toggle
│   └── ControlCenterWidget.swift
├── Sources/
│   ├── QuickShareBridge/            # Swift Bridge Wrapper
│   │   └── Bridge.swift
│   └── QuickShareDaemon/            # LaunchD Background Daemon
│       └── main.swift
├── com.quickshare.daemon.plist      # LaunchAgent plist
├── Package.swift                    # SwiftPM Package (Daemon)
└── build.sh                         # Build-Skript
```

## Quick Build

### 1. Rust Core kompilieren

```bash
cd core && cargo build --release --target aarch64-apple-darwin
cp target/aarch64-apple-darwin/release/libquickshare_core.a ../macOS/Bridge/
```

### 2. Swift Daemon bauen

```bash
cd macOS
swift build -c release
# Output: .build/release/quickshare-daemon
```

### 3. LaunchAgent installieren

```bash
cp com.quickshare.daemon.plist ~/Library/LaunchAgents/
launchctl load ~/Library/LaunchAgents/com.quickshare.daemon.plist
```

### 4. Xcode Projekt für die App

1. Xcode → New Project → macOS → App (SwiftUI)
2. Bundle Identifier: `com.quickshare.app`
3. Swift-Dateien aus `QuickShareApp/` hinzufügen
4. Build Phases:
   - Link Binary With Libraries: `libquickshare_core.a` hinzufügen
   - Library Search Paths: `$(SRCROOT)/Bridge`
5. Build Settings:
   - Objective-C Bridging Header: `$(SRCROOT)/Bridge/quickshare.h`
   - Other Linker Flags: `-lquickshare_core`
6. Capabilities:
   - Bluetooth → ON
   - Network → Incoming/Outgoing
   - Bonjour → `_quickshare._tcp`

## Integrationen

### Teilen-Menü (Share Extension)

Die Share Extension wird als separates Target im Xcode-Projekt angelegt:
- Target Type: `Share Extension` (Appex)
- Principal Class: `ShareViewController`
- Activation Rule: Dateien beliebiger Extension

### Control Center Toggle

macOS 14+ erlaubt keine öffentliche Control Center API für Drittanbieter.
Die App verwendet stattdessen:
- `MenuBarExtra` (SwiftUI) – für das Hauptmenü
- `NSStatusItem` – für den Toggle (funktioniert identisch zum CC)

## LaunchD Daemon

Der Hintergrunddienst wird über ein LaunchAgent plist gestartet:

```bash
# Einmalig starten
launchctl load ~/Library/LaunchAgents/com.quickshare.daemon.plist

# Status prüfen
launchctl list com.quickshare.daemon

# Stoppen
launchctl unload ~/Library/LaunchAgents/com.quickshare.daemon.plist
```

Logs: `/tmp/quickshare-daemon.log`
