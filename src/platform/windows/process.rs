pub(crate) fn quote_arg(input: &str) -> String {
    if input.is_empty() {
        return "\"\"".to_string();
    }

    let needs_quotes = input
        .chars()
        .any(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r' | '"'));

    if !needs_quotes {
        return input.to_string();
    }

    let mut quoted = String::with_capacity(input.len() + 2);
    quoted.push('"');

    let mut backslashes = 0;
    for ch in input.chars() {
        match ch {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.extend(std::iter::repeat_n('\\', backslashes));
                quoted.push(ch);
                backslashes = 0;
            }
        }
    }

    quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

pub(crate) fn command_line(program: &str, args: &[String]) -> String {
    std::iter::once(quote_arg(program))
        .chain(args.iter().map(|arg| quote_arg(arg)))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(target_os = "windows")]
pub(crate) struct WindowsLaunch<'a> {
    pub(crate) program: &'a str,
    pub(crate) args: &'a [String],
    pub(crate) current_dir: &'a std::path::Path,
    pub(crate) envs: &'a [(String, String)],
}

#[cfg(target_os = "windows")]
pub(crate) fn launch_appcontainer_process(
    launch: WindowsLaunch<'_>,
    _profile: &super::profile::AppContainerProfile,
) -> crate::error::Result<crate::platform::Child> {
    let _ = command_line(launch.program, launch.args);
    let _ = (launch.current_dir, launch.envs);
    Err(crate::error::Error::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::{command_line, quote_arg};

    #[test]
    fn command_line_quotes_spaces_and_quotes() {
        let args = ["-c".to_string(), "print(\"hi\")".to_string()];

        assert_eq!(
            command_line("C:/Program Files/Python/python.exe", &args),
            "\"C:/Program Files/Python/python.exe\" -c \"print(\\\"hi\\\")\""
        );
    }

    #[test]
    fn quote_arg_quotes_empty_arguments() {
        assert_eq!(quote_arg(""), "\"\"");
    }

    #[test]
    fn quote_arg_preserves_trailing_backslashes_before_closing_quote() {
        assert_eq!(
            quote_arg(r"C:\Program Files\Python\"),
            r#""C:\Program Files\Python\\""#
        );
    }
}
