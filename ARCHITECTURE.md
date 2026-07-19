# QuickShare – Architekturplan

## Ziel

Eine native QuickShare-App für macOS und Linux, die Googles Nearby Share / QuickShare Protokoll implementiert und sich nahtlos in die jeweilige Desktop-Umgebung einfügt.

---

## 1. Überblick: Google QuickShare Protokoll

QuickShare (ehemals Nearby Share) nutzt:

- **Bluetooth LE** – Geräteerkennung & Advertising
- **mDNS / DNS-SD** – Lokale Netzwerkerkennung
- **Wi-Fi Direct / Wi-Fi Aware** – Direkte P2P-Datenübertragung
- **WebRTC** – Fallback über Internet (via Google-TURN-Server)
- **Certificate Voucher** – Authentifizierung über Key-Austausch
- **Encryption** – AES-GCM verschlüsselte Übertragung

*Referenz: [google/nearby](https://github.com/google/nearby) (C++, Referenzimplementierung)*

---

## 2. Gesamtarchitektur

```
┌─────────────────────────────────────────────────────────────────┐
│                   Desktop QuickShare App                        │
│                                                                 │
│  ┌──────────────┐  ┌──────────────────┐  ┌──────────────────┐  │
│  │ macOS UI      │  │ Linux UI (GTK4)  │  │ CLI/TUI          │  │
│  │ (SwiftUI)     │  │ (libadwaita)     │  │ (optional)       │  │
│  └──────┬───────┘  └────────┬─────────┘  └────────┬─────────┘  │
│         │                   │                      │            │
│  ┌──────┴───────────────────┴──────────────────────┴─────────┐  │
│  │              Rust Core Library (quickshare-core)           │  │
│  │  • Protokoll-Implementierung                               │  │
│  │  • BLE / mDNS / WiFi-Direct / WebRTC                      │  │
│  │  • File-Transfer-Management                                │  │
│  │  • C-FFI-Bridge für alle Plattformen                       │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │              Plattform-Services (je nach OS)               │  │
│  │  • D-Bus-Daemon (Linux) / XPC-Service (macOS)             │  │
│  │  • Hintergrundprozess für permanentes Advertising          │  │
│ │  • Datei-Empfang & -Freigabe                               │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### Sprachentscheidungen

| Komponente | Sprache | Begründung |
|---|---|---|
| Core (Protokoll) | **Rust** | Speichersicher, hochperformant, plattformunabhängig |
| Linux UI | **C** (GTK4/libadwaita) | Native GNOME-Integration, libadwaita-idiomatisch |
| macOS UI | **Swift/SwiftUI** | Einzige natuv Option für macOS-Integration |
| Nautilus-Plugin | **Python** (nautilus-python) | Standard-Plugin-API |
| GNOME Control Center | **C/Python** | Pluginsystem des GNOME CC |
| Core-FFI | **C-ABI** | Rust → Swift/GTK über C-Bridge |

---

## 3. Rust Core Library (`quickshare-core`)

### 3.1. Abhängigkeiten

```toml
[package]
name = "quickshare-core"
edition = "2024"

[dependencies]
# Bluetooth
btleplug = "0.11"         # Cross-Plattform BLE (macOS, Linux)
blez = "0.4"               # Alternative: Rust BLE

# Netzwerk
mdns-sd = "0.13"           # mDNS Service Discovery
webrtc = "0.11"            # WebRTC-Stack (P2P-Fallback)
async-tungstenite = "0.28" # WebSocket

# Kryptografie
aes-gcm = "0.10"           # Verschlüsselung
x25519-dalek = "2.0"       # Schlüsselaustausch
ed25519-dalek = "2.0"      # Signing
rand = "0.8"               # Zufallswerte
prost = "0.13"             # Protobuf (Protokoll-Messages)

# Async
tokio = { version = "1", features = ["full"] }
anyhow = "1"
tracing = "0.1"

# FFI
libc = "0.2"
```

### 3.2. Modulstruktur

```
quickshare-core/
├── src/
│   ├── lib.rs                 # Public API, re-exports
│   ├── protocol/
│   │   ├── mod.rs
│   │   ├── frame.rs           # Nearby-Frame-Encoding
│   │   ├── connection.rs      # Connection-Handshake
│   │   ├── payload.rs         # Payload-Transfer
│   │   └── proto/             # Generierte Protobuf-Typen
│   │       ├── advertising.rs
│   │       ├── credentials.rs
│   │       └── connections.proto
│   ├── discovery/
│   │   ├── mod.rs
│   │   ├── ble.rs             # BLE Advertising & Scanning
│   │   └── mdns.rs            # mDNS Service Discovery
│   ├── transfer/
│   │   ├── mod.rs
│   │   ├── sender.rs          # Datei senden
│   │   ├── receiver.rs        # Datei empfangen
│   │   └── encryption.rs      # AES-GCM Payload-Encryption
│   ├── platform/
│   │   ├── mod.rs
│   │   ├── linux.rs           # Linux-spezifisch (Wi-Fi Direct)
│   │   └── macos.rs           # macOS-spezifisch
│   └── ffi/
│       ├── mod.rs
│       ├── ffi.rs             # C-kompatible Exporte
│       └── types.rs           # FFI-sichere Typen
```

### 3.3. Public C-FFI API

```c
// Initialisierung & Lebenszyklus
int qs_init(const char* device_name, qs_log_cb log_callback);
void qs_shutdown(void);

// Discovery
int qs_start_advertising(void);
int qs_stop_advertising(void);
int qs_start_discovery(void);
int qs_stop_discovery(void);

// Events (callbacks)
typedef void (*qs_device_found_cb)(const char* device_id,
                                    const char* device_name,
                                    qs_connection_type conn_type);
typedef void (*qs_transfer_cb)(const char* transfer_id,
                                const char* device_id,
                                qs_transfer_status status,
                                int64_t bytes_sent,
                                int64_t bytes_total);

void qs_set_device_found_callback(qs_device_found_cb cb);
void qs_set_transfer_callback(qs_transfer_cb cb);

// Dateien senden/empfangen
int qs_send_file(const char* device_id, const char* file_path);
int qs_accept_transfer(const char* transfer_id, const char* save_path);
int qs_reject_transfer(const char* transfer_id);

// Status
bool qs_is_advertising(void);
bool qs_is_discovering(void);
```

---

## 4. Linux-Integration

### 4.1. Komponenten

```
┌─────────────────────────────────────────────────────────┐
│                   Linux / GNOME                           │
│                                                          │
│  ┌────────────────┐  ┌──────────────────────────────┐   │
│  │ GNOME CC Panel  │  │ Nautilus Extension           │   │
│  │ (Toggle On/Off) │  │ (Rechtsklick → Teilen)      │   │
│  └───────┬─────────┘  └──────────┬───────────────────┘   │
│          │                       │                        │
│  ┌───────┴───────────────────────┴───────────────────┐   │
│  │           quickshare-daemon (D-Bus)                │   │
│  │  • Systemd-User-Service (org.quickshare.Daemon)    │   │
│  │  • BLE-Advertising (Hintergrund)                   │   │
│  │  • Transfer-Queue                                  │   │
│  └───────────────────┬───────────────────────────────┘   │
│                      │                                    │
│  ┌───────────────────┴───────────────────────────────┐   │
│  │           quickshare-core (Rust lib)               │   │
│  └───────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### 4.2. GNOME Control Center Panel

- **Pfad**: `panels/quickshare/`
- Plugin für `gnome-control-center`
- Ein/Aus-Schalter mit Status-Anzeige
- Einstellungen: Empfangsordner, Gerätename, Sichtbarkeit
- Technik: C + libadwaita, `GtkSwitch` → D-Bus-Aufruf an Daemon

### 4.3. Nautilus Extension

- **Pfad**: `nautilus-quickshare/`
- Python-Plugin via `nautilus-python`
- Erscheint bei Rechtsklick auf Dateien als "Teilen via QuickShare"
- Öffnet Dateiauswahl für Zielgerät oder leitet direkt an UI weiter
- `extension.py` + `metadata.json`

### 4.4. GNOME Shell Quick Settings / Toggle

Zwei Optionen:
1. **GNOME Shell Extension** – Custom Toggle im Quick Settings Menü
2. **Systemd-Dienst** – D-Bus-aktivierter Daemon

Bevorzugt: Option 1 (bessere UX)

### 4.5. Build-System (Linux)

```bash
# Native Pakete (Meson, GNOME-konform)
quickshare-daemon/     # meson.build
nautilus-quickshare/   # setup.py + meson
gnome-quickshare-panel/ # meson.build
quickshare-shell-toggle/ # Makefile + schemas
```

---

## 5. macOS-Integration

### 5.1. Komponenten

```
┌──────────────────────────────────────────────────────────┐
│                     macOS                                 │
│                                                           │
│  ┌────────────────────┐  ┌──────────────────────────┐    │
│  │ QuickShare.app     │  │ Control Center Toggle     │   │
│  │ (MenuBar + UI)     │  │ (Banner im CC-Menü)       │   │
│  └────────┬───────────┘  └──────────┬───────────────┘    │
│           │                         │                     │
│  ┌────────┴─────────────────────────┴─────────────────┐  │
│  │           Share Extension (Appex)                   │  │
│  │  • NSSharingServiceProvider                         │  │
│  │  • Erscheint im systemweiten Teilen-Menü           │  │
│  └───────────────────────┬────────────────────────────┘  │
│                          │                                │
│  ┌───────────────────────┴────────────────────────────┐  │
│  │           XPC Service (quickshare.xpc)              │  │
│  │  • LaunchD-Daemon (hintergrund)                    │  │
│  │  • BLE-Advertising                                  │  │
│  ⌄  • Transfer-Queue                                   │  │
│  └───────────────────────┬────────────────────────────┘  │
│                          │                                │
│  ┌───────────────────────┴────────────────────────────┐  │
│  │           quickshare-core (Rust .dylib via FFI)    │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### 5.2. QuickShare.app – Hauptapp

- **SwiftUI** App mit:
  - Menübar-Icon (NSStatusItem)
  - Empfangsverlauf
  - Geräte in der Nähe anzeigen
  - Einstellungen (Empfangsordner, Gerätename)
  - Drag-and-Drop-Fenster

### 5.3. Share Extension (Teilen-Menü)

- **Typ**: `NSSharingServiceProvider` Subclass
- Registriert als `NSExtension` in `Info.plist`
- Nimmt Dateien/URLs entgegen → leitet an QuickShare-App/XPC weiter
- Erscheint in: Finder, Fotos, Safari etc.

### 5.4. Control Center Toggle

- **Typ**: `NSControlCenterWidget` (macOS 14+ Sonoma)
- Oder alternativ als `NSStatusItem` im Menubar
- Ein/Aus-Schalter mit Status (Empfangen aktiv?)
- Zeigt aktuelle Transfers an

### 5.5. Build-System (macOS)

```bash
# Xcode workspace
QuickShare.xcworkspace/
├── QuickShare/              # Haupt-App (SwiftUI)
├── ShareExtension/          # Appex
├── ControlCenterWidget/     # Appex oder Teil der App
├── XPCService/              # Launch-Daemon
└── Cargo build script       # Rust-Library via cargo-xcode
```

---

## 6. Datentransfer & Protokoll-Implementierung

### 6.1. QuickShare Protokoll-Ablauf

```
Sender                              Empfänger
  │                                     │
  │─── BLE Advertisement (hash) ───────>│
  │<── BLE Scan Response (hash) ────────│
  │                                     │
  │─── mDNS Query (device name) ───────>│
  │<── mDNS Response ───────────────────│
  │                                     │
  │─── TCP Connection Request ────────>│ (Wi-Fi Direct oder LAN)
  │<── TLS Handshake ──────────────────│
  │─── Certificate Exchange ──────────>│
  │<── Certificate Verification ───────│
  │                                     │
  │─── Payload Header (Datei-Meta) ───>│
  │<── Payload Accept ─────────────────│
  │─── Encrypted Payload ──────────────>│
  │<── Payload Complete ───────────────│
```

### 6.2. Protobuf-Definitionen

Die Nearby-Share-Frames sind als Protobuf definiert. Wichtigste Messages:

```protobuf
// Advertising Frame
message AdvertisementFrame {
  string device_name = 1;
  bytes device_id_hash = 2;
  enum DeviceType { PHONE = 0; TABLET = 1; LAPTOP = 2; DESKTOP = 3; }
  DeviceType device_type = 3;
}

// Connection Frame
message ConnectionRequestFrame {
  string endpoint_id = 1;
  bytes certificate = 2;
}

// Payload Transfer
message PayloadTransferFrame {
  string payload_id = 1;
  oneof PayloadContent {
    FileInfo file_info = 2;
    bytes bytes_payload = 3;
  }
  message FileInfo {
    string file_name = 1;
    int64 file_size = 2;
    string mime_type = 3;
  }
}
```

---

## 7. Meilensteine & Implementierungsreihenfolge

| Phase | Inhalt | Dauer (geschätzt) |
|---|---|---|
| **P0** | Rust-Core: BLE-Discovery + mDNS, Send/Empfang unverschlüsselt | 4 Wochen |
| **P1** | Rust-Core: Verschlüsselung, vollständiger Handshake | 2 Wochen |
| **P2** | Linux: D-Bus-Daemon + CLI-Tool | 2 Wochen |
| **P3** | Linux: GNOME CC Panel | 1 Woche |
| **P4** | Linux: Nautilus Extension | 1 Woche |
| **P5** | macOS: XPC-Service + Rust-Bridge | 2 Wochen |
| **P6** | macOS: QuickShare.app (SwiftUI) | 2 Wochen |
| **P7** | macOS: Share Extension | 1 Woche |
| **P8** | macOS: Control Center Toggle | 1 Woche |
| **P9** | Linux: GNOME Shell Quick Settings Toggle | 1 Woche |
| **P10** | Testing, Paketierung, CI/CD | 2 Wochen |

**Gesamt: ~19 Wochen**

---

## 8. Offene Fragen & Risiken

1. **BLE-Kompatibilität**: macOS BLE (CoreBluetooth) vs. Linux BLE (BlueZ) – API-Unterschiede signifikant
2. **Wi-Fi Direct auf macOS**: macOS unterstützt kein Wi-Fi Direct – Fallback auf LAN + mDNS nötig
3. **Google-TURN-Server**: Für WebRTC-Fallback werden Google-Server benötigt – keine öffentliche Spezifikation
4. **Wi-Fi Aware**: Nicht auf Desktop-Plattformen verfügbar – ignorieren
5. **Apple Silicon**: Rosetta für Rust-Builds testen
6. **macOS Sandbox**: Share Extension benötigt App-Sandbox, Dateizugriff ist eingeschränkt (Powerbox)
7. **Android-Kompatibilität**: Desktop muss mit Android-Geräten kompatibel sein → Protokoll-konform implementieren

---

## 9. Verifizierung

### Android-Kompatibilitätstests
- Senden von Android → Linux/macOS
- Senden von Linux/macOS → Android
- Empfangen von Android auf beiden Plattformen

### Integrationstests
- Nautilus Rechtsklick → Datei senden
- macOS Teilen-Menü → Datei senden
- GNOME CC Ein/Aus → Daemon reagiert
- macOS CC Ein/Aus → XPC reagiert

---

## 10. Referenzen

- [Google Nearby (Referenz-Implementierung)](https://github.com/google/nearby)
- [NearDrop (macOS-Empfang only)](https://github.com/grishka/NearDrop)
- [localsend (Cross-Plattform, eigenes Protokoll)](https://github.com/localsend/localsend)
- [nautilus-python Dokumentation](https://wiki.gnome.org/Projects/NautilusPython)
- [Apple NSSharingServiceProvider](https://developer.apple.com/documentation/appkit/nssharingserviceprovider)
- [GNOME Control Center Panel Entwicklung](https://developer.gnome.org/gnome-control-center/)
