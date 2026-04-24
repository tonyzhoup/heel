use std::path::{Path, PathBuf};

use crate::config::SandboxConfigData;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RootAccess {
    Read,
    Write,
    Execute,
    Full,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootGrant {
    pub path: PathBuf,
    pub access: RootAccess,
    pub is_directory: bool,
}

pub(crate) fn grant(path: impl AsRef<Path>, access: RootAccess, is_directory: bool) -> RootGrant {
    RootGrant {
        path: path.as_ref().to_path_buf(),
        access,
        is_directory,
    }
}

pub(crate) fn grants_from_config(config: &SandboxConfigData) -> Vec<RootGrant> {
    let mut grants = Vec::new();

    grants.push(grant(config.working_dir(), RootAccess::Full, true));
    grants.extend(
        config
            .readable_paths()
            .iter()
            .map(|path| grant(path, RootAccess::Read, path.is_dir())),
    );
    grants.extend(
        config
            .writable_paths()
            .iter()
            .map(|path| grant(path, RootAccess::Full, path.is_dir())),
    );
    grants.extend(
        config
            .executable_paths()
            .iter()
            .map(|path| grant(path, RootAccess::Execute, path.is_dir())),
    );

    if let Some(python) = config.python() {
        grants.push(grant(python.venv().path(), RootAccess::Runtime, true));
    }

    grants
}

#[cfg(test)]
mod tests {
    use super::{RootAccess, grant};

    #[test]
    fn grant_keeps_path_and_access() {
        let item = grant("C:/Eureka/session", RootAccess::Full, true);

        assert_eq!(item.path.to_string_lossy(), "C:/Eureka/session");
        assert_eq!(item.access, RootAccess::Full);
        assert!(item.is_directory);
    }

    #[test]
    fn write_access_class_is_available_for_acl_specialization() {
        let item = grant("C:/Eureka/out.log", RootAccess::Write, false);

        assert_eq!(item.access, RootAccess::Write);
        assert!(!item.is_directory);
    }

    #[test]
    fn grants_from_config_maps_roots_to_access_classes() {
        let (_, config) = crate::config::SandboxConfig::builder()
            .working_dir("C:/Eureka/work")
            .readable_path("C:/Eureka/read")
            .writable_path("C:/Eureka/write")
            .executable_path("C:/Eureka/python")
            .build()
            .expect("config")
            .into_parts();

        let grants = super::grants_from_config(&config);

        assert!(
            grants
                .iter()
                .any(|item| item.access == RootAccess::Full && item.path.ends_with("work"))
        );
        assert!(
            grants
                .iter()
                .any(|item| item.access == RootAccess::Read && item.path.ends_with("read"))
        );
        assert!(
            grants
                .iter()
                .any(|item| item.access == RootAccess::Full && item.path.ends_with("write"))
        );
        assert!(
            grants
                .iter()
                .any(|item| item.access == RootAccess::Execute && item.path.ends_with("python"))
        );
    }

    #[test]
    fn grants_from_config_do_not_expand_permissive_security_to_home() {
        let (_, config) = crate::config::SandboxConfig::builder()
            .working_dir("C:/Eureka/work")
            .security(crate::SecurityConfig::permissive())
            .build()
            .expect("config")
            .into_parts();

        let grants = super::grants_from_config(&config);

        assert_eq!(grants.len(), 1);
        assert!(grants[0].path.ends_with("work"));
    }
}
