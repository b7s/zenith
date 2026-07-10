//! DPAPI token protection.
//!
//! Per-account tokens are stored in `config.json` under
//! `widgets.config.git.accounts[].token_blob` as base64-encoded bytes
//! produced by the Windows Data Protection API (`CryptProtectData`). DPAPI
//! encrypts with the user's credentials — no key management, no extra
//! deps, and the blob is only decodeable on the same Windows user
//! account that produced it. Plaintext only ever lives in process
//! memory between `unprotect()` and the in-flight HTTP request.
//!
//! Single home — do not re-implement.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use windows::core::PCWSTR;
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN,
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_PROMPTSTRUCT,
};
use windows::Win32::Foundation::LocalFree;
use windows::Win32::Foundation::HLOCAL;

/// Wrap a plaintext token into a base64 DPAPI blob suitable for storage.
/// Returns `None` if the OS call fails (rare on Win11; e.g. profile not
/// loaded in exotic service-host scenarios) — callers surface a friendly
/// error to the user via `.zen-hint`.
pub fn protect(plaintext: &str) -> Option<String> {
    let bytes = plaintext.as_bytes();
    let out = unsafe { protect_data(bytes) }?;
    Some(B64.encode(&out))
}

/// Unwrap a base64 DPAPI blob back to the plaintext token. Returns
/// `None` on any failure (corrupt blob, wrong user, base64 error).
pub fn unprotect(blob: &str) -> Option<String> {
    let raw = B64.decode(blob).ok()?;
    if raw.is_empty() {
        return None;
    }
    let bytes = unsafe { unprotect_data(&raw) }?;
    String::from_utf8(bytes).ok()
}

unsafe fn protect_data(input: &[u8]) -> Option<Vec<u8>> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut _,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = CryptProtectData(
        &in_blob,
        PCWSTR::null(),
        None,                  // entropy
        None,                  // reserved
        Option::<*const CRYPTPROTECT_PROMPTSTRUCT>::None, // prompt
        CRYPTPROTECT_UI_FORBIDDEN,
        &mut out_blob,
    );
    if ok.is_err() {
        return None;
    }
    let out = slice_from_blob(&out_blob);
    if !out_blob.pbData.is_null() {
        // DPAPI allocates with LocalAlloc; free with LocalFree.
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData as *mut _)));
    }
    Some(out)
}

unsafe fn unprotect_data(input: &[u8]) -> Option<Vec<u8>> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut _,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = CryptUnprotectData(
        &in_blob,
        None,                       // description out
        None,                       // entropy
        None,                       // reserved
        Option::<*const CRYPTPROTECT_PROMPTSTRUCT>::None, // prompt
        CRYPTPROTECT_UI_FORBIDDEN,
        &mut out_blob,
    );
    if ok.is_err() {
        return None;
    }
    let out = slice_from_blob(&out_blob);
    if !out_blob.pbData.is_null() {
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData as *mut _)));
    }
    Some(out)
}

unsafe fn slice_from_blob(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if blob.cbData == 0 || blob.pbData.is_null() {
        return Vec::new();
    }
    let len = blob.cbData as usize;
    let slice = std::slice::from_raw_parts(blob.pbData, len);
    slice.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protect_unprotect_roundtrip() {
        let plaintext = "ghp_someTestToken_xyz_0123456789";
        let blob = protect(plaintext).expect("DPAPI protect should succeed in user session");
        assert!(!blob.is_empty());
        assert_ne!(blob, plaintext);
        let back = unprotect(&blob).expect("DPAPI unprotect should invert protect");
        assert_eq!(back, plaintext);
    }

    #[test]
    fn unprotect_garbage_returns_none() {
        assert!(unprotect("!!!not-base64-valid!!@@").is_none());
        assert!(unprotect("").is_none());
        // Valid base64 but garbage bytes — DPAPI returns failure.
        let garbage = B64.encode(b"definitely not a real DPAPI blob");
        assert!(unprotect(&garbage).is_none());
    }
}
