use super::*;
use crate::brain::entries;

fn at(day: u32, hour: u32, minute: u32) -> DateTime {
    DateTime { year: 2026, month: 11, day, weekday: 4, hour, minute, second: 0 }
}

/// A fresh, empty brain folder for one test.
fn store(name: &str) -> Store {
    let root = std::env::temp_dir().join(format!("notepp_brain_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    Store::new(root)
}

fn key() -> Dek {
    crypto::create_profile("a test password").unwrap().1
}

#[test]
fn captures_land_in_the_one_file_in_order() {
    let s = store("add");
    let (path, line) = s.add(&at(12, 9, 0), "first", None, None).unwrap();
    assert_eq!((path.clone(), line), (note_path(&s.root), 3));
    let (_, line) = s.add(&at(12, 9, 30), "second #tag", Some(&Source { file: "a.rs".into(), line: 7 }), None).unwrap();
    assert_eq!(line, 6);
    let note = s.read(&path, None).unwrap();
    assert_eq!(note, "# 2026-11-12 Thursday\n\n## 09:00\nfirst\n\n## 09:30\nsecond #tag\nfrom: a.rs:7\n");
    assert!(!s.root.join(".lock").exists(), "the lock is given back");
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn writers_at_the_same_time_lose_nothing() {
    let s = store("race");
    let threads: Vec<_> = (0..8u32)
        .map(|t| {
            let s = s.clone();
            std::thread::spawn(move || {
                for i in 0..5u32 {
                    s.add(&at(12, t, i), &format!("writer {t} entry {i}"), None, None).unwrap();
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let note = s.read(&note_path(&s.root), None).unwrap();
    assert_eq!(entries(&note).len(), 40, "{note}");
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn a_lock_left_by_a_dead_writer_is_taken_over() {
    let s = store("stale");
    std::fs::create_dir_all(&s.root).unwrap();
    let lock = s.root.join(".lock");
    std::fs::write(&lock, "").unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options().write(true).open(&lock).unwrap().set_modified(old).unwrap();
    s.add(&at(12, 9, 0), "still works", None, None).unwrap();
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn with_a_key_notes_are_encrypted_and_need_it_to_be_read() {
    let (s, key) = (store("enc"), key());
    let (path, _) = s.add(&at(12, 9, 0), "a secret", None, Some(&key)).unwrap();
    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("secret") && crypto::parse_envelope(raw.as_bytes()).is_some(), "{raw}");
    assert!(matches!(s.read(&path, None), Err(Error::Locked)));
    assert!(matches!(s.add(&at(12, 9, 5), "more", None, None), Err(Error::Locked)), "never added in plain beside it");
    s.add(&at(12, 9, 10), "another", None, Some(&key)).unwrap();
    assert_eq!(entries(&s.read(&path, Some(&key)).unwrap()).len(), 2);
    assert!(s.has_encrypted());
    assert!(matches!(s.read(&path, Some(&self::key())), Err(Error::Decryption)), "another key does not open it");
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn turning_encryption_on_and_off_rewrites_the_file_and_older_notes_and_can_stop_midway() {
    let (s, key) = (store("convert"), key());
    s.add(&at(1, 9, 0), "day 1", None, None).unwrap();
    s.add(&at(2, 9, 0), "day 2", None, None).unwrap();
    let old = s.root.join("topics").join("Azure.md");
    s.write(&old, "# Azure\n\nslots\n", None).unwrap();
    // Stopped before the second note: the Brain's file is encrypted, the older note still plain, and both are readable.
    let done = s.convert(None, Some(&key), &mut |done, _| done < 1).unwrap();
    assert_eq!(done, 1);
    let forms: Vec<bool> = s.list().iter().map(|l| crypto::parse_envelope(&std::fs::read(&l.path).unwrap()).is_some()).collect();
    assert_eq!(forms, [true, false]);
    assert!(s.list().iter().all(|l| s.read(&l.path, Some(&key)).is_ok()));
    assert_eq!(s.convert(None, Some(&key), &mut |_, _| true).unwrap(), 2);
    assert!(s.list().iter().all(|l| matches!(s.read(&l.path, None), Err(Error::Locked))));
    assert_eq!(s.convert(Some(&key), None, &mut |_, _| true).unwrap(), 2);
    assert_eq!(s.read(&old, None).unwrap(), "# Azure\n\nslots\n");
    assert!(!s.has_encrypted());
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn the_brain_file_comes_first_then_older_notes_and_nothing_else() {
    let s = store("list");
    for d in [3, 9, 5] {
        s.add(&at(d, 9, 0), "x", None, None).unwrap();
    }
    for t in ["zeta", "Alpha"] {
        s.write(&s.root.join("topics").join(format!("{t}.md")), "# t\n", None).unwrap();
    }
    s.write(&s.root.join("daily").join("2026").join("2026-11-01.md"), "# 2026-11-01 Sunday\n", None).unwrap();
    std::fs::write(s.root.join("topics").join("not a note.txt"), "x").unwrap();
    let names: Vec<String> = s.list().into_iter().map(|l| l.name).collect();
    assert_eq!(names, ["quick-notes", "zeta", "Alpha", "2026-11-01"]);
    assert!(store("nothing").list().is_empty());
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn several_days_share_the_file_and_each_entry_knows_its_day() {
    let s = store("days");
    for d in [3, 3, 9] {
        s.add(&at(d, 9, 0), "x", None, None).unwrap();
    }
    let note = s.read(&note_path(&s.root), None).unwrap();
    assert_eq!(note.matches("# 2026-11-").count(), 2, "one heading per day: {note}");
    let days: Vec<u32> = entries(&note).iter().map(|e| e.day.unwrap().day).collect();
    assert_eq!(days, [3, 3, 9]);
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn the_index_reads_again_only_what_changed() {
    let s = store("index");
    let (first, _) = s.add(&at(1, 9, 0), "one", None, None).unwrap();
    s.write(&s.root.join("topics").join("Azure.md"), "# Azure\n", None).unwrap();
    let mut index = Index::default();
    assert_eq!(index.notes(&s, None).unwrap().len(), 2);
    // Changed behind the index's back without touching size or time: the kept text is served, so nothing was re-read.
    let stamp = std::fs::metadata(&first).unwrap().modified().unwrap();
    std::fs::write(&first, std::fs::read_to_string(&first).unwrap().replace("one", "ONE")).unwrap();
    std::fs::File::options().write(true).open(&first).unwrap().set_modified(stamp).unwrap();
    assert!(index.notes(&s, None).unwrap().iter().any(|n| n.text.contains("one")));
    s.add(&at(1, 10, 0), "more", None, None).unwrap();
    let notes = index.notes(&s, None).unwrap();
    assert!(notes.iter().any(|n| n.text.contains("ONE") && n.text.contains("more")), "a changed note is read again");
    std::fs::remove_file(&first).unwrap();
    assert_eq!(index.notes(&s, None).unwrap().len(), 1, "a deleted note is gone");
    let _ = std::fs::remove_dir_all(&s.root);
}

#[test]
fn export_zips_every_file_by_its_relative_name() {
    let s = store("export");
    s.add(&at(12, 9, 0), "one", None, None).unwrap();
    std::fs::create_dir_all(s.root.join("topics")).unwrap();
    std::fs::write(s.root.join("topics").join("Azure.md"), "# Azure\n").unwrap();
    let zip_path = s.root.join("out.zip");
    assert_eq!(s.export(&zip_path).unwrap(), 2);
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..z.len()).map(|i| z.by_index(i).unwrap().enclosed_name().unwrap().to_string_lossy().replace('\\', "/")).collect();
    names.sort();
    assert_eq!(names, ["quick-notes.md", "topics/Azure.md"]);
    let _ = std::fs::remove_dir_all(&s.root);
}
