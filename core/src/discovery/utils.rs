use std::net::IpAddr;
use anyhow::Result;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use get_if_addrs::get_if_addrs;
use rand::Rng;

/// Device type values matching Google QuickShare protocol.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum DeviceType {
    Unknown = 0,
    Phone = 1,
    Tablet = 2,
    Laptop = 3,
}

impl DeviceType {
    pub fn from_raw(value: u8) -> Self {
        match value {
            1 => DeviceType::Phone,
            2 => DeviceType::Tablet,
            3 => DeviceType::Laptop,
            _ => DeviceType::Unknown,
        }
    }
}

/// Generate the mDNS service name in the format expected by Google QuickShare.
///
/// Format: 11 bytes base64url-encoded
///   [0x23, endpoint_id[0..4], 0xFC, 0x9F, 0x5E, 0x00, 0x00]
pub fn gen_mdns_name(endpoint_id: [u8; 4]) -> String {
    let mut name = Vec::with_capacity(11);
    name.push(0x23);
    name.extend_from_slice(&endpoint_id);
    name.extend_from_slice(&[0xFC, 0x9F, 0x5E]);
    name.extend_from_slice(&[0x00, 0x00]);
    URL_SAFE_NO_PAD.encode(&name)
}

/// Generate the endpoint info for the mDNS TXT record "n" property.
///
/// Format: base64url-encoded
///   [device_type << 1, random[16], name_len, name_bytes...]
pub fn gen_mdns_endpoint_info(device_type: DeviceType, device_name: &str) -> String {
    let mut record = Vec::new();
    record.push((device_type as u8) << 1);

    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 16] = rng.r#gen();
    record.extend_from_slice(&random_bytes);

    let name_bytes = device_name.as_bytes();
    record.push(name_bytes.len() as u8);
    record.extend_from_slice(name_bytes);

    URL_SAFE_NO_PAD.encode(&record)
}

/// Parse the endpoint info from the mDNS TXT record "n" property.
pub fn parse_mdns_endpoint_info(encoded: &str) -> Result<(DeviceType, String)> {
    let decoded = URL_SAFE_NO_PAD.decode(encoded)?;

    if decoded.len() < 19 {
        anyhow::bail!("Invalid endpoint info: too short ({} bytes)", decoded.len());
    }

    let device_type = (decoded[0] >> 1) & 0x7;
    let name_len = decoded[17] as usize;

    if 18 + name_len > decoded.len() {
        anyhow::bail!(
            "Invalid endpoint info: name length {} exceeds data ({} bytes available)",
            name_len,
            decoded.len() - 18
        );
    }

    let name = String::from_utf8(decoded[18..18 + name_len].to_vec())?;
    Ok((DeviceType::from_raw(device_type), name))
}

/// Check if the given IP is NOT a local interface IP.
pub fn is_not_self_ip(ip: &IpAddr) -> bool {
    if let Ok(ifaces) = get_if_addrs() {
        for iface in ifaces {
            if iface.ip() == *ip {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gen_and_parse_endpoint_info() {
        let device_name = "My MacBook Pro";
        let device_type = DeviceType::Laptop;

        let info = gen_mdns_endpoint_info(device_type, device_name);
        let (parsed_type, parsed_name) = parse_mdns_endpoint_info(&info).unwrap();

        assert_eq!(parsed_type, device_type);
        assert_eq!(parsed_name, device_name);
    }

    #[test]
    fn test_gen_mdns_name() {
        let endpoint_id = [0x01, 0x02, 0x03, 0x04];
        let name = gen_mdns_name(endpoint_id);

        let decoded = URL_SAFE_NO_PAD.decode(&name).unwrap();
        assert_eq!(decoded.len(), 10);
        assert_eq!(decoded[0], 0x23);
        assert_eq!(&decoded[5..8], &[0xFC, 0x9F, 0x5E]);
        assert_eq!(&decoded[8..10], &[0x00, 0x00]);
    }
}
