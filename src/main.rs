//! SharpShares - Network share enumeration
//! For authorized security assessments only.

use clap::Parser;
use sharpshares::options::Arguments;
use sharpshares::shares::{get_all_shares, OutputSink};
use std::fs::OpenOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging to stderr
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .init();

    // Parse and validate arguments
    let args = Arguments::parse();
    args.validate()?;
    args.print_config();

    tracing::info!("SharpShares started - authorized security assessment mode");

    // Collect hosts from LDAP and/or OU and/or direct input
    let mut hosts: Vec<String> = Vec::new();

    // Direct host specification (for testing or small-scale use)
    if !args.hosts.is_empty() {
        hosts.extend(args.hosts.clone());
    }

    // LDAP enumeration (if specified)
    if let Some(ref _ldap_filter) = args.ldap {
        eprintln!("[!] LDAP support not yet implemented");
        eprintln!("[!] Ignoring --ldap flag for now");
        // TODO: Implement LDAP search
        // let ldap_hosts = ldap::search_ldap(&args).await?;
        // hosts.extend(ldap_hosts);
    }

    // OU enumeration (if specified)
    if let Some(ref _ou) = args.ou {
        eprintln!("[!] LDAP OU support not yet implemented");
        eprintln!("[!] Ignoring --ou flag for now");
        // TODO: Implement OU search
        // let ou_hosts = ldap::search_ou(&args).await?;
        // hosts.extend(ou_hosts);
    }

    // Verify we have hosts to enumerate
    if hosts.is_empty() {
        eprintln!("\n[!] No hosts to enumerate");
        eprintln!("[!] LDAP support coming soon. For now, use: --hosts <computer1,computer2,...>");
        eprintln!("\nExample:");
        eprintln!("  sharpshares --hosts localhost,127.0.0.1 --i-have-authorization");
        eprintln!("  sharpshares --hosts DC-01,WEB-01,FILE-01 --stealth --i-have-authorization\n");
        return Ok(());
    }

    // Remove duplicates
    hosts.sort();
    hosts.dedup();

    tracing::info!("Collected {} unique hosts", hosts.len());

    // Create output sink
    let output: Arc<OutputSink> = if let Some(ref outfile) = args.outfile {
        // Open file for appending
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(outfile)?;
        Arc::new(OutputSink::File(Arc::new(tokio::sync::Mutex::new(file))))
    } else {
        Arc::new(OutputSink::Stdout)
    };

    // Wrap args in Arc for sharing
    let args = Arc::new(args);

    // Start enumeration
    tracing::info!("Starting parallel enumeration");
    get_all_shares(hosts, args, output).await?;

    tracing::info!("SharpShares completed successfully");
    Ok(())
}
