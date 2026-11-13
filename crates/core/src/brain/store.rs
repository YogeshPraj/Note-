//! The brain on disk: reading its file, adding entries, and keeping it either plain or encrypted.
//!
//! - Two writers (two windows, or the user and an agent) never lose an entry: an addition holds a lock file, reads the
//!   note again, and writes through a temporary file and a rename.
//! - A note is read whichever form it is in. With a key, notes are written encrypted (`core::crypto`'s envelope, the
//!   same as any encrypted file); an encrypted note without a key is `Error::Locked`, never guessed at.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::search::Note;
use super::{Source, note_path, with_entry};
use crate::crypto::{self, Dek};
use crate::edit::DateTime;
use crate::error::Error;

/// How long an addition waits for another writer before giving up.
const LOCK_WAIT: Duration = Duration::from_secs(3);
/// A lock older than this was left by a writer that died: it is taken over.
const LOCK_STALE: Duration = Duration::from_secs(10);

fn io(e: &std::io::Error) -> Error {
    Error::Io(e.to_string())
}

/// The lock file, held for one addition and removed when dropped.
#[derive(Debug)]
struct Lock(PathBuf);

impl Lock {
    fn take(root: &Path) -> Result<Lock, Error> {
        std::fs::create_dir_all(root).map_err(|e| io(&e))?;
        let path = root.join(".lock");
        let started = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Lock(path)),
                // Windows says PermissionDenied, not AlreadyExists, for a lock file that is being deleted at that moment.
                Err(e) if matches!(e.kind(), std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::PermissionDenied) => {
                    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
                    if age.is_some_and(|a| a > LOCK_STALE) {
                        let _ = std::fs::remove_file(&path);
                    } else if started.elapsed() > LOCK_WAIT {
                        return Err(Error::Io("Quick Notes is busy (another Note++ is writing to it)".into()));
                    } else {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                }
                Err(e) => return Err(io(&e)),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The brain's folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    pub root: PathBuf,
}

/// A note's file, with its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub path: PathBuf,
    /// The file's name without `.md`.
    pub name: String,
}

impl Store {
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    /// The text of the note at `path`; empty when there is no such file.
    ///
    /// # Errors
    /// `Locked` for an encrypted note when `key` is None; the read's or the decryption's error otherwise.
    pub fn read(&self, path: &Path, key: Option<&Dek>) -> Result<String, Error> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(e) => return Err(io(&e)),
        };
        match (crypto::parse_envelope(&bytes), key) {
            (Some(env), Some(key)) => crypto::decrypt_envelope(&env, key),
            (Some(_), None) => Err(Error::Locked),
            (None, _) => Ok(String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).into_owned()),
        }
    }

    /// Write `text` as the note at `path`, encrypted when `key` is given. The file is replaced in one step, so a
    /// reader never sees half of it.
    ///
    /// # Errors
    /// The encryption's or the write's error; the old file is then untouched.
    pub fn write(&self, path: &Path, text: &str, key: Option<&Dek>) -> Result<(), Error> {
        let body = match key {
            Some(key) => crypto::encrypt_text(text, key, "md")?,
            None => text.to_string(),
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| io(&e))?;
        }
        let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
        std::fs::write(&tmp, body).map_err(|e| io(&e))?;
        // A reader (a virus scanner, a search) may hold the file for a moment, and Windows then refuses the rename.
        let mut renamed = std::fs::rename(&tmp, path);
        for _ in 0..50 {
            if !renamed.as_ref().is_err_and(|e| e.kind() == std::io::ErrorKind::PermissionDenied) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
            renamed = std::fs::rename(&tmp, path);
        }
        renamed.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(&e)
        })
    }

    /// Change the Brain's file: `change` gets its text and gives the new text, which is written (nothing is written
    /// when it is the same). Returns the file's path and the new text.
    ///
    /// # Errors
    /// `Locked` when the file is encrypted and `key` is None; "busy" when another writer holds the lock too long;
    /// the read's or write's error.
    pub fn edit(&self, key: Option<&Dek>, change: &dyn Fn(&str) -> String) -> Result<(PathBuf, String), Error> {
        let _lock = Lock::take(&self.root)?;
        let path = note_path(&self.root);
        let old = self.read(&path, key)?;
        let new = change(&old);
        if new != old || !path.exists() {
            self.write(&path, &new, key)?;
        }
        Ok((path, new))
    }

    /// Add an entry at the end of the file, under the heading of `when`'s day. Returns the file's path and the new
    /// entry's line.
    ///
    /// # Errors
    /// As [`Store::edit`].
    pub fn add(&self, when: &DateTime, text: &str, from: Option<&Source>, key: Option<&Dek>) -> Result<(PathBuf, usize), Error> {
        let (path, note) = self.edit(key, &|old| with_entry(old, when, text, from))?;
        Ok((path, super::entries(&note).last().map_or(1, |e| e.line)))
    }

    /// The Brain's file, then the notes of an earlier version that kept one file per day (`daily`) and per topic
    /// (`topics`): those are read and searched but nothing new goes into them.
    pub fn list(&self) -> Vec<Listed> {
        let md = |p: &Path| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md"));
        let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let files = |dir: PathBuf| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path());
        let listed = |p: PathBuf| Listed { name: stem(&p), path: p };
        let main = note_path(&self.root);
        let mut old: Vec<PathBuf> =
            files(self.root.join("daily")).filter(|p| p.is_dir()).flat_map(files).chain(files(self.root.join("topics"))).filter(|p| md(p)).collect();
        old.sort_by_key(|p| std::cmp::Reverse(p.file_name().map(|n| n.to_string_lossy().to_lowercase())));
        main.exists().then_some(main).into_iter().chain(old).map(listed).collect()
    }

    /// Write every note into a zip at `dest`, as it is on disk (encrypted notes stay encrypted). Returns how many.
    ///
    /// # Errors
    /// A note or the zip cannot be read or written; the half-written zip is removed.
    pub fn export(&self, dest: &Path) -> Result<usize, Error> {
        let _lock = Lock::take(&self.root)?;
        let notes = self.list();
        let run = || -> Result<(), Error> {
            let bad = |e: zip::result::ZipError| Error::Io(e.to_string());
            let mut zip = zip::ZipWriter::new(std::fs::File::create(dest).map_err(|e| io(&e))?);
            for n in &notes {
                let name = n.path.strip_prefix(&self.root).unwrap_or(&n.path).to_string_lossy().replace('\\', "/");
                zip.start_file(name, zip::write::SimpleFileOptions::default()).map_err(bad)?;
                std::io::Write::write_all(&mut zip, &std::fs::read(&n.path).map_err(|e| io(&e))?).map_err(|e| io(&e))?;
            }
            zip.finish().map_err(bad)?;
            Ok(())
        };
        run().inspect_err(|_| {
            let _ = std::fs::remove_file(dest);
        })?;
        Ok(notes.len())
    }

    /// True when a note on disk is encrypted (the Brain's file and the first few older ones are looked at).
    pub fn has_encrypted(&self) -> bool {
        self.list().iter().take(20).any(|l| std::fs::read(&l.path).is_ok_and(|b| crypto::parse_envelope(&b).is_some()))
    }

    /// Rewrite every note in the form `to` asks for (encrypted with it, or plain for None), reading with `from`.
    /// `progress(done, total)` is called before each note and stops the work by returning false. Each note is replaced
    /// in one step, so stopping (or a failure) leaves every note readable in one form or the other.
    ///
    /// # Errors
    /// The first note that cannot be read or written; notes before it are converted, it and the rest are untouched.
    pub fn convert(&self, from: Option<&Dek>, to: Option<&Dek>, progress: &mut dyn FnMut(usize, usize) -> bool) -> Result<usize, Error> {
        let _lock = Lock::take(&self.root)?;
        let notes = self.list();
        for (done, note) in notes.iter().enumerate() {
            if !progress(done, notes.len()) {
                return Ok(done);
            }
            let encrypted = std::fs::read(&note.path).is_ok_and(|b| crypto::parse_envelope(&b).is_some());
            if encrypted != to.is_some() {
                self.write(&note.path, &self.read(&note.path, from)?, to)?;
            }
        }
        Ok(notes.len())
    }
}

/// The notes as last read, so that a search reads again only what changed.
#[derive(Debug, Default)]
pub struct Index {
    seen: HashMap<PathBuf, ((SystemTime, u64), Note)>,
}

impl Index {
    /// Every note of `store`, reading only the files that are new or changed since the last call.
    ///
    /// # Errors
    /// `Locked` when a note is encrypted and `key` is None. A note that cannot be read for another reason is left out.
    pub fn notes(&mut self, store: &Store, key: Option<&Dek>) -> Result<Vec<Note>, Error> {
        let listed = store.list();
        let mut out = Vec::with_capacity(listed.len());
        let mut kept = HashMap::with_capacity(listed.len());
        for l in listed {
            let Some(stamp) = std::fs::metadata(&l.path).ok().and_then(|m| Some((m.modified().ok()?, m.len()))) else { continue };
            let note = match self.seen.remove(&l.path) {
                Some((s, note)) if s == stamp => note,
                _ => match store.read(&l.path, key) {
                    Ok(text) => Note { path: l.path.clone(), name: l.name, text },
                    Err(Error::Locked) => return Err(Error::Locked),
                    Err(_) => continue,
                },
            };
            out.push(note.clone());
            kept.insert(l.path, (stamp, note));
        }
        self.seen = kept;
        Ok(out)
    }

    /// Forget what was read (the brain was locked: decrypted text must not stay in memory).
    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

#[cfg(test)]
mod tests;
