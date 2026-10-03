//! OpenAI API key storage in the macOS login Keychain.

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
