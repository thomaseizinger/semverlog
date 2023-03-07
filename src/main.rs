use anyhow::{Context, Result};
use clap::Parser;
use git2::Repository;
use std::cmp::Ordering;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use time::OffsetDateTime;

fn main() -> Result<()> {
    let args = Args::parse();
    let repository = Repository::discover(".").context("failed to open git repository")?;

    let mut changes = std::fs::read_dir(".changes")
        .context("failed to open directory `.changes`")?
        .map(|e| Ok(Change::from_path(&e?.path(), &repository)?))
        .collect::<Result<Vec<_>>>()
        .context("failed to read change files")?;

    match args.command {
        Command::ComputeBumpLevel { current_version } => {
            let level = changes
                .iter()
                .map(|change| change.compute_bump_level(&current_version))
                .max()
                .context("expected at least one changelog entry")?;

            println!("{level}")
        }
        Command::CompileChangelog { new_version: version } => {
            changes.sort_by(highest_priority_then_chronologically);

            let (year, month, day) = OffsetDateTime::now_utc().date().to_calendar_date();

            println!("## {version} - {year}-{}-{day}\n", u8::from(month));

            let mut changes_by_kind =
                changes
                    .into_iter()
                    .fold(HashMap::<_, Vec<_>>::new(), |mut map, change| {
                        map.entry(change.kind).or_default().push(change);

                        map
                    });

            for kind in [
                Kind::Added,
                Kind::Fixed,
                Kind::Changed,
                Kind::Removed,
                Kind::Deprecated,
                Kind::Security,
            ] {
                if let Entry::Occupied(changes) = changes_by_kind.entry(kind) {
                    println!("### {}\n", kind.header());

                    for change in changes.get() {
                        println!("- {}", change.content)
                    }
                }
            }
        }
    }

    Ok(())
}

struct Change {
    kind: Kind,
    breaking: Option<bool>,
    priority: Option<u8>,
    created: OffsetDateTime,
    content: String,
}

impl Change {
    fn from_path(path: &PathBuf, repository: &Repository) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let (frontmatter, content) = parse_file_content(content)?;

        let blame = repository
            .blame_file(&path, None)
            .with_context(|| format!("failed to blame {}", path.display()))?;
        let hunk = blame.iter().last().expect("at least one blame entry");
        let time = repository
            .find_object(hunk.final_commit_id(), None)
            .context("failed to get object")?
            .as_commit()
            .expect("is a commit")
            .time();

        Ok(Change {
            kind: frontmatter.kind,
            breaking: frontmatter.breaking,
            priority: frontmatter.priority,
            created: OffsetDateTime::from_unix_timestamp(time.seconds())?,
            content,
        })
    }

    fn compute_bump_level(&self, version: &semver::Version) -> BumpLevel {
        match (version, self.kind, self.breaking) {
            (_, Kind::Security | Kind::Fixed, _) => BumpLevel::Patch, // Is this correct?

            (semver::Version { major: 1.., .. }, Kind::Changed | Kind::Removed, Some(false)) => {
                BumpLevel::Minor
            }
            (semver::Version { major: 1.., .. }, Kind::Changed | Kind::Removed, _) => {
                BumpLevel::Major
            }

            (semver::Version { major: 1.., .. }, _, Some(true)) => BumpLevel::Major,
            (semver::Version { major: 1.., .. }, _, _) => BumpLevel::Minor,

            (
                semver::Version {
                    major: 0,
                    minor: 1..,
                    ..
                },
                Kind::Changed | Kind::Removed,
                Some(false),
            ) => BumpLevel::Patch,
            (
                semver::Version {
                    major: 0,
                    minor: 1..,
                    ..
                },
                Kind::Changed | Kind::Removed,
                _,
            ) => BumpLevel::Minor,

            (
                semver::Version {
                    major: 0,
                    minor: 1..,
                    ..
                },
                _,
                Some(true),
            ) => BumpLevel::Minor,
            (
                semver::Version {
                    major: 0,
                    minor: 1..,
                    ..
                },
                _,
                _,
            ) => BumpLevel::Patch,

            (
                semver::Version {
                    major: 0, minor: 0, ..
                },
                _,
                _,
            ) => BumpLevel::Patch,
        }
    }
}

fn highest_priority_then_chronologically(a: &Change, b: &Change) -> Ordering {
    b.priority.cmp(&a.priority).then(a.created.cmp(&b.created))
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    ComputeBumpLevel { current_version: semver::Version },
    CompileChangelog { new_version: semver::Version },
}

fn parse_file_content(content: String) -> Result<(FrontMatter, String)> {
    let mut parts = content.splitn(3, "---\n");

    let frontmatter =
        serde_yaml::from_str::<FrontMatter>(parts.nth(1).context("Missing frontmatter")?)
            .context("Failed to parse frontmatter")?;
    let body = parts.next().context("Missing body")?.trim().to_owned();

    Ok((frontmatter, body))
}

#[derive(serde::Deserialize, Debug)]
struct FrontMatter {
    kind: Kind,
    breaking: Option<bool>,
    priority: Option<u8>,
}

#[derive(serde::Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Added,
    Fixed,
    Changed,
    Deprecated,
    Removed,
    Security,
}

impl Kind {
    fn header(&self) -> &str {
        match self {
            Kind::Added => "Added",
            Kind::Fixed => "Fixed",
            Kind::Changed => "Changed",
            Kind::Deprecated => "Deprecated",
            Kind::Removed => "Removed",
            Kind::Security => "Security",
        }
    }
}

#[derive(serde::Deserialize, Debug, Ord, PartialOrd, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum BumpLevel {
    Major = 2,
    Minor = 1,
    Patch = 0,
}

impl fmt::Display for BumpLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BumpLevel::Major => write!(f, "major"),
            BumpLevel::Minor => write!(f, "minor"),
            BumpLevel::Patch => write!(f, "patch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn major_greater_minor_greater_patch() {
        assert!(BumpLevel::Major > BumpLevel::Minor);
        assert!(BumpLevel::Minor > BumpLevel::Patch);
    }

    #[test]
    fn sort_order() {
        let mut changes = [
            Change {
                kind: Kind::Added,
                breaking: None,
                priority: Some(1),
                created: OffsetDateTime::now_utc() - Duration::from_secs(10),
                content: "A".to_string(),
            },
            Change {
                kind: Kind::Added,
                breaking: None,
                priority: None,
                created: OffsetDateTime::now_utc(),
                content: "B".to_string(),
            },
            Change {
                kind: Kind::Added,
                breaking: None,
                priority: None,
                created: OffsetDateTime::now_utc() - Duration::from_secs(30),
                content: "C".to_string(),
            },
            Change {
                kind: Kind::Added,
                breaking: None,
                priority: Some(5),
                created: OffsetDateTime::now_utc(),
                content: "D".to_string(),
            },
            Change {
                kind: Kind::Added,
                breaking: None,
                priority: Some(5),
                created: OffsetDateTime::now_utc() - Duration::from_secs(10),
                content: "E".to_string(),
            },
        ];

        changes.sort_by(highest_priority_then_chronologically);

        assert_eq!(changes.map(|c| c.content), ["E", "D", "A", "C", "B"])
    }

    #[test]
    fn computes_bump_level_correctly() {
        assert_eq!(
            entry(Kind::Added, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Changed, false).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Changed, None).compute_bump_level(&"0.1.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Deprecated, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Removed, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Major
        );
        assert_eq!(
            entry(Kind::Security, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Patch
        );
        assert_eq!(
            entry(Kind::Added, true).compute_bump_level(&"0.1.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Fixed, None).compute_bump_level(&"0.1.0".parse().unwrap()),
            BumpLevel::Patch
        );
        assert_eq!(
            entry(Kind::Fixed, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Patch
        );
    }

    fn entry(kind: Kind, breaking: impl Into<Option<bool>>) -> Change {
        Change {
            kind,
            breaking: breaking.into(),
            priority: None,
            created: OffsetDateTime::now_utc(),
            content: "".to_string(),
        }
    }
}
