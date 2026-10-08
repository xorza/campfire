#![expect(
    unsafe_code,
    reason = "the Win32 calls of the platform layer, the one module of the workspace that makes them"
)]

use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{self, Path};
use std::ptr;

use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE, HLOCAL, WIN32_ERROR};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo,
    SDDL_REVISION_1, SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation,
    LookupAccountSidW, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    SID_NAME_USE, TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_CREATION_DISPOSITION,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FlushFileBuffers, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    MoveFileExW, OPEN_ALWAYS, OPEN_EXISTING,
};
use windows::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, ACCESS_ALLOWED_CALLBACK_ACE_TYPE,
    ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE, ACCESS_ALLOWED_COMPOUND_ACE_TYPE,
    ACCESS_ALLOWED_OBJECT_ACE_TYPE,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::core::{Error, HSTRING, Owned, PWSTR};

/// The Windows side of the platform layer: a security descriptor given as a file or directory is
/// made keeps it its owner's only, `MoveFileExW` writes a rename through, and `FlushFileBuffers`
/// on a directory's handle keeps its names.
#[derive(Debug)]
pub(crate) struct Os;

/// Whom the one entry of an owner-only descriptor reaches: the object alone, a file, or also
/// the files and directories a directory comes to hold.
#[derive(Debug, Clone, Copy)]
enum Reach {
    Object,
    Children,
}

/// The accounts besides the user that a private key's file may name, as OpenSSH for Windows lets
/// them: `LocalSystem`, the Administrators group and `TrustedInstaller`, which can take any file.
const TRUSTED: [&str; 3] = [
    "S-1-5-18",
    "S-1-5-32-544",
    "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464",
];

/// The share mode std's `OpenOptions` gives a file, so a file made here opens as std's do.
const SHARE: FILE_SHARE_MODE =
    FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0);

/// From this many UTF-16 units, std passes a path in its verbatim form, past Win32's 260; a
/// directory's path leaves room for an 8.3 name.
const LONG: usize = 248;

impl Os {
    pub(crate) fn create_owner_only(path: &Path) -> io::Result<File> {
        Os::create_file(path, CREATE_NEW)
    }

    pub(crate) fn open_owner_only(path: &Path) -> io::Result<File> {
        Os::create_file(path, OPEN_ALWAYS)
    }

    /// The file at `path` for writing, as `disposition` makes or opens it, its owner's only when
    /// made.
    fn create_file(path: &Path, disposition: FILE_CREATION_DISPOSITION) -> io::Result<File> {
        let path = wide(path)?;
        let descriptor = owner_only(Reach::Object)?;
        let attributes = attributes(&descriptor);
        // SAFETY: `attributes` and the descriptor it points to live through the call.
        let handle = unsafe {
            CreateFileW(
                &path,
                GENERIC_WRITE.0,
                SHARE,
                Some(&raw const attributes),
                disposition,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(io_error)?;
        // SAFETY: an open handle that nothing else owns.
        Ok(unsafe { File::from_raw_handle(handle.0) })
    }

    pub(crate) fn create_dir_owner_only(path: &Path) -> io::Result<()> {
        let path = wide(path)?;
        // Its entry is inherited, so a file made in it by another way is its owner's only too.
        let descriptor = owner_only(Reach::Children)?;
        let attributes = attributes(&descriptor);
        // SAFETY: as in `create_file`.
        unsafe { CreateDirectoryW(&path, Some(&raw const attributes)) }.map_err(io_error)
    }

    pub(crate) fn exposure(file: &File) -> io::Result<Option<String>> {
        let mut owner = PSID::default();
        let mut dacl = ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: the handle stays open while `file` lives, and each out pointer is valid.
        unsafe {
            GetSecurityInfo(
                HANDLE(file.as_raw_handle()),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                Some(&raw mut owner),
                None,
                Some(&raw mut dacl),
                None,
                Some(&raw mut descriptor),
            )
        }
        .ok()
        .map_err(io_error)?;
        // SAFETY: the descriptor the call allocated, which this alone frees, as the function ends.
        let _descriptor = unsafe { Owned::new(HLOCAL(descriptor.0)) };
        Os::exposed(owner, dacl)
    }

    /// Who besides the user may open a file whose owner is `owner` and whose DACL is `dacl`, as
    /// OpenSSH for Windows judges a private key: its owner and each entry that allows access must
    /// name the user or a trusted account, and a file with no DACL is open to all. An allowing
    /// entry of a kind OpenSSH passes over, which names its account another way, is taken as
    /// another's. Both point into a security descriptor that lives through the call.
    fn exposed(owner: PSID, dacl: *const ACL) -> io::Result<Option<String>> {
        let user = user_sid()?;
        let trusted = |sid: &str| sid == user || TRUSTED.contains(&sid);
        let mut others = Vec::new();
        let owner_sid = sid_text(owner)?;
        if !trusted(&owner_sid) {
            others.push(format!("{} (its owner)", account(owner, &owner_sid)));
        }
        if dacl.is_null() {
            others.push("everyone, as it has no DACL".to_owned());
        } else {
            // SAFETY: a DACL the descriptor holds.
            let count = unsafe { (*dacl).AceCount };
            for index in 0..u32::from(count) {
                let mut ace = ptr::null_mut();
                // SAFETY: an index below the DACL's count, and a valid out pointer.
                unsafe { GetAce(dacl, index, &raw mut ace) }.map_err(io_error)?;
                // SAFETY: every entry starts with its header.
                let kind = u32::from(unsafe { (*ace.cast::<ACE_HEADER>()).AceType });
                match kind {
                    ACCESS_ALLOWED_ACE_TYPE | ACCESS_ALLOWED_CALLBACK_ACE_TYPE => {
                        // SAFETY: an entry of either kind is laid out as an `ACCESS_ALLOWED_ACE`,
                        // whose SID starts at its `SidStart`.
                        let sid = PSID(unsafe {
                            (&raw mut (*ace.cast::<ACCESS_ALLOWED_ACE>()).SidStart).cast()
                        });
                        let text = sid_text(sid)?;
                        if !trusted(&text) {
                            others.push(account(sid, &text));
                        }
                    }
                    ACCESS_ALLOWED_OBJECT_ACE_TYPE
                    | ACCESS_ALLOWED_CALLBACK_OBJECT_ACE_TYPE
                    | ACCESS_ALLOWED_COMPOUND_ACE_TYPE => {
                        others.push(format!("an account an entry of kind {kind} lets in"));
                    }
                    _ => {}
                }
            }
        }
        others.sort_unstable();
        others.dedup();
        Ok((!others.is_empty()).then(|| others.join(", ")))
    }

    pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (wide(from)?, wide(to)?);
        // SAFETY: two paths, which live through the call.
        unsafe {
            MoveFileExW(
                &from,
                &to,
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(io_error)
    }

    pub(crate) fn sync_dir(directory: &Path) -> io::Result<()> {
        let path = wide(directory)?;
        // SAFETY: a path that lives through the call, and no security attributes.
        let handle = unsafe {
            CreateFileW(
                &path,
                GENERIC_WRITE.0,
                SHARE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )
        }
        .map_err(io_error)?;
        // SAFETY: an open handle that nothing else owns.
        let handle = unsafe { Owned::new(handle) };
        // SAFETY: the handle is open, with write access, until it drops.
        unsafe { FlushFileBuffers(*handle) }.map_err(io_error)
    }
}

/// A security descriptor whose DACL, protected from inheritance, lets the user alone in, with
/// its one entry reaching as `reach` says.
fn owner_only(reach: Reach) -> io::Result<Owned<HLOCAL>> {
    let user = user_sid()?;
    let flags = match reach {
        Reach::Object => "",
        Reach::Children => "OICI",
    };
    descriptor(&format!("D:P(A;{flags};FA;;;{user})"))
}

/// The security descriptor that `sddl` spells.
fn descriptor(sddl: &str) -> io::Result<Owned<HLOCAL>> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: a text that lives through the call, and a valid out pointer; no size is asked for.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &HSTRING::from(sddl),
            SDDL_REVISION_1,
            &raw mut descriptor,
            None,
        )
    }
    .map_err(io_error)?;
    // SAFETY: the descriptor the call allocated, which this alone frees.
    Ok(unsafe { Owned::new(HLOCAL(descriptor.0)) })
}

/// Security attributes that give an object `descriptor`, which must outlive them.
fn attributes(descriptor: &Owned<HLOCAL>) -> SECURITY_ATTRIBUTES {
    SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
            .expect("the attributes' size fits its field"),
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    }
}

/// `path` as Win32 takes it: absolute, and in its verbatim form when long, as std passes it.
fn wide(path: &Path) -> io::Result<HSTRING> {
    let units = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
    let mut text: Vec<u16> = path::absolute(path)?.as_os_str().encode_wide().collect();
    let (verbatim, device, share) = (units(r"\\?\"), units(r"\\.\"), units(r"\\"));
    if text.len() >= LONG && !text.starts_with(&verbatim) && !text.starts_with(&device) {
        text = match text.strip_prefix(share.as_slice()) {
            Some(rest) => units(r"\\?\UNC\")
                .into_iter()
                .chain(rest.iter().copied())
                .collect(),
            None => verbatim.into_iter().chain(text).collect(),
        };
    }
    Ok(HSTRING::from_wide(&text))
}

/// The SID of the user this process runs as, as text: `S-1-5-21-…`.
fn user_sid() -> io::Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: the pseudo-handle of this process, and a valid out pointer.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) }
        .map_err(io_error)?;
    // SAFETY: a handle the call opened, which nothing else owns.
    let token = unsafe { Owned::new(token) };
    let mut length = 0;
    // SAFETY: a query of the size alone, with no buffer: it fails, and gives the size.
    let sized = unsafe { GetTokenInformation(*token, TokenUser, None, 0, &raw mut length) };
    debug_assert!(sized.is_err(), "a size query fails");
    // In words of 8 bytes, so the `TOKEN_USER` at its start is aligned.
    let words = usize::try_from(length)
        .expect("a token's user fits memory")
        .div_ceil(8);
    let mut buffer = vec![0_u64; words];
    // SAFETY: a buffer of at least `length` bytes, and a valid out pointer.
    unsafe {
        GetTokenInformation(
            *token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &raw mut length,
        )
    }
    .map_err(io_error)?;
    // SAFETY: the call wrote a `TOKEN_USER` at the buffer's start, whose SID lies in the buffer,
    // which lives through the use.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_text(user.User.Sid)
}

/// The text of `sid`, a SID that lives through the call.
fn sid_text(sid: PSID) -> io::Result<String> {
    let mut text = PWSTR::null();
    // SAFETY: a valid SID and a valid out pointer.
    unsafe { ConvertSidToStringSidW(sid, &raw mut text) }.map_err(io_error)?;
    // SAFETY: the text the call allocated, which this alone frees, as the function ends.
    let _owned = unsafe { Owned::new(HLOCAL(text.0.cast())) };
    // SAFETY: a text that ends in a nul, which lives until `_owned` drops.
    unsafe { text.to_string() }.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// The account `sid` names, as `DOMAIN\name`, or `text`, its SID's text, when it names none this
/// machine knows.
fn account(sid: PSID, text: &str) -> String {
    let (mut name, mut domain) = ([0_u16; 256], [0_u16; 256]);
    let (mut name_length, mut domain_length) = (256_u32, 256_u32);
    let mut kind = SID_NAME_USE::default();
    // SAFETY: a valid SID, and buffers of the lengths given.
    let found = unsafe {
        LookupAccountSidW(
            None,
            sid,
            Some(PWSTR(name.as_mut_ptr())),
            &raw mut name_length,
            Some(PWSTR(domain.as_mut_ptr())),
            &raw mut domain_length,
            &raw mut kind,
        )
    };
    if found.is_err() {
        return text.to_owned();
    }
    let read = |units: &[u16], length: u32| {
        let length = usize::try_from(length).expect("a name fits memory");
        String::from_utf16_lossy(&units[..length.min(units.len())])
    };
    let (name, domain) = (read(&name, name_length), read(&domain, domain_length));
    if domain.is_empty() {
        name
    } else {
        format!("{domain}\\{name}")
    }
}

/// `error` as std's `io::Error`, with the Win32 code its HRESULT wraps, so std gives it its kind,
/// `AlreadyExists` as an example; the crate's own conversion keeps the HRESULT, whose kind std
/// does not know.
fn io_error(error: Error) -> io::Error {
    match WIN32_ERROR::from_error(&error) {
        Some(code) => io::Error::from_raw_os_error(code.0.cast_signed()),
        None => io::Error::other(error),
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::io;
    use std::os::windows::fs::symlink_file;
    use std::path::Path;
    use std::ptr;

    use windows::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR,
    };
    use windows::core::BOOL;

    use crate::platform::os::{Os, descriptor, io_error, user_sid, wide};

    impl Os {
        pub(crate) fn expose(path: &Path) -> io::Result<()> {
            let user = user_sid()?;
            let descriptor = descriptor(&format!("D:P(A;;FA;;;{user})(A;;FR;;;BU)"))?;
            let (mut present, mut defaulted) = (BOOL::default(), BOOL::default());
            let mut dacl = ptr::null_mut();
            // SAFETY: a descriptor that lives through the use of its DACL, and valid out
            // pointers.
            unsafe {
                GetSecurityDescriptorDacl(
                    PSECURITY_DESCRIPTOR(descriptor.0),
                    &raw mut present,
                    &raw mut dacl,
                    &raw mut defaulted,
                )
            }
            .map_err(io_error)?;
            let path = wide(path)?;
            // SAFETY: a path that lives through the call, and the DACL that `descriptor` holds.
            let set = unsafe {
                SetNamedSecurityInfoW(
                    &path,
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(dacl),
                    None,
                )
            };
            drop(descriptor);
            set.ok().map_err(io_error)
        }

        pub(crate) fn link_file(target: &Path, link: &Path) -> io::Result<()> {
            symlink_file(target, link)
        }
    }
}

#[cfg(test)]
pub(crate) mod test_access {
    use std::io;
    use std::path::Path;
    use std::ptr;

    use windows::Win32::Foundation::HLOCAL;
    use windows::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    };
    use windows::core::Owned;

    use crate::platform::os::{Os, io_error, wide};

    impl Os {
        pub(crate) fn exposure_at(path: &Path) -> io::Result<Option<String>> {
            let path = wide(path)?;
            let mut owner = PSID::default();
            let mut dacl = ptr::null_mut();
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            // SAFETY: a path that lives through the call, and valid out pointers.
            unsafe {
                GetNamedSecurityInfoW(
                    &path,
                    SE_FILE_OBJECT,
                    OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
                    Some(&raw mut owner),
                    None,
                    Some(&raw mut dacl),
                    None,
                    &raw mut descriptor,
                )
            }
            .ok()
            .map_err(io_error)?;
            // SAFETY: the descriptor the call allocated, which this alone frees, as the function
            // ends.
            let _descriptor = unsafe { Owned::new(HLOCAL(descriptor.0)) };
            Os::exposed(owner, dacl)
        }
    }
}
