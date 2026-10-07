//! OpenAI API key storage: the macOS login Keychain, or Windows Credential Manager.

pub use imp::{delete, load, store};

#[cfg(target_os = "macos")]
mod imp {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};

    const SERVICE: &str = "com.pastepilot.app";
    const ACCOUNT: &str = "openai-api-key";

    pub fn load() -> Option<String> {
        let bytes = get_generic_password(SERVICE, ACCOUNT).ok()?;
        String::from_utf8(bytes).ok().filter(|k| !k.is_empty())
    }

    pub fn store(key: &str) -> Result<(), String> {
        set_generic_password(SERVICE, ACCOUNT, key.as_bytes()).map_err(|e| format!("Keychain error: {e}"))
    }

    pub fn delete() {
        let _ = delete_generic_password(SERVICE, ACCOUNT);
    }
}

#[cfg(windows)]
mod imp {
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Security::Credentials::{
        CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    /// Shows up under Windows Credentials in Credential Manager.
    const TARGET: &str = "com.pastepilot.app/openai-api-key";

    pub fn load() -> Option<String> {
        let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
        unsafe {
            CredReadW(&HSTRING::from(TARGET), CRED_TYPE_GENERIC, None, &mut cred).ok()?;
            let c = &*cred;
            let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec();
            CredFree(cred as *const _);
            String::from_utf8(bytes).ok().filter(|k| !k.is_empty())
        }
    }

    pub fn store(key: &str) -> Result<(), String> {
        let mut target: Vec<u16> = TARGET.encode_utf16().chain([0]).collect();
        let mut blob = key.as_bytes().to_vec();
        let cred = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: PWSTR(target.as_mut_ptr()),
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        unsafe { CredWriteW(&cred, 0) }.map_err(|e| format!("Credential Manager error: {e}"))
    }

    pub fn delete() {
        let _ = unsafe { CredDeleteW(&HSTRING::from(TARGET), CRED_TYPE_GENERIC, None) };
    }
}
