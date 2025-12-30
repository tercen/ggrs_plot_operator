fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile proto files using tonic-build
    tonic_build::configure()
        .build_server(false) // Client only, no server code generation
        .build_transport(false) // Don't generate transport code (avoid naming conflicts)
        .compile(
            &["protos/tercen.proto", "protos/tercen_model.proto"],
            &["protos"],
        )?;

    Ok(())
}
