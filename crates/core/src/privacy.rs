use std::path::Path;

/// Restrict local session tokens, databases and optional service settings to the
/// current OS user. Does not inspect any credential store or secret contents.
pub fn protect_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|_| "Cannot create private application directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "Cannot protect application directory")?;
    }
    #[cfg(windows)]
    {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
        };
        let absolute =
            std::fs::canonicalize(path).map_err(|_| "Cannot resolve application directory")?;
        let literal = absolute
            .to_string_lossy()
            .trim_start_matches(r"\\?\")
            .replace('\'', "''");
        let script = format!(
            r#"$ErrorActionPreference='Stop'; $acl=New-Object System.Security.AccessControl.DirectorySecurity; $sid=[System.Security.Principal.WindowsIdentity]::GetCurrent().User; $acl.SetAccessRuleProtection($true,$false); $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule($sid,'FullControl','ContainerInherit,ObjectInherit','None','Allow'))); $acl.AddAccessRule((New-Object System.Security.AccessControl.FileSystemAccessRule((New-Object System.Security.Principal.SecurityIdentifier('S-1-5-18')),'FullControl','ContainerInherit,ObjectInherit','None','Allow'))); [System.IO.Directory]::SetAccessControl('{literal}',$acl)"#
        );
        let status = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|_| "Cannot apply private data permissions")?;
        if !status.success() {
            return Err(
                "Cannot protect application directory; refusing to write a session token".into(),
            );
        }
    }
    Ok(())
}
