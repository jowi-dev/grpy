//! Where grpy keeps its config and state, per the XDG Base Directory spec.

use std::path::PathBuf;

/// Locations of grpy's files on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `$XDG_CONFIG_HOME/grpy/config.toml`.
    pub config_file: PathBuf,
    /// `$XDG_DATA_HOME/grpy/`, for mutable state such as OAuth tokens and
    /// the record of events already added.
    pub data_dir: PathBuf,
}

impl Paths {
    /// Resolves paths from `XDG_CONFIG_HOME` and `XDG_DATA_HOME`, falling
    /// back to `$HOME/.config` and `$HOME/.local/share`. Empty or relative
    /// XDG values are ignored, as the spec requires.
    ///
    /// `env` looks up an environment variable; pass
    /// `|name| std::env::var(name).ok()` outside of tests. Returns `None`
    /// when a directory can't be resolved because `HOME` is unset too.
    pub fn from_env(env: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let base = |xdg_var: &str, home_fallback: &str| {
            env(xdg_var)
                .map(PathBuf::from)
                .filter(|dir| dir.is_absolute())
                .or_else(|| {
                    env("HOME")
                        .filter(|home| !home.is_empty())
                        .map(|home| PathBuf::from(home).join(home_fallback))
                })
        };

        Some(Self {
            config_file: base("XDG_CONFIG_HOME", ".config")?.join("grpy/config.toml"),
            data_dir: base("XDG_DATA_HOME", ".local/share")?.join("grpy"),
        })
    }

    /// The local database ([`Store`](crate::store::Store)) in [`data_dir`](Self::data_dir).
    pub fn store_file(&self) -> PathBuf {
        self.data_dir.join("grpy.db")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(vars: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn uses_xdg_dirs_when_set() {
        let paths = Paths::from_env(env(&[
            ("HOME", "/home/me"),
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
        ]))
        .unwrap();

        assert_eq!(paths.config_file, PathBuf::from("/cfg/grpy/config.toml"));
        assert_eq!(paths.data_dir, PathBuf::from("/data/grpy"));
    }

    #[test]
    fn falls_back_to_home_defaults() {
        let paths = Paths::from_env(env(&[("HOME", "/home/me")])).unwrap();

        assert_eq!(
            paths.config_file,
            PathBuf::from("/home/me/.config/grpy/config.toml")
        );
        assert_eq!(paths.data_dir, PathBuf::from("/home/me/.local/share/grpy"));
    }

    #[test]
    fn ignores_empty_and_relative_xdg_dirs() {
        // The XDG spec says relative paths are invalid and must be ignored.
        let paths = Paths::from_env(env(&[
            ("HOME", "/home/me"),
            ("XDG_CONFIG_HOME", ""),
            ("XDG_DATA_HOME", "relative/data"),
        ]))
        .unwrap();

        assert_eq!(
            paths.config_file,
            PathBuf::from("/home/me/.config/grpy/config.toml")
        );
        assert_eq!(paths.data_dir, PathBuf::from("/home/me/.local/share/grpy"));
    }

    #[test]
    fn store_file_is_in_the_data_dir() {
        let paths =
            Paths::from_env(env(&[("XDG_DATA_HOME", "/data"), ("HOME", "/home/me")])).unwrap();

        assert_eq!(paths.store_file(), PathBuf::from("/data/grpy/grpy.db"));
    }

    #[test]
    fn none_without_home_or_xdg() {
        assert_eq!(Paths::from_env(env(&[])), None);
    }
}
