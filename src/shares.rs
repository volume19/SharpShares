//! Network share enumeration using Windows NetApi32
//!
//! SAFETY: This module contains unsafe FFI calls to Windows APIs.
//! All unsafe blocks are documented with justification.

use thiserror::Error;

#[cfg(windows)]
use std::ffi::OsString;
#[cfg(windows)]
use std::os::windows::ffi::OsStringExt;
#[cfg(windows)]
use windows::core::{PCWSTR, PWSTR};
#[cfg(windows)]
use windows::Win32::NetworkManagement::NetManagement::*;

/// Share enumeration errors
#[derive(Error, Debug)]
pub enum ShareError {
    #[error("Network path not found (error 53)")]
    NetworkPathNotFound,

    #[error("Access denied (error 5)")]
    AccessDenied,

    #[error("NetApi error code: {0}")]
    NetApi(i32),

    #[error("UTF-16 conversion error")]
    StringConversion,
}

/// Share type enumeration (matches STYPE_* constants)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum ShareType {
    DiskTree = 0,
    PrintQueue = 1,
    Device = 2,
    Ipc = 3,
    Special = 0x80000000,
}

/// Share information (corresponds to SHARE_INFO_1)
#[derive(Debug, Clone)]
pub struct ShareInfo {
    pub netname: String,
    pub share_type: u32,
    pub remark: String,
}

impl ShareInfo {
    /// Check if this is an error response
    pub fn is_error(&self) -> bool {
        self.netname.starts_with("ERROR=")
    }

    /// Extract error code if this is an error response
    pub fn error_code(&self) -> Option<i32> {
        if self.is_error() {
            self.netname.strip_prefix("ERROR=")
                .and_then(|s| s.parse().ok())
        } else {
            None
        }
    }
}

/// Enumerate network shares on a remote server (Windows only)
///
/// # Arguments
/// * `server` - Server name (e.g., "SERVER01" or "192.168.1.10")
///
/// # Returns
/// * `Ok(Vec<ShareInfo>)` - List of shares
/// * `Err(ShareError)` - If enumeration fails
///
/// # Safety
/// Uses unsafe FFI to call NetShareEnum. Memory is properly managed via NetApiBufferFree.
#[cfg(windows)]
pub fn enum_net_shares(server: &str) -> Result<Vec<ShareInfo>, ShareError> {
    use std::ptr;

    const MAX_PREFERRED_LENGTH: u32 = 0xFFFFFFFF;
    const NERR_SUCCESS: u32 = 0;

    // Convert server name to wide string
    let server_wide: Vec<u16> = OsString::from(server)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut buf_ptr: *mut u8 = ptr::null_mut();
    let mut entries_read: u32 = 0;
    let mut total_entries: u32 = 0;
    let mut resume_handle: u32 = 0;

    // SAFETY: Calling Windows API NetShareEnum
    // - server_wide is null-terminated UTF-16
    // - buf_ptr will be allocated by the API, we free it with NetApiBufferFree
    // - All pointers are valid for the duration of the call
    let result = unsafe {
        NetShareEnum(
            PCWSTR(server_wide.as_ptr()),
            1, // Level 1 (SHARE_INFO_1)
            &mut buf_ptr,
            MAX_PREFERRED_LENGTH,
            &mut entries_read,
            &mut total_entries,
            Some(&mut resume_handle),
        )
    };

    // Check for errors
    if result.0 != NERR_SUCCESS as i32 {
        return Ok(vec![ShareInfo {
            netname: format!("ERROR={}", result.0),
            share_type: 10,
            remark: String::new(),
        }]);
    }

    // Parse results
    let mut shares = Vec::with_capacity(entries_read as usize);

    // SAFETY: buf_ptr points to an array of SHARE_INFO_1 structs allocated by NetShareEnum
    // entries_read tells us how many valid entries exist
    // We must call NetApiBufferFree before returning
    unsafe {
        let share_info_ptr = buf_ptr as *const SHARE_INFO_1;

        for i in 0..entries_read as isize {
            let share = &*share_info_ptr.offset(i);

            // Convert PWSTR to String
            let netname = pwstr_to_string(share.shi1_netname)
                .unwrap_or_else(|| format!("INVALID_NAME_{}", i));
            let remark = pwstr_to_string(share.shi1_remark)
                .unwrap_or_default();

            shares.push(ShareInfo {
                netname,
                share_type: share.shi1_type,
                remark,
            });
        }

        // Free the buffer allocated by NetShareEnum
        let _ = NetApiBufferFree(Some(buf_ptr as *const _));
    }

    Ok(shares)
}

/// Stub for non-Windows platforms
#[cfg(not(windows))]
pub fn enum_net_shares(_server: &str) -> Result<Vec<ShareInfo>, ShareError> {
    Err(ShareError::NetApi(-1)) // Not supported on this platform
}

/// Convert Windows PWSTR to Rust String
///
/// # Safety
/// Assumes pwstr points to a valid null-terminated UTF-16 string
#[cfg(windows)]
unsafe fn pwstr_to_string(pwstr: PWSTR) -> Option<String> {
    if pwstr.is_null() {
        return None;
    }

    // Find null terminator
    let mut len = 0;
    while *pwstr.0.offset(len) != 0 {
        len += 1;
    }

    if len == 0 {
        return Some(String::new());
    }

    // Convert to Rust string
    let slice = std::slice::from_raw_parts(pwstr.0, len as usize);
    let os_string = OsString::from_wide(slice);
    os_string.into_string().ok()
}

/// Output destination for share enumeration results
pub enum OutputSink {
    /// Write to stdout
    Stdout,
    /// Write to a file (thread-safe)
    File(std::sync::Arc<tokio::sync::Mutex<std::fs::File>>),
}

impl OutputSink {
    /// Write a line to the output sink (thread-safe)
    pub async fn write_line(&self, line: &str) -> std::io::Result<()> {
        use std::io::Write;

        match self {
            OutputSink::Stdout => {
                println!("{}", line);
                Ok(())
            }
            OutputSink::File(file) => {
                let mut f = file.lock().await;
                writeln!(f, "{}", line)?;
                f.flush()?;
                Ok(())
            }
        }
    }
}

/// Enumerate shares on a single computer (stealth mode - no permission checks)
///
/// # Arguments
/// * `computer` - Computer name or IP address
/// * `args` - Command-line arguments (filter, stealth, verbose, etc.)
/// * `output` - Where to write results
/// * `status` - Progress tracker
///
/// # Returns
/// * `Ok(())` if enumeration completed (even with errors)
/// * `Err(ShareError)` only on critical failures
pub async fn get_computer_shares_stealth(
    computer: &str,
    args: &crate::options::Arguments,
    output: &OutputSink,
    status: &crate::status::Status,
) -> Result<(), ShareError> {
    // Error codes to skip (not found, access denied)
    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_BAD_NETPATH: i32 = 53;

    // Enumerate shares
    let shares = enum_net_shares(computer)?;

    // Check if we got an error response
    if shares.len() == 1 && shares[0].is_error() {
        let err_code = shares[0].error_code().unwrap_or(0);
        // Silently skip common errors (access denied, network path not found)
        if err_code == ERROR_ACCESS_DENIED || err_code == ERROR_BAD_NETPATH {
            status.increment();
            return Ok(());
        }
        // Log other errors if verbose
        if args.verbose {
            tracing::warn!("Error enumerating {}: code {}", computer, err_code);
        }
        status.increment();
        return Ok(());
    }

    // Filter and output shares
    for share in shares {
        // Skip if in filter list (case-insensitive)
        if args.filter.iter().any(|f| f.eq_ignore_ascii_case(&share.netname)) {
            continue;
        }

        // In stealth mode, just list without permission checks
        let line = format!("[?] \\\\{}\\{}", computer, share.netname);
        if let Err(e) = output.write_line(&line).await {
            tracing::error!("Failed to write output: {}", e);
        }
    }

    // Increment completion counter
    status.increment();
    Ok(())
}

/// Enumerate shares on a single computer with full permission checks
///
/// Tests read and write permissions on each share and outputs accordingly.
/// Requires Windows APIs for permission checking.
///
/// # Arguments
/// * `computer` - Computer name or IP address
/// * `args` - Command-line arguments
/// * `output` - Where to write results
/// * `status` - Progress tracker
#[cfg(windows)]
pub async fn get_computer_shares_full(
    computer: &str,
    args: &crate::options::Arguments,
    output: &OutputSink,
    status: &crate::status::Status,
) -> Result<(), ShareError> {
    use std::path::Path;

    const ERROR_ACCESS_DENIED: i32 = 5;
    const ERROR_BAD_NETPATH: i32 = 53;

    // Enumerate shares
    let shares = enum_net_shares(computer)?;

    // Check if we got an error response
    if shares.len() == 1 && shares[0].is_error() {
        let err_code = shares[0].error_code().unwrap_or(0);
        if err_code == ERROR_ACCESS_DENIED || err_code == ERROR_BAD_NETPATH {
            status.increment();
            return Ok(());
        }
        if args.verbose {
            tracing::warn!("Error enumerating {}: code {}", computer, err_code);
        }
        status.increment();
        return Ok(());
    }

    let mut readable_shares = Vec::new();
    let mut writeable_shares = Vec::new();
    let mut unauthorized_shares = Vec::new();

    // Get current user info once for all shares
    let (user_sid, group_sids) = match get_current_user_and_groups() {
        Ok(info) => info,
        Err(e) => {
            tracing::error!("Failed to get user info: {}", e);
            status.increment();
            return Ok(());
        }
    };

    // Check each share
    for share in shares {
        // Skip if in filter list
        if args.filter.iter().any(|f| f.eq_ignore_ascii_case(&share.netname)) {
            continue;
        }

        let unc_path = format!("\\\\{}\\{}", computer, share.netname);

        // Try to test read access
        match test_read_access(&unc_path) {
            Ok(true) => {
                readable_shares.push(share.netname.clone());

                // Check for write access
                if check_write_permission(&unc_path, &user_sid, &group_sids) {
                    writeable_shares.push(share.netname.clone());
                }
            }
            Ok(false) => {
                unauthorized_shares.push(share.netname);
            }
            Err(e) => {
                tracing::debug!("Error checking {}: {}", unc_path, e);
                unauthorized_shares.push(share.netname);
            }
        }
    }

    // Output readable shares
    for share in readable_shares {
        let line = format!("[r] \\\\{}\\{}", computer, share);
        let _ = output.write_line(&line).await;
    }

    // Output writeable shares
    for share in writeable_shares {
        let line = format!("[w] \\\\{}\\{}", computer, share);
        let _ = output.write_line(&line).await;
    }

    // Output unauthorized shares if verbose
    if args.verbose {
        for share in unauthorized_shares {
            let line = format!("[-] \\\\{}\\{}", computer, share);
            let _ = output.write_line(&line).await;
        }
    }

    status.increment();
    Ok(())
}

/// Stub for non-Windows platforms
#[cfg(not(windows))]
pub async fn get_computer_shares_full(
    computer: &str,
    args: &crate::options::Arguments,
    output: &OutputSink,
    status: &crate::status::Status,
) -> Result<(), ShareError> {
    // Fallback to stealth mode on non-Windows
    get_computer_shares_stealth(computer, args, output, status).await
}

/// Test if we can read from a share by listing directory contents
#[cfg(windows)]
fn test_read_access(path: &str) -> Result<bool, std::io::Error> {
    match std::fs::read_dir(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
        Err(e) => Err(e),
    }
}

/// Get current user SID and group SIDs
#[cfg(windows)]
fn get_current_user_and_groups() -> Result<(String, Vec<String>), ShareError> {
    use windows::Win32::Foundation::*;
    use windows::Win32::Security::*;
    use windows::Win32::System::Threading::*;

    unsafe {
        let mut token_handle = HANDLE::default();

        // Get current process token
        if !OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut token_handle,
        ).is_ok() {
            return Err(ShareError::NetApi(-1));
        }

        // Get user SID
        let mut user_buffer = vec![0u8; 256];
        let mut return_length = 0u32;

        let _ = GetTokenInformation(
            token_handle,
            TokenUser,
            Some(user_buffer.as_mut_ptr() as *mut _),
            user_buffer.len() as u32,
            &mut return_length,
        );

        if return_length > user_buffer.len() as u32 {
            user_buffer.resize(return_length as usize, 0);
            if !GetTokenInformation(
                token_handle,
                TokenUser,
                Some(user_buffer.as_mut_ptr() as *mut _),
                user_buffer.len() as u32,
                &mut return_length,
            ).is_ok() {
                CloseHandle(token_handle);
                return Err(ShareError::NetApi(-2));
            }
        }

        let token_user = &*(user_buffer.as_ptr() as *const TOKEN_USER);
        let user_sid = sid_to_string(token_user.User.Sid)?;

        // Get group SIDs
        let mut groups_buffer = vec![0u8; 1024];
        return_length = 0;

        let _ = GetTokenInformation(
            token_handle,
            TokenGroups,
            Some(groups_buffer.as_mut_ptr() as *mut _),
            groups_buffer.len() as u32,
            &mut return_length,
        );

        if return_length > groups_buffer.len() as u32 {
            groups_buffer.resize(return_length as usize, 0);
            if !GetTokenInformation(
                token_handle,
                TokenGroups,
                Some(groups_buffer.as_mut_ptr() as *mut _),
                groups_buffer.len() as u32,
                &mut return_length,
            ).is_ok() {
                CloseHandle(token_handle);
                return Err(ShareError::NetApi(-3));
            }
        }

        let token_groups = &*(groups_buffer.as_ptr() as *const TOKEN_GROUPS);
        let mut group_sids = Vec::new();

        for i in 0..token_groups.GroupCount {
            let group = &token_groups.Groups[i as usize];
            if let Ok(sid_str) = sid_to_string(group.Sid) {
                group_sids.push(sid_str);
            }
        }

        CloseHandle(token_handle);
        Ok((user_sid, group_sids))
    }
}

/// Convert Windows SID to string representation
#[cfg(windows)]
unsafe fn sid_to_string(sid: PSID) -> Result<String, ShareError> {
    use windows::Win32::Security::*;
    use windows::core::PWSTR;

    let mut sid_string = PWSTR::null();

    if !ConvertSidToStringSidW(sid, &mut sid_string).is_ok() {
        return Err(ShareError::StringConversion);
    }

    let result = pwstr_to_string(sid_string)
        .ok_or(ShareError::StringConversion)?;

    // Free the string allocated by ConvertSidToStringSidW
    let _ = windows::Win32::System::Memory::LocalFree(
        windows::Win32::Foundation::HLOCAL(sid_string.0 as isize)
    );

    Ok(result)
}

/// Check if current user has write permission on a path
#[cfg(windows)]
fn check_write_permission(path: &str, user_sid: &str, group_sids: &[String]) -> bool {
    use windows::Win32::Storage::FileSystem::*;
    use windows::Win32::Security::Authorization::*;
    use windows::Win32::Security::*;
    use std::os::windows::ffi::OsStrExt;

    unsafe {
        // Convert path to wide string
        let path_wide: Vec<u16> = std::ffi::OsStr::new(path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut sd_ptr: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        let mut dacl_ptr: *mut ACL = std::ptr::null_mut();

        // Get security descriptor
        let result = GetNamedSecurityInfoW(
            windows::core::PCWSTR(path_wide.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl_ptr),
            None,
            &mut sd_ptr,
        );

        if result != 0 {
            return false;
        }

        // Check ACL for write permissions
        let has_write = if !dacl_ptr.is_null() {
            check_acl_for_write(dacl_ptr, user_sid, group_sids)
        } else {
            false
        };

        // Free security descriptor
        if !sd_ptr.is_null() {
            let _ = windows::Win32::System::Memory::LocalFree(
                windows::Win32::Foundation::HLOCAL(sd_ptr as isize)
            );
        }

        has_write
    }
}

/// Check ACL entries for write permissions
#[cfg(windows)]
unsafe fn check_acl_for_write(acl: *const ACL, user_sid: &str, group_sids: &[String]) -> bool {
    use windows::Win32::Security::*;

    if acl.is_null() {
        return false;
    }

    let acl_ref = &*acl;

    for i in 0..acl_ref.AceCount {
        let mut ace_ptr: *mut std::ffi::c_void = std::ptr::null_mut();

        if !GetAce(acl, i as u32, &mut ace_ptr).is_ok() {
            continue;
        }

        let ace_header = &*(ace_ptr as *const ACE_HEADER);

        // Only check ACCESS_ALLOWED_ACE
        if ace_header.AceType != ACCESS_ALLOWED_ACE_TYPE {
            continue;
        }

        let access_ace = &*(ace_ptr as *const ACCESS_ALLOWED_ACE);

        // Check if this ACE grants write permissions
        const FILE_WRITE_DATA: u32 = 0x0002;
        const FILE_APPEND_DATA: u32 = 0x0004;
        const GENERIC_WRITE: u32 = 0x40000000;

        if (access_ace.Mask & (FILE_WRITE_DATA | FILE_APPEND_DATA | GENERIC_WRITE)) == 0 {
            continue;
        }

        // Get SID from ACE
        let ace_sid = PSID(&access_ace.SidStart as *const _ as *mut _);
        if let Ok(ace_sid_str) = sid_to_string(ace_sid) {
            // Check if SID matches user or any group
            if ace_sid_str == user_sid || group_sids.contains(&ace_sid_str) {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_info_error_detection() {
        let error_share = ShareInfo {
            netname: "ERROR=5".to_string(),
            share_type: 10,
            remark: String::new(),
        };

        assert!(error_share.is_error());
        assert_eq!(error_share.error_code(), Some(5));
    }

    #[test]
    fn test_share_info_normal() {
        let share = ShareInfo {
            netname: "C$".to_string(),
            share_type: 0,
            remark: "Default share".to_string(),
        };

        assert!(!share.is_error());
        assert_eq!(share.error_code(), None);
    }

    #[cfg(windows)]
    #[test]
    fn test_enum_localhost_shares() {
        // Should work on Windows - at minimum IPC$ should exist
        let result = enum_net_shares("127.0.0.1");
        assert!(result.is_ok());

        let shares = result.unwrap();
        // Most Windows systems have at least IPC$ and ADMIN$
        assert!(!shares.is_empty(), "Expected at least one share on localhost");

        // Check if we got shares or an error
        if shares.len() == 1 && shares[0].is_error() {
            println!("Got error: {:?}", shares[0].error_code());
        } else {
            println!("Found {} shares", shares.len());
            for share in &shares {
                println!("  - {}", share.netname);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_enum_invalid_host() {
        let result = enum_net_shares("nonexistent.invalid.host.local");
        assert!(result.is_ok()); // Returns Ok with ERROR= share

        let shares = result.unwrap();
        assert_eq!(shares.len(), 1);
        assert!(shares[0].is_error());
        // Should be error 53 (network path not found) or 1231 (network location cannot be reached)
        let err_code = shares[0].error_code().unwrap();
        assert!(err_code == 53 || err_code == 1231, "Expected error 53 or 1231, got {}", err_code);
    }

    #[tokio::test]
    async fn test_output_sink_stdout() {
        let sink = OutputSink::Stdout;
        let result = sink.write_line("test").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_output_sink_file() {
        use std::io::Read;
        use tempfile::NamedTempFile;

        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_owned();

        let file = std::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(&path)
            .unwrap();

        let sink = OutputSink::File(std::sync::Arc::new(tokio::sync::Mutex::new(file)));
        sink.write_line("test line 1").await.unwrap();
        sink.write_line("test line 2").await.unwrap();

        drop(sink); // Close file

        let mut contents = String::new();
        std::fs::File::open(&path)
            .unwrap()
            .read_to_string(&mut contents)
            .unwrap();

        assert!(contents.contains("test line 1"));
        assert!(contents.contains("test line 2"));
    }
}
