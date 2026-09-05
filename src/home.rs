//! Trusted account home resolution from OS metadata.
//!
//! Resolves the real account home directory via `getpwuid_r` (account
//! metadata), NOT the untrusted `$HOME` environment variable. Falls back
//! to `dirs::home_dir()` ONLY in test builds via `SWITCHEROO_TEST_HOME`.
//! In production, a `getpwuid_r` failure is fatal (fail closed).

use std::path::PathBuf;

/// Resolve the real account home from `getpwuid_r` (OS account metadata).
/// In production, there is NO `$HOME` fallback — a failure is fatal.
/// In tests, `SWITCHEROO_TEST_HOME` overrides for isolation.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // libc FFI for getpwuid_r
pub fn real_home() -> Result<PathBuf, String> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;

    #[cfg(test)]
    if let Ok(h) = std::env::var("SWITCHEROO_TEST_HOME") {
        return Ok(PathBuf::from(h));
    }

    let uid = unsafe { libc::getuid() };
    let mut buf = vec![0u8; 4096];
    let mut result: *mut libc::passwd = std::ptr::null_mut();
    let mut pwd: libc::passwd = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::getpwuid_r(
            uid,
            std::ptr::addr_of_mut!(pwd),
            buf.as_mut_ptr().cast::<i8>(),
            buf.len(),
            std::ptr::addr_of_mut!(result),
        )
    };
    if rc != 0 || result.is_null() {
        return Err(
            "could not resolve account home from getpwuid_r — refusing to use $HOME".to_string(),
        );
    }
    let dir = unsafe { CStr::from_ptr(pwd.pw_dir) };
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes())))
}

#[cfg(not(target_os = "macos"))]
pub fn real_home() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "could not resolve home directory".to_string())
}
