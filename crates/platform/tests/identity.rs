#![cfg(windows)]
use windows::{
    Win32::{
        Foundation::GENERIC_WRITE,
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_MODE,
            OPEN_EXISTING,
        },
        System::IO::DeviceIoControl,
    },
    core::PCWSTR,
};
use witvoice_platform::identity::*;

#[test]
fn real_local_junction_rejected_and_cleanup_does_not_follow_target() {
    use std::os::windows::io::FromRawHandle;
    let root = root("junction");
    let target = root.join("target");
    let link = root.join("junction");
    assert!(root.is_absolute() && target.starts_with(&root) && link.starts_with(&root));
    std::fs::create_dir(&target).unwrap();
    std::fs::create_dir(&link).unwrap();
    std::fs::write(target.join("sentinel"), b"keep target").unwrap();
    let sub: Vec<u16> = format!(r"\??\{}", target.display())
        .encode_utf16()
        .collect();
    let print: Vec<u16> = target
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .collect();
    let data_len = 8 + (sub.len() + 1 + print.len() + 1) * 2;
    let mut buffer = Vec::with_capacity(data_len + 8);
    buffer.extend_from_slice(&0xa0000003u32.to_le_bytes()); // IO_REPARSE_TAG_MOUNT_POINT
    buffer.extend_from_slice(&(data_len as u16).to_le_bytes());
    buffer.extend_from_slice(&0u16.to_le_bytes());
    for field in [
        0u16,
        (sub.len() * 2) as u16,
        ((sub.len() + 1) * 2) as u16,
        (print.len() * 2) as u16,
    ] {
        buffer.extend_from_slice(&field.to_le_bytes());
    }
    for ch in sub.into_iter().chain(Some(0)).chain(print).chain(Some(0)) {
        buffer.extend_from_slice(&ch.to_le_bytes());
    }
    let wide: Vec<u16> = link
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // SAFETY: owned empty directory; fixed mount-point buffer; no privileges enabled.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            GENERIC_WRITE.0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
        .unwrap()
    };
    let file = unsafe { std::fs::File::from_raw_handle(handle.0) };
    let mut returned = 0;
    let result = unsafe {
        DeviceIoControl(
            handle,
            0x000900a4,
            Some(buffer.as_ptr().cast()),
            buffer.len() as u32,
            None,
            0,
            Some(&mut returned),
            None,
        )
    }; // FSCTL_SET_REPARSE_POINT
    drop(file);
    if let Err(error) = result {
        std::fs::remove_dir(&link).unwrap();
        std::fs::remove_file(target.join("sentinel")).unwrap();
        std::fs::remove_dir(&target).unwrap();
        std::fs::remove_dir(&root).unwrap();
        panic!(
            "ordinary-user junction creation failed: {:#x}",
            error.code().0
        );
    }
    assert_eq!(
        std::fs::read(link.join("sentinel")).unwrap(),
        b"keep target"
    );
    assert!(IdentityStore::open(&link).is_err());
    std::fs::create_dir(target.join("nested")).unwrap();
    assert!(IdentityStore::open(&link.join("nested")).is_err());
    // Nonrecursive unlink of the junction; no target traversal during cleanup.
    std::fs::remove_dir(&link).unwrap();
    assert_eq!(
        std::fs::read(target.join("sentinel")).unwrap(),
        b"keep target"
    );
    std::fs::remove_dir(target.join("nested")).unwrap();
    std::fs::remove_file(target.join("sentinel")).unwrap();
    std::fs::remove_dir(&target).unwrap();
    std::fs::remove_dir(&root).unwrap();
}

#[test]
fn protected_current_user_dacl_is_actual_on_directory_and_key_file() {
    use windows::{
        Win32::{
            Foundation::{HLOCAL, LocalFree},
            Security::{
                Authorization::ConvertSecurityDescriptorToStringSecurityDescriptorW,
                DACL_SECURITY_INFORMATION, GetFileSecurityW, PSECURITY_DESCRIPTOR,
            },
        },
        core::PWSTR,
    };
    let root = root("acl");
    let store = IdentityStore::open(&root).unwrap();
    store.save_new(b"test").unwrap();
    let sid = witvoice_platform::current_user_sid().unwrap();
    for path in [&root, &root.join(KEY_FILE)] {
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let mut needed = 0;
        unsafe {
            let _ = GetFileSecurityW(
                PCWSTR(wide.as_ptr()),
                DACL_SECURITY_INFORMATION.0,
                None,
                0,
                &mut needed,
            );
        }
        assert!((1..4096).contains(&needed));
        let mut bytes = vec![0u64; (needed as usize).div_ceil(8)];
        let sd = PSECURITY_DESCRIPTOR(bytes.as_mut_ptr().cast());
        unsafe {
            GetFileSecurityW(
                PCWSTR(wide.as_ptr()),
                DACL_SECURITY_INFORMATION.0,
                Some(sd),
                needed,
                &mut needed,
            )
            .ok()
            .unwrap();
        }
        let mut text = PWSTR::null();
        let mut size = 0;
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd,
                1,
                DACL_SECURITY_INFORMATION,
                &mut text,
                Some(&mut size),
            )
            .unwrap();
        }
        let sddl = unsafe {
            String::from_utf16(std::slice::from_raw_parts(text.0, size as usize - 1)).unwrap()
        };
        unsafe {
            let _ = LocalFree(Some(HLOCAL(text.0.cast())));
        }
        assert!(sddl.starts_with("D:P"));
        assert_eq!(sddl.matches("(A;").count(), 1);
        assert!(sddl.contains(&format!(";;;{sid})")));
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
fn root(label: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".local")
        .join(format!("t016-platform-{label}-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    path
}
#[test]
fn dpapi_real_roundtrip_fixed_file_no_overwrite_and_bounds() {
    let root = root("roundtrip");
    let store = IdentityStore::open(&root).unwrap();
    let plaintext = b"test-private-key-material-only-not-real-user-key";
    store.save_new(plaintext).unwrap();
    assert_eq!(store.load().unwrap().as_bytes(), plaintext);
    let sealed = std::fs::read(root.join(KEY_FILE)).unwrap();
    assert!(!sealed.windows(plaintext.len()).any(|s| s == plaintext));
    assert!(store.save_new(b"overwrite").is_err());
    assert!(store.save_new(&[]).is_err());
    assert!(store.save_new(&vec![0; MAX_SECRET_BYTES + 1]).is_err());
    drop(store);
    let reopened = IdentityStore::open(&root).unwrap();
    assert_eq!(reopened.load().unwrap().as_bytes(), plaintext);
    drop(reopened);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn corrupt_oversized_and_hard_link_files_rejected() {
    let root = root("corrupt");
    let store = IdentityStore::open(&root).unwrap();
    store.save_new(b"test").unwrap();
    std::fs::hard_link(root.join(KEY_FILE), root.join("alias")).unwrap();
    assert!(store.load().is_err());
    std::fs::remove_file(root.join("alias")).unwrap();
    let mut bytes = std::fs::read(root.join(KEY_FILE)).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    std::fs::write(root.join(KEY_FILE), bytes).unwrap();
    assert!(store.load().is_err());
    std::fs::write(root.join(KEY_FILE), vec![0; MAX_SEALED_BYTES + 1]).unwrap();
    assert!(store.load().is_err());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn reject_traversal_unc_device_and_non_directory_roots() {
    let root = root("paths");
    for path in [
        root.join(".."),
        std::path::PathBuf::from("relative"),
        std::path::PathBuf::from(r"\\localhost\share"),
        std::path::PathBuf::from(r"\\?\D:\Project\witvoice\.local"),
    ] {
        assert!(IdentityStore::open(&path).is_err());
    }
    std::fs::write(root.join("file"), b"not directory").unwrap();
    assert!(IdentityStore::open(&root.join("file")).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
