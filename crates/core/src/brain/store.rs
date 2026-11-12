//! The brain on disk: reading notes, adding entries, and keeping every note either plain or encrypted.
//!
//! - Two writers (two windows, or the user and an agent) never lose an entry: an addition holds a lock file, reads the
//!   note again, and writes through a temporary file and a rename.
//! - A note is read whichever form it is in. With a key, notes are written encrypted (`core::crypto`'s envelope, the
//!   same as any encrypted file); an encrypted note without a key is `Error::Locked`, never guessed at.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::date::Date;
use super::search::Note;
use super::{Source, day_path, with_entry};
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
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
                    if age.is_some_and(|a| a > LOCK_STALE) {
                        let _ = std::fs::remove_file(&path);
                    } else if started.elapsed() > LOCK_WAIT {
                        return Err(Error::Io("the Brain is busy (another Note++ is writing to it)".into()));
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

/// A note's file, with what its name says about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Listed {
    pub path: PathBuf,
    /// The day of a day's note.
    pub day: Option<Date>,
    /// A topic's name, or the day as text.
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
        std::fs::rename(&tmp, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(&e)
        })
    }

    /// Add an entry to the note of `when`'s day. Returns the note's path and the new entry's line.
    ///
    /// # Errors
    /// `Locked` when the day's note is encrypted and `key` is None; "busy" when another writer holds the lock too long;
    /// the read's or write's error.
    pub fn add(&self, when: &DateTime, text: &str, from: Option<&Source>, key: Option<&Dek>) -> Result<(PathBuf, usize), Error> {
        let _lock = Lock::take(&self.root)?;
        let path = day_path(&self.root, Date::of(when));
        let note = with_entry(&self.read(&path, key)?, when, text, from);
        let line = super::entries(&note).last().map_or(1, |e| e.line);
        self.write(&path, &note, key)?;
        Ok((path, line))
    }

    /// Every note's file: day notes newest first, then topics by name.
    pub fn list(&self) -> Vec<Listed> {
        let md = |p: &Path| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md"));
        let stem = |p: &Path| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let files = |dir: PathBuf| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path());
        let mut days: Vec<Listed> = files(self.root.join("daily"))
            .filter(|p| p.is_dir())
            .flat_map(files)
            .filter(|p| md(p))
            .filter_map(|p| Date::parse(&stem(&p)).map(|d| Listed { day: Some(d), name: d.to_string(), path: p }))
            .collect();
        days.sort_by_key(|d| std::cmp::Reverse(d.day));
        let mut topics: Vec<Listed> = files(self.root.join("topics")).filter(|p| md(p)).map(|p| Listed { day: None, name: stem(&p), path: p }).collect();
        topics.sort_by_key(|t| t.name.to_lowercase());
        days.into_iter().chain(topics).collect()
    }

    /// True when a note on disk is encrypted (the newest notes are looked at: they are the ones that get written).
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

/// The notes as last read, so that a search reads again only what changed (years of daily notes are many files).
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
                    Ok(text) => Note { path: l.path.clone(), day: l.day, name: l.name, text },
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
