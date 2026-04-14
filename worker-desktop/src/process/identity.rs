use std::path::Path;

pub(super) fn process_identity_matches_worker(
    exe_path: Option<&Path>,
    command_line: Option<String>,
    expected_binary: Option<&Path>,
    binary_name: &str,
) -> bool {
    if let Some(exe_path) = exe_path {
        if exe_path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
            return true;
        }

        if let Some(expected_binary) = expected_binary {
            if exe_path == expected_binary {
                return true;
            }
        }
    }

    let Some(command_line) = command_line else {
        return false;
    };

    if let Some(expected_binary) = expected_binary {
        let expected_binary = expected_binary.to_string_lossy();
        if command_line.contains(expected_binary.as_ref()) {
            return true;
        }
    }

    command_line.split_whitespace().any(|argument| {
        Path::new(argument)
            .file_name()
            .and_then(|name| name.to_str())
            == Some(binary_name)
    })
}
