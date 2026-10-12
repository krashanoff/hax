// SPDX-License-Identifier: MIT
use std::ffi::{c_char, c_void, CStr};
use std::ptr;

extern "C" {
    fn malloc(size: usize) -> *mut c_void;
}

const STANDARD: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

// These functions replace the C text primitives. Their buffers must use the C
// allocator because every existing caller releases them with free().
unsafe fn output(bytes: &[u8], out_len: *mut usize) -> *mut c_char {
    let Some(size) = bytes.len().checked_add(1) else {
        return ptr::null_mut();
    };
    let result = unsafe { malloc(size) }.cast::<u8>();
    if result.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), result, bytes.len());
        *result.add(bytes.len()) = 0;
        if !out_len.is_null() {
            *out_len = bytes.len();
        }
    }
    result.cast()
}

fn encode(input: &[u8], alphabet: &[u8; 64], padded: bool) -> Option<Vec<u8>> {
    let groups = input.len().checked_add(2)? / 3;
    let capacity = groups.checked_mul(4)?;
    let mut result = Vec::with_capacity(capacity);
    for chunk in input.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        result.push(alphabet[(first >> 2) as usize]);
        result.push(alphabet[(((first & 3) << 4) | (second >> 4)) as usize]);
        if chunk.len() > 1 {
            result.push(alphabet[(((second & 15) << 2) | (third >> 6)) as usize]);
        } else if padded {
            result.push(b'=');
        }
        if chunk.len() > 2 {
            result.push(alphabet[(third & 63) as usize]);
        } else if padded {
            result.push(b'=');
        }
    }
    Some(result)
}

unsafe fn encode_ffi(
    data: *const c_void,
    len: usize,
    out_len: *mut usize,
    padded: bool,
) -> *mut c_char {
    if data.is_null() && len != 0 {
        return ptr::null_mut();
    }
    let input = if len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    match encode(input, if padded { STANDARD } else { URL }, padded) {
        Some(bytes) => unsafe { output(&bytes, out_len) },
        None => ptr::null_mut(),
    }
}

/// # Safety
/// `data` must point to `len` readable bytes (unless `len` is zero); `out_len` must be writable if non-null.
#[no_mangle]
pub unsafe extern "C" fn base64_encode(
    data: *const c_void,
    len: usize,
    out_len: *mut usize,
) -> *mut c_char {
    unsafe { encode_ffi(data, len, out_len, true) }
}

/// # Safety
/// Same requirements as `base64_encode`.
#[no_mangle]
pub unsafe extern "C" fn base64url_encode(
    data: *const c_void,
    len: usize,
    out_len: *mut usize,
) -> *mut c_char {
    unsafe { encode_ffi(data, len, out_len, false) }
}

fn url_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn decode(input: &[u8]) -> Option<Vec<u8>> {
    let payload = input
        .iter()
        .rposition(|&byte| byte != b'=')
        .map_or(0, |i| i + 1);
    let padding = input.len() - payload;
    let remainder = payload % 4;
    if padding > 2
        || remainder == 1
        || (padding != 0 && (input.len() % 4 != 0 || padding != 4 - remainder))
    {
        return None;
    }
    let capacity = (payload / 4)
        .checked_mul(3)?
        .checked_add(if remainder == 0 { 0 } else { remainder - 1 })?;
    let mut result = Vec::with_capacity(capacity);
    let mut chunks = input[..payload].chunks_exact(4);
    for chunk in &mut chunks {
        let a = url_value(chunk[0])?;
        let b = url_value(chunk[1])?;
        let c = url_value(chunk[2])?;
        let d = url_value(chunk[3])?;
        result.extend_from_slice(&[(a << 2) | (b >> 4), (b << 4) | (c >> 2), (c << 6) | d]);
    }
    let tail = chunks.remainder();
    if !tail.is_empty() {
        let a = url_value(tail[0])?;
        let b = url_value(tail[1])?;
        if tail.len() == 2 && b & 15 != 0 {
            return None;
        }
        result.push((a << 2) | (b >> 4));
        if tail.len() == 3 {
            let c = url_value(tail[2])?;
            if c & 3 != 0 {
                return None;
            }
            result.push((b << 4) | (c >> 2));
        }
    }
    Some(result)
}

/// # Safety
/// `encoded` must point to `encoded_len` readable bytes (unless zero); `out_len` must be writable if non-null.
#[no_mangle]
pub unsafe extern "C" fn base64url_decode(
    encoded: *const c_char,
    encoded_len: usize,
    out_len: *mut usize,
) -> *mut u8 {
    if encoded.is_null() && encoded_len != 0 {
        return ptr::null_mut();
    }
    let input = if encoded_len == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(encoded.cast::<u8>(), encoded_len) }
    };
    match decode(input) {
        Some(bytes) => unsafe { output(&bytes, out_len).cast() },
        None => ptr::null_mut(),
    }
}

/// # Safety
/// `str` must be a NUL-terminated string, or null (treated as empty).
#[no_mangle]
pub unsafe extern "C" fn shell_single_quote(str: *const c_char) -> *mut c_char {
    let input = if str.is_null() {
        &[][..]
    } else {
        unsafe { CStr::from_ptr(str) }.to_bytes()
    };
    let Some(capacity) = input.len().checked_mul(4).and_then(|n| n.checked_add(2)) else {
        return ptr::null_mut();
    };
    let mut quoted = Vec::with_capacity(capacity);
    quoted.push(b'\'');
    for &byte in input {
        if byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push(b'\'');
    unsafe { output(&quoted, ptr::null_mut()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        for (plain, standard, url) in [
            (b"".as_slice(), "", ""),
            (b"f", "Zg==", "Zg"),
            (b"fo", "Zm8=", "Zm8"),
            (b"foo", "Zm9v", "Zm9v"),
            (&[0xff, 0xef, 0xbe], "/+++", "_---"),
        ] {
            assert_eq!(encode(plain, STANDARD, true).unwrap(), standard.as_bytes());
            assert_eq!(encode(plain, URL, false).unwrap(), url.as_bytes());
            assert_eq!(decode(url.as_bytes()).unwrap(), plain);
        }
    }

    #[test]
    fn noncanonical_and_invalid_input() {
        for value in [
            "A", "Zh", "Zm9", "Zg=", "Zg===", "Zm9v=", "Zg+/", "Zg==x", "====",
        ] {
            assert!(decode(value.as_bytes()).is_none(), "{value}");
        }
        assert_eq!(decode(b"Zg=="), Some(b"f".to_vec()));
        assert_eq!(decode(b""), Some(vec![]));
    }
}
