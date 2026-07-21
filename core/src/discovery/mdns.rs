use std::net::IpAddr;
use std::sync::Arc;
use anyhow::{Result, Context};
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use tokio::sync::{Mutex, mpsc};
use tracing::{info, error, warn, debug};

use super::utils::{
    gen_mdns_name, gen_mdns_endpoint_info, parse_mdns_endpoint_info, is_not_self_ip, DeviceType,
};

/// Google QuickShare / Nearby Share mDNS service type.
pub const QUICKSHARE_SERVICE_TYPE: &str = "_FC9F5ED42C8A._tcp.local.";

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    is_discovering: Arc<Mutex<bool>>,
    is_advertising: Arc<Mutex<bool>>,
    current_service: Arc<Mutex<Option<ServiceInfo>>>,
    ble_receiver: Arc<Mutex<Option<mpsc::Receiver<()>>>>,
    re_broadcast_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl MdnsDiscovery {
    pub fn new(ble_receiver: Option<mpsc::Receiver<()>>) -> Result<Self> {
        let daemon = ServiceDaemon::new()
            .context("Failed to create mDNS ServiceDaemon")?;

        Ok(Self {
            daemon,
            is_discovering: Arc::new(Mutex::new(false)),
            is_advertising: Arc::new(Mutex::new(false)),
            current_service: Arc::new(Mutex::new(None)),
            ble_receiver: Arc::new(Mutex::new(ble_receiver)),
            re_broadcast_handle: Arc::new(Mutex::new(None)),
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

        info!("Starting mDNS discovery for service: {}", QUICKSHARE_SERVICE_TYPE);

        let receiver = self.daemon.browse(QUICKSHARE_SERVICE_TYPE)
            .context("Failed to start mDNS browsing for QuickShare")?;

        *discovering = true;
        let is_discovering_clone = self.is_discovering.clone();

        tokio::spawn(async move {
            info!("mDNS discovery event loop started");

            while *is_discovering_clone.lock().await {
                match receiver.recv_async().await {
                    Ok(ServiceEvent::SearchStarted(service_type)) => {
                        info!("mDNS SearchStarted: '{}'", service_type);
                    }
                    Ok(ServiceEvent::SearchStopped(service_type)) => {
                        // Do NOT break — this is a normal event, not an error
                        warn!("mDNS SearchStopped for '{}', continuing...", service_type);
                    }
                    Ok(ServiceEvent::ServiceFound(fullname, info)) => {
                        debug!("mDNS ServiceFound: fullname='{}' info={:?}", fullname, info);
                    }
                    Ok(ServiceEvent::ServiceRemoved(fullname, info)) => {
                        debug!("mDNS ServiceRemoved: fullname='{}' info={:?}", fullname, info);
                    }
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        let port = info.get_port();
                        let fullname = info.get_fullname();
                        let hostname = info.get_hostname();

                        info!(
                            "mDNS ServiceResolved: fullname='{}' hostname='{}' port={} addresses_v4={:?}",
                            fullname, hostname, port, info.get_addresses_v4()
                        );

                        // Log all TXT properties
                        for prop in info.get_properties().iter() {
                            debug!("  TXT property: key='{}' val='{}'", prop.key(), prop.val_str());
                        }

                        let addrs = info.get_addresses_v4();
                        if addrs.is_empty() {
                            warn!("mDNS resolved but no IPv4 addresses, skipping");
                            continue;
                        }
                        let ip = match addrs.iter().next() {
                            Some(v4) => std::net::IpAddr::V4(**v4),
                            None => {
                                warn!("mDNS resolved but IPv4 address is null, skipping");
                                continue;
                            }
                        };

                        if !is_not_self_ip(&ip) {
                            info!("mDNS resolved self IP {}, skipping", ip);
                            continue;
                        }

                        let n_property = match info.get_property("n") {
                            Some(prop) => prop,
                            None => {
                                warn!("mDNS service resolved without 'n' property, skipping");
                                continue;
                            }
                        };

                        let (_device_type, device_name) = match parse_mdns_endpoint_info(n_property.val_str()) {
                            Ok(parsed) => parsed,
                            Err(e) => {
                                warn!("Failed to parse endpoint info from '{}': {}", n_property.val_str(), e);
                                continue;
                            }
                        };

                        let ip_port = format!("{}:{}", ip, port);
                        info!(
                            "mDNS device resolved: '{}' (type={:?}) at {}",
                            device_name, _device_type, ip_port
                        );

                        on_device_found(device_name, hostname.to_string(), ip.to_string(), port);
                    }
                    Err(e) => {
                        error!("mDNS discovery recv error: {:?}", e);
                        // Don't break on transient errors, just continue
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }

            info!("mDNS discovery event loop ended");
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

    pub async fn start_advertising(
        &self,
        device_name: &str,
        port: u16,
        device_type: DeviceType,
    ) -> Result<()> {
        let mut advertising = self.is_advertising.lock().await;
        if *advertising {
            info!("Already advertising, skipping");
            return Ok(());
        }

        let mut endpoint_id = [0u8; 4];
        rand::Rng::fill(&mut rand::thread_rng(), &mut endpoint_id);

        let mdns_name = gen_mdns_name(endpoint_id);
        let endpoint_info = gen_mdns_endpoint_info(device_type, device_name);

        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "quickshare.local".to_string());

        // Resolve local IPv4 address explicitly for Android compatibility
        let ipv4_addr = get_if_addrs::get_if_addrs()
            .ok()
            .and_then(|addrs| {
                addrs.into_iter()
                    .filter_map(|iface| match iface.ip() {
                        IpAddr::V4(v4) if !v4.is_loopback() && !v4.is_unspecified() => Some(v4),
                        _ => None,
                    })
                    .next()
            })
            .map(|v4| v4.to_string())
            .unwrap_or_else(|| "0.0.0.0".to_string());

        info!(
            "mDNS advertising: name='{}' device='{}' port={} type={:?} addr='{}' hostname='{}'",
            mdns_name, device_name, port, device_type, ipv4_addr, hostname
        );

        // Log all network interfaces for debugging
        if let Ok(ifaces) = get_if_addrs::get_if_addrs() {
            for iface in &ifaces {
                debug!("  network interface: name='{}' ip={}", iface.name, iface.ip());
            }
        }

        let properties = [("n", endpoint_info.as_str())];
        info!("mDNS service properties: n='{}'", endpoint_info);

        let service_info = ServiceInfo::new(
            QUICKSHARE_SERVICE_TYPE,
            &mdns_name,
            &hostname,
            &ipv4_addr,
            port,
            &properties[..],
        )
        .context("Failed to build mDNS ServiceInfo")?
        .enable_addr_auto();

        info!("Registering mDNS service: fullname='{}'", service_info.get_fullname());

        self.daemon.register(service_info.clone())
            .context("Failed to register mDNS service")?;

        info!("mDNS service registered successfully");

        *self.current_service.lock().await = Some(service_info.clone());
        *advertising = true;

        // Spawn re-broadcast task if BLE receiver is available
        let ble_receiver = self.ble_receiver.lock().await.take();
        if let Some(mut receiver) = ble_receiver {
            let daemon = self.daemon.clone();
            let is_advertising = self.is_advertising.clone();
            let service_fullname = service_info.get_fullname().to_string();
            let service_info_clone = service_info;

            let handle = tokio::spawn(async move {
                info!("mDNS re-broadcast task started, waiting for BLE events...");
                while let Some(()) = receiver.recv().await {
                    if !*is_advertising.lock().await {
                        info!("Advertising stopped, re-broadcast task exiting");
                        break;
                    }

                    info!("BLE event received, re-broadcasting mDNS service (fullname='{}')", service_fullname);
                    if let Ok(rx) = daemon.unregister(&service_fullname) {
                        let _ = rx.recv();
                        debug!("Unregistered old mDNS service");
                    }
                    match daemon.register(service_info_clone.clone()) {
                        Ok(_) => info!("mDNS service re-registered successfully"),
                        Err(e) => error!("Failed to re-register mDNS service: {}", e),
                    }
                }
                info!("mDNS re-broadcast task ended");
            });

            *self.re_broadcast_handle.lock().await = Some(handle);
        }

        Ok(())
    }

    pub async fn stop_advertising(&self) -> Result<()> {
        let mut advertising = self.is_advertising.lock().await;
        if !*advertising {
            return Ok(());
        }

        info!("Stopping mDNS advertising...");

        if let Some(handle) = self.re_broadcast_handle.lock().await.take() {
            handle.abort();
        }

        let mut current_service = self.current_service.lock().await;
        if let Some(service) = current_service.take() {
            if let Ok(rx) = self.daemon.unregister(service.get_fullname()) {
                let _ = rx.recv();
            }
        }

        *advertising = false;
        Ok(())
    }

    pub async fn is_discovering(&self) -> bool {
        *self.is_discovering.lock().await
    }

    pub async fn is_advertising(&self) -> bool {
        *self.is_advertising.lock().await
    }
}
