# QuickShare for macOS

A macOS implementation of Google QuickShare — send and receive files with Android devices over Wi-Fi and Bluetooth.

## How it works

QuickShare uses the same protocol as Google's Nearby Share / QuickShare to transfer files between macOS and Android. It discovers nearby devices via BLE advertisements and mDNS, then establishes an encrypted connection using the UKEY2 handshake protocol.

- **Send** — drag files onto the window, pick a device, done
- **Receive** — toggle "Receive on" in the toolbar, accept incoming transfers via popup
- Animated liquid glass background that shifts between blue (active) and pink (standby)

## Building

**Prerequisites:**
- macOS 14.0+
- Xcode 15+ with Command Line Tools
- Rust toolchain (stable)

```bash
# Build the Rust core
bash macOS/build.sh

# Generate Xcode project (requires xcodegen)
xcodegen generate

# Open in Xcode
open QuickShare.xcodeproj
```

## Tech stack

- **SwiftUI** — macOS native UI
- **Rust** — core protocol engine (UKEY2, AES-256-CBC, HMAC-SHA256)
- **Metal** — animated shader background
- [mdns-sd](https://github.com/keksour/mdns-sd) + [NWBrowser](https://developer.apple.com/networking/nwbrowser/) — device discovery
- [btleplug](https://github.com/deviceplug/btleplug) — BLE scanning

## Status

Early development. Basic protocol handshake with Android devices is implemented. File transfer in progress.
