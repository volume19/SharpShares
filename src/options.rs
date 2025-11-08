//! Command-line argument parsing and configuration

use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use crate::error::ArgsError;

/// SharpShares - Network share enumeration for Active Directory
///
/// For authorized security assessments only. Unauthorized use may be illegal.
#[derive(Parser, Clone, Debug)]
#[command(name = "SharpShares")]
#[command(version, about, long_about = None)]
pub struct Arguments {
    /// Maximum number of parallel threads (default: 25)
    #[arg(long, default_value = "25")]
    pub threads: usize,

    /// Domain controller to query (required if not domain-joined)
    #[arg(long)]
    pub dc: Option<String>,

    /// Domain name (required if not domain-joined)
    #[arg(long)]
    pub domain: Option<String>,

    /// LDAP filter: all, dc, exclude-dc, servers, servers-exclude-dc
    #[arg(long)]
    pub ldap: Option<String>,

    /// LDAP OU to query (e.g., "OU=Servers,DC=example,DC=local")
    #[arg(long)]
    pub ou: Option<String>,

    /// List share names without performing read/write access checks
    #[arg(long, default_value = "false")]
    pub stealth: bool,

    /// Comma-separated shares to exclude (default: SYSVOL,NETLOGON,IPC$,PRINT$)
    #[arg(long, value_delimiter = ',', default_value = "SYSVOL,NETLOGON,IPC$,PRINT$")]
    pub filter: Vec<String>,

    /// File to append results to (instead of stdout)
    #[arg(long)]
    pub outfile: Option<PathBuf>,

    /// Include unauthorized shares in output
    #[arg(long, default_value = "false")]
    pub verbose: bool,

    /// Confirm you have authorization to run this tool
    #[arg(long, default_value = "false")]
    pub i_have_authorization: bool,
}

impl Arguments {
    /// Validate arguments and ensure required conditions are met
    pub fn validate(&self) -> Result<(), ArgsError> {
        // Authorization check
        if !self.i_have_authorization {
            eprintln!("\n⚠️  AUTHORIZATION REQUIRED ⚠️");
            eprintln!("This tool is for authorized security assessments only.");
            eprintln!("Unauthorized use may violate computer fraud and abuse laws.");
            eprintln!("\nTo proceed, add: --i-have-authorization\n");
            return Err(ArgsError::Invalid(
                "Authorization flag required".to_string()
            ));
        }

        // Must specify ldap OR ou
        if self.ldap.is_none() && self.ou.is_none() {
            return Err(ArgsError::MissingTargetSpec);
        }

        // Validate LDAP filter if specified
        if let Some(ref ldap_filter) = self.ldap {
            let valid_filters = [
                "all",
                "dc",
                "exclude-dc",
                "servers",
                "servers-exclude-dc",
            ];
            if !valid_filters.contains(&ldap_filter.as_str()) {
                return Err(ArgsError::Invalid(format!(
                    "Invalid LDAP filter '{}'. Valid options: {}",
                    ldap_filter,
                    valid_filters.join(", ")
                )));
            }
        }

        // Validate threads > 0
        if self.threads == 0 {
            return Err(ArgsError::Invalid(
                "Threads must be greater than 0".to_string()
            ));
        }

        // Create output file if specified
        if let Some(ref outfile) = self.outfile {
            if outfile.exists() {
                tracing::warn!("Output file already exists, will append: {:?}", outfile);
            } else {
                File::create(outfile).map_err(ArgsError::OutfileCreation)?;
                tracing::info!("Created output file: {:?}", outfile);
            }
        }

        Ok(())
    }

    /// Print parsed configuration to stderr
    pub fn print_config(&self) {
        eprintln!("\n[+] Parsed Arguments:");
        eprintln!("    threads: {}", self.threads);
        eprintln!("    dc: {}", self.dc.as_deref().unwrap_or("none"));
        eprintln!("    domain: {}", self.domain.as_deref().unwrap_or("none"));
        eprintln!("    ldap: {}", self.ldap.as_deref().unwrap_or("none"));
        eprintln!("    ou: {}", self.ou.as_deref().unwrap_or("none"));
        eprintln!("    stealth: {}", self.stealth);
        eprintln!("    verbose: {}", self.verbose);

        if self.filter.is_empty() {
            eprintln!("    filter: none");
        } else {
            eprintln!("    filter: {}", self.filter.join(","));
        }

        if let Some(ref outfile) = self.outfile {
            eprintln!("    outfile: {}", outfile.display());
        } else {
            eprintln!("    outfile: none");
        }

        if !self.filter.is_empty() {
            eprintln!("\n[*] Excluding {} shares", self.filter.join(","));
        }
        if self.verbose {
            eprintln!("[*] Including unreadable shares");
        }
        eprintln!("[*] Starting share enumeration with thread limit of {}", self.threads);
        eprintln!("[r] = Readable Share");
        eprintln!("[w] = Writeable Share");
        eprintln!("[-] = Unauthorized Share (requires --verbose)");
        eprintln!("[?] = Unchecked Share (requires --stealth)\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_args_all_flags() {
        let args = Arguments::parse_from([
            "sharpshares",
            "--threads", "50",
            "--ldap", "all",
            "--filter", "SYSVOL,IPC$",
            "--verbose",
            "--stealth",
            "--i-have-authorization",
        ]);

        assert_eq!(args.threads, 50);
        assert_eq!(args.ldap, Some("all".to_string()));
        assert_eq!(args.filter, vec!["SYSVOL", "IPC$"]);
        assert!(args.verbose);
        assert!(args.stealth);
    }

    #[test]
    fn test_default_values() {
        let args = Arguments::parse_from([
            "sharpshares",
            "--ldap", "all",
            "--i-have-authorization",
        ]);

        assert_eq!(args.threads, 25);
        assert_eq!(args.filter, vec!["SYSVOL", "NETLOGON", "IPC$", "PRINT$"]);
        assert!(!args.stealth);
        assert!(!args.verbose);
    }

    #[test]
    fn test_validate_missing_ldap_and_ou() {
        let args = Arguments::parse_from([
            "sharpshares",
            "--i-have-authorization",
        ]);

        let result = args.validate();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ArgsError::MissingTargetSpec));
    }

    #[test]
    fn test_validate_requires_authorization() {
        let args = Arguments::parse_from(["sharpshares", "--ldap", "all"]);
        assert!(args.validate().is_err());
    }
}
