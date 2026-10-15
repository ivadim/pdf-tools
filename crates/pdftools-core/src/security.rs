//! Password protection (AES-256) and its removal.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::encryption::DecryptionError;
use lopdf::{dictionary, Document, EncryptionState, EncryptionVersion, Permissions};

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protection {
    /// Needed to open the file.
    pub user_password: String,
    /// Grants full access. When `None`, a random one is generated if any
    /// permission is restricted (so the restriction can't be lifted with the
    /// open password), otherwise the open password is used.
    pub owner_password: Option<String>,
    pub allow_print: bool,
    pub allow_copy: bool,
    pub allow_modify: bool,
    pub allow_annotate: bool,
}

impl Protection {
    pub fn with_password(password: &str) -> Self {
        Self {
            user_password: password.to_owned(),
            owner_password: None,
            allow_print: true,
            allow_copy: true,
            allow_modify: true,
            allow_annotate: true,
        }
    }

    fn permissions(&self) -> Permissions {
        let mut permissions = Permissions::COPYABLE_FOR_ACCESSIBILITY;
        if self.allow_print {
            permissions |= Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY;
        }
        if self.allow_copy {
            permissions |= Permissions::COPYABLE;
        }
        if self.allow_modify {
            permissions |= Permissions::MODIFIABLE | Permissions::ASSEMBLABLE;
        }
        if self.allow_annotate {
            permissions |= Permissions::ANNOTABLE | Permissions::FILLABLE;
        }
        permissions
    }

    fn restricted(&self) -> bool {
        !(self.allow_print && self.allow_copy && self.allow_modify && self.allow_annotate)
    }
}

pub fn protect_document(doc: &mut Document, protection: &Protection) -> Result<(), Error> {
    if protection.user_password.is_empty() {
        return Err(Error::InvalidOption("the password must not be empty".into()));
    }
    let generated_owner;
    let owner_password = match &protection.owner_password {
        Some(password) if !password.is_empty() => password.as_str(),
        _ if protection.restricted() => {
            generated_owner = random_hex(32)?;
            &generated_owner
        }
        _ => &protection.user_password,
    };

    let mut file_key = [0u8; 32];
    getrandom::fill(&mut file_key).map_err(|e| Error::InvalidOption(format!("no randomness available: {e}")))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let state = EncryptionState::try_from(EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), filter)]),
        file_encryption_key: &file_key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password,
        user_password: &protection.user_password,
        permissions: protection.permissions(),
    })?;

    // AES-256 is declared through Adobe's extension level 8 to PDF 1.7.
    doc.version = "1.7".into();
    doc.catalog_mut()?.set(
        "Extensions",
        dictionary! { "ADBE" => dictionary! { "BaseVersion" => "1.7", "ExtensionLevel" => 8 } },
    );
    // Streams must be compressed before encryption, never after.
    doc.compress();
    doc.encrypt(&state)?;
    Ok(())
}

pub fn protect_file(input: &Path, protection: &Protection, output: &Path) -> Result<(), Error> {
    let mut doc = crate::load(input)?;
    protect_document(&mut doc, protection)?;
    crate::save(&mut doc, output)
}

/// Removes password protection and permission restrictions. `password` may
/// be empty for files that only restrict permissions.
pub fn unlock_document(doc: &mut Document, password: &str) -> Result<(), Error> {
    if doc.is_encrypted() {
        doc.decrypt(password).map_err(|e| match e {
            lopdf::Error::Decryption(DecryptionError::IncorrectPassword) => Error::WrongPassword,
            other => Error::Pdf(other),
        })?;
    } else if doc.encryption_state.is_none() {
        return Err(Error::NotEncrypted);
    }
    // Loading already decrypted the content; saving writes it unencrypted.
    doc.encryption_state = None;
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Extensions");
    }
    Ok(())
}

pub fn unlock_file(input: &Path, password: &str, output: &Path) -> Result<(), Error> {
    let mut doc = crate::load_unchecked(input)?;
    unlock_document(&mut doc, password)?;
    crate::save(&mut doc, output)
}

fn random_hex(bytes: usize) -> Result<String, Error> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| Error::InvalidOption(format!("no randomness available: {e}")))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
fn allows_print(doc: &Document) -> bool {
    let p = doc.get_encrypted().unwrap().get(b"P").and_then(lopdf::Object::as_i64).unwrap();
    p & (1 << 2) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{page_labels, sample, temp_dir};

    #[test]
    fn protects_and_unlocks() {
        let dir = temp_dir("security");
        let plain = dir.join("plain.pdf");
        let locked = dir.join("locked.pdf");
        let unlocked = dir.join("unlocked.pdf");
        sample("a", 2).save(&plain).unwrap();

        protect_file(&plain, &Protection::with_password("s3cret"), &locked).unwrap();
        let info = crate::inspect(&locked).unwrap();
        assert!(info.needs_password);
        assert!(matches!(crate::load(&locked), Err(Error::Encrypted(_))));

        assert!(matches!(unlock_file(&locked, "wrong", &unlocked), Err(Error::WrongPassword)));
        unlock_file(&locked, "s3cret", &unlocked).unwrap();
        let doc = crate::load(&unlocked).unwrap();
        assert!(!doc.is_encrypted());
        assert_eq!(page_labels(&doc), ["a-1", "a-2"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn restrictions_use_a_separate_owner_password() {
        let mut doc = sample("a", 1);
        let protection = Protection { allow_print: false, ..Protection::with_password("open") };
        protect_document(&mut doc, &protection).unwrap();
        assert!(!allows_print(&doc));
        // The open password must not grant owner access.
        assert!(doc.authenticate_owner_password("open").is_err());
        assert!(doc.authenticate_user_password("open").is_ok());
    }

    #[test]
    fn rejects_empty_password_and_unencrypted_unlock() {
        let mut doc = sample("a", 1);
        assert!(matches!(protect_document(&mut doc, &Protection::with_password("")), Err(Error::InvalidOption(_))));
        assert!(matches!(unlock_document(&mut doc, ""), Err(Error::NotEncrypted)));
    }
}
