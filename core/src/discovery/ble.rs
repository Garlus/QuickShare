use std::sync::Arc;
use std::time::Duration;
use anyhow::{Result, Context, anyhow};
use btleplug::api::{Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::{Manager, Adapter};
use futures::stream::StreamExt;
use tokio::sync::Mutex;
use tokio::time;
use tracing::{info, warn, debug};
use uuid::Uuid;

/// Google QuickShare / Nearby Share BLE Service UUID (0xFE2C).
const QUICKSHARE_SERVICE_UUID: Uuid = Uuid::from_bytes([
    0x00, 0x00, 0xFE, 0x2C, 0x00, 0x00, 0x10, 0x00,
    0x80, 0x00, 0x00, 0x80, 0x5F, 0x9B, 0x34, 0xFB,
]);

pub struct BleDiscovery {
    adapter: Option<Adapter>,
    is_scanning: Arc<Mutex<bool>>,
}

impl BleDiscovery {
    pub async fn new() -> Result<Self> {
        let manager = Manager::new().await
            .context("Failed to initialize btleplug BLE Manager")?;

        let adapters = manager.adapters().await
            .context("Failed to get Bluetooth adapters")?;

        let adapter = adapters.into_iter().next();
        if adapter.is_none() {
            warn!("No Bluetooth adapters found. BLE discovery will be disabled.");
        } else {
            info!("Bluetooth adapter initialized successfully.");
        }

        Ok(Self {
            adapter,
            is_scanning: Arc::new(Mutex::new(false)),
        })
    }

    pub async fn start_scanning<F>(
        &self,
        on_device_found: F,
    ) -> Result<()>
    where
        F: FnMut(String, String, Vec<u8>) + Send + 'static,
    {
        let adapter = self.adapter.as_ref()
            .ok_or_else(|| anyhow!("No Bluetooth adapter available"))?;

        let mut scanning = self.is_scanning.lock().await;
        if *scanning {
            return Ok(());
        }

        info!("Starting BLE scanning for QuickShare (UUID: 0xFE2C)...");

        let mut events = adapter.events().await
            .context("Failed to get BLE event stream")?;

        adapter.start_scan(ScanFilter {
            services: vec![QUICKSHARE_SERVICE_UUID],
        }).await
            .context("Failed to start BLE scan")?;

        *scanning = true;
        let is_scanning_clone = self.is_scanning.clone();
        let mut on_device_found = on_device_found;

        tokio::spawn(async move {
            info!("BLE event listener started, scanning for QuickShare advertisements...");

            while *is_scanning_clone.lock().await {
                tokio::select! {
                    Some(event) = events.next() => {
                        match event {
                            CentralEvent::ServiceDataAdvertisement { id, service_data } => {
                                if !service_data.contains_key(&QUICKSHARE_SERVICE_UUID) {
                                    continue;
                                }

                                let data = service_data.get(&QUICKSHARE_SERVICE_UUID)
                                    .cloned()
                                    .unwrap_or_default();

                                info!(
                                    "BLE QuickShare advertisement: id='{}' data_len={} data={}",
                                    id, data.len(),
                                    data.iter().map(|b| format!("{:02X}", b)).collect::<String>()
                                );

                                let name = format!("BLE-{}", id);
                                on_device_found(id.to_string(), name, data);
                            }
                            CentralEvent::DeviceConnected(id) => {
                                debug!("BLE DeviceConnected: id='{}'", id);
                            }
                            CentralEvent::DeviceDisconnected(id) => {
                                debug!("BLE DeviceDisconnected: id='{}'", id);
                            }
                            CentralEvent::DeviceUpdated(id) => {
                                debug!("BLE DeviceUpdated: id='{}'", id);
                            }
                            CentralEvent::DeviceDiscovered(id) => {
                                debug!("BLE DeviceDiscovered: id='{}'", id);
                            }
                            CentralEvent::ServicesAdvertisement { id, services } => {
                                debug!("BLE ServicesAdvertisement: id='{}' services={:?}", id, services);
                            }
                            CentralEvent::ManufacturerDataAdvertisement { id, manufacturer_data } => {
                                debug!("BLE ManufacturerDataAdvertisement: id='{}' data={:?}", id, manufacturer_data);
                            }
                            CentralEvent::StateUpdate(state) => {
                                debug!("BLE StateUpdate: {:?}", state);
                            }
                        }
                    }
                    _ = time::sleep(Duration::from_millis(500)) => {}
                }
            }

            info!("BLE event listener stopped");
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

    pub async fn is_scanning(&self) -> bool {
        *self.is_scanning.lock().await
    }
}
