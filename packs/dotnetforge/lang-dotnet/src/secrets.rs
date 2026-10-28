//! User secrets (Rider's "Manage User Secrets"): a project's `UserSecretsId`, where its
//! `secrets.json` lives (outside the project, in the user profile, so it is never
//! committed), and adding a `UserSecretsId` to a project file that has none — what
//! `dotnet user-secrets init` does, without running anything.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSecrets {
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// The `secrets.json` path (absolute; `None` without an id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub exists: bool,
}

/// `secrets.json` of an id: `%APPDATA%\Microsoft\UserSecrets\<id>\secrets.json` on
/// Windows, `~/.microsoft/usersecrets/<id>/secrets.json` elsewhere.
pub fn secrets_path(id: &str, os: &str, appdata: Option<&str>, home: &str) -> String {
    if os == "windows" {
        let base = appdata
            .map(str::to_string)
            .unwrap_or_else(|| format!("{home}\\AppData\\Roaming"));
        format!(
            "{}\\Microsoft\\UserSecrets\\{id}\\secrets.json",
            base.trim_end_matches('\\')
        )
    } else {
        format!(
            "{}/.microsoft/usersecrets/{id}/secrets.json",
            home.trim_end_matches('/')
        )
    }
}

/// Whether an id is safe as a folder name (`dotnet user-secrets` rejects the others).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !id.starts_with('.')
}

/// A random-looking GUID (version 4 layout) from 16 bytes.
pub fn new_id(bytes: [u8; 16]) -> String {
    let mut b = bytes;
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        h[0..4].concat(),
        h[4..6].concat(),
        h[6..8].concat(),
        h[8..10].concat(),
        h[10..16].concat()
    )
}

/// The project file with `<UserSecretsId>` added to its first unconditional
/// `<PropertyGroup>` (or a new one), keeping the file's indentation.
pub fn add_user_secrets_id(project_xml: &str, id: &str) -> Result<String, String> {
    if project_xml.contains("<UserSecretsId>") {
        return Err("The project already has a UserSecretsId".into());
    }
    let mut search = 0;
    while let Some(rel) = project_xml[search..].find("<PropertyGroup") {
        let start = search + rel;
        let Some(gt) = project_xml[start..].find('>') else {
            break;
        };
        let open_tag = &project_xml[start..start + gt + 1];
        search = start + gt + 1;
        if open_tag.contains("Condition") || open_tag.ends_with("/>") {
            continue;
        }
        let Some(close_rel) = project_xml[search..].find("</PropertyGroup>") else {
            break;
        };
        let close = search + close_rel;
        // Indentation of the group's last child, else two more spaces than the group.
        let body = &project_xml[search..close];
        let line_start = project_xml[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let group_indent: String = project_xml[line_start..start]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let child_indent = body
            .lines()
            .rev()
            .find(|l| l.trim_start().starts_with('<'))
            .map(|l| {
                l.chars()
                    .take_while(|c| c.is_whitespace())
                    .collect::<String>()
            })
            .unwrap_or_else(|| format!("{group_indent}  "));
        let before_close = project_xml[..close].trim_end_matches([' ', '\t']);
        let mut out = String::new();
        out.push_str(before_close);
        if !before_close.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!(
            "{child_indent}<UserSecretsId>{id}</UserSecretsId>\n{group_indent}"
        ));
        out.push_str(&project_xml[close..]);
        return Ok(out);
    }
    let end = project_xml
        .rfind("</Project>")
        .ok_or("Not an MSBuild project file")?;
    let mut out = project_xml[..end].trim_end().to_string();
    out.push_str(&format!(
        "\n\n  <PropertyGroup>\n    <UserSecretsId>{id}</UserSecretsId>\n  </PropertyGroup>\n\n"
    ));
    out.push_str(&project_xml[end..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_per_platform() {
        assert_eq!(
            secrets_path("abc", "linux", None, "/home/anna/"),
            "/home/anna/.microsoft/usersecrets/abc/secrets.json"
        );
        assert_eq!(
            secrets_path(
                "abc",
                "windows",
                Some("C:\\Users\\anna\\AppData\\Roaming"),
                "C:\\Users\\anna"
            ),
            "C:\\Users\\anna\\AppData\\Roaming\\Microsoft\\UserSecrets\\abc\\secrets.json"
        );
        assert!(valid_id("aspnet-Bookstore-1f2e"));
        assert!(!valid_id("../etc"));
        assert!(!valid_id(""));
    }

    #[test]
    fn ids() {
        let id = new_id([0xab; 16]);
        assert_eq!(id, "abababab-abab-4bab-abab-abababababab");
        assert!(valid_id(&id));
    }

    #[test]
    fn adds_the_id_to_the_project() {
        let csproj = "<Project Sdk=\"Microsoft.NET.Sdk.Web\">\n\n  <PropertyGroup Condition=\"'$(Configuration)' == 'Release'\">\n    <Optimize>true</Optimize>\n  </PropertyGroup>\n\n  <PropertyGroup>\n    <TargetFramework>net8.0</TargetFramework>\n    <Nullable>enable</Nullable>\n  </PropertyGroup>\n\n</Project>\n";
        let out = add_user_secrets_id(csproj, "id-1").unwrap();
        assert!(out.contains("    <Nullable>enable</Nullable>\n    <UserSecretsId>id-1</UserSecretsId>\n  </PropertyGroup>\n\n</Project>\n"), "{out}");
        assert!(add_user_secrets_id(&out, "id-2").is_err());
        let bare = add_user_secrets_id("<Project Sdk=\"Microsoft.NET.Sdk\">\n</Project>\n", "id-3")
            .unwrap();
        assert_eq!(bare, "<Project Sdk=\"Microsoft.NET.Sdk\">\n\n  <PropertyGroup>\n    <UserSecretsId>id-3</UserSecretsId>\n  </PropertyGroup>\n\n</Project>\n");
    }
}
