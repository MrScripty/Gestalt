use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, PartialEq, Eq)]
struct HostOpenRequest {
    program: &'static str,
    args: Vec<OsString>,
}

pub(super) fn open_path_in_host(path: &Path) -> Result<String, String> {
    let request = host_open_request(path)?;
    let status = Command::new(request.program)
        .args(&request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| {
            format!(
                "Failed to launch host app for '{}': {error}",
                path.display()
            )
        })?;

    if !status.success() {
        return Err(format!(
            "Host app launch failed for '{}' with status {status}.",
            path.display()
        ));
    }

    Ok(format!(
        "Opened '{}' in the host default app.",
        path.display()
    ))
}

fn host_open_request(path: &Path) -> Result<HostOpenRequest, String> {
    #[cfg(target_os = "linux")]
    {
        return Ok(HostOpenRequest {
            program: "xdg-open",
            args: vec![path.as_os_str().to_os_string()],
        });
    }

    #[cfg(target_os = "macos")]
    {
        return Ok(HostOpenRequest {
            program: "open",
            args: vec![path.as_os_str().to_os_string()],
        });
    }

    #[cfg(target_os = "windows")]
    {
        return Ok(HostOpenRequest {
            program: "explorer",
            args: vec![path.as_os_str().to_os_string()],
        });
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        Err("Host default-open is not implemented on this platform.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::host_open_request;
    use std::path::Path;

    #[test]
    fn host_open_request_uses_expected_launcher_for_current_platform() {
        let request = host_open_request(Path::new("/tmp/example.txt")).expect("request");

        #[cfg(target_os = "linux")]
        assert_eq!(request.program, "xdg-open");

        #[cfg(target_os = "macos")]
        assert_eq!(request.program, "open");

        #[cfg(target_os = "windows")]
        assert_eq!(request.program, "explorer");
    }

    #[test]
    fn host_open_request_passes_path_as_single_argument() {
        let target = Path::new("/tmp/example.txt");
        let request = host_open_request(target).expect("request");

        assert_eq!(request.args.len(), 1);
        assert_eq!(request.args[0], target.as_os_str());
    }
}
