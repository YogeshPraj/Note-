//! Reading vault items, creating them and generating passwords.

use super::*;

// ---- reading --------------------------------------------------------------------------------

/// What the list shows about an item. Never the secret itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url: String,
    pub has_password: bool,
    pub has_totp: bool,
}

pub fn parse_items(json: &str) -> Result<Vec<Item>, Error> {
    let v: Value = serde_json::from_str(json).map_err(|_| Error::ItemListUnreadable)?;
    let arr = v.as_array().ok_or(Error::ItemListUnreadable)?;
    Ok(arr
        .iter()
        .filter_map(|it| {
            let id = it.get("id")?.as_str()?.to_string();
            let login = it.get("login");
            let text = |v: Option<&Value>, k: &str| v.and_then(|l| l.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
            Some(Item {
                id,
                title: it.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("(untitled)").to_string(),
                username: text(login, "username"),
                url: login
                    .and_then(|l| l.get("uris"))
                    .and_then(Value::as_array)
                    .and_then(|u| u.first())
                    .and_then(|u| u.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                has_password: login.and_then(|l| l.get("password")).is_some_and(|p| !p.is_null()),
                has_totp: login.and_then(|l| l.get("totp")).is_some_and(|p| !p.is_null()),
            })
        })
        .collect())
}

pub fn list(query: &str) -> Result<Vec<Item>, Error> {
    if !is_unlocked() {
        return Err(Error::Locked);
    }
    touch();
    let mut args = vec!["list", "items"];
    if !query.is_empty() {
        args.extend(["--search", query]);
    }
    let r = run(&args, &[], None, QUICK);
    parse_items(&r.stdout).map_err(|_| Error::Cli(nonempty(&r.stderr, "could not list items")))
}

pub const FIELDS: [&str; 5] = ["password", "username", "totp", "uri", "notes"];

/// One field of one item, fetched fresh and never cached.
pub fn get_field(id: &str, field: &str) -> Result<String, Error> {
    if !is_unlocked() {
        return Err(Error::Locked);
    }
    if !FIELDS.contains(&field) {
        return Err(Error::FieldNotAllowed);
    }
    touch();
    let r = run(&["get", field, id], &[], None, QUICK);
    if !r.ok {
        return Err(Error::Cli(nonempty(&r.stderr, &format!("bw get {field} failed"))));
    }
    let v = r.stdout.trim_end_matches(['\r', '\n']).to_string();
    if v.is_empty() { Err(Error::MissingField(field.to_string())) } else { Ok(v) }
}

/// "Item name" or "Item name.field" (field: password, username, totp, uri, notes) -> its value.
/// An item is found by exact (case-insensitive) title, else the first search hit.
pub fn resolve_reference(reference: &str) -> Result<String, Error> {
    let (title, field) = split_reference(reference);
    let items = list(&title)?;
    let item = items.iter().find(|i| i.title.eq_ignore_ascii_case(&title)).or_else(|| items.first()).ok_or_else(|| Error::NoMatchingItem(title.clone()))?;
    get_field(&item.id, field)
}

pub fn split_reference(reference: &str) -> (String, &'static str) {
    let r = reference.trim();
    if let Some(dot) = r.rfind('.') {
        let tail = &r[dot + 1..];
        if dot > 0
            && let Some(f) = FIELDS.iter().find(|f| **f == tail)
        {
            return (r[..dot].to_string(), f);
        }
    }
    (r.to_string(), "password")
}

// ---- writing --------------------------------------------------------------------------------

#[derive(Default, Clone, Debug)]
pub struct NewItem {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
}

pub fn create_payload(item: &NewItem) -> Result<String, Error> {
    let title = item.title.trim();
    if title.is_empty() {
        return Err(Error::TitleRequired);
    }
    let opt = |s: &str| if s.is_empty() { Value::Null } else { Value::String(s.to_string()) };
    let uris = if item.url.trim().is_empty() { Value::Null } else { serde_json::json!([{ "match": null, "uri": item.url.trim() }]) };
    let payload = serde_json::json!({
        "organizationId": null, "collectionIds": null, "folderId": null,
        "type": 1, "name": title, "notes": opt(&item.notes), "favorite": false, "fields": [],
        "login": { "uris": uris, "username": opt(&item.username), "password": opt(&item.password), "totp": null },
        "reprompt": 0
    });
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(payload.to_string()))
}

/// Add a login to the vault. The item goes to `bw` on stdin, not on the command line, where any
/// other program could read it.
pub fn create_item(item: &NewItem) -> Result<(), Error> {
    if !is_unlocked() {
        return Err(Error::Locked);
    }
    touch();
    let b64 = create_payload(item)?;
    let r = run(&["create", "item"], &[], Some(&b64), Duration::from_secs(45));
    if r.ok { Ok(()) } else { Err(Error::Cli(nonempty(&r.stderr, "bw create item failed"))) }
}

#[derive(Debug)]
pub struct GenOpts {
    pub length: u32,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for GenOpts {
    fn default() -> Self {
        GenOpts { length: 20, upper: true, lower: true, digits: true, symbols: false }
    }
}

pub fn generate_args(o: &GenOpts) -> Vec<String> {
    let mut a = vec!["generate".to_string(), "--length".into(), o.length.clamp(5, 128).to_string()];
    for (on, flag) in [(o.upper, "--uppercase"), (o.lower, "--lowercase"), (o.digits, "--number"), (o.symbols, "--special")] {
        if on {
            a.push(flag.into());
        }
    }
    a
}

/// A password from Bitwarden's own generator (no generator of our own to get wrong).
pub fn generate(o: &GenOpts) -> Result<String, Error> {
    let args = generate_args(o);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = run(&refs, &[], None, QUICK);
    if !r.ok {
        return Err(Error::Cli(nonempty(&r.stderr, "bw generate failed")));
    }
    let v = r.stdout.trim_end_matches(['\r', '\n']).to_string();
    if v.is_empty() { Err(Error::GenerateEmpty) } else { Ok(v) }
}
