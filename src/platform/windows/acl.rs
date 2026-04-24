use crate::error::Result;

use super::paths::RootGrant;

pub(crate) struct AclGrantGuard {
    count: usize,
}

impl AclGrantGuard {
    pub(crate) fn count(&self) -> usize {
        self.count
    }
}

pub(crate) fn apply_grants_for_sid(
    grants: &[RootGrant],
    _sid_debug_label: &str,
) -> Result<AclGrantGuard> {
    Ok(AclGrantGuard {
        count: grants.len(),
    })
}

#[cfg(target_os = "windows")]
pub(crate) fn apply_grants_for_appcontainer_sid(
    grants: &[RootGrant],
    sid: windows::Win32::Security::PSID,
) -> Result<AclGrantGuard> {
    let _ = sid;
    apply_grants_for_sid(grants, "appcontainer")
}

#[cfg(test)]
mod tests {
    use super::apply_grants_for_sid;
    use crate::platform::windows::paths::{RootAccess, grant};

    #[test]
    fn acl_guard_records_grant_count() {
        let grants = vec![grant("C:/Eureka/work", RootAccess::Full, true)];

        let guard = apply_grants_for_sid(&grants, "S-1-test").expect("guard");

        assert_eq!(guard.count(), 1);
    }
}
