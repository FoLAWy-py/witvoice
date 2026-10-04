//! Fixed-file, current-user DPAPI storage. Trusted launcher supplies the root;
//! the root/path must never come from an IPC client. No network or UI prompts.
pub const MAX_SECRET_BYTES: usize = 32 * 1024;
pub const MAX_SEALED_BYTES: usize = 64 * 1024;
pub const KEY_FILE: &str = "identity.v1.dpapi";

/// Intentionally no Debug/Clone. Wiping is best effort, not locked-memory protection.
pub struct SecretBytes(Vec<u8>);
impl SecretBytes {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}
impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        fs::File,
        io::{self, Read, Write},
        mem,
        os::windows::{
            fs::MetadataExt,
            io::{AsRawHandle, FromRawHandle},
        },
        path::{Component, Path, PathBuf},
    };
    use windows::{
        Win32::{
            Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree},
            Security::{
                Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW,
                Cryptography::{
                    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData,
                    CryptUnprotectData,
                },
                DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
                PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, SetFileSecurityW,
            },
            Storage::FileSystem::{
                BY_HANDLE_FILE_INFORMATION, CREATE_NEW, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS,
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE,
                GetFileInformationByHandle, OPEN_EXISTING,
            },
        },
        core::PCWSTR,
    };
    fn invalid() -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, "identity storage rejected")
    }
    fn api(_: windows::core::Error) -> io::Error {
        io::Error::other("identity Windows API failed")
    }
    fn wide(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }
    struct Allocation(*mut core::ffi::c_void);
    impl Drop for Allocation {
        fn drop(&mut self) {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
    fn descriptor() -> io::Result<(PSECURITY_DESCRIPTOR, Allocation)> {
        let sid = crate::current_user_sid()?;
        let sddl: Vec<u16> = format!("D:P(A;OICI;GA;;;{sid})")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut result = PSECURITY_DESCRIPTOR::default();
        // SAFETY: valid NUL-terminated SDDL; allocation returned by Windows is owned here.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(sddl.as_ptr()),
                1,
                &mut result,
                None,
            )
            .map_err(api)?;
        }
        Ok((result, Allocation(result.0)))
    }
    fn crypt(input: &[u8], protect: bool) -> io::Result<SecretBytes> {
        let limit = if protect {
            MAX_SECRET_BYTES
        } else {
            MAX_SEALED_BYTES
        };
        if input.is_empty() || input.len() > limit {
            return Err(invalid());
        }
        let incoming = CRYPT_INTEGER_BLOB {
            cbData: input.len() as u32,
            pbData: input.as_ptr().cast_mut(),
        };
        let entropy = b"witvoice.identity.dpapi.current-user.v1";
        let entropy = CRYPT_INTEGER_BLOB {
            cbData: entropy.len() as u32,
            pbData: entropy.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: input/entropy buffers live through the synchronous call; no UI, no machine flag.
        unsafe {
            if protect {
                CryptProtectData(
                    &incoming,
                    PCWSTR::null(),
                    Some(&entropy),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .map_err(api)?;
            } else {
                CryptUnprotectData(
                    &incoming,
                    None,
                    Some(&entropy),
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
                .map_err(api)?;
            }
            let allocation = Allocation(output.pbData.cast());
            let maximum = if protect {
                MAX_SEALED_BYTES
            } else {
                MAX_SECRET_BYTES
            };
            if output.pbData.is_null() || output.cbData == 0 || output.cbData as usize > maximum {
                return Err(invalid());
            }
            let slice = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
            let bytes = slice.to_vec();
            slice.fill(0);
            drop(allocation);
            Ok(SecretBytes(bytes))
        }
    }
    pub struct IdentityStore {
        root: PathBuf,
        // No FILE_SHARE_DELETE: every existing ancestor and root stays pinned against rename.
        _directories: Vec<File>,
    }
    impl IdentityStore {
        pub fn open(trusted_root: &Path) -> io::Result<Self> {
            if !trusted_root.is_absolute() || trusted_root.as_os_str().len() > 240 {
                return Err(invalid());
            }
            let mut current = PathBuf::new();
            let mut directories = Vec::new();
            for component in trusted_root.components() {
                match component {
                    Component::Prefix(p) => {
                        if !matches!(p.kind(), std::path::Prefix::Disk(_)) {
                            return Err(invalid());
                        }
                        current.push(p.as_os_str());
                    }
                    Component::RootDir => current.push(component.as_os_str()),
                    Component::Normal(name) => {
                        let text = name.to_string_lossy();
                        if text.contains(':') || text.ends_with(['.', ' ']) || text.contains('\0') {
                            return Err(invalid());
                        }
                        current.push(name);
                        let name = wide(&current);
                        // SAFETY: directories opened for metadata only without following reparse points.
                        let handle = unsafe {
                            CreateFileW(
                                PCWSTR(name.as_ptr()),
                                0,
                                FILE_SHARE_READ | FILE_SHARE_WRITE,
                                None,
                                OPEN_EXISTING,
                                FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
                                None,
                            )
                            .map_err(api)?
                        };
                        let file = unsafe { File::from_raw_handle(handle.0) };
                        let metadata = file.metadata()?;
                        if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
                            return Err(invalid());
                        }
                        directories.push(file);
                    }
                    _ => return Err(invalid()),
                }
            }
            if directories.is_empty() {
                return Err(invalid());
            }
            let (sd, _allocation) = descriptor()?;
            let name = wide(trusted_root);
            // SAFETY: pinned root path cannot be replaced; SD remains allocated for the call.
            unsafe {
                SetFileSecurityW(
                    PCWSTR(name.as_ptr()),
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    sd,
                )
                .ok()
                .map_err(api)?;
            }
            Ok(Self {
                root: trusted_root.to_owned(),
                _directories: directories,
            })
        }
        pub fn save_new(&self, secret: &[u8]) -> io::Result<()> {
            let sealed = crypt(secret, true)?;
            let (sd, _allocation) = descriptor()?;
            let security = SECURITY_ATTRIBUTES {
                nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            let name = wide(&self.root.join(KEY_FILE));
            // SAFETY: fixed name in pinned root, exclusive CREATE_NEW, ACL at creation, noninheritable.
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_MODE(0),
                    Some(&security),
                    CREATE_NEW,
                    FILE_FLAG_OPEN_REPARSE_POINT,
                    None,
                )
                .map_err(api)?
            };
            let mut file = unsafe { File::from_raw_handle(handle.0) };
            file.write_all(sealed.as_bytes())?;
            file.sync_all()
        }
        pub fn load(&self) -> io::Result<SecretBytes> {
            let name = wide(&self.root.join(KEY_FILE));
            // SAFETY: fixed root/name, no reparse traversal and no concurrent writers/delete.
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(name.as_ptr()),
                    GENERIC_READ.0,
                    FILE_SHARE_READ,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OPEN_REPARSE_POINT,
                    None,
                )
                .map_err(api)?
            };
            let mut file = unsafe { File::from_raw_handle(handle.0) };
            let metadata = file.metadata()?;
            if !metadata.is_file()
                || metadata.file_attributes() & 0x400 != 0
                || metadata.len() == 0
                || metadata.len() > MAX_SEALED_BYTES as u64
            {
                return Err(invalid());
            }
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            unsafe {
                GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info).map_err(api)?;
            }
            if info.nNumberOfLinks != 1 {
                return Err(invalid());
            }
            let mut bytes = Vec::with_capacity(metadata.len() as usize);
            Read::by_ref(&mut file)
                .take((MAX_SEALED_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            if bytes.len() > MAX_SEALED_BYTES {
                return Err(invalid());
            }
            crypt(&bytes, false)
        }
    }
}
#[cfg(windows)]
pub use native::IdentityStore;

#[cfg(not(windows))]
pub struct IdentityStore;
#[cfg(not(windows))]
impl IdentityStore {
    pub fn open(_: &std::path::Path) -> std::io::Result<Self> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "identity protection not implemented on this platform",
        ))
    }
}
