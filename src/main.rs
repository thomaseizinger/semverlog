use anyhow::{Context, Result};
use clap::Parser;
use std::fmt;

fn main() -> Result<()> {
    let args = Args::parse();

    let changelogs = std::fs::read_dir(".changes")?
        .map(|e| {
            let content = std::fs::read_to_string(e?.path())?;
            let parsed = parse_file_content(content)?;

            Ok(parsed)
        })
        .collect::<Result<Vec<_>>>()?;

    match args.command {
        Command::ComputeBumpLevel { current_version } => {
            let level = changelogs
                .iter()
                .map(|(frontmatter, _)| frontmatter.compute_bump_level(&current_version))
                .max()
                .context("Expected at least one changelog entry")?;

            println!("{level}")
        }
        Command::CompileChangelog => {

        }
    }

    Ok(())
}

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    ComputeBumpLevel { current_version: semver::Version },
    CompileChangelog,
}

fn parse_file_content(content: String) -> Result<(Frontmatter, String)> {
    let mut parts = content.splitn(3, "---\n");

    let frontmatter =
        serde_yaml::from_str::<Frontmatter>(parts.nth(1).context("Missing frontmatter")?)
            .context("Failed to parse frontmatter")?;
    let body = parts.next().context("Missing body")?.trim().to_owned();

    Ok((frontmatter, body))
}

#[derive(serde::Deserialize, Debug)]
struct Frontmatter {
    kind: Kind,
    breaking: Option<bool>,
}

impl Frontmatter {
    fn compute_bump_level(&self, version: &semver::Version) -> BumpLevel {
        match (version, self.kind, self.breaking) {
            (_, Kind::Security, _) => BumpLevel::Patch, // Is this correct?

            (semver::Version { major: 1.., .. }, Kind::Change | Kind::Removal, Some(false)) => {
                BumpLevel::Minor
            }
            (semver::Version { major: 1.., .. }, Kind::Change | Kind::Removal, _) => {
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
                Kind::Change | Kind::Removal,
                Some(false),
            ) => BumpLevel::Patch,
            (
                semver::Version {
                    major: 0,
                    minor: 1..,
                    ..
                },
                Kind::Change | Kind::Removal,
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

#[derive(serde::Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Addition,
    Change,
    Deprecation,
    Removal,
    Security,
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

    #[test]
    fn major_greater_minor_greater_patch() {
        assert!(BumpLevel::Major > BumpLevel::Minor);
        assert!(BumpLevel::Minor > BumpLevel::Patch);
    }

    #[test]
    fn computes_bump_level_correctly() {
        assert_eq!(
            entry(Kind::Addition, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Change, false).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Change, None).compute_bump_level(&"0.1.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Deprecation, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Minor
        );
        assert_eq!(
            entry(Kind::Removal, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Major
        );
        assert_eq!(
            entry(Kind::Security, None).compute_bump_level(&"1.0.0".parse().unwrap()),
            BumpLevel::Patch
        );
        assert_eq!(
            entry(Kind::Addition, true).compute_bump_level(&"0.1.0".parse().unwrap()),
            BumpLevel::Minor
        );
    }

    fn entry(kind: Kind, breaking: impl Into<Option<bool>>) -> Frontmatter {
        Frontmatter {
            kind,
            breaking: breaking.into(),
        }
    }
}
