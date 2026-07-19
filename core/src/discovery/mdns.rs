use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{Result, Context};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::Mutex;
use tracing::{info, error};

// Google Nearby Connections service ID or a standard service type for LAN
pub const QUICKSHARE_SERVICE_TYPE: &str = "_quickshare._tcp.local.";
pub const NEARBY_CONNECTIONS_SERVICE_TYPE: &str = "_fc9a5d41._tcp.local."; // Google's official nearby protocol service type

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    is_discovering: Arc<Mutex<bool>>,
    is_advertising: Arc<Mutex<bool>>,
    current_service: Arc<Mutex<Option<ServiceInfo>>>,
}

impl MdnsDiscovery {
    pub fn new() -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .context("Failed to create mDNS ServiceDaemon")?;
        
        Ok(Self {
            daemon,
            is_discovering: Arc::new(Mutex::new(false)),
            is_advertising: Arc::new(Mutex::new(false)),
            current_service: Arc::new(Mutex::new(None)),
        })
    }

    pub async fn start_discovery<F>(&self, mut on_device_found: F) -> Result<()>
    where
        F: FnMut(String, String, String, u16) + Send + 'static,
    {
        let mut discovering = self.is_discovering.lock().await;
        if *discovering {
            return Ok(());
        }

        info!("Starting mDNS discovery for service: {}...", QUICKSHARE_SERVICE_TYPE);
        
        // Browse for both our service and nearby connections service
        let receiver = self.daemon.browse(QUICKSHARE_SERVICE_TYPE)
            .context("Failed to start mDNS browsing for QuickShare")?;
        
        *discovering = true;
        let is_discovering_clone = self.is_discovering.clone();

        tokio::spawn(async move {
            while *is_discovering_clone.lock().await {
                match receiver.recv_async().await {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        let name = info.get_fullname().to_string();
                        let port = info.get_port();
                        
                        // Extract IP address (IPv4)
                        if let Some(addr) = info.get_addresses().iter().next() {
                            let ip = addr.to_string();
                            info!("mDNS device resolved: {} at {}:{}", name, ip, port);
                            on_device_found(name, info.get_hostname().to_string(), ip, port);
                        }
                    }
                    Ok(ServiceEvent::SearchStopped(_)) => {
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("mDNS discovery error: {:?}", e);
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    pub async fn stop_discovery(&self) -> Result<()> {
        let mut discovering = self.is_discovering.lock().await;
        if !*discovering {
            return Ok(());
        }

        info!("Stopping mDNS discovery...");
        self.daemon.stop_browse(QUICKSHARE_SERVICE_TYPE)
            .context("Failed to stop mDNS browsing")?;
        
        *discovering = false;
        Ok(())
    }

    pub async fn start_advertising(&self, device_name: &str, port: u16) -> Result<()> {
        let mut advertising = self.is_advertising.lock().await;
        if *advertising {
            return Ok(());
        }

        info!("Starting mDNS advertising as '{}' on port {}...", device_name, port);
        
        // Define TXT properties
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), device_name.to_string());
        properties.insert("version".to_string(), "1".to_string());
        properties.insert("type".to_string(), "desktop".to_string());

        // Create the service info
        // Note: hostname must end with .local.
        let hostname = format!("{}.local.", device_name.to_ascii_lowercase().replace(' ', "-"));
        let service_info = ServiceInfo::new(
            QUICKSHARE_SERVICE_TYPE,
            device_name,
            &hostname,
            "", // Address can be left empty for auto-resolve or use "0.0.0.0"
            port,
            properties,
        ).context("Failed to build mDNS ServiceInfo")?;

        self.daemon.register(service_info.clone())
            .context("Failed to register mDNS service")?;
        
        *self.current_service.lock().await = Some(service_info);
        *advertising = true;
        Ok(())
    }

    pub async fn stop_advertising(&self) -> Result<()> {
        let mut advertising = self.is_advertising.lock().await;
        if !*advertising {
            return Ok(());
        }

        info!("Stopping mDNS advertising...");
        let mut current_service = self.current_service.lock().await;
        if let Some(service) = current_service.take() {
            self.daemon.unregister(&service.get_fullname())
                .context("Failed to unregister mDNS service")?;
        }

        *advertising = false;
        Ok(())
    }
}
