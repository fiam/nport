use std::io::Result;
fn main() -> Result<()> {
    prost_build::compile_protos(
        &["src/common.proto", "src/client.proto", "src/server.proto"],
        &["src/"],
    )?;
    Ok(())
}
