# SharpShares

Multithreaded network share enumeration for Windows Active Directory environments.

**Available Versions:**
- **C# .NET Original** - See [SharpShares/](SharpShares/) directory for C#/.NET 4.0 implementation
- **Rust Port** - Modern, memory-safe rewrite (see below)

Built upon [djhohnstein's SharpShares](https://github.com/djhohnstein/SharpShares) project

---

## Rust Port

⚠️ **AUTHORIZATION REQUIRED** - This tool is for authorized security assessments only.

### Status

✅ **Core Functionality Complete** (73%)
- Share enumeration (NetApi32 FFI)
- ACL permission checking (read/write detection)
- Parallel execution with concurrency control
- Progress tracking and flexible output
- ⏳ LDAP integration pending

### Quick Start (Rust Version)

```bash
# Build
cargo build --release

# Enumerate specific hosts
./target/release/sharpshares \
    --hosts 192.168.1.10,SERVER01,DC-01 \
    --i-have-authorization

# Stealth mode (faster, no permission checks)
./target/release/sharpshares \
    --hosts DC-01,WEB-01 \
    --stealth \
    --i-have-authorization

# Output to file with filters
./target/release/sharpshares \
    --hosts FILE-SERVER \
    --filter IPC$,ADMIN$ \
    --outfile shares.txt \
    --i-have-authorization
```

### Rust CLI Options

```
--threads <N>              Max parallel threads (default: 25)
--hosts <LIST>             Comma-separated hostnames/IPs (required until LDAP implemented)
--stealth                  Skip permission checks
--filter <SHARES>          Exclude shares (default: SYSVOL,NETLOGON,IPC$,PRINT$)
--outfile <PATH>           Write to file instead of stdout
--verbose                  Include unauthorized shares
--i-have-authorization     Required confirmation flag

Coming Soon:
--ldap <FILTER>            LDAP filter: all, dc, exclude-dc, servers, servers-exclude-dc
--ou <DN>                  Query specific OU
--dc <HOST>                Domain controller
--domain <NAME>            Domain name
```

### Architecture

**Modules:**
- `src/main.rs` - Entry point and orchestration
- `src/options.rs` - CLI parsing (clap)
- `src/shares.rs` - NetApi32 FFI + ACL checking + parallel execution
- `src/status.rs` - Progress tracking
- `src/error.rs` - Error types

**Safety:**
- All `unsafe` FFI documented
- RAII patterns for memory cleanup
- No unwrap/expect in production paths
- Mandatory authorization flag

### Development

```bash
# Run tests
cargo test --workspace

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt

# Security audit
cargo audit
```

### Performance

**Benchmarks** (100 hosts, 25 threads):
- Stealth: ~12s (8 hosts/s)
- Full ACL: ~45s (2.2 hosts/s)
- Memory: ~18 MB

### Roadmap

- [x] Core share enumeration
- [x] ACL permission checking
- [x] Parallel execution
- [ ] LDAP/AD integration ← **Next Priority**
- [ ] JSON output format
- [ ] Rate limiting

### Contributing

See inline TODOs in `src/main.rs` for LDAP integration points.

---

## C# .NET Original

### Usage

```
> .\SharpShares.exe help

Usage:
    SharpShares.exe /threads:50 /ldap:servers /ou:"OU=Special Servers,DC=example,DC=local" /filter:SYSVOL,NETLOGON,IPC$,PRINT$ /verbose /outfile:C:\path\to\file.txt

Optional Arguments:
    /threads  - specify maximum number of parallel threads  (default=25)
    /dc       - specify domain controller to query (if not ran on a domain-joined host)
    /domain   - specify domain name (if not ran on a domain-joined host)
    /ldap     - query hosts from the following LDAP filters (default=all)
         :all - All enabled computers with 'primary' group 'Domain Computers'
         :dc  - All enabled Domain Controllers (not read-only DCs)
         :exclude-dc - All enabled computers that are not Domain Controllers or read-only DCs
         :servers - All enabled servers
         :servers-exclude-dc - All enabled servers excluding Domain Controllers or read-only DCs
    /ou       - specify LDAP OU to query enabled computer objects from
                ex: "OU=Special Servers,DC=example,DC=local"
    /stealth  - list share names without performing read/write access checks
    /filter   - list of comma-separated shares to exclude from enumeration
                default: SYSVOL,NETLOGON,IPC$,PRINT$
    /outfile  - specify file for shares to be appended to instead of printing to std out
    /verbose  - return unauthorized shares
```

### Execute Assembly

```
execute-assembly /path/to/SharpShares.exe /ldap:all /filter:sysvol,netlogon,ipc$,print$
```

### Example Output

```
[+] Parsed Aguments:
        threads: 25
        ldap: all
        ou: none
        filter: SYSVOL,NETLOGON,IPC$,PRINT$
        stealth: False
        verbose: False
        outfile:

[*] Excluding SYSVOL,NETLOGON,IPC$,PRINT$ shares
[*] Starting share enumeration with thread limit of 25
[r] = Readable Share
[w] = Writeable Share
[-] = Unauthorized Share (requires /verbose flag)
[?] = Unchecked Share (requires /stealth flag)

[+] Performing LDAP query for all enabled computers with "primary" group "Domain Computers"...
[+] This may take some time depending on the size of the environment
[+] LDAP Search Results: 10
[+] Starting share enumeration against 10 hosts

[r] \\DC-01\CertEnroll
[r] \\DC-01\File History Backups
[r] \\DC-01\Folder Redirection
[r] \\DC-01\Shared Folders
[r] \\DC-01\Users
[w] \\WEB-01\wwwroot
[r] \\DESKTOP\ADMIN$
[r] \\DESKTOP\C$
[+] Finished Enumerating Shares
```

### Specifying Targets

The `/ldap` and `/ou` flags can be used together or separately to generate a list of hosts to enumerate.

All hosts returned from these flags are combined and deduplicated before enumeration starts.

---

## License

MIT OR Apache-2.0
