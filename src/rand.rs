use std::os::raw::c_void;

#[cfg(target_os = "windows")]
mod platform_rng {
    use super::*;
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        // NTSTATUS BCryptGenRandom(BCRYPT_ALG_HANDLE hAlgorithm, PUCHAR pbBuffer, ULONG cbBuffer, ULONG dwFlags);
        fn BCryptGenRandom(hAlgorithm: *mut c_void, pbBuffer: *mut u8, cbBuffer: u32, dwFlags: u32) -> i32;
    }

    pub fn fill_bytes(buf: &mut [u8]) -> Result<(), String> {
        let res = unsafe { BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32, 0x00000002) };
        if res == 0 { Ok(()) } else { Err(format!("BCryptGenRandom failed: status=0x{:X}", res)) }
    }
}

#[cfg(not(target_os = "windows"))]
mod platform_rng {
    use super::*;
    pub fn fill_bytes(buf: &mut [u8]) -> Result<(), String> {
        use std::io::Read;
        use std::fs::File;
        // Try reading from /dev/urandom
        match File::open("/dev/urandom") {
            Ok(mut f) => {
                f.read_exact(buf).map_err(|e| format!("read /dev/urandom failed: {}", e))
            }
            Err(e) => Err(format!("open /dev/urandom failed: {}", e)),
        }
    }
}

/// Returns a secure random u64 in range [0, max)
pub fn secure_random_u64(max: u64) -> Result<u64, String> {
    if max == 0 { return Err("secure_random: max must be > 0".to_string()); }
    // rejection sampling to avoid modulo bias
    let mut buf = [0u8; 8];
    loop {
        platform_rng::fill_bytes(&mut buf)?;
        let v = u64::from_le_bytes(buf);
        // if max is power-of-two, simple mask works, but we do generic rejection
        let limit = u64::MAX - (u64::MAX % max);
        if v < limit {
            return Ok(v % max);
        }
        // otherwise retry
    }
}
