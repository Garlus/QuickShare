fn main() -> Result<(), Box<dyn std::error::Error>> {
    prost_build::compile_protos(
        &[
            "proto/ukey.proto",
            "proto/securemessage.proto",
            "proto/device_to_device_messages.proto",
            "proto/offline_wire_formats.proto",
            "proto/wire_format.proto",
        ],
        &["proto/"],
    )?;
    Ok(())
}
