# QuickShare for macOS

A macOS implementation of Google QuickShare — send and receive files with Android devices over Wi-Fi and Bluetooth.

## Screenshots

![Main Window with Shader](Concept/App%20mit%20Shader.jpg)
*Main window with animated liquid glass background*

![Device Picker](Concept/Frame%202.png)
*Selecting a target device to send files*

![Sending in Progress](Concept/Frame%203.png)
*File transfer progress*

![Incoming Transfer](Concept/Frame%206.png)
*Receiving a file from an Android device*

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
- **NWBrowser / NetService** — mDNS discovery and advertising
- **btleplug** — BLE scanning
- **CoreBluetooth** — BLE advertising

## Status

Functional. Supports file transfer with Android devices over LAN (mDNS discovery, UKEY2 handshake, AES-256-CBC encryption).
