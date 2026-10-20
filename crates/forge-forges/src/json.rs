//! Lenient accessors for API JSON. Hosts differ between versions (fields appear, vanish or
//! become null), so missing values fall back to defaults instead of failing a whole list.

use serde_json::Value;

use crate::model::{Label, User};

pub fn s(v: &Value, key: &str) -> String {
    os(v, key).unwrap_or_default()
}

pub fn os(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn u(v: &Value, key: &str) -> u64 {
    ou(v, key).unwrap_or(0)
}

pub fn ou(v: &Value, key: &str) -> Option<u64> {
    match v.get(key)? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub fn b(v: &Value, key: &str) -> bool {
    v.get(key).and_then(Value::as_bool).unwrap_or(false)
}

pub fn arr<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// A user object: GitLab `username`/`name`, GitHub/Gitea `login`/`full_name`/`name`.
pub fn user(v: &Value) -> User {
    let username = os(v, "username")
        .or_else(|| os(v, "login"))
        .unwrap_or_default();
    let name = os(v, "full_name")
        .or_else(|| os(v, "name"))
        .filter(|n| !n.is_empty() && *n != username);
    User {
        username,
        name,
        avatar_url: os(v, "avatar_url").filter(|s| !s.is_empty()),
    }
}

pub fn user_at(v: &Value, key: &str) -> User {
    v.get(key).filter(|u| u.is_object()).map(user).unwrap_or_default()
}

pub fn users(v: &Value, key: &str) -> Vec<User> {
    arr(v, key).iter().map(user).collect()
}

/// Labels as strings (GitLab default) or objects with `name` and `color`.
pub fn labels(v: &Value, key: &str) -> Vec<Label> {
    arr(v, key)
        .iter()
        .filter_map(|l| match l {
            Value::String(s) => Some(Label {
                name: s.clone(),
                color: None,
            }),
            Value::Object(_) => Some(Label {
                name: os(l, "name")?,
                color: os(l, "color").map(|c| {
                    if c.starts_with('#') {
                        c
                    } else {
                        format!("#{c}")
                    }
                }),
            }),
            _ => None,
        })
        .collect()
}
