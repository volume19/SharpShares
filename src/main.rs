//! SharpShares - Network share enumeration
//! For authorized security assessments only.

use clap::Parser;
use sharpshares::options::Arguments;
use tracing_subscriber;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Parse and validate arguments
    let args = Arguments::parse();
    args.validate()?;
    args.print_config();

    tracing::info!("SharpShares started");
    println!("[+] TODO: Implementation in progress");

    Ok(())
}
