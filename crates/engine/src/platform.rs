use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use url::Url;

pub fn checked_url(value: &str) -> Result<Url> {
    if value.len() > 16384 {
        bail!("url_too_large");
    }
    let url = Url::parse(value).map_err(|_| anyhow::anyhow!("invalid_url"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        bail!("unsupported_protocol");
    }
    if !url.username().is_empty() || url.password().is_some() {
        bail!("url_credentials_unsupported");
    }
    Ok(url)
}

pub fn display_url(url: &Url) -> String {
    let mut redacted = url.clone();
    redacted.set_query(None);
    redacted.set_fragment(None);
    redacted.to_string()
}

pub fn destination(path: &Path) -> Result<std::path::PathBuf> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let name = absolute
        .file_name()
        .and_then(|x| x.to_str())
        .context("invalid_filename")?;
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if name.is_empty()
        || name.ends_with(['.', ' '])
        || name.contains([':', '<', '>', '"', '|', '?', '*'])
        || name.chars().any(|c| c.is_control())
        || reserved
    {
        bail!("invalid_filename");
    }
    let parent = absolute
        .parent()
        .context("invalid_destination")?
        .canonicalize()
        .context("destination_folder_missing")?;
    let output = parent.join(name);
    if output.exists() {
        bail!("destination_exists");
    }
    Ok(output)
}

#[cfg(windows)]
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData},
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len().try_into()?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI allocates output; copy it before releasing it with LocalFree.
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        bail!("credential_protection_failed");
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(bytes)
}

#[cfg(windows)]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
        },
    };
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len().try_into()?,
        pbData: data.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null_mut(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        bail!("credential_unprotection_failed");
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize) }.to_vec();
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(bytes)
}

#[cfg(not(windows))]
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    // Portable fixture builds do not accept token-bearing URLs; production target is Windows.
    let text = std::str::from_utf8(data)?;
    if text.contains('?') {
        bail!("secret_storage_requires_windows");
    }
    Ok(data.to_vec())
}
#[cfg(not(windows))]
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}

pub fn write_at(file: &File, mut data: &[u8], mut offset: u64) -> std::io::Result<()> {
    while !data.is_empty() {
        #[cfg(windows)]
        let count = {
            use std::os::windows::fs::FileExt;
            file.seek_write(data, offset)?
        };
        #[cfg(unix)]
        let count = {
            use std::os::unix::fs::FileExt;
            file.write_at(data, offset)?
        };
        if count == 0 {
            return Err(std::io::ErrorKind::WriteZero.into());
        }
        offset += count as u64;
        data = &data[count..];
    }
    Ok(())
}

pub fn hash_extent(file: &File, offset: u64, length: u64) -> Result<String> {
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(offset))?;
    let mut remaining = length;
    let mut buffer = [0u8; 65536];
    let mut hash = Sha256::new();
    while remaining > 0 {
        let limit = remaining.min(buffer.len() as u64) as usize;
        let count = reader.read(&mut buffer[..limit])?;
        if count == 0 {
            bail!("staging_truncated");
        }
        hash.update(&buffer[..count]);
        remaining -= count as u64;
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(windows)]
pub fn publish(staging: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
    let from: Vec<u16> = staging.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Omitting REPLACE_EXISTING makes publication fail on a raced destination collision.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        bail!("publication_failed");
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn publish(staging: &Path, destination: &Path) -> Result<()> {
    std::fs::hard_link(staging, destination).context("publication_failed")?;
    std::fs::remove_file(staging)?;
    Ok(())
}

pub fn is_reparse(path: &Path) -> Result<bool> {
    let metadata = path.symlink_metadata()?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        Ok(metadata.file_attributes() & 0x400 != 0)
    }
    #[cfg(not(windows))]
    {
        Ok(metadata.file_type().is_symlink())
    }
}

pub fn file_id(file: &File) -> Result<String> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
            bail!("file_identity_unavailable");
        }
        Ok(format!(
            "{}:{}:{}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = file.metadata()?;
        Ok(format!("{}:{}", m.dev(), m.ino()))
    }
}

pub fn mark_origin(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::io::Write;
        let stream = format!(
            "{}:Zone.Identifier",
            path.to_str().context("non_unicode_path")?
        );
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(stream)
            .context("attachment_metadata_failed")?;
        file.write_all(b"[ZoneTransfer]\r\nZoneId=3\r\n")?;
        file.sync_all()?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secrets_do_not_enter_display_url() {
        let url = checked_url("https://example.test/file?token=secret#private").unwrap();
        assert_eq!(display_url(&url), "https://example.test/file");
        assert!(checked_url("https://user:secret@example.test/file").is_err());
        assert!(checked_url("file:///C:/Windows/test").is_err());
    }
    #[test]
    fn rejects_windows_device_and_stream_names() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["CON.txt", "LPT1", "x:stream", "bad.", "bad "] {
            assert!(destination(&dir.path().join(name)).is_err(), "{name}");
        }
    }
    #[test]
    fn protected_context_round_trips() {
        let original = b"https://example.test/file";
        assert_eq!(unprotect(&protect(original).unwrap()).unwrap(), original);
    }
}
