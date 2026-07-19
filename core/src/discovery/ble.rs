use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, Context, anyhow};
use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Manager, Adapter};
use tokio::sync::Mutex;
use tracing::{info, warn, error};

// Google Nearby Share / QuickShare standard 16-bit Service UUID
pub const NEARBY_SHARE_SERVICE_UUID_16: u16 = 0xFEF3;

pub struct BleDiscovery {
    adapter: Option<Adapter>,
    is_scanning: Arc<Mutex<bool>>,
    is_advertising: Arc<Mutex<bool>>,
}

impl BleDiscovery {
    pub async fn new() -> Result<Self> {
        let manager = Manager::new().await
            .context("Failed to initialize btleplug BLE Manager")?;
        
        let adapters = manager.adapters().await
            .context("Failed to get Bluetooth adapters")?;
        
        let adapter = adapters.into_iter().next();
        if adapter.is_none() {
            warn!("No Bluetooth adapters found. BLE discovery and advertising will be disabled.");
        } else {
            info!("Successfully initialized Bluetooth adapter.");
        }

        Ok(Self {
            adapter,
            is_scanning: Arc::new(Mutex::new(false)),
            is_advertising: Arc::new(Mutex::new(false)),
        })
    }

    pub async fn start_scanning<F>(&self, on_device_found: F) -> Result<()>
    where
        F: FnMut(String, String, Vec<u8>) + Send + 'static,
    {
        let adapter = self.adapter.as_ref()
            .ok_or_else(|| anyhow!("No Bluetooth adapter available"))?;
        
        let mut scanning = self.is_scanning.lock().await;
        if *scanning {
            return Ok(());
        }

        info!("Starting BLE scanning for QuickShare...");
        
        // We can scan with a filter for the Nearby Share Service UUID (0xFEF3)
        // Note: some platforms require a specific UUID, some don't. We'll listen for everything or filter.
        adapter.start_scan(ScanFilter::default()).await
            .context("Failed to start BLE scan")?;
        
        *scanning = true;

        // Monitor scan results in a separate task
        let adapter_clone = adapter.clone();
        let is_scanning_clone = self.is_scanning.clone();
        let mut on_device_found = on_device_found;

        tokio::spawn(async move {
            while *is_scanning_clone.lock().await {
                tokio::time::sleep(Duration::from_secs(2)).await;
                match adapter_clone.peripherals().await {
                    Ok(peripherals) => {
                        for peripheral in peripherals {
                            if let Ok(Some(properties)) = peripheral.properties().await {
                                // Extract service data matching Nearby Share UUID
                                for (uuid, data) in &properties.service_data {
                                    // Check if the UUID matches 0xFEF3 (Nearby Share)
                                    // UUIDs can be 16-bit, 32-bit or 128-bit.
                                    let is_nearby = uuid.to_string().contains("fef3") || 
                                                     uuid.to_string().contains("FEF3");
                                    
                                    if is_nearby {
                                        let name = properties.local_name
                                            .clone()
                                            .unwrap_or_else(|| "Unknown QuickShare Device".to_string());
                                        let id = properties.address.to_string();
                                        info!("Found QuickShare device via BLE: {} ({}) with data: {:?}", name, id, data);
                                        on_device_found(id, name, data.clone());
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Error fetching BLE peripherals: {:?}", e);
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop_scanning(&self) -> Result<()> {
        let adapter = self.adapter.as_ref()
            .ok_or_else(|| anyhow!("No Bluetooth adapter available"))?;
        
        let mut scanning = self.is_scanning.lock().await;
        if !*scanning {
            return Ok(());
        }

        info!("Stopping BLE scanning...");
        adapter.stop_scan().await
            .context("Failed to stop BLE scan")?;
        
        *scanning = false;
        Ok(())
    }

    pub async fn start_advertising(&self, device_name: &str) -> Result<()> {
        let _adapter = self.adapter.as_ref()
            .ok_or_else(|| anyhow!("No Bluetooth adapter available"))?;
        
        let mut advertising = self.is_advertising.lock().await;
        if *advertising {
            return Ok(());
        }

        info!("Starting BLE advertising as '{}'...", device_name);
        
        // Note: btleplug peripheral role is supported on macOS and Linux (BlueZ).
        // It allows creating a GATT server or advertising custom service data.
        // Google Nearby Share advertising payload format:
        // Byte 0: Version (usually 0x01)
        // Byte 1: Device Type & visibility
        // Byte 2..: Salt & private credentials hash
        
        // For a prototype, we build a compatible-like payload or custom metadata
        // In full Nearby Share, the advertisement contains the decrypted certificate verification metadata
        
        // TODO: Implement proper advertising payload format
        
        *advertising = true;
        Ok(())
    }

    pub async fn stop_advertising(&self) -> Result<()> {
        let mut advertising = self.is_advertising.lock().await;
        if !*advertising {
            return Ok(());
        }

        info!("Stopping BLE advertising...");
        // TODO: Implement stop peripheral advertising via btleplug
        
        *advertising = false;
        Ok(())
    }
}
