fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Make the project self-contained for portfolio/demo use: prost still
    // needs protoc, but contributors should not need to install it manually.
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    std::env::set_var("PROTOC", protoc);

    tonic_build::compile_protos("proto/kademlia.proto")?;
    Ok(())
}
