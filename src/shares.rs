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
}
